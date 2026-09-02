//! Wiring an ORIA engine onto the router, and the production backend's shape.

use std::path::PathBuf;
use std::sync::Arc;

use apollia_aip::bridge::AIPBridge;
use apollia_core::{AgentManifest, PendingApprovals, StepBudgetConfig};
use apollia_llm::LlmRouter;
use apollia_oria::budget::StepBudget;
use apollia_oria::engine::ORIAEngine;
use apollia_runtime::eventbus::EventBusSender;
use apollia_tools::{AuditTrailHandle, TaskRepository, ToolRegistryHandle};

// Real per-agent execution backend (AIPBridge + RuntimeContext)
// ─────────────────────────────────────────────────────────────

/// Wires an [`ORIAEngine`] with the LLM router and Reasoner needed for
/// orchestrated execution. Extracted from
/// [`AIPProductionBackend::execute`] so the regression where orchestrated
/// agents fell through to `[NO_LLM]` (because the Reasoner was never plugged
/// in) has a unit-test guard.
///
/// Behaviour:
/// - `llm_router == None` → engine returned unchanged, warning logged.
///   Orchestrated execution will fail with `NO_LLM` at runtime.
/// - `llm_router == Some(_)` and `route_precise` succeeds → engine
///   gets both `with_llm_router` and `with_reasoner`. Orchestrated
///   execution can plan.
/// - `llm_router == Some(_)` but no `precise` backend resolved →
///   engine gets `with_llm_router` only (step LLM calls work), no
///   Reasoner. Orchestrated execution still fails with `NO_LLM` but
///   any LLM step within `execute_direct` keeps working. Warning
///   logged.
pub(super) fn wire_engine_with_llm(
    mut engine: ORIAEngine,
    llm_router: Option<Arc<LlmRouter>>,
    agent_id: &str,
    max_steps: u32,
) -> ORIAEngine {
    let Some(router_arc) = llm_router else {
        tracing::warn!(
            agent = %agent_id,
            detail = "orchestrated execution fails with NO_LLM",
            "orchestration.llm_router.absent"
        );
        return engine;
    };
    let owned_router: LlmRouter = match Arc::try_unwrap(router_arc) {
        Ok(owned) => owned,
        Err(shared) => (*shared).clone(),
    };
    match owned_router.route_precise() {
        Ok(model) => {
            engine = engine
                .with_llm_router(owned_router)
                .with_reasoner(model, max_steps);
        }
        Err(err) => {
            tracing::warn!(
                agent = %agent_id,
                error = %err,
                detail = "orchestrated execution fails with NO_LLM",
                "orchestration.llm_backend.unresolved"
            );
            engine = engine.with_llm_router(owned_router);
        }
    }
    engine
}

/// Builds the bounded [`StepBudget`] for the direct execution path.
///
/// Caps the agent-declared budget to the runtime ceiling
/// ([`StepBudgetConfig::default`]: 30 steps / 60 tool calls / 600s wall clock)
/// so `apollia-os run` never executes under an unlimited budget. This restores
/// principle #7 on the direct path, which previously used
/// `StepBudget::unlimited()` (u32::MAX steps + 24h). Extracted for unit testing.
pub(super) fn direct_path_budget(agent_budget: &StepBudgetConfig) -> StepBudget {
    StepBudget::from_capped(agent_budget, &StepBudgetConfig::default())
}

/// Per-agent backend that calls Python via `AIPBridge`.
///
/// Created once per agent at start time by `ProductionBackendFactory`.
/// All fields are `Arc`-wrapped, so cloning is cheap.
pub(super) struct AIPProductionBackend {
    pub(super) bridge: Arc<AIPBridge>,
    pub(super) agent_id: String,
    /// Manifest snapshot captured at factory time. Used to drive the
    /// `engine.execute_direct` vs `engine.execute_orchestrated_plan`
    /// routing decision in [`AIPProductionBackend::execute`].
    pub(super) manifest: AgentManifest,
    pub(super) allowed_tools: Vec<String>,
    pub(super) llm_router: Option<Arc<LlmRouter>>,
    pub(super) event_bus: EventBusSender,
    pub(super) pending_approvals: Option<Arc<PendingApprovals>>,
    pub(super) plan_gates: Option<Arc<apollia_oria::PendingPlanGates>>,
    /// Shared plan cache, the repository the supervisor opened at boot and
    /// exposes over REST. `None` leaves the engine re-planning every run.
    pub(super) plan_cache:
        Option<Arc<std::sync::Mutex<apollia_oria::plan_cache::PlanCacheRepository>>>,
    pub(super) task_repository: Option<Arc<TaskRepository>>,
    pub(super) tool_registry: Option<ToolRegistryHandle>,
    pub(super) audit_trail: Option<AuditTrailHandle>,
    /// Memory namespace declared in the manifest (e.g. "apollia-guide").
    pub(super) memory_namespace: Option<String>,
    /// Root directory of the memory files (e.g. `~/.apollia/memory/`).
    pub(super) memory_base_dir: PathBuf,
    /// Manifest opt-in for `ctx.memory.remember_user()` writes into `__user__`.
    pub(super) user_memory_write: bool,
    /// High-level A2A orchestrator, `None` when registry or router are not initialized.
    pub(super) a2a_invoker: Option<Arc<apollia_runtime::a2a::A2AInvoker>>,
    /// Operator-supplied tools configuration loaded from `apollia.toml`.
    pub(super) tools_config: apollia_core::ToolsConfig,
    /// Filesystem roots an agent may reach (`[filesystem] trusted_paths`, `~`
    /// already resolved). The first is the anchor for relative paths.
    pub(super) trusted_paths: Vec<std::path::PathBuf>,
    /// Datasources declared in the manifest (`manifest.datasources`). Empty
    /// when the agent declares none.
    pub(super) datasources_declared: Vec<String>,
    /// Jinja2 templates declared in the manifest (`manifest.templates`). Empty
    /// when the agent declares none.
    pub(super) templates_declared: Vec<String>,
    /// Root directory of the agent, used to resolve
    /// `datasources/<name>.yaml` and `templates/<name>.j2`.
    pub(super) agent_dir: Option<PathBuf>,
    /// Secrets declared in the manifest (`manifest.secrets`). Strict allowlist
    /// for `ctx.secrets.get()`. Empty when the agent declares none.
    pub(super) secrets_declared: Vec<String>,
    /// Apollia data directory (`~/.apollia/` by default), used to open the
    /// shared [`ToolCredentialStore`] that backs `ctx.secrets`. `None` is
    /// accepted in degraded mode.
    pub(super) secrets_data_dir: Option<PathBuf>,
    /// MCP client manager handle, `None` when no MCP server is configured. Lets
    /// the `BridgeRunner` route `mcp:<server>/<tool>` calls to the MCP manager.
    pub(super) mcp_handle: Option<apollia_mcp::manager::McpClientManagerHandle>,
}

impl Clone for AIPProductionBackend {
    fn clone(&self) -> Self {
        Self {
            bridge: Arc::clone(&self.bridge),
            agent_id: self.agent_id.clone(),
            manifest: self.manifest.clone(),
            allowed_tools: self.allowed_tools.clone(),
            llm_router: self.llm_router.clone(),
            event_bus: self.event_bus.clone(),
            tool_registry: self.tool_registry.clone(),
            audit_trail: self.audit_trail.clone(),
            memory_namespace: self.memory_namespace.clone(),
            memory_base_dir: self.memory_base_dir.clone(),
            pending_approvals: self.pending_approvals.clone(),
            plan_gates: self.plan_gates.clone(),
            plan_cache: self.plan_cache.clone(),
            task_repository: self.task_repository.clone(),
            a2a_invoker: self.a2a_invoker.clone(),
            tools_config: self.tools_config.clone(),
            trusted_paths: self.trusted_paths.clone(),
            user_memory_write: self.user_memory_write,
            datasources_declared: self.datasources_declared.clone(),
            templates_declared: self.templates_declared.clone(),
            agent_dir: self.agent_dir.clone(),
            secrets_declared: self.secrets_declared.clone(),
            secrets_data_dir: self.secrets_data_dir.clone(),
            mcp_handle: self.mcp_handle.clone(),
        }
    }
}
