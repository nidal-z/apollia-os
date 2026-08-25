//! Integration tests for ORIA orchestrated mode.
//!
//! Covers 7 scenarios:
//! - 4-step plan executed sequentially, `PlanCompleted` event emitted
//! - Topological order respected (diamond dependency)
//! - `StepBudget::with_max(2)` stops at step 3 of 5
//! - Step failure triggers replanification, execution continues
//! - `MAX_REPLAN_EXCEEDED` when `max_replans = 0` and step fails
//! - `Reasoner` retry ×3 on invalid JSON → `PlanParseError`
//! - Agent without `on_plan_complete` → automatic output concatenation

use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc, Mutex,
};

use apollia_core::{
    events::RuntimeEvent, AIPPart, AIPTask, AgentManifest, StepBudgetConfig, TaskStatus,
};
use apollia_llm::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, LlmRouter,
    StreamChunk, TokenUsage,
};
use apollia_oria::{
    actor::{ActorLoop, StepDeps, ToolProxyTrait},
    budget::StepBudget,
    engine::{AIPAgent, ORIAEngine},
    observer::ContextBundle,
    plan::{ExecutionPlan, PlanStep},
    plan_repository::PlanRepository,
    reasoner::{Reasoner, ReasonerError},
    resilience::ResilienceLayer,
};
use async_trait::async_trait;

// ── Mock: CompletionModel ────────────────────────────────────────────────────

/// Mock `CompletionModel` that drains a pre-configured queue of responses.
///
/// Returns a fallback empty JSON object once the queue is exhausted.
/// Tracks the total call count via an atomic counter for assertion purposes.
struct MockCompletionModel {
    queue: Mutex<VecDeque<String>>,
    fallback: String,
    call_count: AtomicU32,
}

impl MockCompletionModel {
    /// Creates a mock that returns `responses` in order, then an empty JSON fallback.
    fn with_responses(responses: Vec<&str>) -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(responses.iter().map(|s| s.to_string()).collect()),
            fallback: r#"{"steps":[]}"#.to_string(),
            call_count: AtomicU32::new(0),
        })
    }

    /// Returns the total number of `complete()` calls made so far.
    fn call_count(&self) -> u32 {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl CompletionModel for MockCompletionModel {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let content = {
            let mut q = self
                .queue
                .lock()
                .expect("mock queue lock must not be poisoned");
            q.pop_front().unwrap_or_else(|| self.fallback.clone())
        };
        Ok(CompletionResponse {
            engine_timings: None,
            content,
            tool_calls: vec![],
            usage: TokenUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                cost_usd: None,
                cache_read_input_tokens: 0,
                cache_write_input_tokens: 0,
            },
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
        Err(LlmError::InferenceError(
            "MockCompletionModel does not support streaming".into(),
        ))
    }

    fn is_available(&self) -> bool {
        true
    }

    fn backend_name(&self) -> &str {
        "mock"
    }

    fn model_id(&self) -> &str {
        "mock-model"
    }
}

// ── Mock: RecordingToolProxy ─────────────────────────────────────────────────

/// `ToolProxy` that records the step description of every invocation, in order.
///
/// `ActorLoop::execute_step` passes `{"input": "<description>"}` as the JSON argument.
/// This proxy extracts `input["input"]` as the step label. Returns `response` for
/// every call.
struct RecordingToolProxy {
    calls: Mutex<Vec<String>>,
    response: String,
}

impl RecordingToolProxy {
    /// Creates a recording proxy that returns `response` for every invocation.
    fn new(response: &str) -> Self {
        Self {
            calls: Mutex::new(vec![]),
            response: response.to_string(),
        }
    }

    /// Returns the list of step descriptions invoked, in call order.
    fn recorded_calls(&self) -> Vec<String> {
        self.calls
            .lock()
            .expect("recording proxy lock must not be poisoned")
            .clone()
    }
}

#[async_trait]
impl ToolProxyTrait for RecordingToolProxy {
    async fn invoke(&self, tool_name: &str, input: &serde_json::Value) -> Result<String, String> {
        // ActorLoop passes {"input": "<step description>"} - extract the description.
        let label = input["input"].as_str().unwrap_or(tool_name).to_string();
        self.calls
            .lock()
            .expect("recording proxy lock must not be poisoned")
            .push(label);
        Ok(self.response.clone())
    }
}

// ── Mock: FailingToolProxy ───────────────────────────────────────────────────

/// `ToolProxy` that always returns a retryable error (simulates a transient timeout).
struct FailingToolProxy;

#[async_trait]
impl ToolProxyTrait for FailingToolProxy {
    async fn invoke(&self, _tool_name: &str, _input: &serde_json::Value) -> Result<String, String> {
        Err("tool timeout - simulated transient failure".to_string())
    }
}

// ── Mock: SelectiveFailProxy ─────────────────────────────────────────────────

/// `ToolProxy` that fails when the step description matches `fail_on_description` exactly.
///
/// All other steps succeed. Uses exact equality to avoid false positives when
/// step IDs share prefixes (e.g. "s2" vs "s2b").
struct SelectiveFailProxy {
    fail_on_description: String,
}

#[async_trait]
impl ToolProxyTrait for SelectiveFailProxy {
    async fn invoke(&self, tool_name: &str, input: &serde_json::Value) -> Result<String, String> {
        let desc = input["input"].as_str().unwrap_or(tool_name);
        if desc == self.fail_on_description {
            Err(format!(
                "simulated failure on '{}'",
                self.fail_on_description
            ))
        } else {
            Ok(format!("ok: {desc}"))
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Builds an `ExecutionPlan` from `(step_id, depends_on)` pairs.
///
/// All steps use `tool_hint = Some("mock_tool")` and `description = "Step <id>"`.
fn make_plan(steps: Vec<(&str, &[&str])>) -> ExecutionPlan {
    ExecutionPlan {
        plan_id: uuid::Uuid::new_v4().to_string(),
        task_id: "task-test".to_string(),
        steps: steps
            .into_iter()
            .map(|(id, deps)| PlanStep {
                step_id: id.to_string(),
                description: format!("Step {id}"),
                tool_hint: Some("mock_tool".to_string()),
                depends_on: deps.iter().map(|s| s.to_string()).collect(),
                model_hint: None,
                title: String::new(),
                status: Default::default(),
                rationale: None,
                provenance: Default::default(),
                args: None,
            })
            .collect(),
    }
}

/// Opens an in-memory `PlanRepository`.
fn make_db() -> PlanRepository {
    PlanRepository::new(":memory:").expect("in-memory SQLite must always succeed")
}

/// Creates an `EventBusSender` with a 64-slot buffer.
fn make_bus() -> tokio::sync::broadcast::Sender<RuntimeEvent> {
    tokio::sync::broadcast::channel(64).0
}

/// Builds a minimal `AgentManifest` for integration tests.
fn make_manifest() -> AgentManifest {
    serde_json::from_str(
        r#"{"name":"test","version":"0.1.0","description":"test","tools_required":[]}"#,
    )
    .expect("minimal manifest must deserialize")
}

// ── Plan séquentiel ───────────────────────────────────────────────────────────

/// Plan valide de 4 steps exécuté séquentiellement - `PlanCompleted` émis.
///
/// ÉTANT DONNÉ un plan linéaire (s1→s2→s3→s4) et un `RecordingToolProxy`
/// QUAND `ActorLoop::execute()` est appelé
/// ALORS `Completed` est retourné, 4 appels sont enregistrés,
///   ET `RuntimeEvent::PlanCompleted { step_count: 4 }` est émis.
#[tokio::test]
async fn test_ac1_plan_4_steps_execute_sequentiellement() {
    // GIVEN
    let plan = make_plan(vec![
        ("s1", &[]),
        ("s2", &["s1"]),
        ("s3", &["s2"]),
        ("s4", &["s3"]),
    ]);
    let db = make_db();
    db.insert_plan(&plan, "test-agent")
        .expect("insert_plan must succeed");
    db.insert_steps(&plan.plan_id, &plan.steps)
        .expect("insert_steps must succeed");

    let bus = make_bus();
    let mut rx = bus.subscribe();
    let proxy = RecordingToolProxy::new("ok");
    let llm = LlmRouter::empty();
    let budget = StepBudget::unlimited();
    let resilience = ResilienceLayer::default();
    let reasoner = Reasoner::new(MockCompletionModel::with_responses(vec![]), 10);
    let mut actor = ActorLoop::new(plan, 2, db, bus.clone(), make_manifest());

    // WHEN
    let result = actor
        .execute(
            StepDeps {
                tool_proxy: &proxy,
                llm_router: &llm,
                budget: &budget,
                reasoner: &reasoner,
            },
            &resilience,
        )
        .await;

    // THEN - Completed
    assert_eq!(
        result.status,
        TaskStatus::Completed,
        "expected Completed, got: {:?}",
        result.error
    );

    // THEN - 4 proxy calls recorded
    let calls = proxy.recorded_calls();
    assert_eq!(calls.len(), 4, "expected 4 proxy calls, got: {calls:?}");

    // THEN - PlanCompleted { step_count: 4 } emitted on bus
    let mut found_completed = false;
    while let Ok(event) = rx.try_recv() {
        if let RuntimeEvent::PlanCompleted { step_count, .. } = event {
            assert_eq!(step_count, 4, "expected step_count=4, got={step_count}");
            found_completed = true;
        }
    }
    assert!(
        found_completed,
        "RuntimeEvent::PlanCompleted must have been emitted"
    );
}

// ── Ordre topologique ─────────────────────────────────────────────────────────

/// Dépendances topologiques respectées - plan en diamant.
///
/// ÉTANT DONNÉ s1 et s3 sans deps, s2 dépend de s1, s4 dépend de s2 et s3
/// QUAND `ActorLoop::execute()` est appelé
/// ALORS l'ordre observé satisfait : s1 < s2, s3 < s4, s2 < s4.
#[tokio::test]
async fn test_ac2_depends_on_respectes() {
    // GIVEN - diamond dependency: s1 and s3 independent, s2 needs s1, s4 needs s2+s3
    let plan = make_plan(vec![
        ("s1", &[]),
        ("s3", &[]),
        ("s2", &["s1"]),
        ("s4", &["s2", "s3"]),
    ]);
    let db = make_db();
    db.insert_plan(&plan, "test")
        .expect("insert_plan must succeed");
    db.insert_steps(&plan.plan_id, &plan.steps)
        .expect("insert_steps must succeed");

    let bus = make_bus();
    let proxy = RecordingToolProxy::new("ok");
    let llm = LlmRouter::empty();
    let budget = StepBudget::unlimited();
    let resilience = ResilienceLayer::default();
    let reasoner = Reasoner::new(MockCompletionModel::with_responses(vec![]), 10);
    let mut actor = ActorLoop::new(plan, 2, db, bus, make_manifest());

    // WHEN
    let result = actor
        .execute(
            StepDeps {
                tool_proxy: &proxy,
                llm_router: &llm,
                budget: &budget,
                reasoner: &reasoner,
            },
            &resilience,
        )
        .await;
    assert_eq!(
        result.status,
        TaskStatus::Completed,
        "expected Completed, got: {:?}",
        result.error
    );

    // THEN - verify ordering via recorded step descriptions
    let calls = proxy.recorded_calls();
    assert_eq!(calls.len(), 4, "expected 4 proxy calls");

    let pos = |id: &str| {
        calls
            .iter()
            .position(|c| c.contains(id))
            .unwrap_or_else(|| panic!("step '{id}' not found in calls: {calls:?}"))
    };
    let (s1, s2, s3, s4) = (pos("s1"), pos("s2"), pos("s3"), pos("s4"));

    assert!(s1 < s2, "s1 must precede s2 (s1={s1}, s2={s2})");
    assert!(s2 < s4, "s2 must precede s4 (s2={s2}, s4={s4})");
    assert!(s3 < s4, "s3 must precede s4 (s3={s3}, s4={s4})");
}

// ── Budget épuisé ─────────────────────────────────────────────────────────────

/// Budget épuisé - plan de 5 steps indépendants, max 2 steps autorisés.
///
/// ÉTANT DONNÉ 5 steps sans dépendances et `StepBudget::with_max(2)`
/// QUAND `ActorLoop::execute()` est appelé
/// ALORS `Failed("STEP_BUDGET_EXCEEDED")` est retourné après 2 exécutions.
#[tokio::test]
async fn test_ac3_budget_epuise_step_3_sur_5() {
    // GIVEN
    let plan = make_plan(vec![
        ("s1", &[]),
        ("s2", &[]),
        ("s3", &[]),
        ("s4", &[]),
        ("s5", &[]),
    ]);
    let db = make_db();
    db.insert_plan(&plan, "test")
        .expect("insert_plan must succeed");
    db.insert_steps(&plan.plan_id, &plan.steps)
        .expect("insert_steps must succeed");

    let bus = make_bus();
    let proxy = RecordingToolProxy::new("ok");
    let llm = LlmRouter::empty();
    let budget = StepBudget::with_max(2); // only 2 steps allowed
    let resilience = ResilienceLayer::default();
    let reasoner = Reasoner::new(MockCompletionModel::with_responses(vec![]), 10);
    let mut actor = ActorLoop::new(plan, 2, db, bus, make_manifest());

    // WHEN
    let result = actor
        .execute(
            StepDeps {
                tool_proxy: &proxy,
                llm_router: &llm,
                budget: &budget,
                reasoner: &reasoner,
            },
            &resilience,
        )
        .await;

    // THEN - Failed with STEP_BUDGET_EXCEEDED
    assert_eq!(result.status, TaskStatus::Failed, "expected Failed");
    let code = result.error.as_ref().map(|e| e.code.as_str()).unwrap_or("");
    assert_eq!(
        code, "STEP_BUDGET_EXCEEDED",
        "expected STEP_BUDGET_EXCEEDED error code"
    );

    // THEN - exactly 2 proxy calls (step 3 was blocked by budget)
    let calls = proxy.recorded_calls();
    assert_eq!(
        calls.len(),
        2,
        "expected 2 proxy calls before budget exhaustion, got: {calls:?}"
    );
}

// ── Replanification ───────────────────────────────────────────────────────────

/// Replanification déclenchée sur step retryable.
///
/// ÉTANT DONNÉ un plan (s1 ok, s2 retryable fail) avec `max_replans = 1`
///   ET un `Reasoner` qui retourne un plan de remplacement (s2b)
/// QUAND `ActorLoop::execute()` est appelé
/// ALORS `PlanReplanning { attempt: 1 }` est émis
///   ET l'exécution continue avec s2b
///   ET `Completed` est retourné si s2b réussit.
#[tokio::test]
async fn test_ac4_replanification_step_echec() {
    // GIVEN - plan s1 (succeeds), s2 (will fail with retryable error)
    let plan = make_plan(vec![("s1", &[]), ("s2", &["s1"])]);
    let db = make_db();
    db.insert_plan(&plan, "test")
        .expect("insert_plan must succeed");
    db.insert_steps(&plan.plan_id, &plan.steps)
        .expect("insert_steps must succeed");

    let bus = make_bus();
    let mut rx = bus.subscribe();

    // Proxy fails only on the exact description "Step s2" - not on "Step s2b".
    let proxy = SelectiveFailProxy {
        fail_on_description: "Step s2".to_string(),
    };
    let llm = LlmRouter::empty();
    let budget = StepBudget::unlimited();
    let resilience = ResilienceLayer::default();

    // Reasoner::replan() will be called once - return a single replacement step.
    let replacement_plan = r#"{"steps":[{"step_id":"s2b","description":"Step s2b","tool_hint":"mock_tool","depends_on":[]}]}"#;
    let mock_model = MockCompletionModel::with_responses(vec![replacement_plan]);
    let reasoner = Reasoner::new(mock_model, 10);

    // max_replans=1: one replan allowed before MAX_REPLAN_EXCEEDED
    let mut actor = ActorLoop::new(plan, 1, db, bus.clone(), make_manifest());

    // WHEN
    let result = actor
        .execute(
            StepDeps {
                tool_proxy: &proxy,
                llm_router: &llm,
                budget: &budget,
                reasoner: &reasoner,
            },
            &resilience,
        )
        .await;

    // THEN - Completed after s1 ok, s2 fail → replan → s2b ok
    assert_eq!(
        result.status,
        TaskStatus::Completed,
        "expected Completed after replan, got: {:?}",
        result.error
    );

    // THEN - PlanReplanning event emitted with attempt=1
    let mut found_replan = false;
    while let Ok(event) = rx.try_recv() {
        if let RuntimeEvent::PlanReplanning { attempt, .. } = event {
            assert_eq!(attempt, 1, "first replan must be attempt=1, got={attempt}");
            found_replan = true;
        }
    }
    assert!(
        found_replan,
        "RuntimeEvent::PlanReplanning must have been emitted"
    );
}

// ── MAX_REPLAN_EXCEEDED ───────────────────────────────────────────────────────

/// `MAX_REPLAN_EXCEEDED` - zéro replanification autorisée, step retryable échoue.
///
/// ÉTANT DONNÉ un plan avec un step qui produit toujours une erreur retryable
///   ET `max_replans = 0` (aucune replanification autorisée)
/// QUAND `ActorLoop::execute()` est appelé
/// ALORS `Failed("MAX_REPLAN_EXCEEDED")` est retourné immédiatement.
#[tokio::test]
async fn test_ac5_max_replan_exceeded() {
    // GIVEN - single step that always fails (FailingToolProxy → ToolCallFailed → retryable)
    let plan = make_plan(vec![("s1", &[])]);
    let db = make_db();
    db.insert_plan(&plan, "test")
        .expect("insert_plan must succeed");
    db.insert_steps(&plan.plan_id, &plan.steps)
        .expect("insert_steps must succeed");

    let bus = make_bus();
    let proxy = FailingToolProxy;
    let llm = LlmRouter::empty();
    let budget = StepBudget::unlimited();
    let resilience = ResilienceLayer::default();
    let reasoner = Reasoner::new(MockCompletionModel::with_responses(vec![]), 10);

    // max_replans=0 → any retryable failure immediately yields MAX_REPLAN_EXCEEDED
    let mut actor = ActorLoop::new(plan, 0, db, bus, make_manifest());

    // WHEN
    let result = actor
        .execute(
            StepDeps {
                tool_proxy: &proxy,
                llm_router: &llm,
                budget: &budget,
                reasoner: &reasoner,
            },
            &resilience,
        )
        .await;

    // THEN - Failed with MAX_REPLAN_EXCEEDED
    assert_eq!(result.status, TaskStatus::Failed, "expected Failed");
    let code = result.error.as_ref().map(|e| e.code.as_str()).unwrap_or("");
    assert_eq!(
        code, "MAX_REPLAN_EXCEEDED",
        "expected MAX_REPLAN_EXCEEDED error code, got: {code}"
    );
}

// ── Reasoner retry ────────────────────────────────────────────────────────────

/// `Reasoner` retry ×3 sur JSON invalide - `PlanParseError(attempts: 3)`.
///
/// ÉTANT DONNÉ un mock `CompletionModel` retournant 3 fois du texte non-JSON
/// QUAND `Reasoner::plan(&ctx).await` est appelé
/// ALORS `Err(ReasonerError::PlanParseError { attempts: 3 })` est retourné
///   ET le mock a été appelé exactement 3 fois.
#[tokio::test]
async fn test_ac6_reasoner_retry_3_fois_json_invalide() {
    // GIVEN
    let model = MockCompletionModel::with_responses(vec![
        "pas du json",
        "toujours pas du json",
        "définitivement pas du json",
    ]);
    let reasoner = Reasoner::new(model.clone(), 10);
    let ctx = ContextBundle::default();

    // WHEN
    let result = reasoner.plan(&ctx).await;

    // THEN - PlanParseError after 3 failed attempts
    assert!(
        matches!(
            result,
            Err(ReasonerError::PlanParseError { attempts: 3, .. })
        ),
        "expected PlanParseError{{attempts:3}}, got: {result:?}"
    );
    assert_eq!(
        model.call_count(),
        3,
        "LLM must have been called exactly 3 times"
    );
}

// ── Concaténation automatique ─────────────────────────────────────────────────

/// Agent sans `on_plan_complete` → concaténation automatique des outputs.
///
/// ÉTANT DONNÉ un agent Rust implémentant `AIPAgent` sans `on_plan_complete`
///   ET un plan de 2 steps avec des outputs non vides
/// QUAND `ORIAEngine::execute(task, &agent)` est appelé
/// ALORS `Completed` est retourné
///   ET l'output contient les sorties des 2 steps concaténées.
#[tokio::test]
async fn test_ac7_agent_sans_hook_concatenation() {
    // GIVEN - minimal agent, no on_plan_complete (trait default: false)
    struct SimpleAgent;

    impl AIPAgent for SimpleAgent {
        fn manifest(&self) -> AgentManifest {
            AgentManifest {
                format_version: 1,
                name: "simple-agent".to_string(),
                version: "1.0.0".to_string(),
                description: "Test agent without hook".to_string(),
                execution_mode: "orchestrated".to_string(),
                system_prompt: Some("Tu es un assistant de test.".to_string()),
                tools_required: vec!["mock_tool".to_string()],
                tools_optional: vec![],
                step_budget: Some(StepBudgetConfig {
                    max_steps: 20,
                    max_tool_calls: 50,
                    wall_clock_secs: 600,
                }),
                supports_streaming: false,
                supports_a2a: false,
                supports_mailbox: false,
                mailbox_allowlist: None,
                memory_namespace: None,
                shared_memory_namespaces: vec![],
                max_concurrent_tasks: 1,
                network_allowlist: None,
                dangerous_tools_allowed: false,
                tags: vec![],
                skills: vec![],
                tools_requiring_approval: vec![],
                llm_backend: None,
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
        // has_on_plan_complete() returns false by default - auto-concat is used
    }

    let two_step_plan = r#"{"steps":[
        {"step_id":"s1","description":"Step one","tool_hint":"mock_tool","depends_on":[]},
        {"step_id":"s2","description":"Step two","tool_hint":"mock_tool","depends_on":["s1"]}
    ]}"#;
    let model = MockCompletionModel::with_responses(vec![two_step_plan]);
    let proxy = Arc::new(RecordingToolProxy::new("result-value"));
    let engine = ORIAEngine::new()
        .with_reasoner(model, 20)
        .with_tool_proxy(proxy as Arc<dyn ToolProxyTrait>);

    let task = AIPTask::default();

    // WHEN
    let result = engine.execute(task, &SimpleAgent).await;

    // THEN - Completed
    assert_eq!(
        result.status,
        TaskStatus::Completed,
        "expected Completed, got: {:?}",
        result.error
    );

    // THEN - output is non-empty (auto-concatenation of step outputs)
    let output_text: String = result
        .output
        .iter()
        .filter_map(|p| {
            if let AIPPart::Text(t) = p {
                Some(t.text.clone())
            } else {
                None
            }
        })
        .collect();
    assert!(
        !output_text.is_empty(),
        "automatic concatenation must produce non-empty output text"
    );
}
