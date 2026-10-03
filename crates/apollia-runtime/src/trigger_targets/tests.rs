//! A trigger starts what it names: an agent stored without code on the agent
//! executor, a workflow on the workflow engine; the history records each fire.

use std::collections::{BTreeMap, HashMap};
use std::pin::Pin;
use std::sync::Arc;

use apollia_core::{ObservabilityConfig, RuntimeEvent, StepBudgetConfig};
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, StreamChunk,
    TokenUsage,
};
use apollia_llm::LlmRouter;
use apollia_mcp::session::LoadingMode;
use apollia_tools::{
    AgentOrigin, AgentRepository, StoredAgent, TaskRepository, ToolRegistryHandle,
};
use apollia_triggers::{
    InputTemplate, OnBusyPolicy, TriggerDefinition, TriggerEngineHandle, TriggerPersistence,
    TriggerSourceConfig, TriggerTarget,
};
use serde_json::{json, Value};
use tokio::sync::broadcast;

use super::*;
use crate::api::routes_agents::StubAgentLoader;
use crate::api::routes_llm::tests::{test_app_state_no_llm, MockBackend};
use crate::chat::ChatSessionManagerHandle;
use crate::workflows::{BoxFuture, WorkflowCall, WorkflowDescriptor, WorkflowHost};

const AGENT: &str = "morning-digest";

/// Answers every request with the same text, in one turn.
struct Answers;

#[async_trait::async_trait]
impl CompletionModel for Answers {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            engine_timings: None,
            content: "Digest ready.".into(),
            tool_calls: vec![],
            usage: TokenUsage::default(),
            finish_reason: FinishReason::Stop,
            latency_ms: 1,
            ttft_ms: None,
        })
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
    {
        let chunk = StreamChunk::Text("Digest ready.".into());
        Ok(Box::pin(futures::stream::iter(vec![Ok(chunk)])))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "answers"
    }
    fn model_id(&self) -> &str {
        "answers"
    }
}

/// One workflow, `reports/weekly`, requiring a text input `when`, which it
/// returns.
struct WeeklyHost;

fn weekly() -> WorkflowDescriptor {
    WorkflowDescriptor {
        id: "reports/weekly".into(),
        version: "1.0.0".into(),
        description: "the weekly report".into(),
        steps: vec![],
        input_schema: Some(json!({
            "type": "object",
            "properties": { "when": { "type": "string" } },
            "required": ["when"]
        })),
    }
}

impl WorkflowHost for WeeklyHost {
    fn describe(&self, workflow_id: &str) -> Option<WorkflowDescriptor> {
        (workflow_id == "reports/weekly").then(weekly)
    }

    fn list(&self) -> Vec<WorkflowDescriptor> {
        vec![weekly()]
    }

    fn run(&self, call: WorkflowCall) -> BoxFuture<'static, Result<Value, String>> {
        Box::pin(async move { Ok(call.inputs) })
    }
}

struct Fixture {
    engine: TriggerEngineHandle,
    events: broadcast::Receiver<RuntimeEvent>,
    workflows: WorkflowHandle,
    _dir: tempfile::TempDir,
}

fn trigger(id: &str, target: TriggerTarget) -> TriggerDefinition {
    TriggerDefinition {
        id: id.to_string(),
        target,
        enabled: true,
        on_busy: OnBusyPolicy::Queue { max_depth: 10 },
        source: TriggerSourceConfig::Interval { every: "1h".into() },
    }
}

/// A runtime holding the stored agent without code `morning-digest`, the
/// workflow `reports/weekly`, and a trigger engine over `definitions`.
async fn fixture(definitions: Vec<TriggerDefinition>) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(dir.path()))
        .unwrap()
        .save_definition(&StoredAgent {
            name: AGENT.into(),
            origin: AgentOrigin::Interface,
            package: None,
            version: "1.0.0".into(),
            definition: json!({ "id": AGENT, "name": "Morning digest", "mission": "Digest." }),
            enabled: true,
            installed_at: "2026-10-01T00:00:00Z".into(),
            updated_at: "2026-10-01T00:00:00Z".into(),
        })
        .unwrap();
    let state = test_app_state_no_llm();
    let events = state.event_sender.subscribe();
    let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
    backends.insert("answers".into(), Arc::new(Answers));
    let chat = ChatSessionManagerHandle::spawn(
        &dir.path().join("chat.db"),
        Some(Arc::new(LlmRouter::with_backends(backends, "answers"))),
        ToolRegistryHandle::start(),
        Arc::new(StubAgentLoader),
        None,
        state.event_sender.clone(),
        StepBudgetConfig::default(),
        None,
        state.registry_handle.clone(),
        None,
        None,
        None,
        None,
        None,
        LoadingMode::Eager,
        20,
        None,
        false,
    )
    .unwrap();
    let repository = Arc::new(
        TaskRepository::open(&apollia_core::paths::DataFile::Hitl.path(dir.path()))
            .await
            .unwrap(),
    );
    let workflows = WorkflowHandle::spawn(
        repository.clone(),
        state.event_sender.clone(),
        StepBudgetConfig::default(),
        None,
    );
    workflows.set_host(Arc::new(WeeklyHost)).await.unwrap();
    let targets: TriggerTargets<MockBackend> = TriggerTargets {
        router: state.router_handle.clone(),
        agents: Some(AgentTaskLauncher {
            router: state.router_handle.clone(),
            chat,
            repository: Some(repository),
            event_bus: state.event_sender.clone(),
            observability: ObservabilityConfig::default(),
        }),
        workflows: Some(workflows.clone()),
        data_dir: dir.path().to_path_buf(),
    };
    let engine = TriggerEngineHandle::spawn(
        definitions,
        targets,
        state.event_sender.clone(),
        Some(TriggerPersistence::open(&dir.path().join("triggers.db")).unwrap()),
        ObservabilityConfig::default(),
    )
    .await;
    Fixture {
        engine,
        events,
        workflows,
        _dir: dir,
    }
}

/// Waits on the bus for the end of the task `task_id`, and returns its
/// success and output.
async fn task_end(
    events: &mut broadcast::Receiver<RuntimeEvent>,
    task_id: &str,
) -> (bool, Option<String>) {
    loop {
        match events.recv().await {
            Ok(RuntimeEvent::TaskCompleted {
                task_id: t,
                success,
                output,
                ..
            }) if t.as_str() == task_id => return (success, output),
            Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
            Err(broadcast::error::RecvError::Closed) => return (false, None),
        }
    }
}

#[tokio::test]
async fn test_a_trigger_on_an_agent_without_code_runs_it_outside_any_conversation() {
    // GIVEN a trigger on the stored agent without code
    let mut fx = fixture(vec![trigger(
        "digest",
        TriggerTarget::agent(AGENT, "Digest of {{fired_at}}"),
    )])
    .await;

    // WHEN it fires
    let task_id = fx.engine.fire_now("digest").await.unwrap();

    // THEN the agent executor runs the task to its answer
    let (success, output) = task_end(&mut fx.events, task_id.as_ref()).await;
    assert!(success);
    assert_eq!(output.as_deref(), Some("Digest ready."));
    // AND the history records the fire with that task
    let history = fx.engine.query_history("digest", 10).await;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].status, "fired");
    assert_eq!(history[0].task_id.as_deref(), Some(task_id.as_ref()));
}

#[tokio::test]
async fn test_a_trigger_on_a_workflow_launches_a_run_on_its_rendered_inputs() {
    // GIVEN a trigger on the workflow, its input templated on the fire time
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "when".to_string(),
        InputTemplate("week of {{fired_at}}".into()),
    );
    let fx = fixture(vec![trigger(
        "weekly",
        TriggerTarget::Workflow {
            workflow: "reports/weekly".into(),
            inputs,
        },
    )])
    .await;

    // WHEN it fires
    let run_id = fx.engine.fire_now("weekly").await.unwrap();

    // THEN the workflow engine holds a run of that workflow on the rendered input
    let detail = fx.workflows.detail(run_id.as_ref()).await.unwrap().unwrap();
    assert_eq!(detail.run.workflow_id, "reports/weekly");
    assert!(detail.run.inputs["when"]
        .as_str()
        .is_some_and(|w| w.starts_with("week of 20")));
    // AND the history records the fire with that run
    let history = fx.engine.query_history("weekly", 10).await;
    assert_eq!(history[0].status, "fired");
    assert_eq!(history[0].task_id.as_deref(), Some(run_id.as_ref()));
}

#[tokio::test]
async fn test_a_workflow_trigger_missing_a_required_input_is_recorded_as_an_error() {
    // GIVEN a trigger on the workflow that gives none of its inputs
    let fx = fixture(vec![trigger(
        "weekly",
        TriggerTarget::Workflow {
            workflow: "reports/weekly".into(),
            inputs: BTreeMap::new(),
        },
    )])
    .await;

    // WHEN it fires
    let result = fx.engine.fire_now("weekly").await;

    // THEN no run starts, and the history names the missing input
    assert!(result.is_err());
    let history = fx.engine.query_history("weekly", 10).await;
    assert_eq!(history[0].status, "error");
    assert!(
        history[0]
            .reason
            .as_deref()
            .is_some_and(|r| r.contains("when")),
        "{:?}",
        history[0].reason
    );
}
