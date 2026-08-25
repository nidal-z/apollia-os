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

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::Instant;

use apollia_core::{
    AIPPart, AIPResult, AIPTask, AgentManifest, AutonomyLevel, AutonomyLevelConfig, DataPart,
    EventBusSender, ORIAConfig, PendingApprovals, RuntimeEvent, StepBudgetConfig, TaskStatus,
};
use apollia_llm::{CompletionModel, LlmRouter};
use apollia_memory::manager::MemoryManager;

use apollia_workspace::ProjectRuntime;

use crate::actor::{ActorLoop, StepDeps, ToolProxyTrait};
use crate::budget::StepBudget;
use crate::context_manager::ContextManager;
use crate::observer::{classify, ContextBundle, ExecutionMode, ObserverError};
use crate::plan::ExecutionPlan;
use crate::plan_cache::{compute_cache_key, PlanCacheRepository};
use crate::plan_gate::{PendingPlanGates, PlanGateDecision};
use crate::plan_repository::PlanRepository;
use crate::reasoner::{Reasoner, ReasonerError};
use crate::resilience::ResilienceLayer;
use crate::verification::{
    run_post_run_verification, verdict_feedback, CriticPass, VerificationLoop,
};

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
struct NoopToolProxy;

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
    reasoner: Option<Reasoner>,
    tool_proxy: Option<Arc<dyn ToolProxyTrait>>,
    llm_router: LlmRouter,
    resilience: ResilienceLayer,
    event_bus: EventBusSender,
    runtime_config: StepBudgetConfig,
    /// ORIA engine configuration injected from `apollia.toml`.
    oria_config: ORIAConfig,
    db_path: Option<String>,
    /// HITL registry of pending approvals, shared with the `ResumeHandler`.
    ///
    /// Required for `execute_direct()` to suspend the task and wait for the
    /// human decision. When `None`, `InputRequired` results are returned
    /// as-is without suspension.
    pending_approvals: Option<Arc<PendingApprovals>>,
    /// Plan-gate registry, shared with the consumer that submits the decision.
    ///
    /// When `Some` and the gate is active, the engine suspends after plan
    /// generation, registers a oneshot here, and awaits an approve/reject
    /// decision before starting the `ActorLoop`. When `None`, the gate cannot
    /// suspend and execution proceeds directly.
    pending_plan_gates: Option<Arc<PendingPlanGates>>,
    /// Per-run plan-gate override.
    ///
    /// `Some(true)` forces the gate active, `Some(false)` bypasses it, `None`
    /// defers to the autonomy tier. Set by the operator (CLI `run --plan`); other
    /// submission paths leave it `None` so the tier governs.
    plan_gate_override: Option<bool>,
    /// HITL SQLite repository, persists the prompt and context on suspension.
    ///
    /// When `None`, persistence is skipped (logged warning) but execution continues.
    task_repository: Option<Arc<apollia_tools::TaskRepository>>,
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
    memory_manager: Option<Arc<Mutex<MemoryManager>>>,
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
    plan_cache: Option<Arc<Mutex<PlanCacheRepository>>>,
    /// Workspace context assembler with a TTL cache.
    ///
    /// Collects the git branch, file status, and `APOLLIA.md` content at the
    /// start of an orchestrated task, then injects the result into the system prompt.
    workspace_assembler: ProjectRuntime,
    /// Working directory used as the root for workspace collection.
    ///
    /// Initialized to `"."` by default; overridable via [`with_cwd`](ORIAEngine::with_cwd).
    cwd: PathBuf,
    /// LLM context-window manager, compacts history when needed.
    ///
    /// Initialized from `ORIAConfig::context_compact_threshold` and
    /// `ORIAConfig::context_summary_max_chars`. Passed to the `ActorLoop` during
    /// orchestrated execution to protect long LLM steps.
    context_manager: ContextManager,
}

impl ORIAEngine {
    /// Create an `ORIAEngine` with default values (Mode Direct only).
    ///
    /// To enable Mode Orchestrated, chain with [`with_reasoner`].
    /// To enable HITL, chain with [`with_pending_approvals`] and [`with_task_repository`].
    ///
    /// [`with_reasoner`]: ORIAEngine::with_reasoner
    /// [`with_pending_approvals`]: ORIAEngine::with_pending_approvals
    /// [`with_task_repository`]: ORIAEngine::with_task_repository
    pub fn new() -> Self {
        let (event_bus, _) = tokio::sync::broadcast::channel(64);
        let oria_config = ORIAConfig::default();
        let context_manager = ContextManager::from_config(&oria_config);
        Self {
            reasoner: None,
            tool_proxy: None,
            llm_router: LlmRouter::empty(),
            resilience: ResilienceLayer::new(3, Duration::from_secs(30)),
            event_bus,
            runtime_config: StepBudgetConfig::default(),
            oria_config,
            db_path: None,
            pending_approvals: None,
            pending_plan_gates: None,
            plan_gate_override: None,
            task_repository: None,
            memory_manager: None,
            plan_cache: None,
            workspace_assembler: ProjectRuntime::default_project(),
            cwd: PathBuf::from("."),
            context_manager,
        }
    }

    /// Configure the `ORIAEngine` with an LLM to enable Mode Orchestrated.
    ///
    /// `max_steps` bounds the size of plans generated by the Reasoner
    /// (non-negotiable safety guardrail).
    pub fn with_reasoner(mut self, model: Arc<dyn CompletionModel>, max_steps: u32) -> Self {
        self.reasoner = Some(Reasoner::new(model, max_steps));
        self
    }

    /// Returns `true` when a [`Reasoner`] has been wired via
    /// [`with_reasoner`](Self::with_reasoner) or
    /// [`with_llm_router_and_reasoner`](Self::with_llm_router_and_reasoner).
    ///
    /// The runtime relies on this to fail orchestrated tasks with a
    /// stable `NO_LLM` code at `engine.execute()` time: without this check,
    /// orchestrated agents would fall through to NO_HANDLER because the engine
    /// was never wired with a Reasoner.
    pub fn has_reasoner(&self) -> bool {
        self.reasoner.is_some()
    }

    /// Configure the `ToolProxy` for executing orchestrated steps.
    ///
    /// Without a `ToolProxy`, steps with a `tool_hint` fail via `NoopToolProxy`.
    pub fn with_tool_proxy(mut self, proxy: Arc<dyn ToolProxyTrait>) -> Self {
        self.tool_proxy = Some(proxy);
        self
    }

    /// Configure the `LlmRouter` for LLM synthesis in orchestrated steps.
    pub fn with_llm_router(mut self, router: LlmRouter) -> Self {
        self.llm_router = router;
        self
    }

    /// Configure the LLM router and instantiate the Reasoner from the precise backend.
    ///
    /// Combines [`with_llm_router`](Self::with_llm_router) and [`with_reasoner`](Self::with_reasoner):
    /// selects `route_precise()` for orchestrated planning, then stores the router
    /// for step-level LLM calls.
    ///
    /// # Errors
    ///
    /// - [`apollia_llm::LlmError::RoutingConfigMissing`]: `[llm.routing]` missing from the config.
    /// - [`apollia_llm::LlmError::BackendNotFound`]: `precise` backend not found in the router.
    pub fn with_llm_router_and_reasoner(
        mut self,
        router: LlmRouter,
        max_steps: u32,
    ) -> Result<Self, apollia_llm::LlmError> {
        let model = router.route_precise()?;
        self.reasoner = Some(Reasoner::new(model, max_steps));
        self.llm_router = router;
        Ok(self)
    }

    /// Inject an `EventBusSender` to broadcast plan events on the bus.
    pub fn with_event_bus(mut self, bus: EventBusSender) -> Self {
        self.event_bus = bus;
        self
    }

    /// Configure the global runtime budget (cap applied via `StepBudget::from_capped`).
    pub fn with_runtime_config(mut self, config: StepBudgetConfig) -> Self {
        self.runtime_config = config;
        self
    }

    /// Inject the ORIA configuration read from `apollia.toml`.
    ///
    /// If not called, [`ORIAConfig::default`] is used (`max_replans = 2`).
    /// The `max_replans` value controls how many re-plans are allowed in
    /// Orchestrated mode before failing permanently.
    /// Also updates the `ContextManager` with the configured compaction thresholds.
    pub fn with_oria_config(mut self, config: ORIAConfig) -> Self {
        self.context_manager = ContextManager::from_config(&config);
        self.oria_config = config;
        self
    }

    /// Configure the SQLite path for execution-plan persistence.
    ///
    /// If absent, a `:memory:` fallback is used (no persistence across restarts).
    pub fn with_db_path(mut self, path: impl Into<String>) -> Self {
        self.db_path = Some(path.into());
        self
    }

    /// Inject the HITL registry of pending approvals.
    ///
    /// Required for `execute_direct()` to suspend the task in `input_required` status
    /// and wait for the human decision through a oneshot channel.
    /// Shared between the `ORIAEngine` and the REST routes via `AppState`.
    pub fn with_pending_approvals(mut self, pending: Arc<PendingApprovals>) -> Self {
        self.pending_approvals = Some(pending);
        self
    }

    /// Inject the plan-gate registry, shared with the decision consumer.
    ///
    /// Required for an active gate to suspend the run after plan generation and
    /// await an approve/reject decision. Shared between the `ORIAEngine` and the
    /// REST routes via `AppState`.
    pub fn with_pending_plan_gates(mut self, gates: Arc<PendingPlanGates>) -> Self {
        self.pending_plan_gates = Some(gates);
        self
    }

    /// Force the plan gate active for every orchestrated run.
    ///
    /// Convenience for `with_plan_gate_override(Some(force))`: independent of the
    /// autonomy tier, `true` always gates and `false` always bypasses.
    pub fn with_force_plan_gate(mut self, force: bool) -> Self {
        self.plan_gate_override = Some(force);
        self
    }

    /// Set the per-run plan-gate override.
    ///
    /// `Some(true)` forces the gate, `Some(false)` bypasses it, `None` defers to
    /// the autonomy tier.
    pub fn with_plan_gate_override(mut self, override_: Option<bool>) -> Self {
        self.plan_gate_override = override_;
        self
    }

    /// Inject the HITL SQLite repository to persist the prompt and context.
    ///
    /// If absent, SQLite persistence is skipped but HITL execution continues
    /// (logged warning: fail fast only for detectable errors).
    pub fn with_task_repository(mut self, repo: Arc<apollia_tools::TaskRepository>) -> Self {
        self.task_repository = Some(repo);
        self
    }

    /// Inject a [`MemoryManager`] for per-step episodic recording.
    ///
    /// Passed to the [`ActorLoop`] during orchestrated execution. Each completed step
    /// automatically records an episodic entry in the agent's namespace.
    pub fn with_memory_manager(mut self, mm: Arc<Mutex<MemoryManager>>) -> Self {
        self.memory_manager = Some(mm);
        self
    }

    /// Add the plan cache to the engine, shared with the rest of the runtime.
    ///
    /// [`execute_orchestrated_plan`] then checks it before calling the Reasoner:
    /// a hit avoids the LLM call, clones the plan with a new `plan_id`, and emits
    /// [`RuntimeEvent::PlanCacheHit`]. Cache errors are logged at `warn` without
    /// blocking execution.
    ///
    /// The repository is borrowed, not owned, because the supervisor opens it at
    /// boot and hands the same handle to the REST stats and clear routes. The
    /// builder this replaces took ownership, which no caller could satisfy, and
    /// for want of that signature the engine ran with no cache at all: every
    /// orchestrated run re-planned from scratch while `plan cache stats` reported
    /// an empty cache, which was true and read as "nothing cached yet".
    ///
    /// [`execute_orchestrated_plan`]: ORIAEngine::execute_orchestrated_plan
    pub fn with_shared_plan_cache(mut self, repo: Arc<Mutex<PlanCacheRepository>>) -> Self {
        self.plan_cache = Some(repo);
        self
    }

    /// Configure the working directory for workspace context collection.
    ///
    /// Used by `execute_orchestrated_plan` to collect the git branch, file status,
    /// and `APOLLIA.md` content before each execution.
    pub fn with_cwd(mut self, cwd: PathBuf) -> Self {
        self.cwd = cwd;
        self
    }

    /// Create an `ORIAEngine` with a specific working directory.
    ///
    /// Shorthand equivalent to `ORIAEngine::new().with_cwd(cwd)`.
    /// Mainly used in unit tests.
    pub fn new_with_cwd(cwd: PathBuf) -> Self {
        Self::new().with_cwd(cwd)
    }

    /// Collect the workspace context and return the `<context name="...">` blocks.
    ///
    /// Uses [`ProjectRuntime`] with the configured TTL cache to avoid repeated I/O.
    /// Returns an empty string when no context is available (directory outside a git
    /// repo, no `APOLLIA.md`, or the collection timeout was exceeded).
    /// Each section is wrapped in its own `<context name="...">` tag.
    pub async fn build_system_prompt(&self) -> String {
        let snapshot = self.workspace_assembler.collect(&self.cwd).await;
        snapshot.format_for_prompt()
    }

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
            "plan alternatives emitted on EventBus"
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

    /// Full orchestrated execution: plan, persist, ActorLoop, concat.
    ///
    /// Implements the pipeline:
    /// 1. Validate `system_prompt` is present (fail fast)
    /// 2. Generate the plan via `Reasoner` (internal retry x3)
    /// 3. Persist plan + steps in SQLite (non-blocking on error)
    /// 4. Emit `RuntimeEvent::PlanGenerated`
    /// 5. Create `StepBudget::from_capped(manifest, runtime)`
    /// 6. Execute via `ActorLoop`
    /// 7. Concatenate outputs (or stub `on_plan_complete`)
    ///
    /// Whether the plan gate is active for the current run.
    ///
    /// A per-run override (`--plan`) wins when present: `Some(true)` gates,
    /// `Some(false)` bypasses. Without an override the autonomy tier decides; the
    /// tier defaults to `Assisted` (gate active) when unset, so the safe default
    /// is to gate.
    fn plan_gate_active(&self) -> bool {
        if let Some(forced) = self.plan_gate_override {
            return forced;
        }
        let tier = self
            .oria_config
            .autonomy_level
            .unwrap_or(apollia_core::AutonomyLevel::Assisted);
        tier.gate_policy() == apollia_core::GatePolicy::Active
    }

    /// Suspend after plan generation and await an approve/reject decision.
    ///
    /// Registers a oneshot in [`PendingPlanGates`], emits
    /// [`RuntimeEvent::PlanApprovalRequired`], and waits up to
    /// `oria_config.plan_gate_ttl_secs`. No `StepBudget` exists yet, so the
    /// budget cannot progress during the wait.
    ///
    /// # Errors
    ///
    /// - [`ORIAError::PlanGateTimeout`] when no decision arrives within the TTL.
    /// - [`ORIAError::PlanGateChannelClosed`] when the sender is dropped first.
    async fn await_plan_gate(
        &self,
        run_id: &str,
        plan: &ExecutionPlan,
    ) -> Result<PlanGateDecision, ORIAError> {
        let plan_id = &plan.plan_id;
        let ttl_secs = self.oria_config.plan_gate_ttl_secs;
        let gates = match self.pending_plan_gates.as_ref() {
            Some(g) => g,
            None => {
                // No registry wired for this run: the gate cannot collect a
                // decision, so execution proceeds (gate is effectively inactive).
                tracing::debug!(run_id = %run_id, "plan.gate.no_registry");
                return Ok(PlanGateDecision::Approved);
            }
        };

        let rx = gates.register(run_id);
        let _ = self.event_bus.send(RuntimeEvent::PlanApprovalRequired {
            run_id: run_id.to_string(),
            plan_id: plan_id.to_string(),
            task_id: run_id.to_string(),
            step_count: plan.steps.len(),
            steps: plan.steps.clone(),
            ttl_secs,
        });

        match tokio::time::timeout(Duration::from_secs(ttl_secs), rx).await {
            Ok(Ok(decision)) => Ok(decision),
            Ok(Err(_)) => Err(ORIAError::PlanGateChannelClosed {
                run_id: run_id.to_string(),
            }),
            Err(_) => Err(ORIAError::PlanGateTimeout {
                run_id: run_id.to_string(),
                plan_id: plan_id.to_string(),
                ttl_secs,
            }),
        }
    }

    async fn execute_orchestrated_plan(
        &self,
        task: AIPTask,
        agent: &(dyn AIPAgent + Send + Sync),
        manifest: AgentManifest,
    ) -> AIPResult {
        // validate system_prompt
        if manifest.system_prompt.is_none() {
            return AIPResult::failed(
                "MISSING_SYSTEM_PROMPT",
                "execution_mode=orchestrated requires system_prompt in the agent manifest",
            );
        }

        // Collect workspace context and enrich system prompt
        let workspace_block = self.build_system_prompt().await;
        let enriched_system_prompt = if workspace_block.is_empty() {
            manifest.system_prompt.clone()
        } else {
            manifest
                .system_prompt
                .as_ref()
                .map(|sp| format!("{}\n\n{}", sp, workspace_block))
        };

        // Build ContextBundle
        let available_tools: Vec<String> = manifest
            .tools_required
            .iter()
            .chain(manifest.tools_optional.iter())
            .cloned()
            .collect();

        let ctx = ContextBundle {
            task: task.clone(),
            memory_snapshot: None,
            execution_mode: ExecutionMode::Orchestrated,
            available_tools,
            manifest_system_prompt: enriched_system_prompt,
            llm_backend_names: vec![],
        };

        // get reasoner or fail
        let reasoner = match self.reasoner.as_ref() {
            Some(r) => r,
            None => {
                return AIPResult::failed(
                    "NO_LLM",
                    "Orchestrated mode requires a configured LLM (use with_reasoner())",
                )
            }
        };

        // Plan cache lookup
        let task_text = extract_task_text(&task);
        let cache_key = compute_cache_key(
            &manifest.name,
            &manifest.version,
            &ctx.available_tools,
            &task_text,
        );

        if let Some(plan) = self.lookup_cached_plan(&cache_key, &task.task_id) {
            let _ = self.event_bus.send(RuntimeEvent::PlanCacheHit {
                task_id: task.task_id.clone().into(),
                cache_key: cache_key.clone(),
            });

            return self
                .execute_cached_plan(plan, task, agent, manifest, &ctx, &cache_key)
                .await;
        }

        // Generate plan (Reasoner handles retries internally)
        let mut plan = match reasoner.plan(&ctx).await {
            Ok(p) => p,
            Err(e) => return AIPResult::failed("PLAN_FAILED", &e.to_string()),
        };

        // Resolve each tool step's structured arguments so the persisted plan is
        // fully specified before it is cached, audited and executed. Best-effort:
        // unresolved steps are handled just in time.
        self.enrich_plan_with_args(&mut plan).await;

        self.store_plan_in_cache(&cache_key, &plan, &manifest);

        let task_id_str = task.task_id.clone();
        let db_path = self.db_path.as_deref().unwrap_or(":memory:");

        // emit PlanGenerated for the initial plan
        let _ = self.event_bus.send(RuntimeEvent::PlanGenerated {
            task_id: task_id_str.clone().into(),
            agent_name: manifest.name.clone(),
            plan_id: plan.plan_id.clone(),
            step_count: plan.steps.len(),
            // The orchestrated engine path correlates via task_id, not a chat run.
            run_id: None,
        });

        // Plan gate: when active, pause and await an operator decision before any
        // budget is created or step executed. The wait holds no budget (principle
        // #7). On rejection the engine replans with the feedback and re-opens the
        // gate, bounded by plan_gate_max_replans; a timeout or closed channel ends
        // the run cleanly.
        if self.plan_gate_active() {
            let max_replans = self.oria_config.plan_gate_max_replans;
            let mut replans_count: u32 = 0;
            loop {
                let plan_id = plan.plan_id.clone();
                match self.await_plan_gate(&task_id_str, &plan).await {
                    Ok(PlanGateDecision::Approved) => {
                        let _ = self.event_bus.send(RuntimeEvent::PlanApproved {
                            run_id: task_id_str.clone(),
                            plan_id,
                            task_id: task_id_str.clone(),
                        });
                        break;
                    }
                    Ok(PlanGateDecision::Rejected { feedback }) => {
                        // Enforce the ceiling before any further LLM call (principle #7).
                        if replans_count >= max_replans {
                            tracing::warn!(
                                run_id = %task_id_str,
                                replans_count,
                                "plan.gate.replan_limit"
                            );
                            let _ = self.event_bus.send(RuntimeEvent::PlanAbandoned {
                                run_id: task_id_str.clone(),
                                task_id: task_id_str.clone(),
                                reason: "replan_limit".into(),
                            });
                            return AIPResult::failed(
                                "PLAN_REPLAN_LIMIT_EXCEEDED",
                                "plan rejected more times than plan_gate_max_replans allows",
                            );
                        }
                        let _ = self.event_bus.send(RuntimeEvent::PlanRejected {
                            run_id: task_id_str.clone(),
                            plan_id: plan_id.clone(),
                            task_id: task_id_str.clone(),
                            feedback: feedback.clone(),
                            replans_so_far: replans_count,
                        });
                        replans_count += 1;
                        match reasoner
                            .plan_with_feedback(&ctx, &plan_id, feedback.as_deref())
                            .await
                        {
                            Ok(new_plan) => {
                                plan = new_plan;
                                self.store_plan_in_cache(&cache_key, &plan, &manifest);
                                let _ = self.event_bus.send(RuntimeEvent::PlanGenerated {
                                    task_id: task_id_str.clone().into(),
                                    agent_name: manifest.name.clone(),
                                    plan_id: plan.plan_id.clone(),
                                    step_count: plan.steps.len(),
                                    run_id: None,
                                });
                                // Loop re-opens the gate for the new plan.
                            }
                            Err(e) => {
                                tracing::error!(run_id = %task_id_str, error = %e, "plan.gate.replan_failed");
                                let _ = self.event_bus.send(RuntimeEvent::PlanAbandoned {
                                    run_id: task_id_str.clone(),
                                    task_id: task_id_str.clone(),
                                    reason: "replan_failed".into(),
                                });
                                return AIPResult::failed("REPLAN_FAILED", &e.to_string());
                            }
                        }
                    }
                    Ok(PlanGateDecision::Edited { revised_steps }) => {
                        // Execute the operator's revised plan directly. Re-validate
                        // the edited steps (unique ids, resolvable deps, no cycle)
                        // before committing: a malformed edit ends the run cleanly.
                        let steps = revised_steps;

                        if let Err(e) = crate::reasoner::validate_steps(&steps) {
                            tracing::warn!(
                                run_id = %task_id_str,
                                error = %e,
                                "plan.gate.edit_invalid"
                            );
                            let _ = self.event_bus.send(RuntimeEvent::PlanAbandoned {
                                run_id: task_id_str.clone(),
                                task_id: task_id_str.clone(),
                                reason: "edit_invalid".into(),
                            });
                            return AIPResult::failed("PLAN_EDIT_INVALID", &e.to_string());
                        }

                        plan = ExecutionPlan {
                            plan_id: plan_id.clone(),
                            task_id: task_id_str.clone(),
                            steps,
                        };
                        self.store_plan_in_cache(&cache_key, &plan, &manifest);
                        let _ = self.event_bus.send(RuntimeEvent::PlanApproved {
                            run_id: task_id_str.clone(),
                            plan_id: plan.plan_id.clone(),
                            task_id: task_id_str.clone(),
                        });
                        break;
                    }
                    Err(ORIAError::PlanGateTimeout {
                        run_id, ttl_secs, ..
                    }) => {
                        tracing::warn!(run_id = %run_id, ttl_secs, "plan.gate.timeout");
                        return AIPResult::failed(
                            "PLAN_GATE_TIMEOUT",
                            "plan gate timed out before a decision was received",
                        );
                    }
                    Err(ORIAError::PlanGateChannelClosed { run_id }) => {
                        tracing::warn!(run_id = %run_id, "plan.gate.channel_closed");
                        return AIPResult::failed(
                            "PLAN_GATE_CHANNEL_CLOSED",
                            "plan gate channel closed before a decision",
                        );
                    }
                    Err(e) => return AIPResult::failed("PLAN_GATE_ERROR", &e.to_string()),
                }
            }
        }

        // Execute the plan, verify the result, and replan on a failing verdict.
        self.run_plan_with_verification(plan, &task, agent, &manifest, &ctx, &cache_key, db_path)
            .await
    }

    /// Execute a plan via the `ActorLoop`, then run the post-run verification and,
    /// on a failing verdict, replan and re-execute up to
    /// `oria_config.verification_max_replans` times.
    ///
    /// The `StepBudget` is created once and shared across every replan iteration,
    /// so it remains the non-bypassable ceiling for the whole run (principle #7).
    /// The critic call is off-budget by construction (it routes directly); the
    /// replan re-execution is on-budget (the `ActorLoop` increments), and the loop
    /// stops once the budget is exhausted.
    ///
    /// Verification is gated by the autonomy tier, mirroring the chat path: the
    /// tier's `run_verification` flag decides whether the pass runs at all. When it
    /// does not, the completed result is returned unverified after a `PlanCompleted`.
    ///
    /// The final verdict is emitted as [`RuntimeEvent::VerificationCompleted`] so it
    /// lands in the signed audit journal.
    #[allow(clippy::too_many_arguments)]
    async fn run_plan_with_verification(
        &self,
        mut plan: ExecutionPlan,
        task: &AIPTask,
        agent: &(dyn AIPAgent + Send + Sync),
        manifest: &AgentManifest,
        ctx: &ContextBundle,
        cache_key: &str,
        db_path: &str,
    ) -> AIPResult {
        let reasoner = match self.reasoner.as_ref() {
            Some(r) => r,
            None => {
                return AIPResult::failed(
                    "NO_LLM",
                    "Orchestrated mode requires a configured LLM (use with_reasoner())",
                )
            }
        };
        let task_id_str = task.task_id.clone();

        // StepBudget created once, shared across every verification replan.
        let agent_budget = manifest.step_budget.clone().unwrap_or_default();
        let budget = StepBudget::from_capped(&agent_budget, &self.runtime_config);

        // Resolve the verification gate from the autonomy tier (chat parity: the
        // chat reads `AutonomyConfig::default().level_config(level).run_verification`).
        let tier = self
            .oria_config
            .autonomy_level
            .unwrap_or(AutonomyLevel::Assisted);
        let run_verification = AutonomyLevelConfig::default_for(tier).run_verification;
        let verifier = if run_verification {
            Some((
                VerificationLoop::new(manifest.check_commands.clone(), Vec::new()),
                CriticPass::new(Arc::new(self.llm_router.clone())),
            ))
        } else {
            None
        };

        let objective = extract_task_text(task);
        let max_replans = self.oria_config.verification_max_replans;
        let mut replans: u32 = 0;

        loop {
            let repo = self.open_repo_with_plan(db_path, &plan, &manifest.name);
            let plan_id = plan.plan_id.clone();
            let step_count = plan.steps.len();

            // Reset per-task token budget before each execution.
            self.llm_router.reset_session_budget();

            let noop_proxy = NoopToolProxy;
            let tool_proxy: &dyn ToolProxyTrait = match &self.tool_proxy {
                Some(p) => p.as_ref(),
                None => &noop_proxy,
            };

            let plan_start = Instant::now();
            let mut actor = ActorLoop::new(
                plan,
                self.oria_config.max_replans,
                repo,
                self.event_bus.clone(),
                manifest.clone(),
            )
            .with_pending_approvals(self.pending_approvals.clone())
            .with_memory_manager(self.memory_manager.clone())
            .with_step_memory_max_chars(self.oria_config.step_memory_max_chars)
            .with_context_manager(self.context_manager.clone());
            let step_result = actor
                .execute(
                    StepDeps {
                        tool_proxy,
                        llm_router: &self.llm_router,
                        budget: &budget,
                        reasoner,
                    },
                    &self.resilience,
                )
                .await;
            let duration_ms = plan_start.elapsed().as_millis() as u64;

            // Emit final session budget snapshot for this execution.
            let token_budget = self.llm_router.session_budget();
            let _ = self.event_bus.send(RuntimeEvent::TokenBudgetUpdated {
                session_cost_usd: token_budget.cost_usd,
                total_input_tokens: token_budget.input_tokens,
                total_output_tokens: token_budget.output_tokens,
                total_cache_read_tokens: token_budget.cache_read_tokens,
                threshold_usd: f64::MAX,
                threshold_exceeded: false,
            });

            // A non-completed run (budget exceeded, plan failure) carries its own
            // failure and is not verified.
            if step_result.status != TaskStatus::Completed {
                return step_result;
            }

            let final_result = self.finalize_completed(agent, step_result).await;

            // Verification disabled for this tier: return the completed result.
            let Some((verification, critic)) = verifier.as_ref() else {
                let _ = self.event_bus.send(RuntimeEvent::PlanCompleted {
                    task_id: task_id_str.clone().into(),
                    plan_id,
                    step_count,
                    duration_ms,
                });
                return final_result;
            };

            // Run the post-run verification. The critic is off-budget by design.
            let output_text = result_text(&final_result);
            let verdict =
                run_post_run_verification(verification, critic, &objective, &output_text).await;
            let _ = self.event_bus.send(RuntimeEvent::VerificationCompleted {
                task_id: task_id_str.clone().into(),
                passed: verdict.passed,
                check_failures: verdict.check_failures.len() as u32,
                corrections: verdict.corrections.len() as u32,
                skipped: verdict.skipped,
                replans,
            });

            // Accept the result when the verdict passes, the replan ceiling is
            // reached, or the shared budget is spent.
            if verdict.passed || replans >= max_replans || budget.is_exhausted() {
                let _ = self.event_bus.send(RuntimeEvent::PlanCompleted {
                    task_id: task_id_str.clone().into(),
                    plan_id,
                    step_count,
                    duration_ms,
                });
                return final_result;
            }

            // Failing verdict with replan budget remaining: replan with feedback.
            let feedback = verdict_feedback(&verdict);
            match reasoner
                .plan_with_feedback(ctx, &plan_id, Some(&feedback))
                .await
            {
                Ok(mut new_plan) => {
                    self.enrich_plan_with_args(&mut new_plan).await;
                    self.store_plan_in_cache(cache_key, &new_plan, manifest);
                    let _ = self.event_bus.send(RuntimeEvent::PlanGenerated {
                        task_id: task_id_str.clone().into(),
                        agent_name: manifest.name.clone(),
                        plan_id: new_plan.plan_id.clone(),
                        step_count: new_plan.steps.len(),
                        run_id: None,
                    });
                    plan = new_plan;
                    replans += 1;
                }
                Err(e) => {
                    tracing::event!(
                        tracing::Level::WARN,
                        task_id = %task_id_str,
                        error = %e,
                        "oria.verification.replan_failed"
                    );
                    // The run has a valid result; only the hardening step failed.
                    let _ = self.event_bus.send(RuntimeEvent::PlanCompleted {
                        task_id: task_id_str.clone().into(),
                        plan_id,
                        step_count,
                        duration_ms,
                    });
                    return final_result;
                }
            }
        }
    }

    /// Assemble a completed orchestrated run into its user-facing result.
    ///
    /// Calls the agent's `on_plan_complete()` hook when present, otherwise
    /// concatenates the per-step outputs.
    async fn finalize_completed(
        &self,
        agent: &(dyn AIPAgent + Send + Sync),
        step_result: AIPResult,
    ) -> AIPResult {
        let outputs = extract_step_outputs(&step_result);
        if agent.has_on_plan_complete() {
            agent.call_on_plan_complete(outputs).await
        } else {
            concat_outputs(&outputs)
        }
    }

    /// Looks up a cached plan for `cache_key`, returning a ready-to-run
    /// [`ExecutionPlan`] (fresh `plan_id`, the supplied `task_id`, cached steps)
    /// on a hit, or `None` on a miss, an absent cache, or a recoverable error.
    ///
    /// Lock poisoning and lookup errors are logged and treated as a miss so the
    /// caller falls back to the Reasoner.
    fn lookup_cached_plan(&self, cache_key: &str, task_id: &str) -> Option<ExecutionPlan> {
        let cache_mutex = self.plan_cache.as_ref()?;
        let cache = match cache_mutex.lock() {
            Ok(cache) => cache,
            Err(e) => {
                tracing::warn!(error = %e, "plan cache mutex poisoned, skipping lookup");
                return None;
            }
        };
        let cached_plan = match cache.lookup(cache_key) {
            Ok(Some(cached_plan)) => cached_plan,
            Ok(None) => return None,
            Err(e) => {
                tracing::warn!(error = %e, "plan cache lookup failed");
                return None;
            }
        };
        Some(ExecutionPlan {
            plan_id: uuid::Uuid::new_v4().to_string(),
            task_id: task_id.to_string(),
            steps: cached_plan.steps,
        })
    }

    /// Stores `plan` in the plan cache under `cache_key`.
    ///
    /// No-op when no cache is configured. Lock poisoning and store errors are
    /// logged and otherwise ignored (caching is best-effort).
    fn store_plan_in_cache(&self, cache_key: &str, plan: &ExecutionPlan, manifest: &AgentManifest) {
        let Some(cache_mutex) = self.plan_cache.as_ref() else {
            return;
        };
        match cache_mutex.lock() {
            Ok(cache) => {
                if let Err(e) = cache.store(cache_key, plan, &manifest.name, &manifest.version) {
                    tracing::warn!(error = %e, "plan cache store failed");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "plan cache mutex poisoned, skipping store");
            }
        }
    }

    /// Fills each tool step's structured arguments before the plan is cached,
    /// persisted and executed.
    ///
    /// For every tool step whose `args` are absent or invalid against the
    /// target tool's input schema, resolves them with a schema-guided model
    /// call so the persisted plan is fully specified, auditable and replayable.
    /// Best-effort: a step that cannot be resolved keeps `args = None`, and the
    /// [`crate::actor::ActorLoop`] resolves it just in time at execution.
    ///
    /// No-op without an injected tool proxy (the schema source) or when the
    /// router has no backend to answer the resolution call.
    async fn enrich_plan_with_args(&self, plan: &mut ExecutionPlan) {
        let Some(proxy) = self.tool_proxy.as_ref() else {
            return;
        };
        for step in plan.steps.iter_mut() {
            let Some(tool_name) = step.tool_hint.as_deref() else {
                continue;
            };
            if tool_name == "llm" {
                continue;
            }
            let Some(schema) = proxy.tool_schema(tool_name).await else {
                continue;
            };
            // Keep already-valid plan-time args untouched.
            if step
                .args
                .as_ref()
                .is_some_and(|args| crate::arg_resolver::validate_args(args, &schema).is_ok())
            {
                continue;
            }
            let Some(model) = self.llm_router.get(step.model_hint.as_deref()) else {
                continue;
            };
            match crate::arg_resolver::resolve_tool_args(
                &model,
                tool_name,
                &schema,
                &step.description,
                0.0,
            )
            .await
            {
                Ok(args) => step.args = Some(args),
                Err(e) => tracing::event!(
                    tracing::Level::WARN,
                    step_id = %step.step_id,
                    tool = %tool_name,
                    error = %e,
                    "oria.plan.arg_enrichment_failed"
                ),
            }
        }
    }

    /// Execute a plan retrieved from the cache.
    ///
    /// Mirrors the post-Reasoner path of [`execute_orchestrated_plan`]: emit
    /// PlanGenerated for the cached plan, then delegate execution, verification,
    /// and replan to [`run_plan_with_verification`](Self::run_plan_with_verification).
    // Explicit dependency list for the cached-plan execution path; bundling
    // these into a struct would only relocate the argument list.
    #[allow(clippy::too_many_arguments)]
    async fn execute_cached_plan(
        &self,
        plan: ExecutionPlan,
        task: AIPTask,
        agent: &(dyn AIPAgent + Send + Sync),
        manifest: AgentManifest,
        ctx: &ContextBundle,
        cache_key: &str,
    ) -> AIPResult {
        let plan_id = plan.plan_id.clone();
        let step_count = plan.steps.len();
        let task_id_str = task.task_id.clone();

        let db_path = self.db_path.as_deref().unwrap_or(":memory:");

        let _ = self.event_bus.send(RuntimeEvent::PlanGenerated {
            task_id: task_id_str.clone().into(),
            agent_name: manifest.name.clone(),
            plan_id: plan_id.clone(),
            step_count,
            // The orchestrated engine path correlates via task_id, not a chat run.
            run_id: None,
        });

        self.run_plan_with_verification(plan, &task, agent, &manifest, ctx, cache_key, db_path)
            .await
    }

    /// Opens a `PlanRepository` at `db_path`, inserts the plan and its steps.
    ///
    /// Falls back to `:memory:` if `db_path` fails. Errors during `insert_plan`
    /// or `insert_steps` are logged but do not abort execution (persistence is
    /// non-blocking).
    fn open_repo_with_plan(
        &self,
        db_path: &str,
        plan: &ExecutionPlan,
        agent_name: &str,
    ) -> PlanRepository {
        let repo = match PlanRepository::new(db_path) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(error = %e, "Failed to open PlanRepository - falling back to :memory:");
                PlanRepository::new(":memory:").expect("in-memory SQLite must always succeed")
            }
        };

        if let Err(e) = repo.insert_plan(plan, agent_name) {
            tracing::error!(error = %e, "Failed to persist plan (non-blocking)");
        }
        if let Err(e) = repo.insert_steps(&plan.plan_id, &plan.steps) {
            tracing::error!(error = %e, "Failed to persist plan steps (non-blocking)");
        }

        repo
    }

    // Mode Direct

    /// Execute a task in Mode Direct with HITL support.
    ///
    /// 1. Check the budget is not already exhausted.
    /// 2. Call `runner.call_run(task)` with `StepBudget` supervision.
    /// 3. On `AIPResult::InputRequired`:
    ///    - Persist prompt + context in SQLite via `task_repository` (when configured).
    ///    - Emit `RuntimeEvent::TaskInputRequired` on the EventBus.
    ///    - Register a oneshot in `pending_approvals` and **wait** for the human decision.
    ///    - If `approved=true`: rebuild `AIPTask` with `is_resumed=true` and call `run()` again.
    ///    - If `approved=false`: return `AIPResult::failed("REJECTED", reason)`.
    /// 4. Otherwise return the result directly.
    ///
    /// **StepBudget paused during suspension**: waiting on the oneshot is a pure
    /// `await`, budget polling does not run during suspension.
    /// The budget does not advance until the human responds.
    pub async fn execute_direct(
        &self,
        task: AIPTask,
        runner: &dyn AgentRunner,
        budget: Arc<StepBudget>,
    ) -> Result<AIPResult, ORIAError> {
        // Check budget before starting
        if budget.is_exhausted() {
            let reason = budget
                .exhaustion_reason()
                .unwrap_or_else(|| "budget already exhausted".into());
            return Err(ORIAError::BudgetExceeded { reason });
        }

        // First run, with budget supervision
        let result = Self::run_with_budget(runner, task.clone(), &budget).await?;

        // Non-HITL path: return immediately
        if result.status != TaskStatus::InputRequired {
            return Ok(result);
        }

        // HITL Suspension
        let (prompt, context) = match result.input_required_data {
            Some(data) => (data.prompt, data.context),
            None => ("Approbation requise".to_string(), serde_json::Value::Null),
        };

        // persist input_required in SQLite (non-blocking on error)
        if let Some(repo) = self.task_repository.as_ref() {
            if let Err(e) = repo
                .save_input_required(&task.task_id, None, &prompt, &context)
                .await
            {
                tracing::warn!(
                    task_id = %task.task_id,
                    error = %e,
                    "failed to persist input_required - continuing without DB record"
                );
            }

            // record suspended_at timestamp for HITL timing
            let suspended_at =
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            if let Err(e) = repo
                .save_suspended_at(&task.task_id, None, &suspended_at)
                .await
            {
                tracing::warn!(
                    task_id = %task.task_id,
                    error = %e,
                    "failed to persist suspended_at - continuing without timing record"
                );
            }
        }

        // broadcast TaskInputRequired on EventBus
        // step_id=None in Mode Direct: the whole task is suspended (not a specific step).
        let _ = self.event_bus.send(RuntimeEvent::TaskInputRequired {
            task_id: task.task_id.clone().into(),
            prompt: prompt.clone(),
            step_id: None,
        });

        tracing::info!(
            task_id = %task.task_id,
            %prompt,
            "task suspended - waiting for human approval"
        );

        // register on PendingApprovals: if not configured, degrade gracefully
        let pending = match self.pending_approvals.as_ref() {
            Some(p) => p,
            None => {
                tracing::warn!(
                    task_id = %task.task_id,
                    "PendingApprovals not configured - returning InputRequired without suspension"
                );
                return Ok(AIPResult::input_required(&prompt, context));
            }
        };

        let rx = pending.register(&task.task_id);

        // plain await: StepBudget does NOT advance during suspension
        let response = rx.await.map_err(|_| ORIAError::ApprovalChannelClosed)?;

        tracing::info!(
            task_id = %task.task_id,
            approved = response.approved,
            "human approval received - resuming task"
        );

        // rejection: AIPResult::failed without calling run()
        if !response.approved {
            return Ok(AIPResult::failed(
                "REJECTED",
                response.reason.as_deref().unwrap_or("Refusé"),
            ));
        }

        // approval: rebuild AIPTask with is_resumed=true and call run() again
        let resumed_task = AIPTask {
            is_resumed: true,
            input_response: Some(response),
            ..task
        };

        // Run resumed task with budget protection
        Self::run_with_budget(runner, resumed_task, &budget).await
    }

    /// Execute `runner.call_run(task)` with concurrent `StepBudget` supervision.
    ///
    /// Returns immediately with `ORIAError::BudgetExceeded` if the budget expires
    /// before execution completes. Used for the first call and for the resume
    /// after HITL.
    ///
    /// Supervision uses a `oneshot` notified by `StepBudget::increment_steps` /
    /// `increment_tool_calls`, combined with a sleep on the remaining wall-clock
    /// duration. No periodic polling.
    async fn run_with_budget(
        runner: &dyn AgentRunner,
        task: AIPTask,
        budget: &Arc<StepBudget>,
    ) -> Result<AIPResult, ORIAError> {
        tokio::select! {
            result = runner.call_run(task) => {
                result.map_err(ORIAError::BridgeError)
            }
            _ = budget.wait_for_exhaustion() => {
                let reason = budget
                    .exhaustion_reason()
                    .unwrap_or_else(|| "budget exhausted during execution".into());
                Err(ORIAError::BudgetExceeded { reason })
            }
        }
    }
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
fn extract_task_text(task: &AIPTask) -> String {
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
fn extract_step_outputs(result: &AIPResult) -> HashMap<String, String> {
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
fn concat_outputs(outputs: &HashMap<String, String>) -> AIPResult {
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
fn result_text(result: &AIPResult) -> String {
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
    use super::*;
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
