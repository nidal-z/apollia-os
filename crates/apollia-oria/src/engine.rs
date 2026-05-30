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
    AIPPart, AIPResult, AIPTask, AgentManifest, DataPart, EventBusSender, ORIAConfig,
    PendingApprovals, RuntimeEvent, StepBudgetConfig, TaskStatus,
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
use crate::plan_repository::PlanRepository;
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
    #[error("approval channel closed before human response — runtime may be shutting down")]
    ApprovalChannelClosed,
}

// NoopToolProxy: fallback when no proxy configured

/// Tool proxy that always returns an error, used when no tool proxy is configured.
struct NoopToolProxy;

#[async_trait::async_trait]
impl ToolProxyTrait for NoopToolProxy {
    async fn invoke(&self, tool_name: &str, _input: &serde_json::Value) -> Result<String, String> {
        Err(format!(
            "No tool proxy configured — cannot invoke '{tool_name}'"
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
    plan_cache: Option<Mutex<PlanCacheRepository>>,
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

    /// Add a plan cache to the engine.
    ///
    /// When configured, [`execute_orchestrated_plan`] checks the cache before calling
    /// the Reasoner. A cache hit avoids the LLM call, clones the plan with a new
    /// `plan_id`, and emits [`RuntimeEvent::PlanCacheHit`] on the EventBus.
    /// Cache errors are logged at `warn` level without blocking execution.
    ///
    /// [`execute_orchestrated_plan`]: ORIAEngine::execute_orchestrated_plan
    pub fn with_plan_cache(mut self, repo: PlanCacheRepository) -> Self {
        self.plan_cache = Some(Mutex::new(repo));
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
    pub async fn execute(
        &self,
        task: AIPTask,
        agent: &(dyn AIPAgent + Send + Sync),
    ) -> AIPResult {
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
                // Will be connected in a follow-up story.
                AIPResult::failed(
                    "DIRECT_MODE_NOT_AVAILABLE_VIA_AIP_AGENT",
                    "Direct mode requires an AgentRunner — use execute_direct() directly",
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

            return self.execute_cached_plan(plan, task, agent, manifest).await;
        }

        // Generate plan (Reasoner handles retries internally)
        let plan = match reasoner.plan(&ctx).await {
            Ok(p) => p,
            Err(e) => return AIPResult::failed("PLAN_FAILED", &e.to_string()),
        };

        // Store in cache
        self.store_plan_in_cache(&cache_key, &plan, &manifest);

        let plan_id = plan.plan_id.clone();
        let step_count = plan.steps.len();
        let task_id_str = task.task_id.clone();

        // Persist plan in SQLite (non-blocking on error)
        let db_path = self.db_path.as_deref().unwrap_or(":memory:");
        let repo = self.open_repo_with_plan(db_path, &plan, &manifest.name);

        // emit PlanGenerated
        let _ = self.event_bus.send(RuntimeEvent::PlanGenerated {
            task_id: task_id_str.clone().into(),
            agent_name: manifest.name.clone(),
            plan_id: plan_id.clone(),
            step_count,
        });

        // create StepBudget via from_capped
        let agent_budget = manifest.step_budget.clone().unwrap_or_default();
        let budget = StepBudget::from_capped(&agent_budget, &self.runtime_config);

        // Reset per-task token budget before execution.
        self.llm_router.reset_session_budget();

        // Execute via ActorLoop
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

        // Emit final session budget snapshot at end of task.
        let token_budget = self.llm_router.session_budget();
        let _ = self.event_bus.send(RuntimeEvent::TokenBudgetUpdated {
            session_cost_usd: token_budget.cost_usd,
            total_input_tokens: token_budget.input_tokens,
            total_output_tokens: token_budget.output_tokens,
            total_cache_read_tokens: token_budget.cache_read_tokens,
            threshold_usd: f64::MAX,
            threshold_exceeded: false,
        });

        // Post-process
        if step_result.status == TaskStatus::Completed {
            let _ = self.event_bus.send(RuntimeEvent::PlanCompleted {
                task_id: task_id_str.into(),
                plan_id,
                step_count,
                duration_ms,
            });

            let outputs = extract_step_outputs(&step_result);

            // call on_plan_complete() if the agent exposes it,
            // otherwise fall back to automatic step-output concatenation.
            if agent.has_on_plan_complete() {
                agent.call_on_plan_complete(outputs).await
            } else {
                concat_outputs(&outputs)
            }
        } else {
            step_result
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

    /// Execute a plan retrieved from the cache.
    ///
    /// Identical to the post-Reasoner path of [`execute_orchestrated_plan`]:
    /// persist, emit PlanGenerated, StepBudget, ActorLoop, concat.
    async fn execute_cached_plan(
        &self,
        plan: ExecutionPlan,
        task: AIPTask,
        agent: &(dyn AIPAgent + Send + Sync),
        manifest: AgentManifest,
    ) -> AIPResult {
        let plan_id = plan.plan_id.clone();
        let step_count = plan.steps.len();
        let task_id_str = task.task_id.clone();

        let db_path = self.db_path.as_deref().unwrap_or(":memory:");
        let repo = self.open_repo_with_plan(db_path, &plan, &manifest.name);

        let _ = self.event_bus.send(RuntimeEvent::PlanGenerated {
            task_id: task_id_str.clone().into(),
            agent_name: manifest.name.clone(),
            plan_id: plan_id.clone(),
            step_count,
        });

        let agent_budget = manifest.step_budget.clone().unwrap_or_default();
        let budget = StepBudget::from_capped(&agent_budget, &self.runtime_config);

        // Reset per-task token budget before execution.
        self.llm_router.reset_session_budget();

        let noop_proxy = NoopToolProxy;
        let tool_proxy: &dyn ToolProxyTrait = match &self.tool_proxy {
            Some(p) => p.as_ref(),
            None => &noop_proxy,
        };

        let plan_start = Instant::now();
        let reasoner = match self.reasoner.as_ref() {
            Some(r) => r,
            None => {
                return AIPResult::failed(
                    "NO_LLM",
                    "Orchestrated mode requires a configured LLM (use with_reasoner())",
                );
            }
        };
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

        // Emit final session budget snapshot at end of task.
        let token_budget = self.llm_router.session_budget();
        let _ = self.event_bus.send(RuntimeEvent::TokenBudgetUpdated {
            session_cost_usd: token_budget.cost_usd,
            total_input_tokens: token_budget.input_tokens,
            total_output_tokens: token_budget.output_tokens,
            total_cache_read_tokens: token_budget.cache_read_tokens,
            threshold_usd: f64::MAX,
            threshold_exceeded: false,
        });

        if step_result.status == TaskStatus::Completed {
            let _ = self.event_bus.send(RuntimeEvent::PlanCompleted {
                task_id: task_id_str.into(),
                plan_id,
                step_count,
                duration_ms,
            });

            let outputs = extract_step_outputs(&step_result);

            if agent.has_on_plan_complete() {
                agent.call_on_plan_complete(outputs).await
            } else {
                concat_outputs(&outputs)
            }
        } else {
            step_result
        }
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
                tracing::error!(error = %e, "Failed to open PlanRepository — falling back to :memory:");
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
                    "failed to persist input_required — continuing without DB record"
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
                    "failed to persist suspended_at — continuing without timing record"
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
            "task suspended — waiting for human approval"
        );

        // register on PendingApprovals: if not configured, degrade gracefully
        let pending = match self.pending_approvals.as_ref() {
            Some(p) => p,
            None => {
                tracing::warn!(
                    task_id = %task.task_id,
                    "PendingApprovals not configured — returning InputRequired without suspension"
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
            "human approval received — resuming task"
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
        // GIVEN un budget deja epuise (max_steps=0)
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

        // WHEN on appelle execute_direct()
        let result = engine.execute_direct(make_task(), &runner, budget).await;

        // THEN retourne ORIAError::BudgetExceeded
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ORIAError::BudgetExceeded { .. }),
            "expected BudgetExceeded, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_execute_direct_success() {
        // GIVEN un budget valide et un runner mock qui retourne Ok(AIPResult)
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

        // WHEN on appelle execute_direct()
        let result = engine.execute_direct(make_task(), &runner, budget).await;

        // THEN retourne Ok(AIPResult) avec le resultat attendu
        assert!(result.is_ok());
        let aip_result = result.expect("should be ok");
        assert_eq!(aip_result.task_id, "task-001");
        assert!(matches!(aip_result.status, TaskStatus::Completed));
    }

    #[tokio::test]
    async fn test_execute_direct_bridge_error() {
        // GIVEN un budget valide et un runner mock qui retourne Err
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

        // WHEN on appelle execute_direct()
        let result = engine.execute_direct(make_task(), &runner, budget).await;

        // THEN retourne ORIAError::BridgeError
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ORIAError::BridgeError(_)),
            "expected BridgeError, got: {err}"
        );
    }

    // ── HITL tests ───────────────────────────────────────────

    /// Runner qui retourne InputRequired au premier appel, puis Completed au second.
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
    async fn test_ac1_input_required_emits_event() {
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
    async fn test_ac2_approve_resumes_and_recalls_run() {
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
}
