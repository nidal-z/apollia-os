//! Integration tests for HITL Mode Orchestré — STORY-106 AC-3 and AC-4.
//!
//! Tests `ActorLoop` with `tools_requiring_approval`:
//! - AC-3: step with sensitive tool → suspend mid-plan → approve → step executed
//! - AC-4: step rejected → plan stopped, subsequent steps NOT executed
//!
//! All tests use mock `ToolProxyTrait` and `Reasoner` with a stub LLM.
//! No Python or real LLM required.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use apollia_core::{AgentManifest, InputResponseData, PendingApprovals, RuntimeEvent, TaskStatus};
use apollia_llm::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, LlmRouter,
    TokenUsage,
};
use apollia_oria::{
    actor::{ActorLoop, ToolProxyTrait},
    budget::StepBudget,
    plan::{ExecutionPlan, PlanStep},
    plan_repository::PlanRepository,
    reasoner::Reasoner,
    resilience::ResilienceLayer,
};
use async_trait::async_trait;

// ── Stub LLM (required by ActorLoop::execute signature) ─────────────────────

/// Minimal `CompletionModel` that always returns an empty plan.
/// Used to satisfy the `reasoner` parameter — replanification is never triggered
/// in these tests because we pre-insert the plan and steps succeed or reject.
struct StubLlm;

#[async_trait]
impl CompletionModel for StubLlm {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            content: r#"{"steps":[]}"#.into(),
            tool_calls: vec![],
            usage: TokenUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                cost_usd: None,
            },
            finish_reason: FinishReason::Stop,
            latency_ms: 0,
        })
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<String, LlmError>> + Send>>, LlmError>
    {
        Err(LlmError::InferenceError(
            "stub does not support streaming".into(),
        ))
    }

    fn is_available(&self) -> bool {
        true
    }

    fn backend_name(&self) -> &str {
        "stub"
    }

    fn model_id(&self) -> &str {
        "stub-model"
    }
}

// ── Mock ToolProxy ───────────────────────────────────────────────────────────

/// Records every `invoke()` call (by tool name) and returns a configurable response.
struct RecordingToolProxy {
    /// Tool-name → response string.
    responses: HashMap<String, String>,
    /// All tool names invoked, in order.
    invocations: Mutex<Vec<String>>,
}

impl RecordingToolProxy {
    fn new(responses: &[(&str, &str)]) -> Arc<Self> {
        Arc::new(Self {
            responses: responses
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            invocations: Mutex::new(vec![]),
        })
    }

    /// Returns how many times any tool was invoked.
    fn invocation_count(&self) -> usize {
        self.invocations.lock().unwrap().len()
    }

    /// Returns how many times `tool_name` was invoked.
    fn count_for(&self, tool_name: &str) -> usize {
        self.invocations
            .lock()
            .unwrap()
            .iter()
            .filter(|t| t.as_str() == tool_name)
            .count()
    }
}

#[async_trait]
impl ToolProxyTrait for RecordingToolProxy {
    async fn invoke(&self, tool_name: &str, _input: &serde_json::Value) -> Result<String, String> {
        self.invocations.lock().unwrap().push(tool_name.to_string());
        Ok(self
            .responses
            .get(tool_name)
            .cloned()
            .unwrap_or_else(|| format!("{tool_name}: ok")))
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Opens an in-memory `PlanRepository`.
fn make_db() -> PlanRepository {
    PlanRepository::new(":memory:").expect("in-memory SQLite must always succeed")
}

/// Builds an `AgentManifest` with `tools_requiring_approval`.
fn make_manifest_with_approval(tools: &[&str]) -> AgentManifest {
    let tools_json: serde_json::Value = tools
        .iter()
        .map(|t| serde_json::Value::String(t.to_string()))
        .collect::<Vec<_>>()
        .into();
    serde_json::from_value(serde_json::json!({
        "name": "hitl-test-agent",
        "version": "0.1.0",
        "description": "HITL integration test agent",
        "tools_required": [],
        "tools_requiring_approval": tools_json
    }))
    .expect("manifest must deserialize")
}

// ── AC-3 — tools_requiring_approval → suspend mid-plan → approve → step exécuté

/// ÉTANT DONNÉ un plan [s1:file_io, s2:smtp] + manifest.tools_requiring_approval=["smtp"]
///             + mock outil "smtp" qui retourne "envoyé"
/// QUAND execute() tourne, s1 s'exécute normalement,
///       s2 se suspend (smtp dans tools_requiring_approval),
///       puis PendingApprovals.resolve(approved=true)
/// ALORS s2 est exécuté après approve,
///       plan terminé avec statut Completed,
///       TaskInputRequired{step_id:Some("s2")} émis sur l'EventBus,
///       tool_proxy appelé 2 fois (s1 et s2)
#[tokio::test]
async fn test_ac3_orchestrated_tool_suspend_approve_resume() {
    // GIVEN
    let task_id = "t-orch-001";
    let plan = ExecutionPlan {
        plan_id: "plan-ac3".into(),
        task_id: task_id.into(),
        steps: vec![
            PlanStep {
                step_id: "s1".into(),
                description: "Lire le fichier".into(),
                tool_hint: Some("file_io".into()),
                depends_on: vec![],
            },
            PlanStep {
                step_id: "s2".into(),
                description: "Envoyer l'email".into(),
                tool_hint: Some("smtp".into()),
                depends_on: vec!["s1".into()],
            },
        ],
    };

    let db = make_db();
    db.insert_plan(&plan, "hitl-test-agent")
        .expect("insert_plan");
    db.insert_steps(&plan.plan_id, &plan.steps)
        .expect("insert_steps");

    let pending = Arc::new(PendingApprovals::new());
    let (bus_tx, mut bus_rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(64);
    let manifest = make_manifest_with_approval(&["smtp"]);

    let tool_proxy =
        RecordingToolProxy::new(&[("file_io", "fichier lu"), ("smtp", "email envoyé")]);

    let budget = StepBudget::unlimited();
    let resilience = ResilienceLayer::default();
    let reasoner = Reasoner::new(Arc::new(StubLlm), 10);
    let llm = LlmRouter::empty();

    let mut actor = ActorLoop::new(plan, 0, db, bus_tx, manifest)
        .with_pending_approvals(Some(Arc::clone(&pending)));

    // WHEN — spawn resolver that approves once TaskInputRequired{step_id=Some("s2")} seen
    let pending_clone = Arc::clone(&pending);
    let task_id_clone = task_id.to_string();
    let resolver = tokio::spawn(async move {
        // Poll the bus until TaskInputRequired for step s2 is emitted (max ~4 s)
        for _ in 0..200_u32 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            match bus_rx.try_recv() {
                Ok(RuntimeEvent::TaskInputRequired {
                    step_id: Some(ref id),
                    ..
                }) if id == "s2" => {
                    break;
                }
                _ => {}
            }
        }
        // Approval key for orchestrated mode = "{task_id}::{step_id}"
        pending_clone
            .resolve(
                &format!("{task_id_clone}::s2"),
                InputResponseData {
                    approved: true,
                    reason: None,
                    context: serde_json::Value::Null,
                    responded_at: "2026-01-01T00:00:00Z".into(),
                },
            )
            .expect("resolve s2 must succeed");
    });

    let result = actor
        .execute(tool_proxy.as_ref(), &llm, &budget, &resilience, &reasoner)
        .await;
    resolver.await.expect("resolver must not panic");

    // THEN — plan completed successfully
    assert_eq!(
        result.status,
        TaskStatus::Completed,
        "plan must complete after approval; error: {:?}",
        result.error
    );

    // THEN — both tools invoked (s1 file_io + s2 smtp)
    assert_eq!(
        tool_proxy.invocation_count(),
        2,
        "both file_io and smtp must be invoked"
    );
    assert_eq!(tool_proxy.count_for("smtp"), 1, "smtp invoked exactly once");
    assert_eq!(
        tool_proxy.count_for("file_io"),
        1,
        "file_io invoked exactly once"
    );
}

// ── AC-4 — step rejeté → plan stoppé, steps suivants non exécutés ──────────

/// ÉTANT DONNÉ un plan [s1:file_io, s2:smtp, s3:file_io]
///             + manifest.tools_requiring_approval=["smtp"]
/// QUAND s2 est rejeté (approved=false)
/// ALORS s3 n'est jamais exécuté,
///       plan retourne AIPResult::failed("REJECTED"),
///       file_io appelé 1 seule fois (pour s1, pas s3)
#[tokio::test]
async fn test_ac4_orchestrated_tool_reject_stops_plan() {
    // GIVEN
    let task_id = "t-orch-002";
    let plan = ExecutionPlan {
        plan_id: "plan-ac4".into(),
        task_id: task_id.into(),
        steps: vec![
            PlanStep {
                step_id: "s1".into(),
                description: "Lire le fichier".into(),
                tool_hint: Some("file_io".into()),
                depends_on: vec![],
            },
            PlanStep {
                step_id: "s2".into(),
                description: "Envoyer l'email".into(),
                tool_hint: Some("smtp".into()),
                depends_on: vec!["s1".into()],
            },
            PlanStep {
                step_id: "s3".into(),
                description: "Archiver le résultat".into(),
                tool_hint: Some("file_io".into()),
                depends_on: vec!["s2".into()],
            },
        ],
    };

    let db = make_db();
    db.insert_plan(&plan, "hitl-test-agent")
        .expect("insert_plan");
    db.insert_steps(&plan.plan_id, &plan.steps)
        .expect("insert_steps");

    let pending = Arc::new(PendingApprovals::new());
    let (bus_tx, mut bus_rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(64);
    let manifest = make_manifest_with_approval(&["smtp"]);

    // Track invocations — s3 must NOT be called
    let invocation_log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
    let invocation_log_clone = Arc::clone(&invocation_log);

    struct TrackingProxy {
        log: Arc<Mutex<Vec<String>>>,
    }
    #[async_trait]
    impl ToolProxyTrait for TrackingProxy {
        async fn invoke(
            &self,
            tool_name: &str,
            _input: &serde_json::Value,
        ) -> Result<String, String> {
            self.log.lock().unwrap().push(tool_name.to_string());
            Ok(format!("{tool_name}: ok"))
        }
    }

    let tool_proxy = TrackingProxy {
        log: invocation_log_clone,
    };

    let budget = StepBudget::unlimited();
    let resilience = ResilienceLayer::default();
    let reasoner = Reasoner::new(Arc::new(StubLlm), 10);
    let llm = LlmRouter::empty();

    let mut actor = ActorLoop::new(plan, 0, db, bus_tx, manifest)
        .with_pending_approvals(Some(Arc::clone(&pending)));

    // WHEN — spawn resolver that rejects s2
    let pending_clone = Arc::clone(&pending);
    let task_id_clone = task_id.to_string();
    let resolver = tokio::spawn(async move {
        for _ in 0..200_u32 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            match bus_rx.try_recv() {
                Ok(RuntimeEvent::TaskInputRequired {
                    step_id: Some(ref id),
                    ..
                }) if id == "s2" => {
                    break;
                }
                _ => {}
            }
        }
        pending_clone
            .resolve(
                &format!("{task_id_clone}::s2"),
                InputResponseData {
                    approved: false,
                    reason: Some("Destinataire invalide".into()),
                    context: serde_json::Value::Null,
                    responded_at: "2026-01-01T00:00:00Z".into(),
                },
            )
            .expect("resolve s2 (reject) must succeed");
    });

    let result = actor
        .execute(&tool_proxy, &llm, &budget, &resilience, &reasoner)
        .await;
    resolver.await.expect("resolver must not panic");

    // THEN — plan failed with REJECTED code
    assert_eq!(
        result.status,
        TaskStatus::Failed,
        "plan must fail after rejection"
    );
    let code = result.error.as_ref().map(|e| e.code.as_str()).unwrap_or("");
    assert_eq!(code, "REJECTED", "error code must be REJECTED");

    // THEN — file_io called exactly once (s1 only, NOT s3)
    let log = invocation_log.lock().unwrap();
    let file_io_count = log.iter().filter(|t| t.as_str() == "file_io").count();
    assert_eq!(
        file_io_count, 1,
        "file_io must only be called once (s1) — s3 must not execute; invocations: {log:?}"
    );
    let smtp_count = log.iter().filter(|t| t.as_str() == "smtp").count();
    assert_eq!(smtp_count, 0, "smtp must never be invoked after rejection");
}
