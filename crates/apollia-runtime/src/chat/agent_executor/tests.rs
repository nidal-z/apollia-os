//! The executor reads the resolved agent, and only the Apollia agent receives
//! the principle 6 injections.

use std::collections::HashSet;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};

use apollia_core::model_preset::{AgentModel, ModelSource};
use apollia_core::RuntimeEvent;
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, StreamChunk,
    TokenUsage,
};
use apollia_memory::user_memory::WrittenBy;
use apollia_tools::{resolve_agent, AgentOrigin, AgentOverride, OverrideField, StoredAgent};
use serde_json::json;

use super::*;

/// A backend that counts the turns streamed through it.
struct Counting {
    name: &'static str,
    streams: Arc<AtomicU32>,
}

#[async_trait::async_trait]
impl CompletionModel for Counting {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            engine_timings: None,
            content: self.name.to_string(),
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
        self.streams.fetch_add(1, Ordering::SeqCst);
        let name = self.name.to_string();
        Ok(Box::pin(futures::stream::once(async move {
            Ok(StreamChunk::Text(name))
        })))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        self.name
    }
    fn model_id(&self) -> &str {
        self.name
    }
}

struct NoTool;

#[async_trait::async_trait]
impl ToolInvoker for NoTool {
    async fn invoke(&self, _tool: &str, _args: &serde_json::Value) -> Result<String, String> {
        Ok(String::new())
    }
}

/// Counts the project context blocks it is asked for.
struct CountingProject {
    calls: Arc<AtomicU32>,
}

#[async_trait::async_trait]
impl ProjectContextProvider for CountingProject {
    async fn build_context(&self, _project_id: &str) -> Option<String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Some("## Project context\nThe ledger project.".into())
    }
}

/// A router holding `default` and `assigned`, with the stream counters of
/// each, in that order.
fn two_backend_router() -> (Arc<LlmRouter>, Arc<AtomicU32>, Arc<AtomicU32>) {
    let default_streams = Arc::new(AtomicU32::new(0));
    let assigned_streams = Arc::new(AtomicU32::new(0));
    let mut backends: std::collections::HashMap<String, Arc<dyn CompletionModel>> =
        std::collections::HashMap::new();
    backends.insert(
        "default".into(),
        Arc::new(Counting {
            name: "default",
            streams: Arc::clone(&default_streams),
        }),
    );
    backends.insert(
        "assigned".into(),
        Arc::new(Counting {
            name: "assigned",
            streams: Arc::clone(&assigned_streams),
        }),
    );
    (
        Arc::new(LlmRouter::with_backends(backends, "default")),
        default_streams,
        assigned_streams,
    )
}

/// An installed agent declaring the tier `declared`, with a user override of
/// its mission, and the model `model_id` assigned to it.
fn resolved_agent(model_id: &str, declared: AutonomyLevel) -> ResolvedAgent {
    let stored = StoredAgent {
        name: "ledger-clerk".into(),
        origin: AgentOrigin::Package,
        package: Some("ledger".into()),
        version: "1.0.0".into(),
        definition: json!({
            "id": "ledger-clerk",
            "name": "Ledger clerk",
            "mission": "Reconcile the ledger.",
            "defaults": {
                "access": ["file_read", "file_list"],
                "autonomy": declared.as_str()
            }
        }),
        enabled: true,
        installed_at: "2026-10-01T00:00:00Z".into(),
        updated_at: "2026-10-01T00:00:00Z".into(),
    };
    let overrides = [AgentOverride {
        field: OverrideField::Mission,
        value: json!("Reconcile the ledger, then report the gaps."),
    }];
    let model = AgentModel {
        agent: "ledger-clerk".into(),
        model_source: ModelSource::Server,
        model_id: model_id.into(),
        preset_id: None,
    };
    resolve_agent(&stored, &overrides, Some(model), AutonomyLevel::Assisted)
}

fn deps(router: Arc<LlmRouter>, bus: EventBusSender) -> AgentExecutorDeps {
    AgentExecutorDeps {
        llm_router: router,
        tool_registry: ToolRegistryHandle::start(),
        tool_invoker: Arc::new(NoTool),
        event_bus: bus,
        a2a_invoker: None,
        todo: None,
        plan: None,
        plan_cache: None,
    }
}

/// The three injections, each one observable: a persona in memory, a past
/// sessions block, and a project provider that counts its calls.
fn full_apollia_context(project_calls: &Arc<AtomicU32>) -> ApolliaContext {
    let dir = std::env::temp_dir().join(format!("apollia_exec_um_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let repo = UserMemoryRepository::new(&dir.join("user_memory.db")).expect("open user memory");
    repo.set("langue", "francais", WrittenBy::User)
        .expect("set entry");
    ApolliaContext {
        user_memory: Some(Arc::new(std::sync::Mutex::new(repo))),
        past_sessions: Some("## Context from earlier, unrelated sessions\n- [x] budget".into()),
        project: Some(ProjectContextSource {
            project_id: "p1".into(),
            provider: Arc::new(CountingProject {
                calls: Arc::clone(project_calls),
            }),
        }),
    }
}

fn ceiling() -> StepBudgetConfig {
    StepBudgetConfig {
        max_steps: 7,
        max_tool_calls: 9,
        wall_clock_secs: 60,
    }
}

// GIVEN an installed agent whose mission is overridden, with granted tools
//       and an assigned model
// WHEN the run spec is read from it, the agent declaring the bounded
//      autonomous tier
// THEN the spec carries the overridden mission, the granted tools, the model
//      and the tier, under the agent's identifier
#[test]
fn test_spec_reads_the_resolved_agent() {
    // GIVEN
    let agent = resolved_agent("assigned", AutonomyLevel::BoundedAutonomous);

    // WHEN
    let spec = AgentRunSpec::from_resolved(&agent);

    // THEN
    assert_eq!(spec.agent_id, "ledger-clerk");
    assert_eq!(spec.mission, "Reconcile the ledger, then report the gaps.");
    assert_eq!(spec.granted_tools, vec!["file_read", "file_list"]);
    assert_eq!(spec.model.as_deref(), Some("assigned"));
    assert_eq!(spec.autonomy, AutonomyLevel::BoundedAutonomous);
    assert!(!spec.is_apollia());
}

// GIVEN a resolved agent declaring the long autonomous tier, under a runtime
//       ceiling below the tier's budget
// WHEN its executor is built
// THEN the tier settings are those of the tier, the budget is capped by the
//      ceiling, and the run verifies with a critic
#[tokio::test]
async fn test_executor_reads_the_autonomy_of_the_agent() {
    // GIVEN
    let (router, _, _) = two_backend_router();
    let (bus, _) = tokio::sync::broadcast::channel(16);
    let spec =
        AgentRunSpec::from_resolved(&resolved_agent("assigned", AutonomyLevel::LongAutonomous));

    // WHEN
    let executor = AgentExecutor::new(
        spec,
        deps(router, bus),
        ApolliaContext::default(),
        SessionWiring::default(),
        &ceiling(),
        CancellationToken::new(),
    );

    // THEN
    let expected = AutonomyConfig::default().level_config(AutonomyLevel::LongAutonomous);
    assert_eq!(
        executor.level_config().inject_memory,
        expected.inject_memory
    );
    assert_eq!(
        executor.level_config().run_verification,
        expected.run_verification
    );
    assert_eq!(
        executor.level_config().budget.max_steps,
        expected.budget.max_steps
    );
    assert!(expected.budget.max_steps > ceiling().max_steps);
    assert_eq!(executor.budget().max_steps, ceiling().max_steps);
    assert!(executor.verifies());
}

// GIVEN the same agent declaring the assisted tier
// WHEN its executor is built
// THEN the run carries no verification and no critic
#[tokio::test]
async fn test_executor_at_the_assisted_tier_does_not_verify() {
    // GIVEN
    let (router, _, _) = two_backend_router();
    let (bus, _) = tokio::sync::broadcast::channel(16);
    let spec = AgentRunSpec::from_resolved(&resolved_agent("assigned", AutonomyLevel::Assisted));

    // WHEN
    let executor = AgentExecutor::new(
        spec,
        deps(router, bus),
        ApolliaContext::default(),
        SessionWiring::default(),
        &ceiling(),
        CancellationToken::new(),
    );

    // THEN
    assert!(!executor.verifies());
}

// GIVEN a router whose default is `default`, also holding `assigned`, and a
//       resolved agent whose model is `assigned`
// WHEN the executor runs one exchange
// THEN the turn streams through `assigned`, never through the default
#[tokio::test]
async fn test_executor_routes_to_the_model_of_the_agent() {
    // GIVEN
    let (router, default_streams, assigned_streams) = two_backend_router();
    let (bus, _) = tokio::sync::broadcast::channel(16);
    let spec = AgentRunSpec::from_resolved(&resolved_agent("assigned", AutonomyLevel::Assisted));
    let executor = AgentExecutor::new(
        spec,
        deps(router, bus),
        ApolliaContext::default(),
        SessionWiring::default(),
        &ceiling(),
        CancellationToken::new(),
    );

    // WHEN
    let response = executor
        .run(RunInput {
            session_id: "s1",
            message_id: "m1",
            run_id: &RunId::new(),
            user_message: "Reconcile March.",
            history: &[],
            authorized_tools: &HashSet::new(),
            pending_approvals: &PendingChatApprovals::default(),
            summary: None,
            context_window_size: 20,
        })
        .await
        .expect("the exchange runs");

    // THEN
    assert_eq!(response.content, "assigned");
    assert_eq!(assigned_streams.load(Ordering::SeqCst), 1);
    assert_eq!(default_streams.load(Ordering::SeqCst), 0);
}

// GIVEN a resolved agent whose assigned model the router does not hold
// WHEN the executor runs one exchange
// THEN the turn falls back to the default, and the fallback names the agent
#[tokio::test]
async fn test_executor_fallback_names_the_agent() {
    // GIVEN
    let (router, default_streams, _) = two_backend_router();
    let (bus, mut events) = tokio::sync::broadcast::channel(16);
    let spec = AgentRunSpec::from_resolved(&resolved_agent("absent", AutonomyLevel::Assisted));
    let executor = AgentExecutor::new(
        spec,
        deps(router, bus),
        ApolliaContext::default(),
        SessionWiring::default(),
        &ceiling(),
        CancellationToken::new(),
    );

    // WHEN
    executor
        .run(RunInput {
            session_id: "s1",
            message_id: "m1",
            run_id: &RunId::new(),
            user_message: "Reconcile March.",
            history: &[],
            authorized_tools: &HashSet::new(),
            pending_approvals: &PendingChatApprovals::default(),
            summary: None,
            context_window_size: 20,
        })
        .await
        .expect("the exchange runs");

    // THEN
    assert_eq!(default_streams.load(Ordering::SeqCst), 1);
    let mut fallback_agent = None;
    while let Ok(event) = events.try_recv() {
        if let RuntimeEvent::LlmAgentModelFallback { agent, .. } = event {
            fallback_agent = Some(agent);
        }
    }
    assert_eq!(fallback_agent.as_deref(), Some("ledger-clerk"));
}

// GIVEN a resolved agent that is not Apollia, handed the three injections
//       (a persona in memory, a past sessions block, a project provider)
// WHEN its executor is built and its system prompt composed
// THEN the prompt is the mission alone, the project provider is never asked,
//      and even the memory tier composes no persona block
#[tokio::test]
async fn test_other_agent_receives_none_of_the_apollia_injections() {
    // GIVEN
    let (router, _, _) = two_backend_router();
    let (bus, _) = tokio::sync::broadcast::channel(16);
    let project_calls = Arc::new(AtomicU32::new(0));
    let spec =
        AgentRunSpec::from_resolved(&resolved_agent("assigned", AutonomyLevel::LongAutonomous));

    // WHEN
    let executor = AgentExecutor::new(
        spec,
        deps(router, bus),
        full_apollia_context(&project_calls),
        SessionWiring::default(),
        &ceiling(),
        CancellationToken::new(),
    );
    let prompt = executor.system_prompt().await;
    let composed =
        executor
            .agent()
            .build_system_prompt(Some(&prompt), AutonomyLevel::LongAutonomous, true);

    // THEN
    assert_eq!(prompt, "Reconcile the ledger, then report the gaps.");
    assert_eq!(project_calls.load(Ordering::SeqCst), 0);
    assert!(!composed.contains("User Persona"), "{composed}");
    assert!(!composed.contains("earlier, unrelated sessions"));
    assert!(!composed.contains("Project context"));
}

// GIVEN the Apollia agent, handed the same three injections
// WHEN its executor is built and its system prompt composed
// THEN the prompt carries the mission, the past sessions block and the
//      project block in that order, and the memory tier adds the persona
#[tokio::test]
async fn test_apollia_receives_the_three_injections() {
    // GIVEN
    let (router, _, _) = two_backend_router();
    let (bus, _) = tokio::sync::broadcast::channel(16);
    let project_calls = Arc::new(AtomicU32::new(0));
    let spec = AgentRunSpec {
        agent_id: APOLLIA_CHAT_AGENT_ID.to_string(),
        mission: "You are Apollia.".into(),
        granted_tools: vec![],
        model: None,
        autonomy: AutonomyLevel::LongAutonomous,
        planning: PlanningMode::Never,
        version: String::new(),
    };

    // WHEN
    let executor = AgentExecutor::new(
        spec,
        deps(router, bus),
        full_apollia_context(&project_calls),
        SessionWiring::default(),
        &ceiling(),
        CancellationToken::new(),
    );
    let prompt = executor.system_prompt().await;
    let composed =
        executor
            .agent()
            .build_system_prompt(Some(&prompt), AutonomyLevel::LongAutonomous, true);

    // THEN
    assert_eq!(
        prompt,
        "You are Apollia.\n\n## Context from earlier, unrelated sessions\n- [x] budget\n\n\
         ## Project context\nThe ledger project."
    );
    assert_eq!(project_calls.load(Ordering::SeqCst), 1);
    assert!(composed.contains("User Persona"), "{composed}");
}

// GIVEN an agent declaring the supervised tier, overridden by the user to the
//       long autonomous one, and an agent that declares no tier
// WHEN their run specs are read under the global default
// THEN the first runs at the user's tier, the second at the global default
#[test]
fn test_spec_reads_the_overridden_tier_then_the_global_default() {
    // GIVEN
    let mut overridden = StoredAgent {
        name: "ledger-clerk".into(),
        origin: AgentOrigin::Package,
        package: Some("ledger".into()),
        version: "1.0.0".into(),
        definition: json!({
            "id": "ledger-clerk",
            "mission": "Reconcile the ledger.",
            "defaults": { "autonomy": "supervised" }
        }),
        enabled: true,
        installed_at: "2026-10-01T00:00:00Z".into(),
        updated_at: "2026-10-01T00:00:00Z".into(),
    };
    let user_tier = [AgentOverride {
        field: OverrideField::Autonomy,
        value: json!("long_autonomous"),
    }];
    let global = crate::agents::global_default_autonomy();

    // WHEN
    let from_override =
        AgentRunSpec::from_resolved(&resolve_agent(&overridden, &user_tier, None, global));
    overridden.definition = json!({ "id": "ledger-clerk", "defaults": {} });
    let from_default = AgentRunSpec::from_resolved(&resolve_agent(&overridden, &[], None, global));

    // THEN
    assert_eq!(from_override.autonomy, AutonomyLevel::LongAutonomous);
    assert_eq!(from_default.autonomy, global);
}

// GIVEN a resolved agent
// WHEN one executor is built with the wiring of a run outside any
//      conversation, and one with an approval delay as the chat sets it
// THEN the first never refuses a pending approval on its own, the second
//      refuses it after that delay
#[tokio::test]
async fn test_run_outside_a_conversation_waits_on_its_approvals() {
    // GIVEN
    let (router, _, _) = two_backend_router();
    let (bus, _) = tokio::sync::broadcast::channel(16);
    let spec = AgentRunSpec::from_resolved(&resolved_agent("assigned", AutonomyLevel::Assisted));

    // WHEN
    let off_chat = AgentExecutor::new(
        spec.clone(),
        deps(Arc::clone(&router), bus.clone()),
        ApolliaContext::default(),
        SessionWiring::default(),
        &ceiling(),
        CancellationToken::new(),
    );
    let in_chat = AgentExecutor::new(
        spec,
        deps(router, bus),
        ApolliaContext::default(),
        SessionWiring {
            approval_timeout: Some(crate::chat::builtin_agent::CHAT_APPROVAL_TIMEOUT),
            ..SessionWiring::default()
        },
        &ceiling(),
        CancellationToken::new(),
    );

    // THEN
    assert_eq!(off_chat.agent().approval_timeout(), None);
    assert_eq!(
        in_chat.agent().approval_timeout(),
        Some(crate::chat::builtin_agent::CHAT_APPROVAL_TIMEOUT)
    );
}
