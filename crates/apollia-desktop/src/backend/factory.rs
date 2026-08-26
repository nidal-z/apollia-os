//! What builds one execution backend per agent at start time, and the local
//! resources it needs: the step budget of the direct path, the memory and data
//! directories, and the secret store the tools read credentials from.

use std::path::Path;
use std::sync::Arc;

use apollia_aip::bridge::AIPBridge;
use apollia_core::{AgentManifest, PendingApprovals, StepBudgetConfig, ToolsConfig};
use apollia_llm::LlmRouter;
use apollia_oria::budget::StepBudget;
use apollia_oria::engine::ORIAEngine;
use apollia_oria::plan_cache::PlanCacheRepository;
use apollia_oria::PendingPlanGates;
use apollia_runtime::a2a::A2AInvoker;
use apollia_runtime::api::routes_agents::AgentBackendFactory;
use apollia_runtime::coordinator::DynBackend;
use apollia_runtime::eventbus::EventBusSender;
use apollia_runtime::mailbox::AgentMailboxHandle;
use apollia_runtime::registry::AgentRegistryHandle;
use apollia_runtime::router::TaskRouterHandle;
use apollia_tools::{AuditTrailHandle, TaskRepository, ToolRegistryHandle};

use super::runner::{llm_router_snapshot, AIPProductionBackend, NoopBackend};
use super::{default_memory_dir, venv_site_packages_for_agent_name};

// ─── Factory ──────────────────────────────────────────────────────────────────

/// Creates a real `AIPProductionBackend` per agent at `agent start` time.
///
/// OnceLocks are populated by `main()` immediately after `init_embedded()` returns,
/// before any HTTP request can arrive.
pub struct ProductionBackendFactory {
    pub event_bus: Arc<std::sync::OnceLock<EventBusSender>>,
    pub llm_router: Arc<std::sync::RwLock<Option<Arc<LlmRouter>>>>,
    pub tool_registry: Arc<std::sync::OnceLock<ToolRegistryHandle>>,
    pub audit_trail: Arc<std::sync::OnceLock<AuditTrailHandle>>,
    pub pending_approvals: Arc<std::sync::OnceLock<Arc<PendingApprovals>>>,
    pub task_repository: Arc<std::sync::OnceLock<Arc<TaskRepository>>>,
    pub mcp_handle: Arc<std::sync::OnceLock<apollia_mcp::manager::McpClientManagerHandle>>,
    /// Agent registry handle, required to build the A2A invoker so
    /// trigger-fired agents and manual fires can call `ctx.a2a_invoke(...)`
    /// on parity with Chat Libre.
    pub agent_registry: Arc<std::sync::OnceLock<AgentRegistryHandle>>,
    /// Task router handle, required to build the A2A delegate.
    pub task_router: Arc<std::sync::OnceLock<TaskRouterHandle<DynBackend>>>,
    /// Inter-agent mailbox handle, exposed as `ctx.mailbox`.
    pub mailbox_handle: Arc<std::sync::OnceLock<AgentMailboxHandle>>,
    /// Operator tools configuration (`[tools]` of apollia.toml).
    pub tools_config: Arc<std::sync::OnceLock<ToolsConfig>>,
    /// Shared plan-gate registry, forwarded to each per-agent engine so the
    /// desktop UI can resolve a pending plan gate.
    pub plan_gates: Arc<std::sync::OnceLock<Arc<PendingPlanGates>>>,
    /// Shared plan cache, forwarded to each per-agent engine. Without it the
    /// engine's cache lookup and store both return on their first line.
    pub plan_cache: Arc<std::sync::OnceLock<Arc<std::sync::Mutex<PlanCacheRepository>>>>,
}

impl AgentBackendFactory for ProductionBackendFactory {
    fn create_for_agent(&self, agent_path: &Path, manifest: &AgentManifest) -> DynBackend {
        let agent_id = manifest.name.clone();

        let event_bus = match self.event_bus.get() {
            Some(bus) => bus.clone(),
            None => {
                tracing::error!(
                    agent = %agent_id,
                    reason = "the event bus is not initialised yet",
                    "agent.factory.premature"
                );
                return DynBackend::new(NoopBackend);
            }
        };
        let llm_router = llm_router_snapshot(&self.llm_router);
        let tool_registry = self.tool_registry.get().cloned();
        let audit_trail = self.audit_trail.get().cloned();
        let pending_approvals = self.pending_approvals.get().cloned();
        let task_repository = self.task_repository.get().cloned();
        let mcp_handle = self.mcp_handle.get().cloned();
        let mailbox = self.mailbox_handle.get().cloned();
        let tools_config = self
            .tools_config
            .get()
            .cloned()
            .unwrap_or_else(ToolsConfig::default);
        let plan_gates = self.plan_gates.get().cloned();
        let plan_cache = self.plan_cache.get().cloned();
        let backend_manifest = manifest.clone();

        // Build the A2A invoker when registry+router are available.
        let a2a_invoker = match (
            self.agent_registry.get().cloned(),
            self.task_router.get().cloned(),
        ) {
            (Some(registry), Some(router)) => {
                let invoker = Arc::new(A2AInvoker::new(
                    registry,
                    router,
                    event_bus.clone(),
                    apollia_core::A2AConfig::default(),
                ));
                Some(invoker)
            }
            _ => {
                tracing::warn!(
                    agent = %agent_id,
                    reason = "registry or router not initialised yet",
                    "agent.a2a.invoker.unavailable"
                );
                None
            }
        };

        let result: Result<AIPProductionBackend, String> = (|| {
            // Inject the per-agent venv site-packages into sys.path so top-level
            // imports of pip-installed packages resolve correctly at agent start.
            let extras = venv_site_packages_for_agent_name(&manifest.name);
            let module = apollia_aip::loader::load_agent_module_with_sys_paths(agent_path, &extras)
                .map_err(|e| e.to_string())?;
            let validated =
                apollia_aip::validator::validate_agent(&module).map_err(|e| e.to_string())?;
            let allowed_tools = validated.manifest.tools_required.clone();
            let memory_namespace = validated.manifest.memory_namespace.clone();
            let user_memory_write = validated.manifest.user_memory_write;
            // Capture the declarations and the agent directory to wire up
            // ctx.datasources / ctx.templates.
            let datasources_declared = validated.manifest.datasources.clone();
            let templates_declared = validated.manifest.templates.clone();
            let agent_dir = agent_path.parent().map(Path::to_path_buf);
            // Capture the list of declared secrets.
            let secrets_declared = validated.manifest.secrets.clone();
            let bridge = Arc::new(AIPBridge::new(validated).map_err(|e| e.to_string())?);
            Ok(AIPProductionBackend {
                bridge,
                agent_id: agent_id.clone(),
                allowed_tools,
                llm_router,
                event_bus,
                tool_registry,
                audit_trail,
                memory_namespace,
                memory_base_dir: default_memory_dir(),
                pending_approvals,
                task_repository,
                user_memory_write,
                mcp_handle,
                a2a_invoker,
                mailbox,
                tools_config,
                datasources_declared,
                templates_declared,
                agent_dir,
                secrets_declared,
                plan_gates,
                plan_cache,
                manifest: backend_manifest,
            })
        })();

        match result {
            Ok(backend) => DynBackend::new(backend),
            Err(e) => {
                tracing::error!(
                    agent = %agent_id,
                    path = %agent_path.display(),
                    error = %e,
                    detail = "falling back to a no-op backend",
                    "agent.module.load.failed"
                );
                DynBackend::new(NoopBackend)
            }
        }
    }
}

/// Builds the bounded [`StepBudget`] for the direct execution path.
///
/// Caps the agent-declared budget to the runtime ceiling
/// ([`StepBudgetConfig::default`]: 30 steps / 60 tool calls / 600s wall clock)
/// so the desktop never runs an agent under an unlimited budget. Mirrors the
/// CLI helper of the same name and restores principle #7 on this path.
pub(super) fn direct_path_budget(agent_budget: &StepBudgetConfig) -> StepBudget {
    StepBudget::from_capped(agent_budget, &StepBudgetConfig::default())
}

/// Wires the `LlmRouter` and a `Reasoner` into the engine so the orchestrated
/// path can plan. Mirrors the CLI wiring: without a precise backend the engine
/// keeps the router but planning fails with `NO_LLM` if invoked.
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
