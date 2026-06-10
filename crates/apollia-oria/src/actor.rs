//! `ActorLoop`: topological execution loop for an [`crate::plan::ExecutionPlan`].
//!
//! `ActorLoop` is the centerpiece of Orchestrated mode (Option B): ORIA executes
//! tools and the LLM directly, `agent.run()` is **not** called during steps.
//! The agent only provides its `manifest()` and optionally `on_plan_complete()`.
//!
//! ## Execution pipeline
//!
//! ```text
//! ActorLoop::execute()
//!   |-- topological_sort(plan.steps)         -> execution order
//!   |-- For each step_id in order:
//!   |   |-- StepBudget::is_exhausted()       -> STEP_BUDGET_EXCEEDED if exhausted
//!   |   |-- db.start_step()                  -> SQLite
//!   |   |-- execute_step()                   -> tool via ToolProxyTrait OR LLM via LlmRouter
//!   |   |-- budget.increment_steps()
//!   |   |-- db.complete_step() / fail_step()
//!   |   `-- EventBus: StepStarted / StepCompleted / StepFailed
//!   |-- If a step fails (retryable) and replan_count < max_replans:
//!   |   `-- reasoner.replan() -> new plan -> execute_remaining()
//!   `-- All steps completed -> db.complete_plan() + AIPResult::completed_with_steps()
//! ```
//!
//! ## Thread safety
//!
//! `ActorLoop` holds a [`crate::plan_repository::PlanRepository`] which is `!Send`
//! (SQLite connection via `RefCell`). It must be created and consumed on the same thread.
//! The futures produced by `execute()` are therefore `!Send`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use apollia_core::events::{EventBusSender, RuntimeEvent};
use apollia_core::manifest::AgentManifest;
use apollia_core::observability::ObservabilityConfig;
use apollia_core::{AIPResult, ORIAConfig, PendingApprovals};
use apollia_llm::{
    router::ObservabilityConfig as LlmObsConfig, ChatMessage, CompletionRequest, LlmRouter,
};
use apollia_memory::manager::MemoryManager;

use crate::budget::StepBudget;
use crate::context_manager::{message_char_len, ContextManager};
use crate::observer::{ContextBundle, ExecutionMode};
use crate::plan::{ExecutionPlan, PlanStep};
use crate::plan_repository::PlanRepository;
use crate::reasoner::Reasoner;
use crate::resilience::{ErrorClass, ResilienceLayer, RetryContext, RetryPolicy};
use crate::topo::{topological_levels, topological_sort};

// ToolProxyTrait

/// ToolProxy abstraction for the `ActorLoop`, enables testing without PyO3.
///
/// Same abstraction pattern as `ToolExecutor` and `AgentRunner`.
/// The concrete implementation delegates to `ToolProxy::call()` via the AIP bridge.
/// Tests use a mock implementing this trait.
#[async_trait::async_trait]
pub trait ToolProxyTrait: Send + Sync {
    /// Invoke tool `tool_name` with `input` serialized as JSON.
    ///
    /// Returns the tool's text output on success, or an error message on failure.
    async fn invoke(&self, tool_name: &str, input: &serde_json::Value) -> Result<String, String>;

    /// Returns `true` if `tool_name` does not modify any external state.
    ///
    /// Used by [`ActorLoop::execute_tool_steps`] to decide whether a batch of steps
    /// at the same topological level can be executed concurrently.
    /// Defaults to `false` (conservative): unknown tools are never parallelised.
    fn is_tool_read_only(&self, _tool_name: &str) -> bool {
        false
    }
}

// StepError

/// Error from a single step produced by [`ActorLoop::execute_step`].
#[derive(Debug, thiserror::Error)]
pub enum StepError {
    /// The tool call failed.
    #[error("Tool call failed: {0}")]
    ToolCallFailed(String),
    /// The LLM call failed.
    #[error("LLM call failed: {0}")]
    LlmCallFailed(String),
    /// No LLM backend is configured in the `LlmRouter`.
    #[error("No LLM backend configured")]
    NoLlmBackend,
    /// The requested tool is not registered in the registry.
    #[error("Tool not found: {0}")]
    ToolNotFound(String),
    /// The step was rejected by the user before execution (HITL Orchestrated mode).
    ///
    /// Returned by [`ActorLoop::suspend_for_approval`] when the `ResumeHandler`
    /// sends `approved=false`. Plan execution stops immediately, the following
    /// steps are not attempted.
    #[error("step rejeté par l'utilisateur : {reason}")]
    RejectedByUser {
        /// Reason provided by the operator on rejection.
        reason: String,
    },
    /// The approval oneshot channel was closed before a response (runtime shutdown).
    ///
    /// Indicates the runtime is shutting down. Plan execution is stopped cleanly
    /// without panicking.
    #[error("channel d'approbation fermé - runtime en cours d'arrêt")]
    ApprovalChannelClosed,
}

impl StepError {
    /// Returns `true` if this error can trigger a replan.
    ///
    /// `ToolCallFailed` and `LlmCallFailed` are retryable (transient problems).
    /// Other variants are permanent and do not trigger a replan.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            StepError::ToolCallFailed(_) | StepError::LlmCallFailed(_)
        )
    }
}

// Constants

/// Importance level for step-level episodic memory entries.
///
/// Set to 0.6, above the default recall threshold (0.5) so that step outputs
/// appear in standard memory queries, but below critical events (1.0).
const STEP_MEMORY_IMPORTANCE: f64 = 0.6;

/// Default maximum character length for step output stored in episodic memory.
///
/// Used as the field default in [`ActorLoop`]. The configurable value is
/// injected via [`ActorLoop::with_step_memory_max_chars`].
const DEFAULT_STEP_MEMORY_OUTPUT_MAX_CHARS: usize = 200;

/// Maximum number of read-only tool steps driven concurrently by
/// [`ActorLoop::execute_tool_steps`].
///
/// Mirrors the same constant in `apollia-tools` to cap OS-level concurrency
/// (file descriptors, process handles) without requiring a shared config at
/// this layer.
const MAX_CONCURRENT_ORIA_TOOLS: usize = 10;

// StepContext

/// Context accumulated during plan execution, injected into each step.
///
/// Built incrementally by [`ActorLoop::execute`]: after each step completes,
/// its output is added to `previous_outputs`. Each subsequent step receives
/// a `StepContext` reflecting all prior results and the current budget state.
///
/// For LLM steps, `previous_outputs` is formatted into a system message
/// (`"Previous step results:\n- s1: …\n- s2: …"`) to enrich the prompt.
/// For tool steps, outputs are interpolated via `{{step_id}}` placeholders.
pub struct StepContext {
    /// Outputs of all previously completed steps, keyed by `step_id`.
    pub previous_outputs: HashMap<String, String>,
    /// Zero-based index of the current step in the topological order.
    pub step_index: usize,
    /// Total number of steps in the plan.
    pub total_steps: usize,
    /// Snapshot of the remaining budget at the moment this context was built.
    pub remaining_budget: apollia_llm::StepBudgetView,
}

impl StepContext {
    /// Formats `previous_outputs` as a human-readable block for LLM system messages.
    ///
    /// Returns `None` if there are no previous outputs.
    /// Returns lines formatted as `"Previous step results:\n- s1: {output}\n- s2: {output}"`.
    pub fn format_previous_outputs(&self) -> Option<String> {
        if self.previous_outputs.is_empty() {
            return None;
        }
        let mut lines = Vec::with_capacity(self.previous_outputs.len() + 1);
        lines.push("Previous step results:".to_string());
        // Sort by key for deterministic ordering.
        let mut entries: Vec<_> = self.previous_outputs.iter().collect();
        entries.sort_by_key(|(k, _)| (*k).clone());
        for (step_id, output) in entries {
            lines.push(format!("- {step_id}: {output}"));
        }
        Some(lines.join("\n"))
    }
}

// LevelOutcome

/// Result of executing a single topological level inside [`ActorLoop::execute`].
///
/// `Continue` hands ownership of the accumulated step outputs back to the
/// driving loop so the next level can run. `Terminal` carries the final
/// [`AIPResult`] the loop must return immediately (budget exhausted, replan
/// outcome, or terminal step failure).
enum LevelOutcome {
    /// The level completed; resume with the returned accumulated outputs.
    Continue(HashMap<String, String>),
    /// Plan execution must stop now with this result.
    Terminal(AIPResult),
}

/// Shared execution dependencies threaded through the level executors and the
/// replan / remaining-step paths.
///
/// Bundles the four borrowed collaborators required to run a step so the
/// private executor methods keep a small parameter surface. All fields are
/// shared references, so the bundle is cheap to copy.
#[derive(Clone, Copy)]
pub struct StepDeps<'a> {
    /// Proxy for executing tool calls.
    pub tool_proxy: &'a dyn ToolProxyTrait,
    /// Shared LLM router for reasoning calls.
    pub llm_router: &'a LlmRouter,
    /// Step budget (tokens / cost) applied to each call.
    pub budget: &'a StepBudget,
    /// Reasoner used for replanning.
    pub reasoner: &'a Reasoner,
}

// ActorLoop

/// Topological execution loop for an [`ExecutionPlan`].
///
/// Runs steps sequentially in the order determined by the topological sort,
/// applying the [`StepBudget`] and the [`ResilienceLayer`] on each tool/LLM call.
/// Persists each status transition in SQLite via [`PlanRepository`].
/// Emits [`RuntimeEvent`]s on the `EventBus` at every state change.
///
/// On a retryable step failure, triggers a replan via the [`Reasoner`]
/// up to `max_replans` times.
///
/// To enable HITL support, inject a [`PendingApprovals`] via
/// [`with_pending_approvals`]. Without it, steps with `tools_requiring_approval`
/// execute directly without suspension.
///
/// [`with_pending_approvals`]: ActorLoop::with_pending_approvals
pub struct ActorLoop {
    plan: ExecutionPlan,
    replan_count: u32,
    max_replans: u32,
    db: PlanRepository,
    event_bus: EventBusSender,
    /// Manifest of the agent that owns this plan.
    ///
    /// Stored read-only so `execute_step` can access `tools_requiring_approval`
    /// on each step.
    pub manifest: AgentManifest,
    /// HITL registry of pending approvals, shared with the `ResumeHandler`.
    ///
    /// `Some`: steps whose tool is in `tools_requiring_approval` suspend execution
    /// and wait for the human decision via a oneshot channel.
    /// `None`: no HITL suspension (degraded mode, steps execute normally).
    pending_approvals: Option<Arc<PendingApprovals>>,
    /// Observability config for truncating persisted inputs/outputs.
    obs_config: ObservabilityConfig,
    /// Memory manager for episodic recording after each step.
    ///
    /// When `Some`, step outputs are automatically recorded as episodic memories
    /// in the agent's namespace. `Arc<Mutex<MemoryManager>>` follows the same
    /// precedent for rare, operator-level writes.
    memory_manager: Option<Arc<Mutex<MemoryManager>>>,
    /// Maximum character length for step output stored in episodic memory.
    ///
    /// Injected from `ORIAConfig::step_memory_max_chars`. Defaults to
    /// [`DEFAULT_STEP_MEMORY_OUTPUT_MAX_CHARS`] when not configured.
    step_memory_max_chars: usize,
    /// LLM context-window manager for LLM-type steps.
    ///
    /// Injected from `ORIAEngine` via [`with_context_manager`].
    /// Compacts an LLM step's messages when their estimated size exceeds the
    /// threshold configured in `[oria] context_compact_threshold`.
    ///
    /// [`with_context_manager`]: ActorLoop::with_context_manager
    context_manager: ContextManager,
}

impl ActorLoop {
    /// Create an `ActorLoop` for a given plan.
    ///
    /// The plan must already be inserted in SQLite before creating the `ActorLoop`
    /// (via `PlanRepository::insert_plan` + `insert_steps`).
    ///
    /// `manifest` is kept read-only so steps can access `tools_requiring_approval`.
    ///
    /// To enable HITL support, chain with [`with_pending_approvals`].
    ///
    /// [`with_pending_approvals`]: ActorLoop::with_pending_approvals
    pub fn new(
        plan: ExecutionPlan,
        max_replans: u32,
        db: PlanRepository,
        event_bus: EventBusSender,
        manifest: AgentManifest,
    ) -> Self {
        Self {
            plan,
            replan_count: 0,
            max_replans,
            db,
            event_bus,
            manifest,
            pending_approvals: None,
            obs_config: ObservabilityConfig::default(),
            memory_manager: None,
            step_memory_max_chars: DEFAULT_STEP_MEMORY_OUTPUT_MAX_CHARS,
            context_manager: ContextManager::from_config(&ORIAConfig::default()),
        }
    }

    /// Inject the HITL registry of pending approvals.
    ///
    /// Required for steps whose tool is in `tools_requiring_approval` to suspend
    /// execution and wait for the human decision.
    /// Shared between the `ActorLoop` and the `ResumeHandler` via `AppState`.
    pub fn with_pending_approvals(mut self, pending: Option<Arc<PendingApprovals>>) -> Self {
        self.pending_approvals = pending;
        self
    }

    /// Configure observability for truncating persisted inputs/outputs.
    ///
    /// Defaults to [`ObservabilityConfig::default()`].
    pub fn with_obs_config(mut self, config: ObservabilityConfig) -> Self {
        self.obs_config = config;
        self
    }

    /// Inject a [`MemoryManager`] for per-step episodic recording.
    ///
    /// When configured, each successfully completed step automatically records an
    /// episodic entry in the agent's namespace. The write is fire-and-forget:
    /// a failure is logged as a warning but never interrupts plan execution.
    ///
    /// `Arc<Mutex<MemoryManager>>` follows the same precedent for rare mutations.
    pub fn with_memory_manager(mut self, mm: Option<Arc<Mutex<MemoryManager>>>) -> Self {
        self.memory_manager = mm;
        self
    }

    /// Overrides the maximum character length for step output stored in episodic memory.
    ///
    /// Injected from `ORIAConfig::step_memory_max_chars`. When not called,
    /// defaults to [`DEFAULT_STEP_MEMORY_OUTPUT_MAX_CHARS`] (200 chars).
    pub fn with_step_memory_max_chars(mut self, max_chars: usize) -> Self {
        self.step_memory_max_chars = max_chars;
        self
    }

    /// Inject the `ContextManager` to compact LLM messages when needed.
    ///
    /// Called by `ORIAEngine` with the `ContextManager` initialized from `ORIAConfig`.
    /// Without this call, the defaults (`threshold = 0.80`, `max_chars = 4000`)
    /// are used.
    pub fn with_context_manager(mut self, cm: ContextManager) -> Self {
        self.context_manager = cm;
        self
    }

    /// Execute the full plan in topological order.
    ///
    /// Returns `AIPResult::completed_with_steps` if all steps complete.
    /// Returns `AIPResult::failed` if the budget is exhausted, too many replans
    /// were attempted, or a step fails permanently.
    ///
    /// All SQLite errors are logged but do not interrupt execution
    /// (fire-and-forget).
    pub async fn execute(
        &mut self,
        deps: StepDeps<'_>,
        resilience: &ResilienceLayer,
    ) -> AIPResult {
        let tool_proxy = deps.tool_proxy;
        let levels = match topological_levels(&self.plan.steps) {
            Ok(l) => l,
            Err(_) => {
                if let Err(e) = self.db.fail_plan(&self.plan.plan_id, "INVALID_PLAN") {
                    tracing::warn!(error = %e, "fail_plan DB call failed (ignored)");
                }
                return AIPResult::failed("INVALID_PLAN", "Circular dependency in execution plan");
            }
        };

        let mut completed_outputs: HashMap<String, String> = HashMap::new();

        // Iterate level by level. Within a level all steps have the same dependency
        // depth and can run concurrently when they are all read-only tool calls.
        for level_ids in levels {
            // Collect the PlanStep objects for this level.
            let level_steps: Vec<PlanStep> = level_ids
                .iter()
                .filter_map(|id| self.plan.steps.iter().find(|s| s.step_id == *id))
                .cloned()
                .collect();

            // A level qualifies for concurrent batch execution when every step is a
            // read-only tool call that does not require human approval.
            let outcome = if self.is_batch_eligible(&level_steps, tool_proxy) {
                self.execute_level_batch(level_steps, completed_outputs, deps, resilience)
                    .await
            } else {
                self.execute_level_sequential(level_ids, completed_outputs, deps, resilience)
                    .await
            };

            completed_outputs = match outcome {
                LevelOutcome::Continue(co) => co,
                LevelOutcome::Terminal(result) => return result,
            };
        }

        // All steps completed.
        if let Err(e) = self.db.complete_plan(&self.plan.plan_id) {
            tracing::warn!(error = %e, "complete_plan DB call failed (ignored)");
        }
        let _ = self.event_bus.send(RuntimeEvent::PlanCompleted {
            task_id: self.plan.task_id.clone().into(),
            plan_id: self.plan.plan_id.clone(),
            step_count: completed_outputs.len(),
            duration_ms: 0,
        });

        AIPResult::completed_with_steps(completed_outputs)
    }

    /// Executes one topological level whose steps are all read-only tool calls,
    /// running them concurrently (batch path).
    ///
    /// Owns `completed_outputs` for the duration of the level and returns it via
    /// [`LevelOutcome::Continue`] when the whole level succeeds, or
    /// [`LevelOutcome::Terminal`] carrying the final [`AIPResult`] when the plan
    /// must stop (budget exhausted, replan, or terminal step failure).
    async fn execute_level_batch<'a>(
        &'a mut self,
        level_steps: Vec<PlanStep>,
        mut completed_outputs: HashMap<String, String>,
        deps: StepDeps<'a>,
        resilience: &'a ResilienceLayer,
    ) -> LevelOutcome {
        // Phase 1 (sequential): budget guard, events, DB pre-execution.
        if deps.budget.is_exhausted() {
            return LevelOutcome::Terminal(self.fail_plan_budget_exhausted(&format!(
                "Budget de {} steps atteint",
                deps.budget.max_steps
            )));
        }
        for step in &level_steps {
            let step_num = completed_outputs.len() + 1;
            self.persist_step_pre_execution(step, step_num, &completed_outputs);
        }

        // Phase 2: Concurrent invocations.
        let started = Instant::now();
        let batch_results = self
            .execute_tool_steps(&level_steps, &completed_outputs, deps.tool_proxy, resilience)
            .await;
        let duration_ms = started.elapsed().as_millis() as u64;

        // Phase 3 (sequential): budget increment, DB post-execution, events, errors.
        for (step, (step_id, result)) in level_steps.iter().zip(batch_results) {
            deps.budget.increment_steps();
            if let Err(e) =
                self.db
                    .save_step_duration(&step_id, &self.plan.plan_id, duration_ms as i64)
            {
                tracing::warn!(error = %e, step_id = %step_id, "save_step_duration DB call failed (ignored)");
            }
            match result {
                Ok(output) => {
                    self.persist_step_success(&step_id, &output, duration_ms);
                    self.record_step_memory(&step_id, &step.description, &output);
                    completed_outputs.insert(step_id, output);
                }
                Err(ref e) if e.is_retryable() && self.replan_count < self.max_replans => {
                    self.persist_step_failure(&step_id, &e.to_string());
                    self.emit_step_failed(&step_id, &e.to_string(), true);
                    return LevelOutcome::Terminal(
                        self.replan_and_continue(
                            step_id,
                            e.to_string(),
                            completed_outputs,
                            deps,
                            resilience,
                        )
                        .await,
                    );
                }
                Err(e) => {
                    return LevelOutcome::Terminal(
                        self.finalize_terminal_failure(&step_id, &e, true),
                    )
                }
            }
        }

        LevelOutcome::Continue(completed_outputs)
    }

    /// Executes one topological level sequentially, processing each step one at
    /// a time (LLM steps, mutating tools, tools requiring approval, single-step
    /// levels).
    ///
    /// Mirrors [`execute_level_batch`](Self::execute_level_batch) for ownership
    /// of `completed_outputs` and the [`LevelOutcome`] return contract.
    async fn execute_level_sequential<'a>(
        &'a mut self,
        level_ids: Vec<String>,
        mut completed_outputs: HashMap<String, String>,
        deps: StepDeps<'a>,
        resilience: &'a ResilienceLayer,
    ) -> LevelOutcome {
        for step_id in level_ids {
            let step = match self.plan.steps.iter().find(|s| s.step_id == step_id) {
                Some(s) => s.clone(),
                None => continue,
            };

            // check the budget before each step.
            if deps.budget.is_exhausted() {
                return LevelOutcome::Terminal(self.fail_plan_budget_exhausted(&format!(
                    "Budget de {} steps atteint",
                    deps.budget.max_steps
                )));
            }

            // Emit StepStarted + persist rendered input + tool name before execution.
            let step_num = completed_outputs.len() + 1;
            self.persist_step_pre_execution(&step, step_num, &completed_outputs);

            // build StepContext with accumulated outputs and budget snapshot.
            let step_ctx = StepContext {
                previous_outputs: completed_outputs.clone(),
                step_index: completed_outputs.len(),
                total_steps: self.plan.steps.len(),
                remaining_budget: deps.budget.to_budget_view(),
            };

            let started = Instant::now();
            let result = self
                .execute_step(&step, &step_ctx, deps.tool_proxy, deps.llm_router, resilience)
                .await;
            let duration_ms = started.elapsed().as_millis() as u64;
            deps.budget.increment_steps();

            // persist duration unconditionally.
            if let Err(e) =
                self.db
                    .save_step_duration(&step_id, &self.plan.plan_id, duration_ms as i64)
            {
                tracing::warn!(error = %e, step_id = %step_id, "save_step_duration DB call failed (ignored)");
            }

            match result {
                Ok(output) => {
                    self.persist_step_success(&step_id, &output, duration_ms);

                    // record episodic memory per step (fire-and-forget).
                    self.record_step_memory(&step_id, &step.description, &output);

                    completed_outputs.insert(step_id, output);
                }

                Err(ref e) if e.is_retryable() && self.replan_count < self.max_replans => {
                    self.persist_step_failure(&step_id, &e.to_string());
                    self.emit_step_failed(&step_id, &e.to_string(), true);
                    return LevelOutcome::Terminal(
                        self.replan_and_continue(
                            step_id,
                            e.to_string(),
                            completed_outputs,
                            deps,
                            resilience,
                        )
                        .await,
                    );
                }

                Err(e) => {
                    return LevelOutcome::Terminal(
                        self.finalize_terminal_failure(&step_id, &e, true),
                    )
                }
            }
        }

        LevelOutcome::Continue(completed_outputs)
    }

    /// Executes a batch of tool-only steps, parallelising when all tools are read-only.
    ///
    /// **Parallel path**: when every step in `steps` targets a read-only tool
    /// (as reported by [`ToolProxyTrait::is_tool_read_only`]) **and** no tool in the
    /// batch requires human approval, all invocations are driven concurrently via
    /// `futures::stream::StreamExt::buffered` with a cap of
    /// `MAX_CONCURRENT_READ_TOOLS` simultaneous calls.
    ///
    /// **Serial path**: in all other cases (LLM steps, mutating tools, tools
    /// requiring approval, or a batch of one): invocations run sequentially.
    ///
    /// Output order matches input order in both paths.
    /// Inputs are interpolated from `completed_outputs` before invocation.
    async fn execute_tool_steps(
        &self,
        steps: &[PlanStep],
        completed_outputs: &HashMap<String, String>,
        tool_proxy: &dyn ToolProxyTrait,
        resilience: &ResilienceLayer,
    ) -> Vec<(String, Result<String, StepError>)> {
        use futures::stream::{self, StreamExt};

        if steps.is_empty() {
            return vec![];
        }

        let all_read_only = steps.len() > 1
            && steps.iter().all(|s| {
                s.tool_hint.as_deref().is_some_and(|t| {
                    t != "llm"
                        && tool_proxy.is_tool_read_only(t)
                        && !self
                            .manifest
                            .tools_requiring_approval
                            .iter()
                            .any(|a| a == t)
                })
            });

        // Pre-compute per-call data to avoid lifetime issues with the async stream.
        let tool_names: Vec<String> = steps
            .iter()
            .map(|s| s.tool_hint.clone().unwrap_or_default())
            .collect();
        let inputs: Vec<serde_json::Value> = steps
            .iter()
            .map(|s| serde_json::json!({"input": interpolate_outputs(&s.description, completed_outputs)}))
            .collect();
        let step_ids: Vec<String> = steps.iter().map(|s| s.step_id.clone()).collect();

        // Register a breaker for every tool so the resilience pre_check never
        // trips on an unknown tool.
        for name in &tool_names {
            resilience.ensure_tool(name);
        }

        // Runs one tool call wrapped by the ResilienceLayer. pre_check (which
        // short-circuits an open breaker without invoking the tool), retry with
        // backoff, and success/failure recording all happen inside the layer.
        // Returns the original step id paired with the outcome so results can be
        // collected in input order.
        let run_one = |i: usize| {
            let tool_name = tool_names[i].clone();
            let input = inputs[i].clone();
            let step_id = step_ids[i].clone();
            async move {
                let policy = RetryPolicy::default();
                let (outcome, _attempts) = resilience
                    .execute_with_observability(
                        RetryContext {
                            tool_name: &tool_name,
                            tool_call_id: &step_id,
                            retry_policy: &policy,
                            bus: Some(&self.event_bus),
                        },
                        Self::classify_tool_error,
                        || tool_proxy.invoke(&tool_name, &input),
                    )
                    .await;
                (
                    step_id.clone(),
                    outcome.map_err(|e| StepError::ToolCallFailed(e.to_string())),
                )
            }
        };

        if all_read_only {
            stream::iter((0..steps.len()).map(&run_one))
                .buffered(MAX_CONCURRENT_ORIA_TOOLS)
                .collect::<Vec<_>>()
                .await
        } else {
            let mut results = Vec::with_capacity(steps.len());
            for i in 0..steps.len() {
                results.push(run_one(i).await);
            }
            results
        }
    }

    /// Execute a single step, tool or LLM depending on `tool_hint`.
    ///
    /// Before the actual execution, checks whether the step's tool is in
    /// `manifest.tools_requiring_approval`. If so and `pending_approvals` is
    /// configured, calls [`suspend_for_approval`] and waits for the human decision.
    ///
    /// - `tool_hint = Some("llm")` or `None`: LLM call, routed via `model_hint`
    ///   when present, otherwise the default backend. Previous outputs are
    ///   injected into the system message.
    /// - `tool_hint = Some(tool_name)`: call via `ToolProxyTrait::invoke`
    ///   (`model_hint` ignored for tool steps).
    ///
    /// Previous step outputs are interpolated into the step description via
    /// [`interpolate_outputs`] before being passed to the tool or the LLM.
    ///
    /// [`suspend_for_approval`]: ActorLoop::suspend_for_approval
    // REASON: cohesive execution dependencies (proxy, router, resilience) plus
    // the step context. A future consolidation may move the resilience layer
    // into the StepDeps bundle once the batch path needs it too.
    #[allow(clippy::too_many_arguments)]
    async fn execute_step(
        &self,
        step: &PlanStep,
        step_ctx: &StepContext,
        tool_proxy: &dyn ToolProxyTrait,
        llm_router: &LlmRouter,
        resilience: &ResilienceLayer,
    ) -> Result<String, StepError> {
        // Check whether the step's tool requires human approval.
        let tool_needs_approval = step
            .tool_hint
            .as_deref()
            .map(|t| {
                self.manifest
                    .tools_requiring_approval
                    .iter()
                    .any(|a| a == t)
            })
            .unwrap_or(false);

        if tool_needs_approval {
            if let Some(pending) = self.pending_approvals.as_ref() {
                self.suspend_for_approval(step, pending).await?;
            } else {
                tracing::warn!(
                    step_id = %step.step_id,
                    tool = ?step.tool_hint,
                    "PendingApprovals not configured - executing sensitive step without approval"
                );
            }
        }

        // Normal step execution after approval (or for a non-sensitive tool).
        let input = interpolate_outputs(&step.description, &step_ctx.previous_outputs);

        match step.tool_hint.as_deref() {
            // LLM step: routed to the backend specified by model_hint.
            // previous outputs injected into the system message.
            Some("llm") | None => {
                self.execute_llm_step(step, input, llm_router, step_ctx)
                    .await
            }
            // Tool step: model_hint ignored. The invocation is wrapped by the
            // ResilienceLayer so a flaky tool trips its circuit breaker and
            // transient failures are retried with backoff before bubbling up.
            Some(tool_name) => {
                resilience.ensure_tool(tool_name);
                let policy = RetryPolicy::default();
                let payload = serde_json::json!({ "input": input });
                let (outcome, _attempts) = resilience
                    .execute_with_observability(
                        RetryContext {
                            tool_name,
                            tool_call_id: step.step_id.as_str(),
                            retry_policy: &policy,
                            bus: Some(&self.event_bus),
                        },
                        Self::classify_tool_error,
                        || tool_proxy.invoke(tool_name, &payload),
                    )
                    .await;
                outcome.map_err(|e| StepError::ToolCallFailed(e.to_string()))
            }
        }
    }

    /// Maps a `ToolProxyTrait::invoke` error message to the [`ErrorClass`] that
    /// drives circuit-breaker and retry decisions.
    ///
    /// Tool invocations return their error as a plain `String`, so the class is
    /// inferred from the message. Unknown shapes default to `Transient` so a
    /// genuine transient fault is retried rather than silently dropped; the
    /// circuit breaker still bounds repeated transient failures.
    fn classify_tool_error(err: &str) -> ErrorClass {
        let lower = err.to_lowercase();
        if lower.contains("budget") {
            ErrorClass::BudgetExceeded
        } else if lower.contains("sandbox")
            || lower.contains("path traversal")
            || lower.contains("unauthorized")
        {
            ErrorClass::SandboxViolation
        } else if lower.contains("not found")
            || lower.contains("invalid input")
            || lower.contains("invalid argument")
        {
            ErrorClass::Permanent
        } else {
            ErrorClass::Transient
        }
    }

    /// Execute an LLM call for a step, honoring `model_hint`.
    ///
    /// - If `model_hint = Some(hint)` and the backend exists in the `LlmRouter`,
    ///   the call is routed to that backend.
    /// - If `model_hint = Some(hint)` but the backend does not exist, a `tracing::warn!`
    ///   is emitted and the default backend is used as fallback.
    /// - If `model_hint = None`, the default backend is used.
    /// - if previous steps completed, their outputs are formatted into a system
    ///   message `"Previous step results:\n- s1: ..."`.
    async fn execute_llm_step(
        &self,
        step: &PlanStep,
        input: String,
        llm_router: &LlmRouter,
        step_ctx: &StepContext,
    ) -> Result<String, StepError> {
        // Build messages: combine manifest system prompt and previous step outputs into a single
        // system message (preserved verbatim by ContextManager during compaction).
        // Omit the system message entirely when neither the manifest nor previous outputs
        // provide any content, preserving existing behaviour for simple steps.
        let system_text_opt = match (
            self.manifest.system_prompt.as_deref(),
            step_ctx.format_previous_outputs(),
        ) {
            (Some(sp), Some(ctx)) => Some(format!("{sp}\n\n{ctx}")),
            (Some(sp), None) => Some(sp.to_owned()),
            (None, Some(ctx)) => Some(ctx),
            (None, None) => None,
        };
        let mut messages: Vec<ChatMessage> = system_text_opt
            .map(|text| vec![ChatMessage::system(text), ChatMessage::user(input.clone())])
            .unwrap_or_else(|| vec![ChatMessage::user(input)]);

        // Compact context if it approaches the model's context limit.
        let (compacted, was_compacted) = self
            .context_manager
            .maybe_compact(&messages, llm_router)
            .await;
        if was_compacted {
            let summary_chars = compacted.get(1).map(message_char_len).unwrap_or(0);
            let original_messages = messages.len();
            messages = compacted;
            tracing::info!(
                summary_chars,
                original_messages,
                step_id = %step.step_id,
                "step context compacted before LLM call"
            );
            let _ = self
                .event_bus
                .send(apollia_core::RuntimeEvent::ContextCompacted {
                    summary_chars,
                    original_messages,
                });
        }

        let request = CompletionRequest {
            messages,
            ..Default::default()
        };

        let backend_name = match &step.model_hint {
            Some(hint) => {
                if llm_router.get(Some(hint)).is_some() {
                    Some(hint.as_str())
                } else {
                    tracing::warn!(
                        step_id = %step.step_id,
                        model_hint = %hint,
                        "model_hint backend not found, falling back to default"
                    );
                    None
                }
            }
            None => None,
        };

        let obs = LlmObsConfig::default();
        let response = llm_router
            .complete_with_observability(backend_name, request, Some(&self.event_bus), &obs)
            .await
            .map_err(|e| StepError::LlmCallFailed(e.to_string()))?;

        Ok(response.content)
    }

    /// Suspend step execution and wait for the human decision (HITL Orchestrated mode).
    ///
    /// ## Sequence
    ///
    /// 1. Register a oneshot channel in `pending_approvals`, receiver `rx`.
    /// 2. Emit [`RuntimeEvent::TaskInputRequired`] with `step_id: Some(step.step_id)`
    ///    on the `EventBus` to notify the user.
    /// 3. Await `rx.await`: the `ResumeHandler` sends on the sender.
    /// 4. If `approved=true`: `Ok(())`, the step's tool runs normally.
    /// 5. If `approved=false`: `Err(StepError::RejectedByUser { reason })`.
    /// 6. If the channel is closed (runtime shutdown): `Err(StepError::ApprovalChannelClosed)`.
    ///
    /// **StepBudget paused during suspension**: the wait is a pure `await`,
    /// the step counter does not advance during the human suspension.
    async fn suspend_for_approval(
        &self,
        step: &PlanStep,
        pending_approvals: &PendingApprovals,
    ) -> Result<(), StepError> {
        // Registration key: task_id + step_id to identify the suspension precisely.
        let approval_key = format!("{}::{}", self.plan.task_id, step.step_id);

        // 1. Register in PendingApprovals, get rx
        let rx = pending_approvals.register(&approval_key);

        // 2. Emit TaskInputRequired with step_id set (distinguishes Direct / Orchestrated mode)
        let prompt = format!(
            "Approbation requise avant d'exécuter '{}' (step: {})",
            step.tool_hint.as_deref().unwrap_or("llm"),
            step.step_id
        );
        let _ = self.event_bus.send(RuntimeEvent::TaskInputRequired {
            task_id: self.plan.task_id.clone().into(),
            prompt,
            step_id: Some(step.step_id.clone()),
        });

        tracing::info!(
            task_id = %self.plan.task_id,
            step_id = %step.step_id,
            tool = ?step.tool_hint,
            "step suspended - waiting for human approval"
        );

        // 3. Wait for the human decision (pure await: StepBudget does not advance)
        let response = rx.await.map_err(|_| StepError::ApprovalChannelClosed)?;

        tracing::info!(
            task_id = %self.plan.task_id,
            step_id = %step.step_id,
            approved = response.approved,
            "human decision received for step"
        );

        // 4/5. Return based on the decision
        if response.approved {
            Ok(())
        } else {
            Err(StepError::RejectedByUser {
                reason: response
                    .reason
                    .unwrap_or_else(|| "Rejeté par l'utilisateur".into()),
            })
        }
    }

    /// Records an episodic memory entry for a completed step.
    ///
    /// Fire-and-forget: errors are logged as warnings but never interrupt execution.
    /// Skipped silently when `memory_manager` is `None` or when the agent manifest
    /// has no `memory_namespace` configured.
    ///
    /// Output is truncated to [`STEP_MEMORY_OUTPUT_MAX_CHARS`] characters.
    fn record_step_memory(&self, step_id: &str, description: &str, output: &str) {
        // skip if no memory_manager or no namespace configured.
        let mm = match self.memory_manager.as_ref() {
            Some(mm) => mm,
            None => return,
        };
        let namespace = match self.manifest.memory_namespace.as_deref() {
            Some(ns) => ns,
            None => return,
        };

        let truncated_output = truncate_chars(output, self.step_memory_max_chars);
        let content = format!("step {step_id}: {description} -> {truncated_output}");
        let task_id = self.plan.task_id.clone();
        let agent_name = self.manifest.name.clone();
        let namespace_owned = namespace.to_string();
        let metadata = serde_json::json!({
            "source": "oria_orchestrated",
            "step_id": step_id,
        });

        let mm = Arc::clone(mm);
        // Fire-and-forget: spawn_blocking for the sync SQLite write.
        tokio::task::spawn_blocking(move || {
            let mut guard = match mm.lock() {
                Ok(g) => g,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        step_id = %task_id,
                        "failed to acquire memory_manager lock for step memory (ignored)"
                    );
                    return;
                }
            };
            let store = match guard.store(&namespace_owned) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        namespace = %namespace_owned,
                        "failed to open memory store for step memory (ignored)"
                    );
                    return;
                }
            };
            let episodic = apollia_memory::episodic::EpisodicMemory::new(store);
            if let Err(e) = episodic.record(
                &namespace_owned,
                &agent_name,
                &content,
                STEP_MEMORY_IMPORTANCE,
                Some(task_id.as_str()),
                None,
                Some(&metadata),
            ) {
                // warn but don't interrupt execution.
                tracing::warn!(
                    error = %e,
                    namespace = %namespace_owned,
                    "failed to record episodic step memory (ignored)"
                );
            }
        });
    }

    /// Trigger a replan after the retryable failure of a step.
    ///
    /// Increments `replan_count`, emits [`RuntimeEvent::PlanReplanning`],
    /// calls `Reasoner::replan()`, updates the SQLite plan and internal state,
    /// then delegates the rest to [`execute_remaining`](Self::execute_remaining).
    ///
    /// Returns `MAX_REPLAN_EXCEEDED` if the Reasoner fails.
    ///
    /// Returns a boxed `Future` to allow mutual recursion with
    /// [`execute_remaining`](Self::execute_remaining).
    // REASON: replanning needs the failed step, the error, accumulated outputs,
    // the execution deps and the resilience layer; the set is cohesive. A future
    // consolidation may move the resilience layer into the StepDeps bundle.
    #[allow(clippy::too_many_arguments)]
    fn replan_and_continue<'a>(
        &'a mut self,
        failed_step_id: String,
        error_message: String,
        completed_outputs: HashMap<String, String>,
        deps: StepDeps<'a>,
        resilience: &'a ResilienceLayer,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = AIPResult> + Send + 'a>> {
        Box::pin(async move {
            self.replan_count += 1;
            let attempt = self.replan_count;

            let _ = self.event_bus.send(RuntimeEvent::PlanReplanning {
                task_id: self.plan.task_id.clone().into(),
                plan_id: self.plan.plan_id.clone(),
                attempt,
                failed_step: failed_step_id.clone(),
                reason: error_message.clone(),
            });

            // Build a minimal context for the Reasoner.
            let ctx = build_replan_context(&self.plan);

            let new_plan = match deps
                .reasoner
                .replan(&ctx, &completed_outputs, &failed_step_id, &error_message)
                .await
            {
                Ok(p) => p,
                Err(e) => {
                    if let Err(db_err) = self.db.fail_plan(&self.plan.plan_id, &e.to_string()) {
                        tracing::warn!(error = %db_err, "fail_plan DB call failed (ignored)");
                    }
                    let _ = self.event_bus.send(RuntimeEvent::PlanFailed {
                        task_id: self.plan.task_id.clone().into(),
                        plan_id: self.plan.plan_id.clone(),
                        reason: e.to_string(),
                    });
                    return AIPResult::failed("REPLAN_FAILED", &e.to_string());
                }
            };

            // Update SQLite: begin_replan removes pending steps, then we reinsert.
            if let Err(e) = self.db.begin_replan(&self.plan.plan_id, self.replan_count) {
                tracing::warn!(error = %e, "begin_replan DB call failed (ignored)");
            }
            if let Err(e) = self.db.insert_steps(&self.plan.plan_id, &new_plan.steps) {
                tracing::warn!(error = %e, "insert_steps DB call failed (ignored)");
            }

            let _ = self.event_bus.send(RuntimeEvent::PlanGenerated {
                task_id: self.plan.task_id.clone().into(),
                agent_name: String::new(),
                plan_id: self.plan.plan_id.clone(),
                step_count: new_plan.steps.len(),
                run_id: None,
            });

            // Update internal state: keep only completed steps plus the new ones.
            self.plan
                .steps
                .retain(|s| completed_outputs.contains_key(&s.step_id));
            self.plan.steps.extend(new_plan.steps);

            self.execute_remaining(completed_outputs, deps, resilience).await
        }) // end Box::pin
    }

    /// Execute the remaining (not yet completed) steps after a replan.
    ///
    /// Determines the remaining steps by filtering `self.plan.steps` to those absent
    /// from `completed_outputs`, performs a topological sort, then runs each one.
    ///
    /// Returns a boxed `Future` to allow mutual recursion with
    /// [`replan_and_continue`](Self::replan_and_continue).
    fn execute_remaining<'a>(
        &'a mut self,
        mut completed_outputs: HashMap<String, String>,
        deps: StepDeps<'a>,
        resilience: &'a ResilienceLayer,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = AIPResult> + Send + 'a>> {
        Box::pin(async move {
            let remaining: Vec<PlanStep> = self
                .plan
                .steps
                .iter()
                .filter(|s| !completed_outputs.contains_key(&s.step_id))
                .cloned()
                .collect();

            let order = match topological_sort(&remaining) {
                Ok(o) => o,
                Err(_) => {
                    if let Err(e) = self.db.fail_plan(&self.plan.plan_id, "INVALID_REPLAN") {
                        tracing::warn!(error = %e, "fail_plan DB call failed (ignored)");
                    }
                    return AIPResult::failed("INVALID_REPLAN", "Circular dependency in replan");
                }
            };

            for step_id in order {
                let step = match remaining.iter().find(|s| s.step_id == step_id) {
                    Some(s) => s.clone(),
                    None => continue,
                };

                if deps.budget.is_exhausted() {
                    return self
                        .fail_plan_budget_exhausted("Budget épuisé lors de la replanification");
                }

                let step_num = completed_outputs.len() + 1;
                self.persist_step_pre_execution(&step, step_num, &completed_outputs);

                // build StepContext for execute_remaining steps.
                let step_ctx = StepContext {
                    previous_outputs: completed_outputs.clone(),
                    step_index: completed_outputs.len(),
                    total_steps: self.plan.steps.len(),
                    remaining_budget: deps.budget.to_budget_view(),
                };

                let started = Instant::now();
                let result = self
                    .execute_step(&step, &step_ctx, deps.tool_proxy, deps.llm_router, resilience)
                    .await;
                let duration_ms = started.elapsed().as_millis() as u64;
                deps.budget.increment_steps();

                // persist duration unconditionally.
                if let Err(e) =
                    self.db
                        .save_step_duration(&step_id, &self.plan.plan_id, duration_ms as i64)
                {
                    tracing::warn!(error = %e, step_id = %step_id, "save_step_duration DB call failed (ignored)");
                }

                match result {
                    Ok(output) => {
                        self.persist_step_success(&step_id, &output, duration_ms);
                        completed_outputs.insert(step_id, output);
                    }

                    Err(ref e) if e.is_retryable() && self.replan_count < self.max_replans => {
                        self.persist_step_failure(&step_id, &e.to_string());
                        self.emit_step_failed(&step_id, &e.to_string(), true);
                        return self
                            .replan_and_continue(
                                step_id,
                                e.to_string(),
                                completed_outputs,
                                deps,
                                resilience,
                            )
                            .await;
                    }

                    Err(e) => return self.finalize_terminal_failure(&step_id, &e, false),
                }
            }

            if let Err(e) = self.db.complete_plan(&self.plan.plan_id) {
                tracing::warn!(error = %e, "complete_plan DB call failed (ignored)");
            }
            let _ = self.event_bus.send(RuntimeEvent::PlanCompleted {
                task_id: self.plan.task_id.clone().into(),
                plan_id: self.plan.plan_id.clone(),
                step_count: completed_outputs.len(),
                duration_ms: 0,
            });

            AIPResult::completed_with_steps(completed_outputs)
        }) // end Box::pin
    }

    /// Persists the side effects of a successfully completed step.
    ///
    /// Saves the observability output, marks the step complete in SQLite, and
    /// emits [`RuntimeEvent::StepCompleted`]. DB errors are logged and ignored
    /// (fire-and-forget), matching the surrounding execution loops.
    fn persist_step_success(&self, step_id: &str, output: &str, duration_ms: u64) {
        if let Err(e) =
            self.db
                .save_step_output(step_id, &self.plan.plan_id, output, &self.obs_config)
        {
            tracing::warn!(error = %e, step_id = %step_id, "save_step_output DB call failed (ignored)");
        }
        if let Err(e) = self.db.complete_step(&self.plan.plan_id, step_id, output) {
            tracing::warn!(error = %e, step_id = %step_id, "complete_step DB call failed (ignored)");
        }
        let _ = self.event_bus.send(RuntimeEvent::StepCompleted {
            task_id: self.plan.task_id.clone().into(),
            plan_id: self.plan.plan_id.clone(),
            step_id: step_id.to_string(),
            duration_ms,
        });
    }

    /// Persists the common failure prefix for a failed step: saves the error
    /// detail and marks the step failed in SQLite.
    ///
    /// Shared by every error arm before more specific plan-level handling.
    /// DB errors are logged and ignored (fire-and-forget).
    fn persist_step_failure(&self, step_id: &str, error_msg: &str) {
        if let Err(e) = self
            .db
            .save_step_error(step_id, &self.plan.plan_id, error_msg)
        {
            tracing::warn!(error = %e, step_id = %step_id, "save_step_error DB call failed (ignored)");
        }
        if let Err(e) = self.db.fail_step(&self.plan.plan_id, step_id, error_msg) {
            tracing::warn!(error = %e, step_id = %step_id, "fail_step DB call failed (ignored)");
        }
    }

    /// Emits a [`RuntimeEvent::StepFailed`] for `step_id`.
    fn emit_step_failed(&self, step_id: &str, error: &str, retryable: bool) {
        let _ = self.event_bus.send(RuntimeEvent::StepFailed {
            task_id: self.plan.task_id.clone().into(),
            plan_id: self.plan.plan_id.clone(),
            step_id: step_id.to_string(),
            error: error.to_string(),
            retryable,
        });
    }

    /// Marks the plan failed in SQLite with `reason` and emits
    /// [`RuntimeEvent::PlanFailed`]. DB errors are logged and ignored.
    fn fail_plan(&self, reason: &str) {
        if let Err(e) = self.db.fail_plan(&self.plan.plan_id, reason) {
            tracing::warn!(error = %e, "fail_plan DB call failed (ignored)");
        }
        let _ = self.event_bus.send(RuntimeEvent::PlanFailed {
            task_id: self.plan.task_id.clone().into(),
            plan_id: self.plan.plan_id.clone(),
            reason: reason.to_string(),
        });
    }

    /// Handles every terminal step failure (all error arms except the one that
    /// triggers a replan), performing the step- and plan-level persistence and
    /// events, then returning the matching terminal [`AIPResult`].
    ///
    /// Variants handled:
    /// - retryable error with replans exhausted → `MAX_REPLAN_EXCEEDED`
    /// - [`StepError::RejectedByUser`] → `REJECTED`
    /// - [`StepError::ApprovalChannelClosed`] → `APPROVAL_CHANNEL_CLOSED`
    /// - any other permanent error → `STEP_FAILED`
    ///
    /// `prefix_step_in_message` selects the `STEP_FAILED` detail wording: the
    /// initial-pass loops report `"Step {id} failed: {e}"`, while the
    /// post-replan loop reports the bare error string (preserving the original
    /// per-loop messages verbatim).
    fn finalize_terminal_failure(
        &self,
        step_id: &str,
        err: &StepError,
        prefix_step_in_message: bool,
    ) -> AIPResult {
        match err {
            e if e.is_retryable() => {
                // Retryable but replan budget exhausted.
                self.persist_step_failure(step_id, &e.to_string());
                self.emit_step_failed(step_id, &e.to_string(), true);
                self.fail_plan("MAX_REPLAN_EXCEEDED");
                AIPResult::failed(
                    "MAX_REPLAN_EXCEEDED",
                    &format!("{} replanifications dépassées", self.max_replans),
                )
            }
            StepError::RejectedByUser { reason } => {
                self.persist_step_failure(step_id, reason);
                self.fail_plan("REJECTED");
                AIPResult::failed("REJECTED", reason)
            }
            StepError::ApprovalChannelClosed => {
                self.persist_step_failure(step_id, "approval_channel_closed");
                self.fail_plan("APPROVAL_CHANNEL_CLOSED");
                AIPResult::failed(
                    "APPROVAL_CHANNEL_CLOSED",
                    "Approval channel closed - runtime shutting down",
                )
            }
            e => {
                self.persist_step_failure(step_id, &e.to_string());
                self.emit_step_failed(step_id, &e.to_string(), false);
                self.fail_plan(&e.to_string());
                let detail = if prefix_step_in_message {
                    format!("Step {} failed: {}", step_id, e)
                } else {
                    e.to_string()
                };
                AIPResult::failed("STEP_FAILED", &detail)
            }
        }
    }

    /// Returns `true` when every step in `level_steps` is a read-only tool call
    /// that does not require human approval, making the level eligible for
    /// concurrent batch execution. A single-step level is never eligible.
    fn is_batch_eligible(&self, level_steps: &[PlanStep], tool_proxy: &dyn ToolProxyTrait) -> bool {
        level_steps.len() > 1
            && level_steps.iter().all(|s| {
                s.tool_hint.as_deref().is_some_and(|t| {
                    t != "llm"
                        && tool_proxy.is_tool_read_only(t)
                        && !self
                            .manifest
                            .tools_requiring_approval
                            .iter()
                            .any(|a| a == t)
                })
            })
    }

    /// Persists the pre-execution bookkeeping for `step`: emits
    /// [`RuntimeEvent::StepStarted`], marks the step started in SQLite, and
    /// saves the interpolated input and resolved tool name.
    ///
    /// `step_num` is the 1-based ordinal used for the StepStarted event.
    /// DB errors are logged and ignored (fire-and-forget).
    fn persist_step_pre_execution(
        &self,
        step: &PlanStep,
        step_num: usize,
        completed_outputs: &HashMap<String, String>,
    ) {
        let step_id = &step.step_id;
        let total = self.plan.steps.len();
        let _ = self.event_bus.send(RuntimeEvent::StepStarted {
            task_id: self.plan.task_id.clone().into(),
            plan_id: self.plan.plan_id.clone(),
            step_id: step_id.clone(),
            step_num,
            total,
            desc: step.description.clone(),
        });
        if let Err(e) = self.db.start_step(&self.plan.plan_id, step_id) {
            tracing::warn!(error = %e, step_id = %step_id, "start_step DB call failed (ignored)");
        }
        let rendered = interpolate_outputs(&step.description, completed_outputs);
        if let Err(e) =
            self.db
                .save_step_input(step_id, &self.plan.plan_id, &rendered, &self.obs_config)
        {
            tracing::warn!(error = %e, step_id = %step_id, "save_step_input DB call failed (ignored)");
        }
        let actual_tool = step.tool_hint.as_deref().unwrap_or("llm");
        if let Err(e) = self
            .db
            .save_step_tool(step_id, &self.plan.plan_id, actual_tool)
        {
            tracing::warn!(error = %e, step_id = %step_id, "save_step_tool DB call failed (ignored)");
        }
    }

    /// Marks the plan failed with `STEP_BUDGET_EXCEEDED` (DB + event) and
    /// returns the corresponding terminal [`AIPResult`] carrying `detail`.
    fn fail_plan_budget_exhausted(&self, detail: &str) -> AIPResult {
        self.fail_plan("STEP_BUDGET_EXCEEDED");
        AIPResult::failed("STEP_BUDGET_EXCEEDED", detail)
    }
}

// Helpers

/// Interpolate previous step outputs into a step description.
///
/// Replaces each occurrence of `{{step_id}}` with the corresponding step's
/// output content. Unrecognized placeholders are left intact.
///
/// # Example
///
/// ```text
/// "Analyser {{s1}} et {{s2}}" + {s1: "42 pages", s2: "3 images"}
/// -> "Analyser 42 pages et 3 images"
/// ```
pub fn interpolate_outputs(description: &str, outputs: &HashMap<String, String>) -> String {
    let mut result = description.to_string();
    for (step_id, output) in outputs {
        result = result.replace(&format!("{{{{{step_id}}}}}"), output);
    }
    result
}

/// Builds a minimal [`ContextBundle`] for replanning.
///
/// The bundle carries only the plan's `task_id`; the other fields
/// (`memory_snapshot`, `available_tools`, `manifest_system_prompt`) are empty.
/// The Reasoner uses this context to build the replanner prompt.
fn build_replan_context(plan: &ExecutionPlan) -> ContextBundle {
    use apollia_core::task::AIPTask;

    ContextBundle {
        task: AIPTask {
            task_id: plan.task_id.clone(),
            ..AIPTask::default()
        },
        memory_snapshot: None,
        execution_mode: ExecutionMode::Orchestrated,
        available_tools: vec![],
        manifest_system_prompt: None,
        llm_backend_names: vec![],
    }
}

/// Truncates a string to at most `max_chars` Unicode characters.
///
/// If the string exceeds `max_chars`, it is truncated and `"…"` is appended.
/// UTF-8 safe: operates on `char` boundaries, not bytes.
fn truncate_chars(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}…")
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{AgentManifest, PendingApprovals, TaskStatus};
    use apollia_llm::{CompletionRequest, CompletionResponse, FinishReason, LlmError, TokenUsage};
    use std::collections::VecDeque;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};

    /// Builds a minimal `AgentManifest` for tests.
    fn make_manifest() -> AgentManifest {
        serde_json::from_str(
            r#"{"name":"test","version":"0.1.0","description":"test","tools_required":[]}"#,
        )
        .expect("minimal manifest must deserialize")
    }

    // ── Mock ToolProxy ────────────────────────────────────────────────────────

    struct MockToolProxy {
        response: String,
    }

    #[async_trait::async_trait]
    impl ToolProxyTrait for MockToolProxy {
        async fn invoke(
            &self,
            _tool_name: &str,
            _input: &serde_json::Value,
        ) -> Result<String, String> {
            Ok(self.response.clone())
        }
    }

    struct FailingToolProxy;

    #[async_trait::async_trait]
    impl ToolProxyTrait for FailingToolProxy {
        async fn invoke(
            &self,
            _tool_name: &str,
            _input: &serde_json::Value,
        ) -> Result<String, String> {
            Err("tool timeout".to_string())
        }
    }

    // ── Mock CompletionModel ─────────────────────────────────────────────────

    struct MockCompletionModel {
        queue: Mutex<VecDeque<String>>,
    }

    impl MockCompletionModel {
        fn new(responses: Vec<&str>) -> Arc<Self> {
            Arc::new(Self {
                queue: Mutex::new(responses.iter().map(|s| s.to_string()).collect()),
            })
        }
    }

    #[async_trait::async_trait]
    impl apollia_llm::CompletionModel for MockCompletionModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            let content = {
                let mut q = self.queue.lock().expect("mock lock");
                q.pop_front().unwrap_or_else(|| "mock response".to_string())
            };
            Ok(CompletionResponse {
                content,
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
            Pin<Box<dyn futures::Stream<Item = Result<apollia_llm::StreamChunk, LlmError>> + Send>>,
            LlmError,
        > {
            Err(LlmError::InferenceError("mock does not stream".into()))
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

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn make_plan(steps: Vec<(&str, &[&str])>) -> ExecutionPlan {
        ExecutionPlan {
            plan_id: "plan-001".into(),
            task_id: "task-001".into(),
            steps: steps
                .into_iter()
                .map(|(id, deps)| {
                    let mut s = PlanStep::new(id, format!("Step {id}"));
                    s.tool_hint = Some("mock_tool".into());
                    s.depends_on = deps.iter().map(|d| d.to_string()).collect();
                    s
                })
                .collect(),
        }
    }

    fn make_actor(
        plan: ExecutionPlan,
    ) -> (ActorLoop, tokio::sync::broadcast::Receiver<RuntimeEvent>) {
        let (bus_tx, bus_rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");
        let actor = ActorLoop::new(plan, 2, db, bus_tx, make_manifest());
        (actor, bus_rx)
    }

    // ── Sequential ResilienceLayer wiring ─────────────────────────────────────

    /// Builds an actor with an explicit replan cap, so a failing single-step
    /// plan terminates without entering the replan path (isolates the resilience
    /// behaviour of the sequential tool call).
    fn make_actor_capped(
        plan: ExecutionPlan,
        max_replans: u32,
    ) -> (ActorLoop, tokio::sync::broadcast::Receiver<RuntimeEvent>) {
        let (bus_tx, bus_rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");
        let actor = ActorLoop::new(plan, max_replans, db, bus_tx, make_manifest());
        (actor, bus_rx)
    }

    /// Tool proxy that counts invocations and optionally returns a fixed error.
    struct CountingProxy {
        calls: std::sync::atomic::AtomicU32,
        error: Option<String>,
    }

    impl CountingProxy {
        fn ok() -> Self {
            Self {
                calls: std::sync::atomic::AtomicU32::new(0),
                error: None,
            }
        }
        fn failing(msg: &str) -> Self {
            Self {
                calls: std::sync::atomic::AtomicU32::new(0),
                error: Some(msg.to_string()),
            }
        }
        fn call_count(&self) -> u32 {
            self.calls.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl ToolProxyTrait for CountingProxy {
        async fn invoke(&self, _: &str, _: &serde_json::Value) -> Result<String, String> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            match &self.error {
                None => Ok("ok".to_string()),
                Some(m) => Err(m.clone()),
            }
        }
    }

    async fn run_single_tool_step(
        actor: &mut ActorLoop,
        proxy: &CountingProxy,
        resilience: &ResilienceLayer,
    ) -> AIPResult {
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);
        actor
            .execute(
                StepDeps {
                    tool_proxy: proxy,
                    llm_router: &llm,
                    budget: &budget,
                    reasoner: &reasoner,
                },
                resilience,
            )
            .await
    }

    /// A successful tool call records success and keeps the circuit closed.
    #[tokio::test]
    async fn test_sequential_success_records_success() {
        // GIVEN a single-step plan and a proxy that succeeds
        let (mut actor, _rx) = make_actor_capped(make_plan(vec![("s1", &[])]), 0);
        let proxy = CountingProxy::ok();
        let resilience = ResilienceLayer::default();

        // WHEN the sequential path executes
        let result = run_single_tool_step(&mut actor, &proxy, &resilience).await;

        // THEN the tool was invoked once and the breaker stays closed
        assert_eq!(result.status, TaskStatus::Completed);
        assert_eq!(proxy.call_count(), 1);
        let cb = resilience.breaker("mock_tool").expect("breaker registered");
        assert!(matches!(cb.state(), crate::resilience::CircuitState::Closed));
        assert_eq!(cb.failure_count(), 0);
    }

    /// A permanent error is not retried (single invocation).
    #[tokio::test]
    async fn test_sequential_permanent_error_no_retry() {
        // GIVEN a proxy returning a permanent-class error
        let (mut actor, _rx) = make_actor_capped(make_plan(vec![("s1", &[])]), 0);
        let proxy = CountingProxy::failing("invalid input: bad argument");
        let resilience = ResilienceLayer::default();

        // WHEN the sequential path executes
        let result = run_single_tool_step(&mut actor, &proxy, &resilience).await;

        // THEN invoke was called exactly once and the breaker did not count it
        assert_eq!(result.status, TaskStatus::Failed);
        assert_eq!(proxy.call_count(), 1);
        let cb = resilience.breaker("mock_tool").expect("breaker registered");
        assert_eq!(cb.failure_count(), 0);
    }

    /// A budget-class error is not retried.
    #[tokio::test]
    async fn test_sequential_budget_exceeded_no_retry() {
        // GIVEN a proxy returning a budget-class error
        let (mut actor, _rx) = make_actor_capped(make_plan(vec![("s1", &[])]), 0);
        let proxy = CountingProxy::failing("step budget exhausted");
        let resilience = ResilienceLayer::default();

        // WHEN the sequential path executes
        let result = run_single_tool_step(&mut actor, &proxy, &resilience).await;

        // THEN invoke was called exactly once (no retry on BudgetExceeded)
        assert_eq!(result.status, TaskStatus::Failed);
        assert_eq!(proxy.call_count(), 1);
    }

    /// An open circuit rejects the call without invoking the tool.
    #[tokio::test]
    async fn test_sequential_circuit_open_rejects_without_invoke() {
        // GIVEN a resilience layer whose breaker for the tool is already open
        let (mut actor, _rx) = make_actor_capped(make_plan(vec![("s1", &[])]), 0);
        let resilience = ResilienceLayer::default();
        resilience.ensure_tool("mock_tool");
        for _ in 0..3 {
            let _ = resilience.record_failure("mock_tool", &ErrorClass::Transient);
        }
        let proxy = CountingProxy::ok();

        // WHEN the sequential path executes
        let result = run_single_tool_step(&mut actor, &proxy, &resilience).await;

        // THEN the tool was never invoked and the step failed
        assert_eq!(proxy.call_count(), 0);
        assert_eq!(result.status, TaskStatus::Failed);
    }

    /// A transient error is retried up to the policy limit, then recorded.
    #[tokio::test]
    async fn test_sequential_transient_error_retries_then_records_failure() {
        // GIVEN a proxy that always fails with a transient-class error
        let (mut actor, _rx) = make_actor_capped(make_plan(vec![("s1", &[])]), 0);
        let proxy = CountingProxy::failing("tool timeout");
        let resilience = ResilienceLayer::default();

        // WHEN the sequential path executes
        let result = run_single_tool_step(&mut actor, &proxy, &resilience).await;

        // THEN invoke was retried up to the default policy (3 attempts) and the
        // breaker counted one transient failure for the exhausted call
        assert_eq!(result.status, TaskStatus::Failed);
        assert_eq!(proxy.call_count(), RetryPolicy::default().max_attempts);
        let cb = resilience.breaker("mock_tool").expect("breaker registered");
        assert_eq!(cb.failure_count(), 1);
    }

    // ── Batch ResilienceLayer wiring ───────────────────────────────────────────

    /// Read-only tool proxy that counts invocations per tool, tracks peak
    /// concurrency, and can return a configured error for specific tools.
    struct BatchProxy {
        errors: std::collections::HashMap<String, String>,
        calls: std::sync::Mutex<std::collections::HashMap<String, u32>>,
        concurrent: std::sync::atomic::AtomicU32,
        peak: std::sync::atomic::AtomicU32,
        delay_ms: u64,
    }

    impl BatchProxy {
        fn new() -> Self {
            Self {
                errors: std::collections::HashMap::new(),
                calls: std::sync::Mutex::new(std::collections::HashMap::new()),
                concurrent: std::sync::atomic::AtomicU32::new(0),
                peak: std::sync::atomic::AtomicU32::new(0),
                delay_ms: 0,
            }
        }
        fn with_error(mut self, tool: &str, msg: &str) -> Self {
            self.errors.insert(tool.to_string(), msg.to_string());
            self
        }
        fn with_delay(mut self, ms: u64) -> Self {
            self.delay_ms = ms;
            self
        }
        fn calls_for(&self, tool: &str) -> u32 {
            *self.calls.lock().unwrap().get(tool).unwrap_or(&0)
        }
        fn peak(&self) -> u32 {
            self.peak.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl ToolProxyTrait for BatchProxy {
        async fn invoke(&self, tool_name: &str, _: &serde_json::Value) -> Result<String, String> {
            use std::sync::atomic::Ordering::SeqCst;
            let cur = self.concurrent.fetch_add(1, SeqCst) + 1;
            self.peak.fetch_max(cur, SeqCst);
            {
                let mut m = self.calls.lock().unwrap();
                *m.entry(tool_name.to_string()).or_insert(0) += 1;
            }
            if self.delay_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
            }
            self.concurrent.fetch_sub(1, SeqCst);
            match self.errors.get(tool_name) {
                Some(e) => Err(e.clone()),
                None => Ok(format!("ok-{tool_name}")),
            }
        }
        fn is_tool_read_only(&self, _: &str) -> bool {
            true
        }
    }

    fn tool_step(id: &str, tool: &str) -> PlanStep {
        let mut s = PlanStep::new(id, format!("Step {id}"));
        s.tool_hint = Some(tool.to_string());
        s
    }

    /// All active tools are invoked once and returned in input order.
    #[tokio::test]
    async fn test_batch_all_active_tools_invoked_in_order() {
        // GIVEN three read-only tools, all with a closed circuit
        let (actor, _rx) = make_actor(make_plan(vec![("x", &[])]));
        let steps = vec![
            tool_step("s1", "tool_a"),
            tool_step("s2", "tool_b"),
            tool_step("s3", "tool_c"),
        ];
        let proxy = BatchProxy::new();
        let resilience = ResilienceLayer::default();

        // WHEN the batch path executes
        let results = actor
            .execute_tool_steps(&steps, &HashMap::new(), &proxy, &resilience)
            .await;

        // THEN each tool is invoked once and results keep the input order
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].0, "s1");
        assert_eq!(results[1].0, "s2");
        assert_eq!(results[2].0, "s3");
        assert!(results.iter().all(|(_, r)| r.is_ok()));
        assert_eq!(proxy.calls_for("tool_a"), 1);
        assert_eq!(proxy.calls_for("tool_b"), 1);
        assert_eq!(proxy.calls_for("tool_c"), 1);
    }

    /// A tool whose circuit is open is not invoked; its slot returns an error.
    #[tokio::test]
    async fn test_batch_circuit_open_tool_skipped() {
        // GIVEN the circuit for tool_b is already open
        let (actor, _rx) = make_actor(make_plan(vec![("x", &[])]));
        let resilience = ResilienceLayer::default();
        resilience.ensure_tool("tool_b");
        for _ in 0..3 {
            let _ = resilience.record_failure("tool_b", &ErrorClass::Transient);
        }
        let steps = vec![
            tool_step("s1", "tool_a"),
            tool_step("s2", "tool_b"),
            tool_step("s3", "tool_c"),
        ];
        let proxy = BatchProxy::new();

        // WHEN the batch path executes
        let results = actor
            .execute_tool_steps(&steps, &HashMap::new(), &proxy, &resilience)
            .await;

        // THEN tool_b is never invoked and its position carries an error
        assert_eq!(proxy.calls_for("tool_b"), 0);
        assert_eq!(proxy.calls_for("tool_a"), 1);
        assert_eq!(proxy.calls_for("tool_c"), 1);
        assert!(results[0].1.is_ok());
        assert!(results[1].1.is_err());
        assert!(results[2].1.is_ok());
    }

    /// A permanent failure on one tool does not affect the others.
    #[tokio::test]
    async fn test_batch_isolated_failure_does_not_affect_others() {
        // GIVEN tool_c returns a permanent-class error
        let (actor, _rx) = make_actor(make_plan(vec![("x", &[])]));
        let steps = vec![
            tool_step("s1", "tool_a"),
            tool_step("s2", "tool_b"),
            tool_step("s3", "tool_c"),
        ];
        let proxy = BatchProxy::new().with_error("tool_c", "invalid input: nope");
        let resilience = ResilienceLayer::default();

        // WHEN the batch path executes
        let results = actor
            .execute_tool_steps(&steps, &HashMap::new(), &proxy, &resilience)
            .await;

        // THEN only tool_c fails, each tool invoked once (no retry on permanent)
        assert!(results[0].1.is_ok());
        assert!(results[1].1.is_ok());
        assert!(results[2].1.is_err());
        assert_eq!(proxy.calls_for("tool_c"), 1);
    }

    /// Concurrency stays bounded by the configured cap.
    #[tokio::test]
    async fn test_batch_concurrency_cap_respected() {
        // GIVEN 15 slow read-only tools
        let (actor, _rx) = make_actor(make_plan(vec![("x", &[])]));
        let steps: Vec<PlanStep> = (0..15)
            .map(|i| tool_step(&format!("s{i}"), &format!("tool_{i}")))
            .collect();
        let proxy = BatchProxy::new().with_delay(40);
        let resilience = ResilienceLayer::default();

        // WHEN the batch path executes
        let results = actor
            .execute_tool_steps(&steps, &HashMap::new(), &proxy, &resilience)
            .await;

        // THEN all 15 complete, in order, and peak concurrency respects the cap
        assert_eq!(results.len(), 15);
        for (i, (step_id, r)) in results.iter().enumerate() {
            assert_eq!(step_id, &format!("s{i}"));
            assert!(r.is_ok());
        }
        assert!(proxy.peak() <= MAX_CONCURRENT_ORIA_TOOLS as u32);
        assert!(proxy.peak() >= 2, "expected some concurrency");
    }

    /// A transient error is retried up to the policy limit inside the batch.
    #[tokio::test]
    async fn test_batch_transient_error_retries() {
        // GIVEN tool_b always returns a transient-class error
        let (actor, _rx) = make_actor(make_plan(vec![("x", &[])]));
        let steps = vec![tool_step("s1", "tool_a"), tool_step("s2", "tool_b")];
        let proxy = BatchProxy::new().with_error("tool_b", "tool timeout");
        let resilience = ResilienceLayer::default();

        // WHEN the batch path executes
        let results = actor
            .execute_tool_steps(&steps, &HashMap::new(), &proxy, &resilience)
            .await;

        // THEN tool_b is retried up to the default policy and fails; tool_a is ok
        assert!(results[0].1.is_ok());
        assert!(results[1].1.is_err());
        assert_eq!(proxy.calls_for("tool_b"), RetryPolicy::default().max_attempts);
        assert_eq!(proxy.calls_for("tool_a"), 1);
        let cb = resilience.breaker("tool_b").expect("breaker registered");
        assert_eq!(cb.failure_count(), 1);
    }

    // Sequential execution in topological order.

    /// GIVEN a plan (s1, s2->s1, s3->s2) and a mock ToolProxy returning "ok"
    /// WHEN actor.execute() is called
    /// THEN AIPResult::Completed is returned
    ///   AND all 3 steps are in the output
    #[tokio::test]
    async fn test_execution_sequentielle() {
        // GIVEN
        let plan = make_plan(vec![("s1", &[]), ("s2", &["s1"]), ("s3", &["s2"])]);
        let (mut actor, _rx) = make_actor(plan);
        let proxy = MockToolProxy {
            response: "ok".into(),
        };
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);

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

        // THEN
        assert_eq!(
            result.status,
            TaskStatus::Completed,
            "expected Completed, got: {result:?}"
        );
    }

    // StepBudget exhausted at step 3/5.

    /// GIVEN a 5-step plan and a StepBudget with max_steps = 2
    /// WHEN actor.execute() is called
    /// THEN AIPResult::failed("STEP_BUDGET_EXCEEDED", _) is returned
    #[tokio::test]
    async fn test_budget_epuise() {
        // GIVEN
        let plan = make_plan(vec![
            ("s1", &[]),
            ("s2", &[]),
            ("s3", &[]),
            ("s4", &[]),
            ("s5", &[]),
        ]);
        let (mut actor, _rx) = make_actor(plan);
        let proxy = MockToolProxy {
            response: "ok".into(),
        };
        let llm = LlmRouter::empty();
        let budget = StepBudget::with_max(2);
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);

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

        // THEN
        assert_eq!(result.status, TaskStatus::Failed);
        let err = result.error.expect("expected error");
        assert_eq!(
            err.code, "STEP_BUDGET_EXCEEDED",
            "expected STEP_BUDGET_EXCEEDED, got: {}",
            err.code
        );
    }

    // Replan triggered on a retryable step.

    /// GIVEN a plan (s1 ok, s2 fails retryable, s3 pending) and a mock Reasoner
    ///        returning an alternative plan (s2b, s3)
    /// WHEN actor.execute() is called
    /// THEN PlanReplanning { attempt: 1 } is emitted
    ///   AND execution continues with the alternative plan
    #[tokio::test]
    async fn test_replanification_declenchee() {
        // GIVEN
        let plan = make_plan(vec![("s1", &[]), ("s2", &["s1"]), ("s3", &["s2"])]);
        let (bus_tx, mut bus_rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        // Proxy that succeeds for s1, fails for s2
        struct SelectiveProxy {
            fail_next: std::sync::atomic::AtomicBool,
        }
        #[async_trait::async_trait]
        impl ToolProxyTrait for SelectiveProxy {
            async fn invoke(&self, _: &str, _: &serde_json::Value) -> Result<String, String> {
                if self
                    .fail_next
                    .swap(false, std::sync::atomic::Ordering::SeqCst)
                {
                    Err("tool timeout".into())
                } else {
                    Ok("ok".into())
                }
            }
        }
        // s1 succeeds, s2 fails, then s2b/s3 succeed
        let _proxy = SelectiveProxy {
            fail_next: std::sync::atomic::AtomicBool::new(false),
        };
        // Set fail for s2, modifying by position.
        // The default proxy always succeeds; use a failing proxy for s2 only.
        // Simplification: a proxy that fails on the 2nd call.
        struct NthFailProxy {
            call: std::sync::atomic::AtomicU32,
            fail_at: u32,
        }
        #[async_trait::async_trait]
        impl ToolProxyTrait for NthFailProxy {
            async fn invoke(&self, _: &str, _: &serde_json::Value) -> Result<String, String> {
                let n = self.call.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n == self.fail_at {
                    // Permanent-class error so the ResilienceLayer does not retry
                    // the call: the step fails on its single attempt, which is
                    // what drives the replanning path exercised by this test.
                    Err("invalid input: simulated step failure".into())
                } else {
                    Ok(format!("output-{n}"))
                }
            }
        }

        let proxy2 = NthFailProxy {
            call: std::sync::atomic::AtomicU32::new(0),
            fail_at: 1, // s2 is the 2nd call (0-indexed)
        };

        // Alternative plan provided by the mock Reasoner
        let replacement_plan = r#"{"steps":[
            {"step_id":"s2b","description":"Retry step","tool_hint":"mock_tool","depends_on":[]},
            {"step_id":"s3","description":"Final step","tool_hint":"mock_tool","depends_on":["s2b"]}
        ]}"#;
        let model = MockCompletionModel::new(vec![replacement_plan]);
        let reasoner = Reasoner::new(model, 10);
        let mut actor = ActorLoop::new(plan, 2, db, bus_tx, make_manifest());
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();

        // WHEN
        let result = actor
            .execute(
                StepDeps {
                    tool_proxy: &proxy2,
                    llm_router: &llm,
                    budget: &budget,
                    reasoner: &reasoner,
                },
                &resilience,
            )
            .await;

        // THEN: check PlanReplanning on the bus
        let mut found_replanning = false;
        while let Ok(event) = bus_rx.try_recv() {
            if let RuntimeEvent::PlanReplanning { attempt, .. } = event {
                assert_eq!(attempt, 1, "expected attempt=1");
                found_replanning = true;
            }
        }
        assert!(found_replanning, "PlanReplanning event not emitted");
        assert!(
            result.status == TaskStatus::Completed || result.status == TaskStatus::Failed,
            "unexpected status: {:?}",
            result.status
        );
        // Execution was replanned (not MAX_REPLAN nor permanent STEP_FAILED)
        if let Some(ref err) = result.error {
            assert_ne!(
                err.code, "STEP_FAILED",
                "unexpected STEP_FAILED after replan"
            );
        }
    }

    // MAX_REPLAN_EXCEEDED after 2 replans.

    /// GIVEN a plan where every step fails (retryable) and max_replans = 2
    /// WHEN actor.execute() is called
    /// THEN AIPResult::failed("MAX_REPLAN_EXCEEDED", _) is returned
    #[tokio::test]
    async fn test_max_replan_exceeded() {
        // GIVEN
        let plan = make_plan(vec![("s1", &[])]);
        let (bus_tx, _rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        // The Reasoner always returns a plan with a step that will fail.
        // Simulate by providing 3 identical plans (one per replan attempt).
        // Each plan has a step s1 that fails, triggering a replan that fails again.
        let failing_plan = r#"{"steps":[{"step_id":"s1b","description":"retry","tool_hint":"mock_tool","depends_on":[]}]}"#;
        let model = MockCompletionModel::new(vec![failing_plan, failing_plan]);
        let reasoner = Reasoner::new(model, 10);
        let proxy = FailingToolProxy;
        let mut actor = ActorLoop::new(plan, 2, db, bus_tx, make_manifest());
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();

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

        // THEN
        assert_eq!(result.status, TaskStatus::Failed);
        let err = result.error.expect("expected error");
        assert_eq!(
            err.code, "MAX_REPLAN_EXCEEDED",
            "expected MAX_REPLAN_EXCEEDED, got: {}",
            err.code
        );
    }

    // interpolate_outputs

    /// GIVEN a description with {{s1}} and {{s2}} and matching outputs
    /// WHEN interpolate_outputs() is called
    /// THEN the placeholders are replaced by the outputs
    #[test]
    fn test_interpolate_outputs() {
        // GIVEN
        let desc = "Analyser {{s1}} et {{s2}}";
        let mut outputs = HashMap::new();
        outputs.insert("s1".into(), "résultat 1".into());
        outputs.insert("s2".into(), "résultat 2".into());

        // WHEN
        let result = interpolate_outputs(desc, &outputs);

        // THEN
        assert_eq!(result, "Analyser résultat 1 et résultat 2");
    }

    // StepError::is_retryable

    #[test]
    fn test_step_error_is_retryable() {
        assert!(StepError::ToolCallFailed("timeout".into()).is_retryable());
        assert!(StepError::LlmCallFailed("network error".into()).is_retryable());
        assert!(!StepError::NoLlmBackend.is_retryable());
        assert!(!StepError::ToolNotFound("bash".into()).is_retryable());
    }

    // Manifest propagation into ActorLoop.

    /// GIVEN an AgentManifest with tools_requiring_approval=["smtp"]
    /// WHEN an ActorLoop is created with this manifest
    /// THEN self.manifest.tools_requiring_approval contains "smtp"
    #[test]
    fn test_manifest_propagated_to_actor_loop() {
        // GIVEN
        let manifest: AgentManifest = serde_json::from_str(
            r#"{
                "name":"test","version":"0.1.0","description":"test","tools_required":[],
                "tools_requiring_approval":["smtp"]
            }"#,
        )
        .expect("manifest must deserialize");
        let plan = make_plan(vec![("s1", &[])]);
        let (bus_tx, _bus_rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        // WHEN
        let actor = ActorLoop::new(plan, 2, db, bus_tx, manifest);

        // THEN
        assert!(
            actor
                .manifest
                .tools_requiring_approval
                .contains(&"smtp".to_string()),
            "expected 'smtp' in tools_requiring_approval"
        );
    }

    // HITL Orchestrated mode

    /// Build an `AgentManifest` with `tools_requiring_approval` for HITL tests.
    fn make_manifest_with_approval(tools: &[&str]) -> AgentManifest {
        let tools_json = tools
            .iter()
            .map(|t| format!("\"{t}\""))
            .collect::<Vec<_>>()
            .join(",");
        serde_json::from_str(&format!(
            r#"{{"name":"hitl-agent","version":"0.1.0","description":"test","tools_required":[],
               "tools_requiring_approval":[{tools_json}]}}"#
        ))
        .expect("manifest must deserialize")
    }

    /// Build a single-step plan with the given tool.
    fn make_plan_with_tool(step_id: &str, tool_name: &str, task_id: &str) -> ExecutionPlan {
        ExecutionPlan {
            plan_id: format!("plan-{step_id}"),
            task_id: task_id.into(),
            steps: vec![{
                let mut s = PlanStep::new(step_id, format!("Step {step_id} using {tool_name}"));
                s.tool_hint = Some(tool_name.into());
                s
            }],
        }
    }

    // Step with a sensitive tool: suspends before execution.
    //
    // GIVEN a manifest with tools_requiring_approval=["smtp"] and a step "s3" with tool_hint="smtp"
    // WHEN execute() is called WITHOUT resolving the oneshot
    // THEN the "smtp" tool is NOT called before approval,
    //      RuntimeEvent::TaskInputRequired{step_id: Some("s3")} is emitted
    #[tokio::test]
    async fn test_step_sensitive_tool_suspends_before_execution() {
        use std::sync::atomic::{AtomicU32, Ordering};

        // GIVEN
        let manifest = make_manifest_with_approval(&["smtp"]);
        let plan = make_plan_with_tool("s3", "smtp", "task-ac1");
        let (bus_tx, mut bus_rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "hitl-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let pending = Arc::new(PendingApprovals::new());
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        struct CountingProxy(Arc<AtomicU32>);
        #[async_trait::async_trait]
        impl ToolProxyTrait for CountingProxy {
            async fn invoke(
                &self,
                _tool_name: &str,
                _input: &serde_json::Value,
            ) -> Result<String, String> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok("tool output".into())
            }
        }

        let pending_clone = pending.clone();
        let call_count_for_bus = call_count.clone();
        let mut actor = ActorLoop::new(plan, 2, db, bus_tx, manifest)
            .with_pending_approvals(Some(pending.clone()));

        let proxy = CountingProxy(call_count_clone);
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);

        // ActorLoop is !Send (PlanRepository wraps RefCell<Connection>), so use tokio::join!
        // so both futures run on the same task without spawning.
        //
        // The observer future: waits for TaskInputRequired, asserts tool not called yet,
        // then resolves the oneshot to unblock actor.execute().
        let observer_fut = async move {
            let mut found_input_required = false;
            let mut found_step_id = false;
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(500);
            loop {
                match tokio::time::timeout_at(deadline, bus_rx.recv()).await {
                    Ok(Ok(event)) => {
                        if let RuntimeEvent::TaskInputRequired { ref step_id, .. } = event {
                            found_input_required = true;
                            found_step_id = step_id.as_deref() == Some("s3");
                            break;
                        }
                    }
                    _ => break,
                }
            }

            // THEN: the smtp tool has not been called yet
            assert_eq!(
                call_count_for_bus.load(Ordering::SeqCst),
                0,
                "smtp tool must NOT be called before approval"
            );
            assert!(found_input_required, "TaskInputRequired event not emitted");
            assert!(found_step_id, "step_id should be Some(\"s3\")");

            // Resolve to unblock actor.execute()
            let _ = pending_clone.resolve(
                "task-ac1::s3",
                apollia_core::result::InputResponseData {
                    approved: false,
                    reason: Some("test cleanup".into()),
                    context: serde_json::Value::Null,
                    responded_at: "2026-01-01T00:00:00Z".into(),
                },
            );
        };

        tokio::join!(
            actor.execute(
                StepDeps {
                    tool_proxy: &proxy,
                    llm_router: &llm,
                    budget: &budget,
                    reasoner: &reasoner,
                },
                &resilience,
            ),
            observer_fut
        );
    }

    // Approval: step executed normally.
    //
    // GIVEN a step "s3" with tool_hint="smtp" suspended
    // WHEN PendingApprovals.resolve(approved=true)
    // THEN the "smtp" tool is called and the plan completes
    #[tokio::test]
    async fn test_approve_executes_step() {
        use std::sync::atomic::{AtomicU32, Ordering};

        // GIVEN
        let manifest = make_manifest_with_approval(&["smtp"]);
        let plan = make_plan_with_tool("s3", "smtp", "task-ac2");
        let (bus_tx, _bus_rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "hitl-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let pending = Arc::new(PendingApprovals::new());
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        struct CountingProxy(Arc<AtomicU32>);
        #[async_trait::async_trait]
        impl ToolProxyTrait for CountingProxy {
            async fn invoke(
                &self,
                _tool_name: &str,
                _input: &serde_json::Value,
            ) -> Result<String, String> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok("smtp sent".into())
            }
        }

        let pending_clone = pending.clone();
        let mut actor = ActorLoop::new(plan, 2, db, bus_tx, manifest)
            .with_pending_approvals(Some(pending.clone()));

        let proxy = CountingProxy(call_count_clone);
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);

        // ActorLoop is !Send, so use tokio::join! instead of tokio::spawn.
        // The resolver future sleeps briefly then approves, unblocking actor.execute().
        let resolve_fut = async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            pending_clone
                .resolve(
                    "task-ac2::s3",
                    apollia_core::result::InputResponseData {
                        approved: true,
                        reason: None,
                        context: serde_json::Value::Null,
                        responded_at: "2026-01-01T00:00:00Z".into(),
                    },
                )
                .expect("resolve must succeed");
        };

        // WHEN
        let (result, _) = tokio::join!(
            actor.execute(
                StepDeps {
                    tool_proxy: &proxy,
                    llm_router: &llm,
                    budget: &budget,
                    reasoner: &reasoner,
                },
                &resilience,
            ),
            resolve_fut
        );
        let result = result;

        // THEN: the plan completes successfully
        assert_eq!(
            result.status,
            TaskStatus::Completed,
            "plan should complete after approval: {result:?}"
        );
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "smtp tool must be called exactly once after approval"
        );
    }

    // Rejection: plan stopped, AIPResult::failed("REJECTED") returned,
    //           following steps not executed.
    //
    // GIVEN a plan [s1:file_io (non-sensitive), s2:smtp (sensitive)]
    // WHEN the operator rejects s2
    // THEN s2 is not executed, plan returns failed("REJECTED", reason)
    #[tokio::test]
    async fn test_reject_stops_plan() {
        use std::sync::atomic::{AtomicU32, Ordering};

        // GIVEN
        let manifest = make_manifest_with_approval(&["smtp"]);
        let plan = ExecutionPlan {
            plan_id: "plan-ac3".into(),
            task_id: "task-ac3".into(),
            steps: vec![
                {
                    let mut s = PlanStep::new("s1", "Lire fichier");
                    s.tool_hint = Some("file_io".into());
                    s
                },
                {
                    let mut s = PlanStep::new("s2", "Envoyer email");
                    s.tool_hint = Some("smtp".into());
                    s.depends_on = vec!["s1".into()];
                    s
                },
            ],
        };
        let (bus_tx, _bus_rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "hitl-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let pending = Arc::new(PendingApprovals::new());
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        struct CountingProxy(Arc<AtomicU32>);
        #[async_trait::async_trait]
        impl ToolProxyTrait for CountingProxy {
            async fn invoke(
                &self,
                _tool_name: &str,
                _input: &serde_json::Value,
            ) -> Result<String, String> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok("ok".into())
            }
        }

        let pending_clone = pending.clone();
        let mut actor = ActorLoop::new(plan, 2, db, bus_tx, manifest)
            .with_pending_approvals(Some(pending.clone()));

        let proxy = CountingProxy(call_count_clone);
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);

        // ActorLoop is !Send, so use tokio::join! instead of tokio::spawn.
        // The resolver future sleeps briefly then rejects s2, causing the plan to fail.
        let resolve_fut = async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            pending_clone
                .resolve(
                    "task-ac3::s2",
                    apollia_core::result::InputResponseData {
                        approved: false,
                        reason: Some("Email non approuvé".into()),
                        context: serde_json::Value::Null,
                        responded_at: "2026-01-01T00:00:00Z".into(),
                    },
                )
                .expect("resolve must succeed");
        };

        // WHEN
        let (result, _) = tokio::join!(
            actor.execute(
                StepDeps {
                    tool_proxy: &proxy,
                    llm_router: &llm,
                    budget: &budget,
                    reasoner: &reasoner,
                },
                &resilience,
            ),
            resolve_fut
        );
        let result = result;

        // THEN: plan returns Failed with code REJECTED
        assert_eq!(result.status, TaskStatus::Failed);
        let err = result.error.expect("error must be set");
        assert_eq!(err.code, "REJECTED");
        assert!(
            err.message.contains("Email non approuvé"),
            "reason must be propagated: {}",
            err.message
        );
        // s1 (file_io, non-sensitive) ran but s2 (smtp) was rejected before the call
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "only s1 should be called - s2 rejected before tool call"
        );
    }

    // Step with a NON-sensitive tool: no suspension.
    //
    // GIVEN a step using "file_io" absent from tools_requiring_approval
    // WHEN execute() is called with PendingApprovals configured
    // THEN no TaskInputRequired is emitted, the step executes directly
    #[tokio::test]
    async fn test_non_sensitive_tool_no_suspension() {
        // GIVEN
        let manifest = make_manifest_with_approval(&["smtp"]);
        let plan = make_plan_with_tool("s1", "file_io", "task-ac5");
        let (bus_tx, mut bus_rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "hitl-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let pending = Arc::new(PendingApprovals::new());
        let mut actor =
            ActorLoop::new(plan, 2, db, bus_tx, manifest).with_pending_approvals(Some(pending));

        let proxy = MockToolProxy {
            response: "file content".into(),
        };
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);

        // WHEN: execute without any resolve (no suspension expected)
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

        // THEN: the plan completes directly without suspension
        assert_eq!(
            result.status,
            TaskStatus::Completed,
            "non-sensitive tool must execute without suspension: {result:?}"
        );

        // THEN: no TaskInputRequired was emitted
        let mut found = false;
        while let Ok(event) = bus_rx.try_recv() {
            if matches!(event, RuntimeEvent::TaskInputRequired { .. }) {
                found = true;
            }
        }
        assert!(
            !found,
            "TaskInputRequired must NOT be emitted for non-sensitive tool"
        );
    }

    // StepError: the new variants are not retryable.
    #[test]
    fn test_step_error_rejected_and_closed_not_retryable() {
        assert!(!StepError::RejectedByUser {
            reason: "Non".into()
        }
        .is_retryable());
        assert!(!StepError::ApprovalChannelClosed.is_retryable());
    }

    // Multi-model dispatch via model_hint

    /// Build a plan with an LLM step and an optional `model_hint`.
    fn make_llm_plan(step_id: &str, model_hint: Option<&str>) -> ExecutionPlan {
        ExecutionPlan {
            plan_id: "plan-llm".into(),
            task_id: "task-llm".into(),
            steps: vec![{
                let mut s = PlanStep::new(step_id, format!("LLM step {step_id}"));
                s.model_hint = model_hint.map(String::from);
                s
            }],
        }
    }

    /// Mock `CompletionModel` that tags its response with the backend name,
    /// so tests can verify which backend handled the request.
    struct TaggedMockModel {
        tag: String,
    }

    #[async_trait::async_trait]
    impl apollia_llm::CompletionModel for TaggedMockModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Ok(CompletionResponse {
                content: format!("response-from-{}", self.tag),
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
            Pin<Box<dyn futures::Stream<Item = Result<apollia_llm::StreamChunk, LlmError>> + Send>>,
            LlmError,
        > {
            Err(LlmError::InferenceError("mock does not stream".into()))
        }

        fn is_available(&self) -> bool {
            true
        }

        fn backend_name(&self) -> &str {
            &self.tag
        }

        fn model_id(&self) -> &str {
            "tagged-mock"
        }
    }

    /// Step with model_hint dispatches to the named backend.
    ///
    /// GIVEN an LlmRouter with backends "default" and "fast"
    ///   AND an LLM PlanStep with model_hint = Some("fast")
    /// WHEN actor.execute() is called
    /// THEN the response comes from the "fast" backend
    #[tokio::test]
    async fn test_story227_ac1_model_hint_dispatches_to_named_backend() {
        // GIVEN
        let plan = make_llm_plan("s1", Some("fast"));
        let (bus_tx, _rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let mut backends = HashMap::new();
        backends.insert(
            "default".to_string(),
            Arc::new(TaggedMockModel {
                tag: "default".into(),
            }) as Arc<dyn apollia_llm::CompletionModel>,
        );
        backends.insert(
            "fast".to_string(),
            Arc::new(TaggedMockModel { tag: "fast".into() })
                as Arc<dyn apollia_llm::CompletionModel>,
        );
        let llm = LlmRouter::with_backends(backends, "default");

        let proxy = MockToolProxy {
            response: "unused".into(),
        };
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);
        let mut actor = ActorLoop::new(plan, 0, db, bus_tx, make_manifest());

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

        // THEN
        assert_eq!(result.status, TaskStatus::Completed);
        let output_debug = format!("{:?}", result.output);
        assert!(
            output_debug.contains("response-from-fast"),
            "expected output from 'fast' backend, got: {output_debug}"
        );
    }

    /// Step with an unknown model_hint falls back to the default with a warning.
    ///
    /// GIVEN an LlmRouter with only a "default" backend
    ///   AND an LLM PlanStep with model_hint = Some("unknown-backend")
    /// WHEN actor.execute() is called
    /// THEN the response comes from the "default" backend (fallback)
    #[tokio::test]
    async fn test_story227_ac2_unknown_model_hint_falls_back_to_default() {
        // GIVEN
        let plan = make_llm_plan("s1", Some("unknown-backend"));
        let (bus_tx, _rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let mut backends = HashMap::new();
        backends.insert(
            "default".to_string(),
            Arc::new(TaggedMockModel {
                tag: "default".into(),
            }) as Arc<dyn apollia_llm::CompletionModel>,
        );
        let llm = LlmRouter::with_backends(backends, "default");

        let proxy = MockToolProxy {
            response: "unused".into(),
        };
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);
        let mut actor = ActorLoop::new(plan, 0, db, bus_tx, make_manifest());

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

        // THEN: fallback to default, no error
        assert_eq!(result.status, TaskStatus::Completed);
        let output_debug = format!("{:?}", result.output);
        assert!(
            output_debug.contains("response-from-default"),
            "expected output from 'default' backend (fallback), got: {output_debug}"
        );
    }

    /// Step without model_hint uses the default (backward compatible).
    ///
    /// GIVEN an LlmRouter with backends "default" and "fast"
    ///   AND an LLM PlanStep with model_hint = None
    /// WHEN actor.execute() is called
    /// THEN the response comes from the "default" backend
    #[tokio::test]
    async fn test_story227_ac3_no_model_hint_uses_default() {
        // GIVEN
        let plan = make_llm_plan("s1", None);
        let (bus_tx, _rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let mut backends = HashMap::new();
        backends.insert(
            "default".to_string(),
            Arc::new(TaggedMockModel {
                tag: "default".into(),
            }) as Arc<dyn apollia_llm::CompletionModel>,
        );
        backends.insert(
            "fast".to_string(),
            Arc::new(TaggedMockModel { tag: "fast".into() })
                as Arc<dyn apollia_llm::CompletionModel>,
        );
        let llm = LlmRouter::with_backends(backends, "default");

        let proxy = MockToolProxy {
            response: "unused".into(),
        };
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);
        let mut actor = ActorLoop::new(plan, 0, db, bus_tx, make_manifest());

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

        // THEN
        assert_eq!(result.status, TaskStatus::Completed);
        let output_debug = format!("{:?}", result.output);
        assert!(
            output_debug.contains("response-from-default"),
            "expected output from 'default' backend, got: {output_debug}"
        );
    }

    /// Tool-type steps ignore model_hint.
    ///
    /// GIVEN a PlanStep with tool_hint = Some("bash_executor") and model_hint = Some("fast")
    /// WHEN actor.execute() is called
    /// THEN the tool runs normally (model_hint ignored)
    #[tokio::test]
    async fn test_story227_ac4_tool_step_ignores_model_hint() {
        // GIVEN: tool step with model_hint set
        let plan = ExecutionPlan {
            plan_id: "plan-tool".into(),
            task_id: "task-tool".into(),
            steps: vec![{
                let mut s = PlanStep::new("s1", "Tool step");
                s.tool_hint = Some("bash_executor".into());
                s.model_hint = Some("fast".into());
                s
            }],
        };
        let (bus_tx, _rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let proxy = MockToolProxy {
            response: "tool-output".into(),
        };
        // Empty LlmRouter: if model_hint were used, this would fail
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);
        let mut actor = ActorLoop::new(plan, 0, db, bus_tx, make_manifest());

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

        // THEN: the tool runs normally, model_hint ignored
        assert_eq!(result.status, TaskStatus::Completed);
        let output_debug = format!("{:?}", result.output);
        assert!(
            output_debug.contains("tool-output"),
            "expected tool output, got: {output_debug}"
        );
    }

    // ── StepContext per-step observation ─────────────────────────

    // Step with dependency receives the output of the predecessor.
    //
    // GIVEN a plan with s1 and s2 depending on s1
    // WHEN s1 completes with output "result_A"
    // THEN s2 receives a StepContext with previous_outputs = {"s1": "result_A"}
    #[tokio::test]
    async fn test_story229_ac1_step_with_dependency_receives_previous_output() {
        // GIVEN: plan: s1 -> s2 (s2 depends on s1).
        // A capturing proxy records the input it receives.
        struct CapturingProxy {
            calls: Mutex<Vec<(String, String)>>,
        }
        #[async_trait::async_trait]
        impl ToolProxyTrait for CapturingProxy {
            async fn invoke(
                &self,
                tool_name: &str,
                input: &serde_json::Value,
            ) -> Result<String, String> {
                let input_str = input.to_string();
                self.calls
                    .lock()
                    .expect("lock")
                    .push((tool_name.to_string(), input_str));
                Ok(format!("result_{tool_name}"))
            }
        }

        let plan = ExecutionPlan {
            plan_id: "plan-229-ac1".into(),
            task_id: "task-229-ac1".into(),
            steps: vec![
                {
                    let mut s = PlanStep::new("s1", "Step s1");
                    s.tool_hint = Some("tool_a".into());
                    s
                },
                {
                    let mut s = PlanStep::new("s2", "Combine {{s1}}");
                    s.tool_hint = Some("tool_b".into());
                    s.depends_on = vec!["s1".into()];
                    s
                },
            ],
        };

        let (bus_tx, _rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let proxy = CapturingProxy {
            calls: Mutex::new(vec![]),
        };
        let mut actor = ActorLoop::new(plan, 0, db, bus_tx, make_manifest());
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);

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

        // THEN: completed, and s2 received the interpolated output from s1.
        assert_eq!(result.status, TaskStatus::Completed);
        let calls = proxy.calls.lock().expect("lock");
        assert_eq!(calls.len(), 2);
        // s2's input should have {{s1}} replaced with "result_tool_a"
        assert!(
            calls[1].1.contains("result_tool_a"),
            "s2 input should contain s1 output, got: {}",
            calls[1].1
        );
    }

    // Step without dependency receives an empty StepContext.
    //
    // GIVEN a plan with a single step s1 without dependencies
    // WHEN s1 starts
    // THEN s1 receives a StepContext with previous_outputs = {}
    #[tokio::test]
    async fn test_story229_ac2_step_without_dependency_receives_empty_context() {
        // We verify this by checking that a standalone LLM step does NOT receive
        // a system message with "Previous step results:" (since context is empty).
        struct CapturingModel {
            received: Mutex<Vec<Vec<ChatMessage>>>,
        }
        #[async_trait::async_trait]
        impl apollia_llm::CompletionModel for CapturingModel {
            async fn complete(
                &self,
                req: CompletionRequest,
            ) -> Result<apollia_llm::CompletionResponse, apollia_llm::LlmError> {
                self.received
                    .lock()
                    .expect("lock")
                    .push(req.messages.clone());
                Ok(apollia_llm::CompletionResponse {
                    content: "llm output".into(),
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
                Pin<
                    Box<
                        dyn futures::Stream<
                                Item = Result<apollia_llm::StreamChunk, apollia_llm::LlmError>,
                            > + Send,
                    >,
                >,
                apollia_llm::LlmError,
            > {
                Err(apollia_llm::LlmError::InferenceError(
                    "mock does not stream".into(),
                ))
            }
            fn is_available(&self) -> bool {
                true
            }
            fn backend_name(&self) -> &str {
                "capturing-mock"
            }
            fn model_id(&self) -> &str {
                "capturing-model"
            }
        }

        let model = Arc::new(CapturingModel {
            received: Mutex::new(vec![]),
        });
        let model_for_check = model.clone();

        // Single LLM step (tool_hint = None → LLM path).
        let plan = ExecutionPlan {
            plan_id: "plan-229-ac2".into(),
            task_id: "task-229-ac2".into(),
            steps: vec![PlanStep::new("s1", "Summarize the document")],
        };

        let (bus_tx, _rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let proxy = MockToolProxy {
            response: "unused".into(),
        };
        let mut backends = HashMap::new();
        backends.insert(
            "capturing-mock".to_string(),
            model as Arc<dyn apollia_llm::CompletionModel>,
        );
        let llm = LlmRouter::with_backends(backends, "capturing-mock");
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);

        let mut actor = ActorLoop::new(plan, 0, db, bus_tx, make_manifest());

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

        // THEN: completed, and the LLM received only a user message (no system context).
        assert_eq!(result.status, TaskStatus::Completed);
        let received = model_for_check.received.lock().expect("lock");
        assert_eq!(received.len(), 1, "expected 1 LLM call");
        let messages = &received[0];
        // No system message should be present since previous_outputs is empty.
        assert_eq!(
            messages.len(),
            1,
            "expected only 1 message (user), got: {messages:?}"
        );
    }

    // Budget view reflects consumed steps.
    //
    // GIVEN a budget with max_steps = 10 and 3 steps already consumed
    // WHEN StepContext is built for the 4th step
    // THEN remaining_budget reflects 7 steps remaining
    #[test]
    fn test_story229_ac3_budget_view_reflects_consumed_steps() {
        // GIVEN
        let budget = StepBudget::with_max(10);
        budget.increment_steps();
        budget.increment_steps();
        budget.increment_steps();

        // WHEN
        let view = budget.to_budget_view();

        // THEN: the view should NOT be exhausted (7 steps remain).
        assert!(
            !view.is_exhausted(),
            "budget should not be exhausted with 7 steps remaining"
        );
    }

    // ── Per-step episodic memory ─────────────────────────────────

    /// Helper: creates a manifest with `memory_namespace` set.
    fn make_manifest_with_memory(namespace: &str) -> AgentManifest {
        let json = format!(
            r#"{{"name":"test-agent","version":"0.1.0","description":"test","tools_required":[],"memory_namespace":"{namespace}"}}"#,
        );
        serde_json::from_str(&json).expect("manifest with memory_namespace must deserialize")
    }

    /// Helper: creates an ActorLoop with a real MemoryManager backed by a temp dir.
    fn make_actor_with_memory(
        plan: ExecutionPlan,
        manifest: AgentManifest,
        memory_dir: &std::path::Path,
    ) -> (ActorLoop, tokio::sync::broadcast::Receiver<RuntimeEvent>) {
        let (bus_tx, bus_rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, &manifest.name).expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let mm = apollia_memory::manager::MemoryManager::new(
            memory_dir,
            manifest.memory_namespace.clone(),
            vec![],
        );
        let mm = Arc::new(Mutex::new(mm));

        let actor = ActorLoop::new(plan, 2, db, bus_tx, manifest).with_memory_manager(Some(mm));
        (actor, bus_rx)
    }

    // Episodic entry created after step completion.
    //
    // GIVEN an agent with memory_namespace configured and a plan of 3 steps
    // WHEN all steps complete successfully
    // THEN an episodic entry exists for each step in the agent's namespace
    #[tokio::test]
    async fn test_story230_ac1_episodic_entry_after_step() {
        // GIVEN
        let tmp = tempfile::tempdir().expect("tempdir");
        let manifest = make_manifest_with_memory("test-ns");
        let plan = make_plan(vec![("s1", &[]), ("s2", &["s1"]), ("s3", &["s2"])]);
        let (mut actor, _rx) = make_actor_with_memory(plan, manifest, tmp.path());
        let proxy = MockToolProxy {
            response: "analysis done".into(),
        };
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);

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

        // THEN: plan completed
        assert_eq!(result.status, TaskStatus::Completed);

        // Wait briefly for spawn_blocking tasks to complete.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Verify episodic entries exist via MemoryManager.
        let mm = actor.memory_manager.as_ref().expect("memory_manager set");
        let mut guard = mm.lock().expect("lock");
        let store = guard.store("test-ns").expect("open store");
        let episodic = apollia_memory::episodic::EpisodicMemory::new(store);
        let entries = episodic.history("test-ns", 100, None).expect("history");

        assert_eq!(
            entries.len(),
            3,
            "expected 3 episodic entries, got {}",
            entries.len()
        );
        // Verify content contains step id and output.
        assert!(
            entries.iter().any(|e| e.content.contains("step s1:")),
            "expected entry for s1"
        );
        assert!(
            entries.iter().any(|e| e.content.contains("analysis done")),
            "expected output in entry"
        );
    }

    // No write when memory_namespace is None.
    //
    // GIVEN an agent without memory_namespace
    // WHEN steps complete
    // THEN no memory write is attempted (no crash, no error)
    #[tokio::test]
    async fn test_story230_ac2_no_write_without_namespace() {
        // GIVEN: default manifest has no memory_namespace
        let plan = make_plan(vec![("s1", &[]), ("s2", &["s1"])]);
        let (mut actor, _rx) = make_actor(plan);
        let proxy = MockToolProxy {
            response: "ok".into(),
        };
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);

        // WHEN: execute completes without memory_manager, no crash.
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

        // THEN
        assert_eq!(result.status, TaskStatus::Completed);
        assert!(
            actor.memory_manager.is_none(),
            "memory_manager should be None for default actor"
        );
    }

    // Memory write failure does not block execution.
    //
    // GIVEN a memory_manager pointing to an invalid/read-only path
    // WHEN steps complete and the memory write fails
    // THEN the plan still completes successfully (warning logged)
    #[tokio::test]
    async fn test_story230_ac3_memory_failure_does_not_block() {
        // GIVEN: use a non-existent directory that will cause SQLite to fail
        let manifest = make_manifest_with_memory("test-ns");
        let bad_path = std::path::PathBuf::from("/nonexistent/path/that/cannot/exist");
        let mm = apollia_memory::manager::MemoryManager::new(
            &bad_path,
            Some("test-ns".to_string()),
            vec![],
        );
        let mm = Arc::new(Mutex::new(mm));

        let plan = make_plan(vec![("s1", &[]), ("s2", &["s1"])]);
        let (bus_tx, _rx) = tokio::sync::broadcast::channel(64);
        let db = PlanRepository::new(":memory:").expect("in-memory DB");
        db.insert_plan(&plan, "test-agent").expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");
        let mut actor = ActorLoop::new(plan, 2, db, bus_tx, manifest).with_memory_manager(Some(mm));

        let proxy = MockToolProxy {
            response: "ok".into(),
        };
        let llm = LlmRouter::empty();
        let budget = StepBudget::unlimited();
        let resilience = ResilienceLayer::default();
        let reasoner = Reasoner::new(MockCompletionModel::new(vec![]), 10);

        // WHEN: memory write will fail but execution should continue
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

        // THEN: plan completed despite memory failure
        assert_eq!(
            result.status,
            TaskStatus::Completed,
            "plan should complete even if memory write fails"
        );
    }
}
