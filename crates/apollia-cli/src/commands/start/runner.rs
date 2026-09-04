//! The bridge runner: direct execution, orchestrated steps and plan callbacks.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use apollia_aip::bridge::AIPBridge;
use apollia_aip::context::{
    effective_memory_namespace, DispatcherExecutor, RuntimeContext, ToolProxy, ToolProxyConfig,
};
use apollia_aip::memory::MemoryInterface;
use apollia_core::{AIPResult, AIPTask, AgentManifest};
use apollia_llm::{LlmRouter, ObservabilityConfig, StepBudgetView, ToolCallHelper};
use apollia_memory::manager::MemoryManager;
use apollia_oria::budget::StepBudget;
use apollia_oria::engine::AgentRunner;
use apollia_runtime::eventbus::EventBusSender;
use apollia_runtime::A2AToolsProvider;
use apollia_tools::{
    build_dispatcher_with, load_governance_snapshot, AuditTrailHandle, NativeDispatcherConfig,
    ToolRegistryHandle,
};
use pyo3::prelude::*;

use super::chat_runner::mcp_executors_for;
use super::llm_glue::{merge_disabled, sandbox_roots_for_agent, NoopToolInvoker, RouterModel};
use super::open_secret_store;

// ─────────────────────────────────────────────────────────────
// BridgeRunner: implements AgentRunner for ORIAEngine::execute_direct
// ─────────────────────────────────────────────────────────────

/// Wraps `AIPBridge` + context-building components as an `AgentRunner`.
///
/// Used by `AIPProductionBackend.execute()` to route tasks through
/// `ORIAEngine::execute_direct()`, which adds HITL suspension support
/// without changing the Python contract.
pub(super) struct BridgeRunner {
    pub(super) bridge: Arc<AIPBridge>,
    pub(super) llm_router: Option<Arc<LlmRouter>>,
    pub(super) event_bus: EventBusSender,
    pub(super) agent_id: String,
    /// Manifest snapshot, exposed via [`AIPAgent::manifest`] so ORIA
    /// can drive the orchestrated path.
    pub(super) manifest: AgentManifest,
    pub(super) allowed_tools: Vec<String>,
    pub(super) tool_registry: Option<ToolRegistryHandle>,
    pub(super) audit_trail: Option<AuditTrailHandle>,
    pub(super) memory_namespace: Option<String>,
    pub(super) memory_base_dir: PathBuf,
    /// High-level A2A invoker, `None` if not available.
    pub(super) a2a_invoker: Option<Arc<apollia_runtime::a2a::A2AInvoker>>,
    /// Operator-supplied tools configuration loaded from `apollia.toml`.
    pub(super) tools_config: apollia_core::ToolsConfig,
    /// Filesystem roots an agent may reach, from `[filesystem] trusted_paths`
    /// with `~` already resolved. The first is the anchor for relative paths.
    pub(super) trusted_paths: Vec<std::path::PathBuf>,
    /// Manifest opt-in for `ctx.memory.remember_user()` writes into `__user__`.
    pub(super) user_memory_write: bool,
    /// Datasources declared in the manifest.
    pub(super) datasources_declared: Vec<String>,
    /// Jinja2 templates declared in the manifest.
    pub(super) templates_declared: Vec<String>,
    /// Root directory of the agent, used to resolve `datasources/` and
    /// `templates/`.
    pub(super) agent_dir: Option<PathBuf>,
    /// Secrets declared in the manifest (`manifest.secrets`), the allowlist
    /// for `ctx.secrets.get()`.
    pub(super) secrets_declared: Vec<String>,
    /// Apollia data directory (`~/.apollia/`), used for lazy opening of the
    /// shared [`ToolCredentialStore`] that backs `ctx.secrets`.
    pub(super) secrets_data_dir: Option<PathBuf>,
    /// MCP client manager handle, `None` when no MCP server is configured. Used
    /// at `call_run` to build one executor per registered MCP tool.
    pub(super) mcp_handle: Option<apollia_mcp::manager::McpClientManagerHandle>,
    /// Direct-path `StepBudget`, shared with `execute_direct`. A live view of it
    /// is wired into the agent's `ctx.tools` and `ctx.llm` so the Python agent's
    /// tool and LLM calls are counted and cut off (principle #7, non-bypassable).
    pub(super) budget: Arc<StepBudget>,
}

impl BridgeRunner {
    /// Builds the governed [`ToolProxy`] used to execute orchestrated plan steps.
    ///
    /// Mirrors the proxy `call_run` builds for the direct/ctx path so the
    /// orchestrated `ActorLoop` runs tools under the same governed path,
    /// audit trail, disabled-tool set, and A2A routing. Returns `None` in
    /// degraded mode (tool registry or audit trail unavailable), matching
    /// `call_run`, in which case orchestrated tool steps fall back to the
    /// engine's `NoopToolProxy`.
    pub(super) async fn build_tool_proxy(&self, task: &AIPTask) -> Option<ToolProxy> {
        let (registry, audit) = match (self.tool_registry.as_ref(), self.audit_trail.as_ref()) {
            (Some(r), Some(a)) => (r.clone(), a.clone()),
            _ => {
                tracing::warn!(
                    agent = %self.agent_id,
                    reason = "tool registry or audit trail missing",
                    detail = "orchestrated tool steps fall back to the no-op proxy",
                    "orchestration.tool_proxy.unavailable"
                );
                return None;
            }
        };

        // Extend allowed_tools with virtual A2A skill names before creating the proxy.
        let mut allowed_tools = self.allowed_tools.clone();
        if let Some(ref invoker) = self.a2a_invoker {
            let a2a_descriptors = A2AToolsProvider::new(Arc::clone(invoker))
                .build_tool_descriptors()
                .await;
            for desc in a2a_descriptors {
                allowed_tools.push(desc.name);
            }
        }

        let governance_base = self
            .memory_base_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.memory_base_dir.clone());
        let snapshot = load_governance_snapshot(&governance_base).unwrap_or_else(|e| {
            tracing::warn!(
                error = %e,
                detail = "every tool is enabled",
                "tools.governance.unavailable"
            );
            Default::default()
        });
        let disabled_tools = merge_disabled(&self.tools_config.disabled, snapshot.disabled_tools);
        let extra_executors = mcp_executors_for(&self.mcp_handle).await;
        let dispatcher = Arc::new(build_dispatcher_with(
            &NativeDispatcherConfig {
                sandbox_roots: sandbox_roots_for_agent(&self.trusted_paths),
                agent_id: self.agent_id.clone(),
                venv_base_dir: self
                    .memory_base_dir
                    .parent()
                    .map(|p| p.join("venvs"))
                    .unwrap_or_else(|| self.memory_base_dir.join("venvs")),
                memory_namespace: self.memory_namespace.clone(),
                memory_shared_namespaces: Vec::new(),
                memory_base_dir: self.memory_base_dir.clone(),
                http_allowlist: None,
                pending_user_inputs: None,
                disabled_tools,
                brave_api_key: snapshot.brave_api_key,
                web_search_config: self.tools_config.web_search.clone(),
                web_read_config: self.tools_config.web_read.clone(),
                governance_db_path: Some(
                    governance_base.join(apollia_tools::GOVERNANCE_DB_FILENAME),
                ),
            },
            extra_executors,
        ));

        let proxy = ToolProxy::new(ToolProxyConfig {
            registry,
            audit,
            executor: Arc::new(DispatcherExecutor::new(dispatcher)),
            allowed_tools,
            agent_id: self.agent_id.clone(),
            task_id: task.task_id.clone(),
            run_id: task.run_id.clone(),
        })
        .with_event_bus(self.event_bus.clone());
        let proxy = if let Some(invoker) = self.a2a_invoker.clone() {
            proxy.with_a2a(invoker, 0, None)
        } else {
            proxy
        };
        Some(proxy)
    }
}

impl AgentRunner for BridgeRunner {
    fn call_run(
        &self,
        task: AIPTask,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send + '_>> {
        let bridge = Arc::clone(&self.bridge);
        let llm_router = self.llm_router.clone();
        let event_bus = self.event_bus.clone();
        let agent_id = self.agent_id.clone();
        let allowed_tools = self.allowed_tools.clone();
        let tool_registry = self.tool_registry.clone();
        let audit_trail = self.audit_trail.clone();
        let memory_namespace = self.memory_namespace.clone();
        let memory_config = self.manifest.memory_config.clone();
        let memory_base_dir = self.memory_base_dir.clone();
        let a2a_invoker = self.a2a_invoker.clone();
        let tools_config = self.tools_config.clone();
        let trusted_paths = self.trusted_paths.clone();
        let user_memory_write = self.user_memory_write;
        let datasources_declared = self.datasources_declared.clone();
        let templates_declared = self.templates_declared.clone();
        let agent_dir = self.agent_dir.clone();
        let secrets_declared = self.secrets_declared.clone();
        let secrets_data_dir = self.secrets_data_dir.clone();
        let mcp_handle = self.mcp_handle.clone();
        // Live view of the Direct-path budget, shared into ctx.tools and ctx.llm.
        let budget_view = Arc::new(self.budget.to_live_budget_view());

        Box::pin(async move {
            let router_for_helper = llm_router
                .clone()
                .unwrap_or_else(|| Arc::new(LlmRouter::empty()));
            let tool_helper = Arc::new(ToolCallHelper::new(
                Arc::new(RouterModel(router_for_helper)),
                Arc::new(NoopToolInvoker),
            ));

            // Extend allowed_tools with virtual A2A skill names before creating the proxy.
            let mut allowed_tools = allowed_tools;
            if let Some(ref invoker) = a2a_invoker {
                let a2a_descriptors = A2AToolsProvider::new(Arc::clone(invoker))
                    .build_tool_descriptors()
                    .await;
                for desc in a2a_descriptors {
                    allowed_tools.push(desc.name);
                }
            }

            let governance_base = memory_base_dir
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| memory_base_dir.clone());
            let snapshot = load_governance_snapshot(&governance_base).unwrap_or_else(|e| {
                tracing::warn!(
                    error = %e,
                    detail = "every tool is enabled",
                    "tools.governance.unavailable"
                );
                Default::default()
            });
            let disabled_tools = merge_disabled(&tools_config.disabled, snapshot.disabled_tools);
            // Inject one MCP executor per registered tool so `ctx.tools.call("mcp:...")`
            // routes through the MCP client manager instead of returning UnknownTool.
            let extra_executors = mcp_executors_for(&mcp_handle).await;
            let dispatcher = Arc::new(build_dispatcher_with(
                &NativeDispatcherConfig {
                    sandbox_roots: sandbox_roots_for_agent(&trusted_paths),
                    agent_id: agent_id.clone(),
                    venv_base_dir: memory_base_dir
                        .parent()
                        .map(|p| p.join("venvs"))
                        .unwrap_or_else(|| memory_base_dir.join("venvs")),
                    memory_namespace: memory_namespace.clone(),
                    memory_shared_namespaces: Vec::new(),
                    memory_base_dir: memory_base_dir.clone(),
                    http_allowlist: None,
                    pending_user_inputs: None,
                    disabled_tools,
                    brave_api_key: snapshot.brave_api_key,
                    web_search_config: tools_config.web_search.clone(),
                    web_read_config: tools_config.web_read.clone(),
                    governance_db_path: Some(
                        governance_base.join(apollia_tools::GOVERNANCE_DB_FILENAME),
                    ),
                },
                extra_executors,
            ));

            let tool_proxy: Option<ToolProxy> = match (tool_registry.as_ref(), audit_trail.as_ref())
            {
                (Some(registry), Some(audit)) => {
                    let proxy = ToolProxy::new(ToolProxyConfig {
                        registry: registry.clone(),
                        audit: audit.clone(),
                        executor: Arc::new(DispatcherExecutor::new(dispatcher)),
                        allowed_tools,
                        agent_id: agent_id.clone(),
                        task_id: task.task_id.clone(),
                        run_id: task.run_id.clone(),
                    })
                    // tool_call_* instrumentation.
                    .with_event_bus(event_bus.clone())
                    // Direct-path budget: count and cap the agent's tool calls.
                    .with_budget(Arc::clone(&budget_view));
                    let proxy = if let Some(invoker) = a2a_invoker.clone() {
                        proxy.with_a2a(invoker, 0, None)
                    } else {
                        proxy
                    };
                    Some(proxy)
                }
                _ => {
                    tracing::warn!(
                        agent = %agent_id,
                        reason = "tool registry or audit trail missing",
                        detail = "the agent falls back to its own tool calls",
                        "agent.tool_proxy.unavailable"
                    );
                    None
                }
            };

            let memory_interface: Option<MemoryInterface> =
                memory_namespace.as_deref().and_then(|ns| {
                    let eff_ns = effective_memory_namespace(ns, task.project_id.as_deref());
                    let mut manager =
                        MemoryManager::new(&memory_base_dir, Some(eff_ns.clone()), vec![]);
                    if let Some(ref memory_config) = memory_config {
                        manager.start_auto_purge(memory_config, &eff_ns);
                    }
                    let iface = MemoryInterface::new(manager, eff_ns, agent_id.clone())?;
                    iface.announce_shared_namespaces(&event_bus);
                    Some(iface)
                });

            let profile_interface = {
                let data_dir = secrets_data_dir.clone().unwrap_or_else(|| {
                    let home = apollia_core::paths::home_dir_or_temp()
                        .display()
                        .to_string();
                    apollia_core::paths::data_dir_under(home)
                });
                let user_memory_db =
                    data_dir.join(apollia_core::paths::DataFile::UserMemory.file_name());
                apollia_aip::profile::ProfileInterface::new(
                    user_memory_db,
                    agent_id.clone(),
                    user_memory_write,
                    agent_id == "onboarding-agent",
                )
            };

            let built = Python::with_gil(|py| {
                let ctx = RuntimeContext::new_with_llm(
                    llm_router,
                    Arc::clone(&budget_view),
                    tool_helper,
                    Arc::new(ObservabilityConfig::default()),
                    event_bus,
                    agent_id.clone().into(),
                    tool_proxy,
                    memory_interface,
                    None, // mailbox: not wired in task mode
                    agent_id,
                    None, // user_context: task mode, not chat
                    a2a_invoker,
                    user_memory_write, // user_memory_writable: manifest-controlled
                )
                .with_profile(profile_interface)
                // Datasources YAML + templates Jinja2.
                .with_datasources(datasources_declared, agent_dir.as_deref())
                .with_templates(templates_declared, agent_dir.as_deref())
                // ctx.secrets read-only, gated by the manifest.
                .with_secrets(apollia_aip::secrets::SecretsInterface::new(
                    secrets_data_dir.as_deref().and_then(open_secret_store),
                    secrets_declared,
                ))
                // task_id used to label ctx.log() in the trace.
                .with_task_id(task.task_id.clone())
                .with_run_id(task.run_id.clone());
                Py::new(py, ctx).map(|p| p.into_any())
            });
            let ctx: PyObject = match built {
                Ok(object) => object,
                Err(error) => return Err(format!("RuntimeContext construction failed: {error}")),
            };

            bridge.call_run(&task, ctx).await.map_err(|e| e.to_string())
        })
    }
}

impl apollia_oria::engine::AIPAgent for BridgeRunner {
    fn manifest(&self) -> AgentManifest {
        self.manifest.clone()
    }

    fn has_on_plan_complete(&self) -> bool {
        self.bridge.has_on_plan_complete()
    }

    fn call_on_plan_complete(
        &self,
        step_results: std::collections::HashMap<String, String>,
    ) -> Pin<Box<dyn std::future::Future<Output = AIPResult> + Send + '_>> {
        // ORIA always calls this hook with an already-built ctx through the
        // bridge. Here we build a minimal ctx: the orchestrated plan step
        // outputs are formatted by the agent on its own, no `ctx.tools`
        // need to be wired for the post-processing hook.
        //
        // The unlimited view below is not a budget hole: the LLM router and the
        // tool proxy are both passed as `None` just after it, so this ctx has no
        // `ctx.llm` and no `ctx.tools`, the only two chokepoints that charge a
        // view. It backs `ctx.budget` reporting inside the hook and nothing else.
        let bridge = Arc::clone(&self.bridge);
        let agent_id = self.agent_id.clone();
        let event_bus = self.event_bus.clone();

        Box::pin(async move {
            let built = Python::with_gil(|py| {
                let ctx = RuntimeContext::new_with_llm(
                    None,
                    Arc::new(StepBudgetView::unlimited()),
                    Arc::new(ToolCallHelper::new(
                        Arc::new(RouterModel(Arc::new(LlmRouter::empty()))),
                        Arc::new(NoopToolInvoker),
                    )),
                    Arc::new(ObservabilityConfig::default()),
                    event_bus,
                    agent_id.clone().into(),
                    None,
                    None,
                    None,
                    agent_id.clone(),
                    None,
                    None,
                    false,
                );
                Py::new(py, ctx).map(|p| p.into_any())
            });
            let ctx: PyObject = match built {
                Ok(object) => object,
                Err(error) => {
                    return AIPResult::failed(
                        "ON_PLAN_COMPLETE_FAILED",
                        &format!("RuntimeContext construction failed: {error}"),
                    )
                }
            };

            match bridge.call_on_plan_complete(step_results, ctx).await {
                Ok(result) => result,
                Err(e) => AIPResult::failed("ON_PLAN_COMPLETE_FAILED", &e.to_string()),
            }
        })
    }
}
