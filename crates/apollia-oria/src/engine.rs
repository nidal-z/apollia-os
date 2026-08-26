//! ORIAEngine: execution engine for agent tasks.
//!
//! Entry point for running agent tasks. Supports two modes:
//! - **Mode Direct**: single `agent.run()` call with `StepBudget` supervision.
//! - **Mode Orchestrated**: `Reasoner` generates a plan, `ActorLoop` executes
//!   each step via `ToolProxy`, and outputs are concatenated or forwarded to
//!   `on_plan_complete()`.
//!
//! The primary entry point is [`ORIAEngine::execute`], which classifies the task
//! and delegates to the appropriate mode. The lower-level [`ORIAEngine::execute_direct`]
//! remains available for callers that already hold an [`AgentRunner`].

mod builder;
mod direct;
mod orchestrated;
mod plan_cache_ops;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use apollia_core::{
    AIPPart, AIPResult, AIPTask, AgentManifest, DataPart, EventBusSender, ORIAConfig,
    PendingApprovals, StepBudgetConfig,
};
use apollia_llm::LlmRouter;
use apollia_memory::manager::MemoryManager;

use apollia_workspace::ProjectRuntime;

use crate::actor::ToolProxyTrait;
use crate::context_manager::ContextManager;
use crate::observer::{classify, ContextBundle, ExecutionMode, ObserverError};
use crate::plan_cache::PlanCacheRepository;
use crate::plan_gate::PendingPlanGates;
use crate::reasoner::{Reasoner, ReasonerError};
use crate::resilience::ResilienceLayer;

// Traits

/// Trait abstracting agent execution for testability.
///
/// Concrete implementation dispatches to `AIPBridge::call_run()`.
/// Tests use a mock runner.
pub trait AgentRunner: Send + Sync {
    /// Executes the agent's `run(task)` method and returns the result.
    fn call_run(
        &self,
        task: AIPTask,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send + '_>>;
}

/// Trait abstracting an agent that can be executed by `ORIAEngine::execute`.
///
/// Provides the `manifest()` for mode classification and tool resolution.
/// Optionally declares `on_plan_complete()` availability for orchestrated post-processing
/// (duck-typing detection).
///
/// Minimum contract: implement `manifest()` only. All other methods have defaults.
pub trait AIPAgent: Send + Sync {
    /// Returns the agent's manifest declaring capabilities and execution mode.
    fn manifest(&self) -> AgentManifest;

    /// Returns `true` if the agent exposes an `on_plan_complete()` method.
    ///
    /// Detected via `hasattr` Python. Returns `false` by default, in which case
    /// the automatic step-output concatenation is used as fallback.
    fn has_on_plan_complete(&self) -> bool {
        false
    }

    /// Calls `on_plan_complete(step_results)` on the agent.
    ///
    /// Invoked by `ORIAEngine::execute_orchestrated_plan()` when [`has_on_plan_complete`]
    /// returns `true`. The concrete Python implementation delegates to
    /// `AIPBridge::call_on_plan_complete()`.
    ///
    /// Default: concatenates step outputs automatically (same as the fallback path).
    ///
    /// [`has_on_plan_complete`]: AIPAgent::has_on_plan_complete
    fn call_on_plan_complete(
        &self,
        step_results: HashMap<String, String>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = AIPResult> + Send + '_>> {
        Box::pin(async move { concat_outputs(&step_results) })
    }
}

// Error types

/// ORIA engine errors.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ORIAError {
    /// Execution budget exhausted.
    #[error("step budget exceeded: {reason}")]
    BudgetExceeded {
        /// Human-readable description of the exhaustion reason.
        reason: String,
    },

    /// Agent execution failure.
    #[error("agent execution failed: {0}")]
    ExecutionFailed(String),

    /// Observer error.
    #[error("observer error: {0}")]
    ObserverError(#[from] ObserverError),

    /// AIP bridge error.
    #[error("bridge error: {0}")]
    BridgeError(String),

    /// No LLM configured, cannot run Orchestrated mode.
    #[error("no LLM configured for orchestrated execution")]
    NoLlmConfigured,

    /// Reasoner error during planning.
    #[error("planning failed: {0}")]
    PlanFailed(#[from] ReasonerError),

    /// The approval oneshot channel was closed before a response (runtime shutdown).
    #[error("approval channel closed before human response - runtime may be shutting down")]
    ApprovalChannelClosed,

    /// The plan gate timed out: no decision arrived within the configured TTL.
    #[error("plan gate timeout for run {run_id} (plan {plan_id}) after {ttl_secs}s")]
    PlanGateTimeout {
        /// Run identifier of the gated run.
        run_id: String,
        /// Identifier of the plan awaiting approval.
        plan_id: String,
        /// Configured TTL in seconds.
        ttl_secs: u64,
    },

    /// The plan gate channel closed before a decision was received.
    #[error("plan gate channel closed for run {run_id}")]
    PlanGateChannelClosed {
        /// Run identifier of the gated run.
        run_id: String,
    },
}

// NoopToolProxy: fallback when no proxy configured

/// Tool proxy that always returns an error, used when no tool proxy is configured.
pub(crate) struct NoopToolProxy;

#[async_trait::async_trait]
impl ToolProxyTrait for NoopToolProxy {
    async fn invoke(&self, tool_name: &str, _input: &serde_json::Value) -> Result<String, String> {
        Err(format!(
            "No tool proxy configured - cannot invoke '{tool_name}'"
        ))
    }
}

// ORIAEngine

/// ORIA execution engine (Observer-Reasoner-Actor).
///
/// Unified entry point for executing agent tasks. Supports two modes:
/// - **Mode Direct**: `execute_direct()` with `StepBudget` supervision.
/// - **Mode Orchestrated**: `execute()`, LLM planning plus `ActorLoop`.
///
/// Uses the builder pattern for dependency injection.
/// All dependencies are optional: an engine without an LLM only supports
/// Mode Direct.
pub struct ORIAEngine {
    pub(crate) reasoner: Option<Reasoner>,
    pub(crate) tool_proxy: Option<Arc<dyn ToolProxyTrait>>,
    pub(crate) llm_router: LlmRouter,
    pub(crate) resilience: ResilienceLayer,
    pub(crate) event_bus: EventBusSender,
    pub(crate) runtime_config: StepBudgetConfig,
    /// ORIA engine configuration injected from `apollia.toml`.
    pub(crate) oria_config: ORIAConfig,
    pub(crate) db_path: Option<String>,
    /// HITL registry of pending approvals, shared with the `ResumeHandler`.
    ///
    /// Required for `execute_direct()` to suspend the task and wait for the
    /// human decision. When `None`, `InputRequired` results are returned
    /// as-is without suspension.
    pub(crate) pending_approvals: Option<Arc<PendingApprovals>>,
    /// Plan-gate registry, shared with the consumer that submits the decision.
    ///
    /// When `Some` and the gate is active, the engine suspends after plan
    /// generation, registers a oneshot here, and awaits an approve/reject
    /// decision before starting the `ActorLoop`. When `None`, the gate cannot
    /// suspend and execution proceeds directly.
    pub(crate) pending_plan_gates: Option<Arc<PendingPlanGates>>,
    /// Per-run plan-gate override.
    ///
    /// `Some(true)` forces the gate active, `Some(false)` bypasses it, `None`
    /// defers to the autonomy tier. Set by the operator (CLI `run --plan`); other
    /// submission paths leave it `None` so the tier governs.
    pub(crate) plan_gate_override: Option<bool>,
    /// HITL SQLite repository, persists the prompt and context on suspension.
    ///
    /// When `None`, persistence is skipped (logged warning) but execution continues.
    pub(crate) task_repository: Option<Arc<apollia_tools::TaskRepository>>,
    /// Memory manager for automatic episodic recording per step.
    ///
    /// Passed to [`ActorLoop`] during orchestrated execution. When `Some`, each completed
    /// step records an episodic memory entry in the agent's namespace.
    ///
    /// Deliberate exception to the one-actor-one-responsibility rule:
    /// `Arc<Mutex<MemoryManager>>` is shared between the `ORIAEngine` (configuration)
    /// and the `ActorLoop` (per-step episodic writes). Mutations are rare (one write
    /// per step, fire-and-forget) and `MemoryManager` wraps a non-`Sync` SQLite
    /// connection.
    pub(crate) memory_manager: Option<Arc<Mutex<MemoryManager>>>,
    /// Execution plan cache.
    ///
    /// Wrapped in a `Mutex` because `rusqlite::Connection` is not `Sync`.
    /// Accesses are short (lookup/store) and not concurrent in practice.
    /// A cache hit avoids the LLM call and emits [`RuntimeEvent::PlanCacheHit`].
    /// Cache errors are logged at `warn` level and never block execution.
    /// Shared with the supervisor, which opens `plan_cache.db` at boot and
    /// exposes the same repository over REST for stats and clearing. An owned
    /// repository here would cache into a second database that no operator
    /// command can see.
    pub(crate) plan_cache: Option<Arc<Mutex<PlanCacheRepository>>>,
    /// Workspace context assembler with a TTL cache.
    ///
    /// Collects the git branch, file status, and `APOLLIA.md` content at the
    /// start of an orchestrated task, then injects the result into the system prompt.
    pub(crate) workspace_assembler: ProjectRuntime,
    /// Working directory used as the root for workspace collection.
    ///
    /// Initialized to `"."` by default; overridable via [`with_cwd`](ORIAEngine::with_cwd).
    pub(crate) cwd: PathBuf,
    /// LLM context-window manager, compacts history when needed.
    ///
    /// Initialized from `ORIAConfig::context_compact_threshold` and
    /// `ORIAConfig::context_summary_max_chars`. Passed to the `ActorLoop` during
    /// orchestrated execution to protect long LLM steps.
    pub(crate) context_manager: ContextManager,
}

impl ORIAEngine {
    // Binary Feedback

    /// Generate two alternative plans for a task and emit the event on the EventBus.
    ///
    /// Calls [`Reasoner::plan_with_alternatives`] via `tokio::join!` (both plans are
    /// produced in parallel) then emits `RuntimeEvent::PlanAlternativesGenerated`.
    ///
    /// The frontend (CLI `--alternatives` or `PlanAlternativesView.svelte`) intercepts
    /// the event, shows both plans, and asks the operator to choose.
    /// The choice is persisted by `PlanChoiceStore::log_plan_choice()`.
    ///
    /// # Errors
    ///
    /// - [`ORIAError::NoLlmConfigured`] when no Reasoner is configured.
    /// - [`ORIAError::PlanFailed`] when generating either plan fails.
    pub async fn run_task_with_alternatives(
        &self,
        ctx: &ContextBundle,
    ) -> Result<apollia_core::PlanAlternatives, ORIAError> {
        let reasoner = self.reasoner.as_ref().ok_or(ORIAError::NoLlmConfigured)?;

        let alternatives = reasoner
            .plan_with_alternatives(ctx, &self.oria_config)
            .await?;

        let _ = self
            .event_bus
            .send(apollia_core::RuntimeEvent::PlanAlternativesGenerated {
                alternatives: alternatives.clone(),
            });

        tracing::info!(
            session_id = %alternatives.session_id,
            "plan.alternatives.emitted"
        );

        Ok(alternatives)
    }

    // Unified entry point

    /// Main entry point: routes the task to Direct or Orchestrated mode.
    ///
    /// The mode is determined by [`classify`] from `manifest.execution_mode`
    /// (explicit override) or complexity heuristics.
    ///
    /// ## Mode Direct
    /// Not implemented through this entry point: use [`execute_direct`] directly
    /// with a concrete [`AgentRunner`].
    ///
    /// ## Mode Orchestrated
    /// Delegates to [`execute_orchestrated_plan`]:
    /// validate, plan, persist, ActorLoop, concat outputs.
    pub async fn execute(&self, task: AIPTask, agent: &(dyn AIPAgent + Send + Sync)) -> AIPResult {
        let manifest = agent.manifest();
        let mode = classify(
            &task,
            &manifest,
            None,
            self.oria_config.orchestrated_threshold as f32,
        );

        match mode {
            ExecutionMode::Direct => {
                // Direct mode via AIPAgent not yet implemented here.
                // Callers should use execute_direct() with an AgentRunner.
                AIPResult::failed(
                    "DIRECT_MODE_NOT_AVAILABLE_VIA_AIP_AGENT",
                    "Direct mode requires an AgentRunner - use execute_direct() directly",
                )
            }
            ExecutionMode::Orchestrated => {
                self.execute_orchestrated_plan(task, agent, manifest).await
            }
        }
    }

    // Mode Orchestrated

    // Mode Direct
}

impl Default for ORIAEngine {
    fn default() -> Self {
        Self::new()
    }
}

// Private helpers

/// Extract a task's text from its `input.parts`.
///
/// Concatenates all `TextPart`s separated by a space. Returns an empty string
/// when no text part is present.
pub(crate) fn extract_task_text(task: &AIPTask) -> String {
    task.input
        .parts
        .iter()
        .filter_map(|p| {
            if let AIPPart::Text(t) = p {
                Some(t.text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `AIPResult::completed_with_steps` stores the `HashMap<step_id, output>` as
/// `AIPPart::Data`. Returns an empty map if the data cannot be parsed.
pub(crate) fn extract_step_outputs(result: &AIPResult) -> HashMap<String, String> {
    if let Some(AIPPart::Data(DataPart { data })) = result.output.first() {
        if let Ok(map) = serde_json::from_value::<HashMap<String, String>>(data.clone()) {
            return map;
        }
    }
    HashMap::new()
}

/// Concatenate step outputs into an `AIPResult::Completed`.
///
/// Steps are sorted by `step_id` for a deterministic result.
/// Separator: two newlines (`\n\n`), aligned with Markdown formatting.
pub(crate) fn concat_outputs(outputs: &HashMap<String, String>) -> AIPResult {
    let mut sorted: Vec<(&String, &String)> = outputs.iter().collect();
    sorted.sort_by_key(|(k, _)| *k);
    let text = sorted
        .iter()
        .map(|(_, v)| v.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    AIPResult::completed(&text)
}

/// Extract a text rendering of a result for the critic's agent-output input.
///
/// Concatenates the text parts and JSON-serializes the data parts. File parts are
/// skipped: the critic reasons over textual output only.
pub(crate) fn result_text(result: &AIPResult) -> String {
    result
        .output
        .iter()
        .filter_map(|part| match part {
            AIPPart::Text(t) => Some(t.text.clone()),
            AIPPart::Data(d) => serde_json::to_string(&d.data).ok(),
            AIPPart::File(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// Tests: execute_direct

#[cfg(test)]
mod tests {

    use crate::budget::StepBudget;

    use super::*;
    use apollia_core::{AIPResult, PendingApprovals, StepBudgetConfig, TaskStatus};

    struct MockRunnerOk {
        result: AIPResult,
    }

    impl AgentRunner for MockRunnerOk {
        fn call_run(
            &self,
            _task: AIPTask,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send + '_>,
        > {
            let result = self.result.clone();
            Box::pin(async move { Ok(result) })
        }
    }

    struct MockRunnerErr {
        message: String,
    }

    impl AgentRunner for MockRunnerErr {
        fn call_run(
            &self,
            _task: AIPTask,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send + '_>,
        > {
            let msg = self.message.clone();
            Box::pin(async move { Err(msg) })
        }
    }

    fn make_task() -> AIPTask {
        AIPTask::default()
    }

    fn make_result() -> AIPResult {
        AIPResult {
            task_id: "task-001".into(),
            status: TaskStatus::Completed,
            output: vec![],
            error: None,
            artifacts: vec![],
            input_required_data: None,
        }
    }

    #[tokio::test]
    async fn test_execute_direct_budget_already_exhausted() {
        // GIVEN a budget already exhausted (max_steps=0)
        let config = StepBudgetConfig {
            max_steps: 0,
            max_tool_calls: 100,
            wall_clock_secs: 300,
        };
        let budget = Arc::new(StepBudget::new(&config));
        let engine = ORIAEngine::new();
        let runner = MockRunnerOk {
            result: make_result(),
        };

        // WHEN execute_direct() is called
        let result = engine.execute_direct(make_task(), &runner, budget).await;

        // THEN returns ORIAError::BudgetExceeded
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ORIAError::BudgetExceeded { .. }),
            "expected BudgetExceeded, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_execute_direct_success() {
        // GIVEN a valid budget and a mock runner returning Ok(AIPResult)
        let config = StepBudgetConfig {
            max_steps: 10,
            max_tool_calls: 20,
            wall_clock_secs: 300,
        };
        let budget = Arc::new(StepBudget::new(&config));
        let engine = ORIAEngine::new();
        let runner = MockRunnerOk {
            result: make_result(),
        };

        // WHEN execute_direct() is called
        let result = engine.execute_direct(make_task(), &runner, budget).await;

        // THEN returns Ok(AIPResult) with the expected result
        assert!(result.is_ok());
        let aip_result = result.expect("should be ok");
        assert_eq!(aip_result.task_id, "task-001");
        assert!(matches!(aip_result.status, TaskStatus::Completed));
    }

    #[tokio::test]
    async fn test_execute_direct_bridge_error() {
        // GIVEN a valid budget and a mock runner returning Err
        let config = StepBudgetConfig {
            max_steps: 10,
            max_tool_calls: 20,
            wall_clock_secs: 300,
        };
        let budget = Arc::new(StepBudget::new(&config));
        let engine = ORIAEngine::new();
        let runner = MockRunnerErr {
            message: "Python exception: crash".into(),
        };

        // WHEN execute_direct() is called
        let result = engine.execute_direct(make_task(), &runner, budget).await;

        // THEN returns ORIAError::BridgeError
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ORIAError::BridgeError(_)),
            "expected BridgeError, got: {err}"
        );
    }

    // ── HITL tests ───────────────────────────────────────────

    /// Runner returning InputRequired on the first call, then Completed on the second.
    struct MockRunnerInputRequired {
        call_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
    }

    impl AgentRunner for MockRunnerInputRequired {
        fn call_run(
            &self,
            task: AIPTask,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send + '_>,
        > {
            let count = self
                .call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Box::pin(async move {
                if count == 0 {
                    // First call: return InputRequired
                    Ok(AIPResult::input_required(
                        "Confirmer l'envoi ?",
                        serde_json::json!({"devis": 42}),
                    ))
                } else {
                    // Second call (resumed): verify is_resumed and return Completed
                    assert!(task.is_resumed, "task should be resumed on second call");
                    assert!(
                        task.input_response.is_some(),
                        "input_response should be set on resume"
                    );
                    Ok(AIPResult {
                        task_id: task.task_id,
                        status: TaskStatus::Completed,
                        output: vec![],
                        error: None,
                        artifacts: vec![],
                        input_required_data: None,
                    })
                }
            })
        }
    }

    fn make_budget() -> Arc<StepBudget> {
        Arc::new(StepBudget::new(&StepBudgetConfig {
            max_steps: 10,
            max_tool_calls: 20,
            wall_clock_secs: 300,
        }))
    }

    // InputRequired: TaskInputRequired emitted on EventBus + suspension recorded.

    /// GIVEN an agent that returns InputRequired
    /// WHEN execute_direct() receives that result
    /// THEN RuntimeEvent::TaskInputRequired is emitted on the EventBus
    #[tokio::test]
    async fn test_input_required_emits_event() {
        // GIVEN
        let (tx, mut rx) = tokio::sync::broadcast::channel::<apollia_core::RuntimeEvent>(16);
        let pending = Arc::new(PendingApprovals::new());
        let engine = ORIAEngine::new()
            .with_event_bus(tx)
            .with_pending_approvals(pending.clone());
        let runner = MockRunnerInputRequired {
            call_count: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
        };
        let task = AIPTask {
            task_id: "t-0001".into(),
            ..AIPTask::default()
        };

        // WHEN: spawn execute_direct in background so we can resolve from this task
        let engine_ref = &engine;
        let runner_ref = &runner;
        let budget = make_budget();
        let task_clone = task.clone();
        let pending_clone = pending.clone();

        let handle = tokio::spawn(async move {
            // Resolve from another task after a short yield
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            pending_clone
                .resolve(
                    "t-0001",
                    apollia_core::InputResponseData {
                        approved: true,
                        reason: None,
                        context: serde_json::Value::Null,
                        responded_at: "2026-01-01T00:00:00Z".into(),
                    },
                )
                .expect("resolve failed");
        });

        let result = engine_ref
            .execute_direct(task_clone, runner_ref, budget)
            .await;

        handle.await.expect("background task failed");

        // THEN result is Ok (Completed from second run)
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
        assert_eq!(result.unwrap().status, TaskStatus::Completed);

        // THEN TaskInputRequired was emitted
        let mut found = false;
        while let Ok(event) = rx.try_recv() {
            if matches!(event, apollia_core::RuntimeEvent::TaskInputRequired { .. }) {
                found = true;
                break;
            }
        }
        assert!(found, "expected TaskInputRequired event on EventBus");
    }

    // Approve: run() is called again with is_resumed=true.

    /// GIVEN a task suspended in input_required
    /// WHEN PendingApprovals.resolve(approved=true)
    /// THEN execute_direct() unblocks and calls run() again with is_resumed=true
    #[tokio::test]
    async fn test_approve_resumes_and_recalls_run() {
        // GIVEN
        let pending = Arc::new(PendingApprovals::new());
        let engine = ORIAEngine::new().with_pending_approvals(pending.clone());
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let runner = MockRunnerInputRequired {
            call_count: call_count.clone(),
        };
        let task = AIPTask {
            task_id: "t-0002".into(),
            ..AIPTask::default()
        };

        // Spawn resolver
        let pending_for_resolver = pending.clone();
        let resolver = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            pending_for_resolver
                .resolve(
                    "t-0002",
                    apollia_core::InputResponseData {
                        approved: true,
                        reason: None,
                        context: serde_json::Value::Null,
                        responded_at: "2026-01-01T00:00:00Z".into(),
                    },
                )
                .expect("resolve failed");
        });

        // WHEN
        let result = engine.execute_direct(task, &runner, make_budget()).await;
        resolver.await.expect("resolver task failed");

        // THEN result is Completed (run() called twice)
        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, TaskStatus::Completed);
        // run() was called twice: first returned InputRequired, second Completed
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    // Reject: AIPResult::failed("REJECTED") without calling run() again.

    /// GIVEN a task suspended in input_required
    /// WHEN PendingApprovals.resolve(approved=false, reason="Trop cher")
    /// THEN execute_direct() returns AIPResult::failed("REJECTED") without calling run() again
    #[tokio::test]
    async fn test_reject_returns_failed_without_run() {
        // GIVEN
        let pending = Arc::new(PendingApprovals::new());
        let engine = ORIAEngine::new().with_pending_approvals(pending.clone());
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let runner = MockRunnerInputRequired {
            call_count: call_count.clone(),
        };
        let task = AIPTask {
            task_id: "t-0003".into(),
            ..AIPTask::default()
        };

        // Spawn resolver with rejection
        let pending_for_resolver = pending.clone();
        let resolver = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            pending_for_resolver
                .resolve(
                    "t-0003",
                    apollia_core::InputResponseData {
                        approved: false,
                        reason: Some("Trop cher".into()),
                        context: serde_json::Value::Null,
                        responded_at: "2026-01-01T00:00:00Z".into(),
                    },
                )
                .expect("resolve failed");
        });

        // WHEN
        let result = engine.execute_direct(task, &runner, make_budget()).await;
        resolver.await.expect("resolver task failed");

        // THEN result is Failed with code REJECTED
        assert!(result.is_ok());
        let aip_result = result.unwrap();
        assert_eq!(aip_result.status, TaskStatus::Failed);
        let code = aip_result
            .error
            .as_ref()
            .map(|e| e.code.as_str())
            .unwrap_or("");
        assert_eq!(code, "REJECTED", "expected REJECTED error code");
        let msg = aip_result
            .error
            .as_ref()
            .map(|e| e.message.as_str())
            .unwrap_or("");
        assert!(
            msg.contains("Trop cher"),
            "reason should appear in message: {msg}"
        );
        // run() was called exactly once (first call only)
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}

// Tests: workspace context injection

#[cfg(test)]
mod workspace_tests {

    use super::*;

    #[tokio::test]
    async fn test_workspace_context_in_system_prompt() {
        // GIVEN a tmpdir with .git and APOLLIA.md containing "Test rules"
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join(".git")).expect("create .git");
        tokio::fs::write(dir.path().join("APOLLIA.md"), "Test rules")
            .await
            .expect("write");
        let engine = ORIAEngine::new_with_cwd(dir.path().to_owned());
        // WHEN
        let prompt = engine.build_system_prompt().await;
        // THEN
        assert!(
            prompt.contains("<context name=\"Règles du projet\">"),
            "expected context tag in: {prompt}"
        );
        assert!(
            prompt.contains("Test rules"),
            "expected APOLLIA.md content in: {prompt}"
        );
    }

    #[tokio::test]
    async fn test_no_apollia_md_no_workspace_rules_section() {
        // GIVEN a tmpdir without APOLLIA.md
        let dir = tempfile::tempdir().expect("tempdir");
        let engine = ORIAEngine::new_with_cwd(dir.path().to_owned());
        // WHEN
        let prompt = engine.build_system_prompt().await;
        // THEN: build_system_prompt returns an empty string (no empty section)
        assert!(
            !prompt.contains("Règles du projet"),
            "no 'Règles du projet' section expected when APOLLIA.md is absent"
        );
    }

    #[tokio::test]
    async fn test_build_system_prompt_empty_without_workspace() {
        // GIVEN a tmpdir with no git repo and no APOLLIA.md
        let dir = tempfile::tempdir().expect("tempdir");
        let engine = ORIAEngine::new_with_cwd(dir.path().to_owned());
        // WHEN
        let prompt = engine.build_system_prompt().await;
        // THEN: no workspace context block emitted
        assert!(
            prompt.is_empty() || !prompt.contains("<context name=\"workspace\">"),
            "no context block expected in empty workspace: {prompt}"
        );
    }
}

// Tests: execute() Mode Orchestrated

#[cfg(test)]
mod orchestrated_tests {
    use apollia_llm::CompletionModel;
    use std::time::Duration;

    use apollia_core::{AutonomyLevel, RuntimeEvent};

    use super::*;
    use crate::plan::ExecutionPlan;
    use crate::plan_cache::compute_cache_key;
    use crate::plan_gate::PlanGateDecision;
    use apollia_core::{AgentManifest, StepBudgetConfig, TaskStatus};
    use apollia_llm::{
        CompletionRequest, CompletionResponse, FinishReason, LlmError, StreamChunk, TokenUsage,
    };
    use std::pin::Pin;

    // ── Mock LLM model ──────────────────────────────────────────────────

    struct SimpleMockModel {
        response: String,
    }

    #[async_trait::async_trait]
    impl CompletionModel for SimpleMockModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Ok(CompletionResponse {
                engine_timings: None,
                content: self.response.clone(),
                tool_calls: vec![],
                usage: TokenUsage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    cost_usd: None,
                    ..Default::default()
                },
                finish_reason: FinishReason::Stop,
                latency_ms: 0,
                ttft_ms: None,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<
            Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>,
            LlmError,
        > {
            Err(LlmError::InferenceError(
                "mock does not support streaming".into(),
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

    // ── Mock LLM that always returns an error ───────────────────────────

    struct ErrorMockModel;

    #[async_trait::async_trait]
    impl CompletionModel for ErrorMockModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Err(LlmError::InferenceError("planned LLM failure".into()))
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<
            Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>,
            LlmError,
        > {
            Err(LlmError::InferenceError(
                "mock does not support streaming".into(),
            ))
        }

        fn is_available(&self) -> bool {
            false
        }

        fn backend_name(&self) -> &str {
            "error-mock"
        }

        fn model_id(&self) -> &str {
            "error-mock"
        }
    }

    // ── Mock ToolProxy ──────────────────────────────────────────────────

    struct MockToolProxy {
        output: String,
    }

    #[async_trait::async_trait]
    impl ToolProxyTrait for MockToolProxy {
        async fn invoke(
            &self,
            _tool_name: &str,
            _input: &serde_json::Value,
        ) -> Result<String, String> {
            Ok(self.output.clone())
        }
    }

    // ── Mock ToolProxy exposing a fixed tool schema (path A) ───────────────

    struct SchemaToolProxy {
        schema: serde_json::Value,
    }

    #[async_trait::async_trait]
    impl ToolProxyTrait for SchemaToolProxy {
        async fn invoke(
            &self,
            _tool_name: &str,
            _input: &serde_json::Value,
        ) -> Result<String, String> {
            Ok("done".to_string())
        }
        async fn tool_schema(&self, _tool_name: &str) -> Option<serde_json::Value> {
            Some(self.schema.clone())
        }
    }

    // ── Mock AIPAgent (no hook) ─────────────────────────────────────────

    struct MockAgent {
        manifest: AgentManifest,
    }

    impl AIPAgent for MockAgent {
        fn manifest(&self) -> AgentManifest {
            self.manifest.clone()
        }
    }

    // ── Mock AIPAgent with on_plan_complete hook ─────────────────────────

    struct MockAgentWithHook {
        manifest: AgentManifest,
    }

    impl AIPAgent for MockAgentWithHook {
        fn manifest(&self) -> AgentManifest {
            self.manifest.clone()
        }

        fn has_on_plan_complete(&self) -> bool {
            true
        }

        fn call_on_plan_complete(
            &self,
            _step_results: HashMap<String, String>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = AIPResult> + Send + '_>> {
            Box::pin(async move { AIPResult::completed("HOOK_CALLED") })
        }
    }

    // ── Helpers ─────────────────────────────────────────────────────────

    /// Valid 2-step plan JSON returned by the mock LLM.
    fn two_step_plan_json() -> String {
        r#"{"steps":[
            {"step_id":"s1","description":"step one","tool_hint":"file_io","depends_on":[]},
            {"step_id":"s2","description":"step two","tool_hint":"bash_executor","depends_on":[]}
        ]}"#
        .to_string()
    }

    /// Valid 4-step plan JSON (for step_count verification).
    fn four_step_plan_json() -> String {
        r#"{"steps":[
            {"step_id":"s1","description":"step 1","tool_hint":"file_io","depends_on":[]},
            {"step_id":"s2","description":"step 2","tool_hint":"file_io","depends_on":[]},
            {"step_id":"s3","description":"step 3","tool_hint":"bash_executor","depends_on":[]},
            {"step_id":"s4","description":"step 4","tool_hint":"bash_executor","depends_on":[]}
        ]}"#
        .to_string()
    }

    /// Plan-time enrichment fills a tool step's args from the tool schema.
    #[tokio::test]
    async fn test_enrich_plan_with_args_fills_tool_step() {
        // GIVEN an engine with a schema-exposing proxy and a model that returns
        // a constrained tool call, and a plan with a tool step lacking args
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["path", "content"]
        });
        let model = Arc::new(SimpleMockModel {
            response: r#"{"name":"file_write","arguments":{"path":"/tmp/x","content":"hi"}}"#
                .to_string(),
        });
        let mut backends: HashMap<String, Arc<dyn apollia_llm::CompletionModel>> = HashMap::new();
        backends.insert("mock".to_string(), model);
        let router = LlmRouter::with_backends(backends, "mock");
        let engine = ORIAEngine::new()
            .with_tool_proxy(Arc::new(SchemaToolProxy { schema }))
            .with_llm_router(router);

        let mut step = apollia_core::plan::PlanStep::new("s1", "write hi to /tmp/x");
        step.tool_hint = Some("file_write".to_string());
        let mut plan = ExecutionPlan {
            plan_id: "p1".to_string(),
            task_id: "t1".to_string(),
            steps: vec![step],
        };

        // WHEN enriching the plan
        engine.enrich_plan_with_args(&mut plan).await;

        // THEN the tool step carries valid structured args
        assert_eq!(
            plan.steps[0].args,
            Some(serde_json::json!({"path": "/tmp/x", "content": "hi"}))
        );
    }

    fn make_engine_with_mock(plan_json: String) -> ORIAEngine {
        let model = Arc::new(SimpleMockModel {
            response: plan_json,
        });
        let proxy = Arc::new(MockToolProxy {
            output: "mock output".into(),
        });
        ORIAEngine::new()
            .with_reasoner(model, 20)
            .with_tool_proxy(proxy)
    }

    fn orchestrated_manifest_with_prompt() -> AgentManifest {
        AgentManifest {
            format_version: 1,
            name: "test-agent".into(),
            version: "1.0.0".into(),
            description: "Test orchestrated agent".into(),
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
            system_prompt: Some("Planifie les étapes nécessaires.".into()),
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

    // agent without a hook: automatic concatenation of outputs

    /// GIVEN an execution_mode=orchestrated agent without on_plan_complete()
    ///      AND a mock LLM returning a 2-step plan
    ///      AND a mock ToolProxy returning an output for each step
    /// WHEN ORIAEngine::execute(task, &agent) is called
    /// THEN AIPResult::Completed is returned
    ///   AND RuntimeEvent::PlanCompleted was emitted
    #[tokio::test]
    async fn test_sans_hook_concatenation() {
        // GIVEN
        let engine = make_engine_with_mock(two_step_plan_json());
        let agent = MockAgent {
            manifest: orchestrated_manifest_with_prompt(),
        };
        let task = AIPTask::default();

        // WHEN
        let result = engine.execute(task, &agent).await;

        // THEN
        assert_eq!(
            result.status,
            TaskStatus::Completed,
            "expected Completed, got: {:?}",
            result.error
        );
    }

    // hook on_plan_complete() called when present

    /// GIVEN an agent with on_plan_complete() returning "HOOK_CALLED"
    ///      AND execute_orchestrated() returning CompletedWithSteps
    /// WHEN ORIAEngine::execute(task, &agent) is called
    /// THEN the result contains "HOOK_CALLED" (not the auto concatenation)
    #[tokio::test]
    async fn test_hook_called_when_present() {
        // GIVEN
        let engine = make_engine_with_mock(two_step_plan_json());
        let agent = MockAgentWithHook {
            manifest: orchestrated_manifest_with_prompt(),
        };
        let task = AIPTask::default();

        // WHEN
        let result = engine.execute(task, &agent).await;

        // THEN status Completed AND output contains "HOOK_CALLED"
        assert_eq!(
            result.status,
            TaskStatus::Completed,
            "expected Completed, got: {:?}",
            result.error
        );
        let output_text = result.output.iter().find_map(|p| {
            if let apollia_core::AIPPart::Text(t) = p {
                Some(t.text.clone())
            } else {
                None
            }
        });
        assert_eq!(
            output_text.as_deref(),
            Some("HOOK_CALLED"),
            "expected hook output 'HOOK_CALLED', got: {output_text:?}"
        );
    }

    // concatenation used when no hook is present

    /// GIVEN an agent WITHOUT on_plan_complete()
    ///      AND execute_orchestrated() returning CompletedWithSteps
    /// WHEN ORIAEngine::execute(task, &agent) is called
    /// THEN call_on_plan_complete() is NOT called
    ///   AND the automatic concatenation of outputs is returned
    #[tokio::test]
    async fn test_concat_used_when_no_hook() {
        // GIVEN
        let engine = make_engine_with_mock(two_step_plan_json());
        let agent = MockAgent {
            manifest: orchestrated_manifest_with_prompt(),
        };
        let task = AIPTask::default();

        // WHEN
        let result = engine.execute(task, &agent).await;

        // THEN status Completed AND output does NOT contain "HOOK_CALLED"
        assert_eq!(
            result.status,
            TaskStatus::Completed,
            "expected Completed, got: {:?}",
            result.error
        );
        let has_hook_called = result.output.iter().any(|p| {
            if let apollia_core::AIPPart::Text(t) = p {
                t.text.contains("HOOK_CALLED")
            } else {
                false
            }
        });
        assert!(
            !has_hook_called,
            "hook output should NOT appear when agent has no on_plan_complete"
        );
    }

    // system_prompt absent: immediate AIPResult::failed

    /// GIVEN an execution_mode=orchestrated agent WITHOUT system_prompt
    /// WHEN ORIAEngine::execute(task, &agent) is called
    /// THEN AIPResult::failed("MISSING_SYSTEM_PROMPT", _) is returned
    ///   AND Reasoner.plan() is NOT called (no LLM configured)
    #[tokio::test]
    async fn test_system_prompt_absent_retourne_failed() {
        // GIVEN: no LLM and no system_prompt (both should be caught at the system_prompt check)
        let engine = ORIAEngine::new();
        let agent = MockAgent {
            manifest: AgentManifest {
                format_version: 1,
                name: "no-prompt-agent".into(),
                version: "1.0.0".into(),
                description: "Agent without system_prompt".into(),
                tools_required: vec![],
                tools_optional: vec![],
                supports_streaming: false,
                supports_a2a: false,
                memory_namespace: None,
                shared_memory_namespaces: vec![],
                max_concurrent_tasks: 1,
                step_budget: None,
                network_allowlist: None,
                dangerous_tools_allowed: false,
                tags: vec![],
                skills: vec![],
                execution_mode: "orchestrated".into(),
                supports_mailbox: false,
                mailbox_allowlist: None,
                system_prompt: None, // ← absent
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
            },
        };
        let task = AIPTask::default();

        // WHEN
        let result = engine.execute(task, &agent).await;

        // THEN
        assert_eq!(result.status, TaskStatus::Failed);
        let err_code = result.error.as_ref().map(|e| e.code.as_str()).unwrap_or("");
        assert_eq!(
            err_code, "MISSING_SYSTEM_PROMPT",
            "expected MISSING_SYSTEM_PROMPT, got: {err_code}"
        );
    }

    // Reasoner fails: AIPResult::failed propagated

    /// GIVEN a mock LLM that always returns an error
    /// WHEN ORIAEngine::execute(task, &agent) is called
    /// THEN AIPResult::failed("PLAN_FAILED", _) is returned
    #[tokio::test]
    async fn test_reasoner_echec_retourne_failed() {
        // GIVEN
        let model = Arc::new(ErrorMockModel);
        let engine = ORIAEngine::new().with_reasoner(model, 20);
        let agent = MockAgent {
            manifest: orchestrated_manifest_with_prompt(),
        };
        let task = AIPTask::default();

        // WHEN
        let result = engine.execute(task, &agent).await;

        // THEN
        assert_eq!(result.status, TaskStatus::Failed);
        let err_code = result.error.as_ref().map(|e| e.code.as_str()).unwrap_or("");
        assert_eq!(
            err_code, "PLAN_FAILED",
            "expected PLAN_FAILED, got: {err_code}"
        );
    }

    // PlanGenerated with correct step_count

    /// GIVEN a 4-step plan generated by the Reasoner
    ///      AND an active EventBus subscriber
    /// WHEN ORIAEngine::execute_orchestrated() is called
    /// THEN the subscriber receives RuntimeEvent::PlanGenerated { step_count: 4, .. }
    #[tokio::test]
    async fn test_plan_generated_event_step_count() {
        // GIVEN
        let (tx, mut rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(32);
        let engine = make_engine_with_mock(four_step_plan_json()).with_event_bus(tx);

        let agent = MockAgent {
            manifest: orchestrated_manifest_with_prompt(),
        };
        let task = AIPTask::default();

        // WHEN
        let _ = engine.execute(task, &agent).await;

        // THEN: collect all events and look for PlanGenerated
        let mut found_step_count = None;
        while let Ok(event) = rx.try_recv() {
            if let RuntimeEvent::PlanGenerated { step_count, .. } = event {
                found_step_count = Some(step_count);
                break;
            }
        }

        assert_eq!(
            found_step_count,
            Some(4),
            "expected PlanGenerated with step_count=4"
        );
    }

    // Plan gate

    /// GIVEN an active plan gate and a generated plan
    /// WHEN execute reaches the gate
    /// THEN PlanApprovalRequired is emitted and the run waits (no PlanCompleted)
    ///   until a decision arrives; an Approval then unblocks the ActorLoop.
    #[tokio::test]
    async fn test_gate_suspends_then_approval_resumes() {
        // GIVEN an engine with the gate forced active and a gate registry
        let gates = PendingPlanGates::new();
        let (tx, mut rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(64);
        let engine = make_engine_with_mock(two_step_plan_json())
            .with_event_bus(tx)
            .with_force_plan_gate(true)
            .with_pending_plan_gates(gates.clone());
        let agent = MockAgent {
            manifest: orchestrated_manifest_with_prompt(),
        };

        // WHEN execute runs in the background
        let handle = tokio::spawn(async move { engine.execute(AIPTask::default(), &agent).await });

        // THEN PlanApprovalRequired is emitted and carries the run id
        let mut run_id = None;
        for _ in 0..200 {
            if let Ok(RuntimeEvent::PlanApprovalRequired {
                run_id: rid,
                step_count,
                ..
            }) = rx.try_recv()
            {
                assert_eq!(step_count, 2);
                run_id = Some(rid);
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let run_id = run_id.expect("PlanApprovalRequired must be emitted");

        // AND the run is still pending (the gate blocks the ActorLoop)
        assert!(!handle.is_finished(), "run must wait at the gate");

        // WHEN the plan is approved
        assert!(gates.decide(&run_id, PlanGateDecision::Approved));

        // THEN the run completes successfully
        let result = handle.await.expect("join");
        assert_eq!(result.status, TaskStatus::Completed);
    }

    /// GIVEN an active gate
    /// WHEN the operator submits a valid edited plan
    /// THEN the revised plan is approved and the run completes.
    #[tokio::test]
    async fn test_gate_edited_executes_revised_plan() {
        // GIVEN an engine with the gate forced active
        let gates = PendingPlanGates::new();
        let (tx, mut rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(64);
        let engine = make_engine_with_mock(two_step_plan_json())
            .with_event_bus(tx)
            .with_force_plan_gate(true)
            .with_pending_plan_gates(gates.clone());
        let agent = MockAgent {
            manifest: orchestrated_manifest_with_prompt(),
        };

        // WHEN execute runs and the gate opens
        let handle = tokio::spawn(async move { engine.execute(AIPTask::default(), &agent).await });
        let mut run_id = None;
        for _ in 0..200 {
            if let Ok(RuntimeEvent::PlanApprovalRequired { run_id: rid, .. }) = rx.try_recv() {
                run_id = Some(rid);
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let run_id = run_id.expect("PlanApprovalRequired must be emitted");

        // WHEN a valid edited plan is submitted
        let revised = vec![apollia_core::plan::PlanStep::new("s1", "edited step")];
        let decision = PlanGateDecision::Edited {
            revised_steps: revised,
        };
        assert!(gates.decide(&run_id, decision));

        // THEN the run completes successfully
        let result = handle.await.expect("join");
        assert_eq!(result.status, TaskStatus::Completed);
    }

    /// GIVEN an active gate
    /// WHEN the operator submits an edited plan with a dependency cycle
    /// THEN the run fails with PLAN_EDIT_INVALID and no step runs.
    #[tokio::test]
    async fn test_gate_edited_invalid_cycle_fails() {
        // GIVEN an engine with the gate forced active
        let gates = PendingPlanGates::new();
        let (tx, mut rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(64);
        let engine = make_engine_with_mock(two_step_plan_json())
            .with_event_bus(tx)
            .with_force_plan_gate(true)
            .with_pending_plan_gates(gates.clone());
        let agent = MockAgent {
            manifest: orchestrated_manifest_with_prompt(),
        };

        // WHEN execute runs and the gate opens
        let handle = tokio::spawn(async move { engine.execute(AIPTask::default(), &agent).await });
        let mut run_id = None;
        for _ in 0..200 {
            if let Ok(RuntimeEvent::PlanApprovalRequired { run_id: rid, .. }) = rx.try_recv() {
                run_id = Some(rid);
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let run_id = run_id.expect("PlanApprovalRequired must be emitted");

        // WHEN an edited plan with a direct cycle is submitted
        let revised = vec![
            {
                let mut s = apollia_core::plan::PlanStep::new("s1", "a");
                s.depends_on = vec!["s2".into()];
                s
            },
            {
                let mut s = apollia_core::plan::PlanStep::new("s2", "b");
                s.depends_on = vec!["s1".into()];
                s
            },
        ];
        let decision = PlanGateDecision::Edited {
            revised_steps: revised,
        };
        assert!(gates.decide(&run_id, decision));

        // THEN the run fails cleanly with the edit-invalid code
        let result = handle.await.expect("join");
        assert_eq!(result.status, TaskStatus::Failed);
        assert_eq!(
            result.error.as_ref().map(|e| e.code.as_str()),
            Some("PLAN_EDIT_INVALID")
        );
    }

    /// GIVEN an active gate with a 1s TTL and no decision
    /// WHEN the TTL elapses
    /// THEN the run fails with PLAN_GATE_TIMEOUT and no step ran
    #[tokio::test]
    async fn test_gate_timeout_returns_failed() {
        // GIVEN a gate forced active with a short TTL
        let gates = PendingPlanGates::new();
        let config = ORIAConfig {
            plan_gate_ttl_secs: 1,
            ..Default::default()
        };
        let engine = make_engine_with_mock(two_step_plan_json())
            .with_force_plan_gate(true)
            .with_pending_plan_gates(gates)
            .with_oria_config(config);
        let agent = MockAgent {
            manifest: orchestrated_manifest_with_prompt(),
        };

        // WHEN no decision arrives before the TTL
        let result = engine.execute(AIPTask::default(), &agent).await;

        // THEN the run fails cleanly with the timeout code
        assert_eq!(result.status, TaskStatus::Failed);
        assert_eq!(
            result.error.as_ref().map(|e| e.code.as_str()),
            Some("PLAN_GATE_TIMEOUT")
        );
    }

    /// GIVEN a BoundedAutonomous tier (gate bypass) with a registry present
    /// WHEN execute runs without forcing the gate
    /// THEN no PlanApprovalRequired is emitted and the run completes directly.
    #[tokio::test]
    async fn test_bounded_autonomous_bypasses_gate() {
        // GIVEN an autonomous tier that bypasses the gate
        let (tx, mut rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(64);
        let config = ORIAConfig {
            autonomy_level: Some(apollia_core::AutonomyLevel::BoundedAutonomous),
            ..Default::default()
        };
        let engine = make_engine_with_mock(two_step_plan_json())
            .with_event_bus(tx)
            .with_pending_plan_gates(PendingPlanGates::new())
            .with_oria_config(config);
        let agent = MockAgent {
            manifest: orchestrated_manifest_with_prompt(),
        };

        // WHEN
        let result = engine.execute(AIPTask::default(), &agent).await;

        // THEN no gate event was emitted and the run completed
        assert_eq!(result.status, TaskStatus::Completed);
        let mut saw_gate = false;
        while let Ok(event) = rx.try_recv() {
            if matches!(event, RuntimeEvent::PlanApprovalRequired { .. }) {
                saw_gate = true;
            }
        }
        assert!(!saw_gate, "BoundedAutonomous must bypass the gate");
    }

    /// GIVEN a bypass tier (BoundedAutonomous) but an explicit gate override on
    /// WHEN execute runs
    /// THEN the override wins and the gate activates.
    #[tokio::test]
    async fn test_plan_gate_override_on_wins_over_bypass_tier() {
        let gates = PendingPlanGates::new();
        let (tx, mut rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(64);
        let config = ORIAConfig {
            autonomy_level: Some(apollia_core::AutonomyLevel::BoundedAutonomous),
            ..Default::default()
        };
        let engine = make_engine_with_mock(two_step_plan_json())
            .with_event_bus(tx)
            .with_plan_gate_override(Some(true))
            .with_pending_plan_gates(gates.clone())
            .with_oria_config(config);
        let agent = MockAgent {
            manifest: orchestrated_manifest_with_prompt(),
        };

        let handle = tokio::spawn(async move { engine.execute(AIPTask::default(), &agent).await });
        let mut saw_gate = false;
        for _ in 0..200 {
            if let Ok(RuntimeEvent::PlanApprovalRequired { run_id, .. }) = rx.try_recv() {
                saw_gate = true;
                assert!(gates.decide(&run_id, PlanGateDecision::Approved));
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let result = handle.await.expect("join");
        assert!(
            saw_gate,
            "override Some(true) must force the gate on a bypass tier"
        );
        assert_eq!(result.status, TaskStatus::Completed);
    }

    /// GIVEN the gating tier (Assisted) but an explicit gate override off
    /// WHEN execute runs
    /// THEN the override wins and the gate is bypassed.
    #[tokio::test]
    async fn test_plan_gate_override_off_wins_over_gating_tier() {
        let (tx, mut rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(64);
        let engine = make_engine_with_mock(two_step_plan_json())
            .with_event_bus(tx)
            .with_plan_gate_override(Some(false))
            .with_pending_plan_gates(PendingPlanGates::new());
        let agent = MockAgent {
            manifest: orchestrated_manifest_with_prompt(),
        };

        let result = engine.execute(AIPTask::default(), &agent).await;

        assert_eq!(result.status, TaskStatus::Completed);
        let mut saw_gate = false;
        while let Ok(event) = rx.try_recv() {
            if matches!(event, RuntimeEvent::PlanApprovalRequired { .. }) {
                saw_gate = true;
            }
        }
        assert!(
            !saw_gate,
            "override Some(false) must bypass the gate on a gating tier"
        );
    }

    /// GIVEN the default tier (Assisted) with a registry present
    /// WHEN execute runs without forcing the gate
    /// THEN the gate activates and PlanApprovalRequired is emitted.
    #[tokio::test]
    async fn test_assisted_default_activates_gate() {
        // GIVEN an engine at the default (Assisted) tier with a registry
        let gates = PendingPlanGates::new();
        let (tx, mut rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(64);
        let engine = make_engine_with_mock(two_step_plan_json())
            .with_event_bus(tx)
            .with_pending_plan_gates(gates.clone());
        let agent = MockAgent {
            manifest: orchestrated_manifest_with_prompt(),
        };

        // WHEN execute runs in the background
        let handle = tokio::spawn(async move { engine.execute(AIPTask::default(), &agent).await });

        // THEN the gate activates (PlanApprovalRequired emitted), then approve to finish
        let mut saw_gate = false;
        for _ in 0..200 {
            if let Ok(RuntimeEvent::PlanApprovalRequired { run_id, .. }) = rx.try_recv() {
                saw_gate = true;
                assert!(gates.decide(&run_id, PlanGateDecision::Approved));
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let result = handle.await.expect("join");
        assert!(saw_gate, "Assisted default must activate the gate");
        assert_eq!(result.status, TaskStatus::Completed);
    }

    /// GIVEN a gate that is rejected twice then approved
    /// WHEN execute runs with replanning enabled
    /// THEN PlanRejected is emitted twice, PlanApproved once, and the run completes.
    #[tokio::test]
    async fn test_gate_rejection_replans_then_approves() {
        // GIVEN an engine with the gate forced active and a gate registry
        let gates = PendingPlanGates::new();
        let (tx, mut rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(256);
        let engine = make_engine_with_mock(two_step_plan_json())
            .with_event_bus(tx)
            .with_force_plan_gate(true)
            .with_pending_plan_gates(gates.clone());
        let agent = MockAgent {
            manifest: orchestrated_manifest_with_prompt(),
        };

        // WHEN execute runs and the operator rejects twice then approves
        let handle = tokio::spawn(async move { engine.execute(AIPTask::default(), &agent).await });
        let mut decisions = vec![
            PlanGateDecision::Rejected {
                feedback: Some("too long".into()),
            },
            PlanGateDecision::Rejected { feedback: None },
            PlanGateDecision::Approved,
        ];
        let mut rejected = 0;
        let mut approved = 0;
        for _ in 0..1000 {
            match rx.try_recv() {
                Ok(RuntimeEvent::PlanApprovalRequired { run_id, .. }) => {
                    if !decisions.is_empty() {
                        let d = decisions.remove(0);
                        assert!(gates.decide(&run_id, d));
                    }
                }
                Ok(RuntimeEvent::PlanRejected { .. }) => rejected += 1,
                Ok(RuntimeEvent::PlanApproved { .. }) => approved += 1,
                Ok(_) => {}
                Err(_) => {
                    if handle.is_finished() && decisions.is_empty() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
            }
        }

        // THEN the run completes after two rejections and one approval
        let result = handle.await.expect("join");
        assert_eq!(result.status, TaskStatus::Completed);
        assert_eq!(rejected, 2, "two PlanRejected expected");
        assert_eq!(approved, 1, "one PlanApproved expected");
    }

    /// GIVEN a replan limit of 1 and repeated rejections
    /// WHEN the limit is reached
    /// THEN the run is abandoned with PLAN_REPLAN_LIMIT_EXCEEDED and PlanAbandoned.
    #[tokio::test]
    async fn test_gate_replan_limit_exceeded() {
        // GIVEN an engine allowing a single replan
        let gates = PendingPlanGates::new();
        let (tx, mut rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(256);
        let config = ORIAConfig {
            plan_gate_max_replans: 1,
            ..Default::default()
        };
        let engine = make_engine_with_mock(two_step_plan_json())
            .with_event_bus(tx)
            .with_force_plan_gate(true)
            .with_pending_plan_gates(gates.clone())
            .with_oria_config(config);
        let agent = MockAgent {
            manifest: orchestrated_manifest_with_prompt(),
        };

        // WHEN the operator rejects every plan
        let handle = tokio::spawn(async move { engine.execute(AIPTask::default(), &agent).await });
        let mut abandoned = false;
        for _ in 0..1000 {
            match rx.try_recv() {
                Ok(RuntimeEvent::PlanApprovalRequired { run_id, .. }) => {
                    let _ = gates.decide(&run_id, PlanGateDecision::Rejected { feedback: None });
                }
                Ok(RuntimeEvent::PlanAbandoned { reason, .. }) => {
                    assert_eq!(reason, "replan_limit");
                    abandoned = true;
                }
                Ok(_) => {}
                Err(_) => {
                    if handle.is_finished() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
            }
        }

        // THEN the run fails with the replan-limit code and PlanAbandoned was emitted
        let result = handle.await.expect("join");
        assert_eq!(result.status, TaskStatus::Failed);
        assert_eq!(
            result.error.as_ref().map(|e| e.code.as_str()),
            Some("PLAN_REPLAN_LIMIT_EXCEEDED")
        );
        assert!(abandoned, "PlanAbandoned must be emitted");
    }

    // Helper tests

    /// GIVEN an AIPResult::completed_with_steps with 2 outputs
    /// WHEN extract_step_outputs is called
    /// THEN both outputs are extracted correctly
    #[test]
    fn test_extract_step_outputs_parses_data_part() {
        // GIVEN
        let mut steps = HashMap::new();
        steps.insert("s1".to_string(), "output one".to_string());
        steps.insert("s2".to_string(), "output two".to_string());
        let result = AIPResult::completed_with_steps(steps);

        // WHEN
        let outputs = extract_step_outputs(&result);

        // THEN
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs.get("s1").map(String::as_str), Some("output one"));
        assert_eq!(outputs.get("s2").map(String::as_str), Some("output two"));
    }

    /// GIVEN an empty map
    /// WHEN concat_outputs is called
    /// THEN AIPResult::Completed with an empty output is returned
    #[test]
    fn test_concat_outputs_empty_map_returns_completed() {
        // GIVEN
        let outputs = HashMap::new();

        // WHEN
        let result = concat_outputs(&outputs);

        // THEN
        assert_eq!(result.status, TaskStatus::Completed);
    }

    // Plan Cache Integration

    /// GIVEN two different versions of the same agent
    /// WHEN compute_cache_key is called with "1.0" then "1.1"
    /// THEN the cache keys differ
    #[test]
    fn test_version_change_produces_different_key() {
        // GIVEN
        let tools = vec!["bash".to_string(), "file_io".to_string()];
        let text = "analyze logs";

        // WHEN
        let key_v1 = compute_cache_key("analyzer", "1.0", &tools, text);
        let key_v2 = compute_cache_key("analyzer", "1.1", &tools, text);

        // THEN
        assert_ne!(key_v1, key_v2);
        assert_eq!(key_v1.len(), 64, "SHA-256 hex should be 64 chars");
        assert_eq!(key_v2.len(), 64, "SHA-256 hex should be 64 chars");
    }

    /// GIVEN a task with text parts
    /// WHEN extract_task_text is called
    /// THEN the texts are concatenated with a space
    #[test]
    fn test_extract_task_text_concatenates_text_parts() {
        // GIVEN
        let task = AIPTask {
            input: apollia_core::AIPInput {
                parts: vec![
                    AIPPart::Text(apollia_core::TextPart {
                        text: "analyze".into(),
                    }),
                    AIPPart::Data(DataPart {
                        data: serde_json::json!({"key": "val"}),
                    }),
                    AIPPart::Text(apollia_core::TextPart {
                        text: "logs".into(),
                    }),
                ],
            },
            ..AIPTask::default()
        };

        // WHEN
        let text = extract_task_text(&task);

        // THEN
        assert_eq!(text, "analyze logs");
    }

    /// GIVEN a task without any text part
    /// WHEN extract_task_text is called
    /// THEN an empty string is returned
    #[test]
    fn test_extract_task_text_empty_when_no_text_parts() {
        // GIVEN
        let task = AIPTask::default();

        // WHEN
        let text = extract_task_text(&task);

        // THEN
        assert!(text.is_empty());
    }

    /// GIVEN a PlanCacheHit event
    /// WHEN it is emitted on the EventBus
    /// THEN it is received with the correct fields
    #[test]
    fn test_cache_hit_event_emits_on_bus() {
        // GIVEN
        let (tx, mut rx) = tokio::sync::broadcast::channel::<apollia_core::RuntimeEvent>(16);

        // WHEN
        let _ = tx.send(RuntimeEvent::PlanCacheHit {
            task_id: "task-42".into(),
            cache_key: "abc123".into(),
        });

        // THEN
        let event = rx.try_recv().expect("should receive event");
        match event {
            RuntimeEvent::PlanCacheHit { task_id, cache_key } => {
                assert_eq!(task_id.as_ref(), "task-42");
                assert_eq!(cache_key, "abc123");
            }
            other => panic!("expected PlanCacheHit, got: {other:?}"),
        }
    }

    // ORIAConfig: max_replans

    /// GIVEN ORIAConfig without an explicit field
    /// WHEN Default::default() is called
    /// THEN max_replans equals 2
    #[test]
    fn test_default_max_replans_is_two() {
        // GIVEN / WHEN
        let config = ORIAConfig::default();

        // THEN
        assert_eq!(config.max_replans, 2);
    }

    /// GIVEN ORIAConfig with max_replans = 11
    /// WHEN validate() is called
    /// THEN a ConfigError::InvalidValue error is returned
    #[test]
    fn test_max_replans_eleven_fails_validation() {
        // GIVEN
        let config = ORIAConfig {
            max_replans: 11,
            ..ORIAConfig::default()
        };

        // WHEN
        let result = config.validate();

        // THEN
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("oria.max_replans"),
            "error should name the field, got: {msg}"
        );
        assert!(
            msg.contains("must be between 0 and 10"),
            "error should contain the bound description, got: {msg}"
        );
    }

    /// GIVEN ORIAConfig with max_replans = 10
    /// WHEN validate() is called
    /// THEN Ok(()) is returned (upper bound accepted)
    #[test]
    fn test_max_replans_ten_is_valid() {
        // GIVEN
        let config = ORIAConfig {
            max_replans: 10,
            ..ORIAConfig::default()
        };

        // WHEN
        let result = config.validate();

        // THEN
        assert!(result.is_ok());
    }

    /// GIVEN ORIAConfig with max_replans = 0
    /// WHEN with_oria_config is injected into ORIAEngine
    /// THEN oria_config.max_replans equals 0 (no replan allowed)
    #[test]
    fn test_max_replans_zero_disallows_replan() {
        // GIVEN
        let config = ORIAConfig {
            max_replans: 0,
            ..ORIAConfig::default()
        };

        // WHEN
        let engine = ORIAEngine::new().with_oria_config(config);

        // THEN
        assert_eq!(engine.oria_config.max_replans, 0);
    }

    /// GIVEN ORIAConfig with max_replans = 5
    /// WHEN with_oria_config is injected into ORIAEngine
    /// THEN oria_config.max_replans equals 5
    #[test]
    fn test_max_replans_five_allows_up_to_five() {
        // GIVEN
        let config = ORIAConfig {
            max_replans: 5,
            ..ORIAConfig::default()
        };

        // WHEN
        let engine = ORIAEngine::new().with_oria_config(config);

        // THEN
        assert_eq!(engine.oria_config.max_replans, 5);
    }

    /// GIVEN a negative value in the JSON (`max_replans: -1`)
    /// WHEN serde_json tries to deserialize it into ORIAConfig (u32)
    /// THEN deserialization fails because u32 natively rejects negatives
    #[test]
    fn test_max_replans_negative_handled() {
        // GIVEN: JSON with a negative value
        let json = serde_json::json!({ "max_replans": -1 });

        // WHEN
        let result = serde_json::from_value::<ORIAConfig>(json);

        // THEN: serde rejects -1 for a u32 field
        assert!(
            result.is_err(),
            "serde should reject negative values for u32 max_replans"
        );
    }

    // ── Post-run verification / critic (cap 2.8) ────────────────────────

    /// Mock model that answers planning requests with a fixed plan and critic
    /// requests with a scripted queue of verdicts.
    ///
    /// A critic request is recognized by the `AGENT OUTPUT:` marker the
    /// `CriticPass` embeds in its user message; every other request is treated as
    /// a planning request.
    struct ScriptedMockModel {
        plan: String,
        critic_queue: Mutex<Vec<String>>,
        critic_default: String,
    }

    impl ScriptedMockModel {
        fn is_critic_request(req: &CompletionRequest) -> bool {
            req.messages.iter().any(|m| {
                matches!(&m.content, apollia_llm::MessageContent::Text(t) if t.contains("AGENT OUTPUT:"))
            })
        }
    }

    #[async_trait::async_trait]
    impl CompletionModel for ScriptedMockModel {
        async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            let content = if Self::is_critic_request(&req) {
                let mut queue = self.critic_queue.lock().expect("mock lock");
                if queue.is_empty() {
                    self.critic_default.clone()
                } else {
                    queue.remove(0)
                }
            } else {
                self.plan.clone()
            };
            Ok(CompletionResponse {
                engine_timings: None,
                content,
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
        ) -> Result<
            Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>,
            LlmError,
        > {
            Err(LlmError::InferenceError("mock no stream".into()))
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

    /// Build an orchestrated engine whose single mock backend serves both planning
    /// and the critic, wired for the given tier and replan bound.
    // Test builder wiring many independent knobs; a struct adds no clarity here.
    // REASON: test builder mirroring the production engine constructor, dependency by dependency.
    #[allow(clippy::too_many_arguments)]
    fn make_engine_with_critic(
        plan: String,
        critic_queue: Vec<String>,
        critic_default: String,
        tier: AutonomyLevel,
        max_replans: u32,
        tx: EventBusSender,
    ) -> ORIAEngine {
        let model = Arc::new(ScriptedMockModel {
            plan,
            critic_queue: Mutex::new(critic_queue),
            critic_default,
        });
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
        backends.insert("mock".to_string(), model);
        let router = LlmRouter::with_backends(backends, "mock");
        let proxy = Arc::new(MockToolProxy {
            output: "mock output".into(),
        });
        let config = ORIAConfig {
            autonomy_level: Some(tier),
            verification_max_replans: max_replans,
            ..ORIAConfig::default()
        };
        ORIAEngine::new()
            .with_llm_router_and_reasoner(router, 20)
            .expect("router has a precise backend")
            .with_tool_proxy(proxy)
            .with_event_bus(tx)
            .with_oria_config(config)
    }

    /// Drain every event currently buffered on the receiver.
    fn drain_events(rx: &mut tokio::sync::broadcast::Receiver<RuntimeEvent>) -> Vec<RuntimeEvent> {
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        events
    }

    /// GIVEN an orchestrated run at a tier that requests verification, a passing
    ///       critic, and replan disabled
    /// WHEN the run completes
    /// THEN a VerificationCompleted verdict is emitted (passed, not skipped).
    #[tokio::test]
    async fn test_orchestrated_verification_emits_passing_verdict() {
        // GIVEN a bounded-autonomous run (verification on, gate bypassed) whose
        // critic returns no corrections, with replan disabled.
        let (tx, mut rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(64);
        let engine = make_engine_with_critic(
            two_step_plan_json(),
            vec![],
            r#"{"corrections":[]}"#.to_string(),
            AutonomyLevel::BoundedAutonomous,
            0,
            tx,
        );
        let agent = MockAgent {
            manifest: orchestrated_manifest_with_prompt(),
        };

        // WHEN the orchestrated run completes
        let result = engine.execute(AIPTask::default(), &agent).await;

        // THEN it completes and a passing, non-skipped verdict is emitted
        assert_eq!(result.status, TaskStatus::Completed);
        let events = drain_events(&mut rx);
        let verdict = events
            .iter()
            .find_map(|e| match e {
                RuntimeEvent::VerificationCompleted {
                    passed,
                    skipped,
                    replans,
                    ..
                } => Some((*passed, *skipped, *replans)),
                _ => None,
            })
            .expect("a VerificationCompleted event must be emitted");
        assert_eq!(verdict, (true, false, 0));
    }

    /// GIVEN a failing critic verdict on the first pass, a passing one after, and
    ///       a replan budget of 1
    /// WHEN the run completes
    /// THEN two verdicts (fail then pass) and a replanned PlanGenerated are
    ///      observed, and the second verdict records one replan.
    #[tokio::test]
    async fn test_orchestrated_verification_replans_on_fail() {
        // GIVEN a critic that objects once then accepts, with one replan allowed
        let (tx, mut rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(64);
        let fail = r#"{"corrections":[
            {"kind":"missing","description":"nothing produced","suggestion":"produce it"}
        ]}"#
        .to_string();
        let engine = make_engine_with_critic(
            two_step_plan_json(),
            vec![fail],
            r#"{"corrections":[]}"#.to_string(),
            AutonomyLevel::BoundedAutonomous,
            1,
            tx,
        );
        let agent = MockAgent {
            manifest: orchestrated_manifest_with_prompt(),
        };

        // WHEN the orchestrated run completes
        let result = engine.execute(AIPTask::default(), &agent).await;

        // THEN it completes, two verdicts are emitted (fail then pass), and a
        // replanned plan was generated in between.
        assert_eq!(result.status, TaskStatus::Completed);
        let events = drain_events(&mut rx);
        let verdicts: Vec<(bool, u32)> = events
            .iter()
            .filter_map(|e| match e {
                RuntimeEvent::VerificationCompleted {
                    passed, replans, ..
                } => Some((*passed, *replans)),
                _ => None,
            })
            .collect();
        assert_eq!(verdicts, vec![(false, 0), (true, 1)]);
        // Two PlanGenerated: the initial plan and the verification-driven replan.
        let plans = events
            .iter()
            .filter(|e| matches!(e, RuntimeEvent::PlanGenerated { .. }))
            .count();
        assert_eq!(plans, 2);
    }

    /// GIVEN the default (Assisted) tier, which does not request verification
    /// WHEN an orchestrated run completes
    /// THEN no VerificationCompleted verdict is emitted.
    #[tokio::test]
    async fn test_orchestrated_verification_dark_on_default_tier() {
        // GIVEN an assisted-tier run (verification off by default)
        let (tx, mut rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(64);
        let engine = make_engine_with_critic(
            two_step_plan_json(),
            vec![],
            r#"{"corrections":[]}"#.to_string(),
            AutonomyLevel::Assisted,
            2,
            tx,
        )
        // Assisted gates the plan; without a registry the gate auto-approves.
        .with_force_plan_gate(false);
        let agent = MockAgent {
            manifest: orchestrated_manifest_with_prompt(),
        };

        // WHEN the orchestrated run completes
        let result = engine.execute(AIPTask::default(), &agent).await;

        // THEN it completes but no verdict is emitted
        assert_eq!(result.status, TaskStatus::Completed);
        let events = drain_events(&mut rx);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, RuntimeEvent::VerificationCompleted { .. })),
            "verification must stay dark at the assisted tier"
        );
    }

    /// GIVEN a verification-enabled tier but no critic backend (empty router)
    /// WHEN an orchestrated run completes
    /// THEN a skipped, passing verdict is emitted and no replan occurs.
    #[tokio::test]
    async fn test_orchestrated_verification_degrades_without_backend() {
        // GIVEN an engine whose reasoner is wired but whose llm_router is empty,
        // so the critic route resolves nothing and the pass is skipped.
        let (tx, mut rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(64);
        let config = ORIAConfig {
            autonomy_level: Some(AutonomyLevel::BoundedAutonomous),
            verification_max_replans: 2,
            ..ORIAConfig::default()
        };
        let engine = make_engine_with_mock(two_step_plan_json())
            .with_event_bus(tx)
            .with_oria_config(config);
        let agent = MockAgent {
            manifest: orchestrated_manifest_with_prompt(),
        };

        // WHEN the orchestrated run completes
        let result = engine.execute(AIPTask::default(), &agent).await;

        // THEN a skipped, passing verdict is emitted with no replan
        assert_eq!(result.status, TaskStatus::Completed);
        let events = drain_events(&mut rx);
        let verdict = events
            .iter()
            .find_map(|e| match e {
                RuntimeEvent::VerificationCompleted {
                    passed,
                    skipped,
                    replans,
                    ..
                } => Some((*passed, *skipped, *replans)),
                _ => None,
            })
            .expect("a VerificationCompleted event must be emitted");
        assert_eq!(verdict, (true, true, 0));
    }

    /// GIVEN a plan cache repository shared with the runtime, as the supervisor
    ///       holds it
    /// WHEN  the engine stores a plan and then looks it up
    /// THEN  the entry is found, and it is visible through the shared handle the
    ///       operator commands read
    ///
    /// The cache path had no test at all, which is the other half of why it went
    /// unnoticed: the only builder took an owned repository that no caller could
    /// hand it, so `lookup_cached_plan` returned on its first line in production
    /// and nothing exercised the branch below it.
    #[test]
    fn test_shared_plan_cache_is_reachable_and_visible_to_the_operator() {
        // GIVEN
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = crate::plan_cache::PlanCacheRepository::open(&dir.path().join("plan_cache.db"))
            .expect("open repository");
        let shared = Arc::new(Mutex::new(repo));
        let engine = ORIAEngine::new().with_shared_plan_cache(Arc::clone(&shared));
        let manifest = orchestrated_manifest_with_prompt();
        let plan = crate::plan::ExecutionPlan {
            plan_id: "plan-shared-1".to_string(),
            task_id: "task-shared-1".to_string(),
            steps: vec![crate::plan::PlanStep::new("s1", "Read file")],
        };

        // WHEN
        engine.store_plan_in_cache("key-shared-1", &plan, &manifest);
        let found = engine.lookup_cached_plan("key-shared-1", "task-shared-1");

        // THEN
        assert!(
            found.is_some(),
            "a plan stored through the engine must be found again; before this \
             was wired the engine held no cache and both calls were no-ops"
        );
        let stats = shared
            .lock()
            .expect("cache mutex")
            .stats()
            .expect("read stats");
        assert_eq!(
            stats.total_entries, 1,
            "the operator reads this same repository through `plan cache stats`; \
             an owned copy inside the engine would leave it reporting zero"
        );
    }
}
