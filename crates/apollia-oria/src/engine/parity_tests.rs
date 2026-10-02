//! Characterization test for an orchestrated-mode capability that had no
//! end-to-end proof through the engine: a plan cache hit that skips the
//! Reasoner.
//!
//! It pins today's behaviour so a change of execution engine can be checked
//! against it, driving the public `ORIAEngine` surface with a counting mock
//! model rather than the private cache helpers.

use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use apollia_core::{AIPTask, AgentManifest, RuntimeEvent, StepBudgetConfig, TaskStatus};
use apollia_llm::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, StreamChunk,
    TokenUsage,
};

use crate::actor::ToolProxyTrait;
use crate::engine::{AIPAgent, ORIAEngine};

const PLAN_JSON: &str = r#"{"steps":[
    {"step_id":"s1","description":"read the input","tool_hint":"file_io","depends_on":[]},
    {"step_id":"s2","description":"run the check","tool_hint":"bash_executor","depends_on":["s1"]}
]}"#;

/// Answers every completion with the same plan and counts the calls.
struct CountingPlanModel {
    calls: AtomicU32,
}

impl CountingPlanModel {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicU32::new(0),
        })
    }

    fn calls(&self) -> u32 {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl CompletionModel for CountingPlanModel {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(CompletionResponse {
            engine_timings: None,
            content: PLAN_JSON.to_string(),
            tool_calls: vec![],
            usage: TokenUsage::default(),
            finish_reason: FinishReason::Stop,
            latency_ms: 0,
            ttft_ms: None,
        })
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
    {
        Err(LlmError::InferenceError("mock does not stream".into()))
    }

    fn is_available(&self) -> bool {
        true
    }

    fn backend_name(&self) -> &str {
        "counting-plan"
    }

    fn model_id(&self) -> &str {
        "counting-plan"
    }
}

struct EchoToolProxy;

#[async_trait::async_trait]
impl ToolProxyTrait for EchoToolProxy {
    async fn invoke(&self, tool_name: &str, _input: &serde_json::Value) -> Result<String, String> {
        Ok(format!("{tool_name} ok"))
    }
}

struct OrchestratedAgent;

impl AIPAgent for OrchestratedAgent {
    fn manifest(&self) -> AgentManifest {
        orchestrated_manifest()
    }
}

fn orchestrated_manifest() -> AgentManifest {
    AgentManifest {
        autonomy_level: None,
        format_version: 1,
        name: "parity-agent".into(),
        version: "1.0.0".into(),
        description: "Orchestrated agent used to pin the plan cache".into(),
        tools_required: vec!["file_io".into(), "bash_executor".into()],
        tools_optional: vec![],
        supports_streaming: false,
        supports_a2a: false,
        memory_namespace: None,
        shared_memory_namespaces: vec![],
        max_concurrent_tasks: 1,
        step_budget: Some(StepBudgetConfig {
            max_steps: 20,
            max_tool_calls: 50,
            wall_clock_secs: 600,
        }),
        network_allowlist: None,
        dangerous_tools_allowed: false,
        tags: vec![],
        skills: vec![],
        execution_mode: "orchestrated".into(),
        supports_mailbox: false,
        mailbox_allowlist: None,
        system_prompt: Some("Plan the necessary steps.".into()),
        tools_requiring_approval: vec![],
        llm_backend: None,
        model: None,
        packages: vec![],
        memory_config: None,
        agent_type: None,
        examples: vec![],
        limitations: vec![],
        setup_notes: None,
        agent_class: None,
        user_memory_write: false,
        datasources: vec![],
        templates: vec![],
        secrets: vec![],
        check_commands: vec![],
    }
}

fn drain(rx: &mut tokio::sync::broadcast::Receiver<RuntimeEvent>) -> Vec<RuntimeEvent> {
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    events
}

fn count_cache_hits(events: &[RuntimeEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, RuntimeEvent::PlanCacheHit { .. }))
        .count()
}

fn count_plans_generated(events: &[RuntimeEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, RuntimeEvent::PlanGenerated { .. }))
        .count()
}

#[tokio::test]
async fn test_orchestrated_second_identical_task_is_served_by_the_plan_cache() {
    // GIVEN an orchestrated engine with a shared plan cache and a counting model
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = crate::plan_cache::PlanCacheRepository::open(&dir.path().join("plan_cache.db"))
        .expect("open plan cache");
    let model = CountingPlanModel::new();
    let (tx, mut rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(128);
    let engine = ORIAEngine::new()
        .with_reasoner(model.clone(), 20)
        .with_tool_proxy(Arc::new(EchoToolProxy))
        .with_event_bus(tx)
        .with_shared_plan_cache(Arc::new(Mutex::new(repo)));
    let first = engine.execute(AIPTask::default(), &OrchestratedAgent).await;
    let first_events = drain(&mut rx);
    let calls_after_first = model.calls();

    // WHEN the same task runs a second time
    let second = engine.execute(AIPTask::default(), &OrchestratedAgent).await;
    let second_events = drain(&mut rx);

    // THEN both runs complete, the first plans through the Reasoner and the
    // second is a cache hit that never asks the model for a plan
    assert_eq!(
        (first.status, second.status),
        (TaskStatus::Completed, TaskStatus::Completed),
        "both runs must complete: {:?} / {:?}",
        first.error,
        second.error
    );
    assert_eq!(
        (
            count_plans_generated(&first_events),
            count_cache_hits(&first_events)
        ),
        (1, 0),
        "the first run plans through the Reasoner"
    );
    // The cached path still publishes `PlanGenerated`, so the interface shows
    // the plan it runs; what tells the two apart is the `PlanCacheHit` before it.
    assert_eq!(
        (
            count_plans_generated(&second_events),
            count_cache_hits(&second_events)
        ),
        (1, 1),
        "the second run is served from the cache"
    );
    assert!(
        calls_after_first >= 1,
        "the first run must call the model to plan"
    );
    assert_eq!(
        model.calls(),
        calls_after_first,
        "a cache hit runs the cached plan without one more model call"
    );
}
