//! The chat-agent runner: one PyO3 bridge per turn, plus its execution stubs.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use apollia_aip::bridge::AIPBridge;
use apollia_aip::context::{
    effective_memory_namespace, DispatcherExecutor, RuntimeContext, ToolProxy, ToolProxyConfig,
};
use apollia_aip::memory::MemoryInterface;
use apollia_core::{AIPResult, AIPTask, AgentManifest, TaskStatus};
use apollia_llm::{LlmRouter, ObservabilityConfig, StepBudgetView, ToolCallHelper};
use apollia_memory::manager::MemoryManager;
use apollia_runtime::coordinator::ExecutionBackend;
use apollia_runtime::eventbus::EventBusSender;
use apollia_tools::tools::ask_user::PendingUserInputs;
use apollia_tools::{
    build_dispatcher_with, load_governance_snapshot, AuditTrailHandle, NativeDispatcherConfig,
    ToolRegistryHandle,
};
use pyo3::prelude::*;

use super::engine::direct_path_budget;
use super::llm_glue::{merge_disabled, sandbox_roots_for_agent, NoopToolInvoker, RouterModel};
use super::open_secret_store;

// ─────────────────────────────────────────────────────────────
// AIPChatAgentRunner: concrete ChatAgentRunner for Chat Agent mode.
// ─────────────────────────────────────────────────────────────

/// Concrete [`ChatAgentRunner`] implementation using PyO3 + AIPBridge.
///
/// Loads the Python agent from `data_dir/agents/<name>/`, validates AIP duck
/// typing, builds a `RuntimeContext` with tools/memory/LLM, and calls `run()`.
///
/// Uses the same `OnceLock` pattern as [`ProductionBackendFactory`] to access
/// runtime handles created inside `supervisor.start()`.
pub(super) struct AIPChatAgentRunner {
    /// EventBus sender, populated after supervisor.start().
    pub(super) event_bus: Arc<std::sync::OnceLock<EventBusSender>>,
    /// LLM router, populated after supervisor.start().
    pub(super) llm_router: Arc<std::sync::OnceLock<Option<Arc<LlmRouter>>>>,
    /// Tool registry, populated after supervisor.start().
    pub(super) tool_registry: Arc<std::sync::OnceLock<ToolRegistryHandle>>,
    /// Audit trail, populated after supervisor.start().
    pub(super) audit_trail: Arc<std::sync::OnceLock<AuditTrailHandle>>,
    /// Global user memory repository, populated after supervisor.start().
    pub(super) user_memory: Arc<
        std::sync::OnceLock<
            Option<Arc<std::sync::Mutex<apollia_memory::user_memory::UserMemoryRepository>>>,
        >,
    >,
    /// Chat manager's `ask_user` pending-input registry, populated after
    /// `supervisor.start()` so the native dispatcher can wire
    /// `AskUserExecutor` to the chat HITL loop.
    pub(super) pending_user_inputs: Arc<std::sync::OnceLock<PendingUserInputs>>,
    /// Agent registry, required to build the A2A delegate + invoker so chat-agent
    /// Python agents get the same A2A capabilities as task-mode agents.
    pub(super) agent_registry:
        Arc<std::sync::OnceLock<apollia_runtime::registry::AgentRegistryHandle>>,
    /// Task router, required to build the A2A delegate.
    pub(super) task_router: Arc<
        std::sync::OnceLock<
            apollia_runtime::router::TaskRouterHandle<apollia_runtime::coordinator::DynBackend>,
        >,
    >,
    /// Base data directory (e.g. `~/.apollia/`).
    pub(super) data_dir: PathBuf,
    /// Operator-supplied tools configuration loaded from `apollia.toml`.
    /// Drives `disabled` tools and `web_search` / `web_read` parameters.
    pub(super) tools_config: apollia_core::ToolsConfig,
    /// Filesystem roots an agent may reach, from `[filesystem] trusted_paths`
    /// with `~` already resolved. The first is the anchor for relative paths.
    pub(super) trusted_paths: Vec<PathBuf>,
    /// MCP client manager handle, populated after `supervisor.start()` so the
    /// chat-agent dispatcher can route `mcp:<server>/<tool>` invocations.
    pub(super) mcp_handle:
        Arc<std::sync::OnceLock<Option<apollia_mcp::manager::McpClientManagerHandle>>>,
}

#[async_trait::async_trait]
impl apollia_runtime::chat::ChatAgentRunner for AIPChatAgentRunner {
    async fn run_agent(&self, agent_name: &str, task: AIPTask) -> Result<AIPResult, String> {
        let agent_path = self.data_dir.join("agents").join(agent_name);

        // Load and validate agent via PyO3. Re-use the shared community helper
        // so the chat-agent runner sees the same sys.path as `agent install`
        // (per-agent venv + enclosing package root + workspace SDK fallback).
        let extras = crate::community::validation_sys_paths(&agent_path);
        let module = apollia_aip::loader::load_agent_module_with_sys_paths(&agent_path, &extras)
            .map_err(|e| e.to_string())?;
        let validated =
            apollia_aip::validator::validate_agent(&module).map_err(|e| e.to_string())?;
        let manifest = validated.manifest.clone();
        let bridge = Arc::new(AIPBridge::new(validated).map_err(|e| e.to_string())?);

        // Get runtime handles from OnceLocks
        let event_bus = self
            .event_bus
            .get()
            .cloned()
            .ok_or("event bus not initialized - chat agent called before runtime ready")?;
        let llm_router = self.llm_router.get().cloned().flatten();
        let tool_registry = self.tool_registry.get().cloned();
        let audit_trail = self.audit_trail.get().cloned();

        // Build RuntimeContext components
        let router_for_helper = llm_router
            .clone()
            .unwrap_or_else(|| Arc::new(LlmRouter::empty()));
        let tool_helper = Arc::new(ToolCallHelper::new(
            Arc::new(RouterModel(router_for_helper)),
            Arc::new(NoopToolInvoker),
        ));

        let allowed_tools: Vec<String> = manifest
            .tools_required
            .iter()
            .chain(manifest.tools_optional.iter())
            .cloned()
            .collect();

        let memory_base_dir = self.data_dir.join("memory");
        let snapshot = load_governance_snapshot(&self.data_dir).unwrap_or_else(|e| {
            tracing::warn!(
                error = %e,
                detail = "every tool is enabled",
                "tools.governance.unavailable"
            );
            Default::default()
        });
        let disabled_tools = merge_disabled(&self.tools_config.disabled, snapshot.disabled_tools);
        // Inject one MCP executor per registered tool so `ctx.tools.call("mcp:...")`
        // routes through the MCP client manager instead of returning UnknownTool.
        let mcp_handle = self.mcp_handle.get().cloned().flatten();
        let extra_executors = mcp_executors_for(&mcp_handle).await;
        let dispatcher = Arc::new(build_dispatcher_with(
            &NativeDispatcherConfig {
                sandbox_roots: sandbox_roots_for_agent(&self.trusted_paths),
                agent_id: agent_name.to_string(),
                venv_base_dir: self.data_dir.join("venvs"),
                memory_namespace: manifest.memory_namespace.clone(),
                memory_shared_namespaces: Vec::new(),
                memory_base_dir: memory_base_dir.clone(),
                http_allowlist: None,
                pending_user_inputs: self.pending_user_inputs.get().cloned(),
                disabled_tools,
                brave_api_key: snapshot.brave_api_key,
                web_search_config: self.tools_config.web_search.clone(),
                web_read_config: self.tools_config.web_read.clone(),
                governance_db_path: Some(self.data_dir.join(apollia_tools::GOVERNANCE_DB_FILENAME)),
            },
            extra_executors,
        ));

        // Build A2A delegate + invoker so chat-agent Python agents can call
        // `ctx.a2a_invoke(...)` and `ctx.tools.invoke("a2a:<skill>")` on parity
        // with task-mode (triggers/API). Fixes the previous "not available in
        // chat mode" gap.
        let a2a_invoker = match (
            self.agent_registry.get().cloned(),
            self.task_router.get().cloned(),
        ) {
            (Some(registry), Some(router)) => {
                let invoker = Arc::new(apollia_runtime::a2a::A2AInvoker::new(
                    registry,
                    router,
                    event_bus.clone(),
                    apollia_core::A2AConfig::default(),
                ));
                Some(invoker)
            }
            _ => {
                tracing::warn!(
                    agent = %agent_name,
                    reason = "registry or router not initialised yet",
                    "chat.a2a.invoker.unavailable"
                );
                None
            }
        };

        // Augment allowed_tools with live A2A virtual skills so the ToolProxy
        // accepts `a2a:*` calls (the registry filter would otherwise reject them).
        let mut allowed_tools = allowed_tools;
        if let Some(ref invoker) = a2a_invoker {
            let descriptors = apollia_runtime::a2a::A2AToolsProvider::new(Arc::clone(invoker))
                .build_tool_descriptors()
                .await;
            for desc in descriptors {
                if !allowed_tools.iter().any(|n| n == &desc.name) {
                    allowed_tools.push(desc.name);
                }
            }
        }

        // Direct-path budget (principle #7): a Chat Agent turn on the CLI daemon
        // runs under the same runtime ceiling the desktop already applies to its
        // own chat-agent turns. The view is shared into ctx.tools and ctx.llm, so
        // the agent's tool and LLM calls are counted against a real budget.
        let budget_view = Arc::new(chat_agent_budget_view(&manifest));

        let tool_proxy: Option<ToolProxy> = match (tool_registry.as_ref(), audit_trail.as_ref()) {
            (Some(registry), Some(audit)) => {
                let proxy = ToolProxy::new(ToolProxyConfig {
                    registry: registry.clone(),
                    audit: audit.clone(),
                    executor: Arc::new(DispatcherExecutor::new(dispatcher)),
                    allowed_tools,
                    agent_id: agent_name.to_string(),
                    task_id: task.task_id.clone(),
                    run_id: task.run_id.clone(),
                })
                // tool_call_* instrumentation.
                .with_event_bus(event_bus.clone())
                // Direct-path budget: count and cap the agent's tool calls.
                .with_budget(Arc::clone(&budget_view));
                let proxy = if let Some(ref invoker) = a2a_invoker {
                    proxy.with_a2a(Arc::clone(invoker), 0, None)
                } else {
                    proxy
                };
                Some(proxy)
            }
            _ => None,
        };
        let memory_interface: Option<MemoryInterface> =
            manifest.memory_namespace.as_deref().and_then(|ns| {
                let eff_ns = effective_memory_namespace(ns, task.project_id.as_deref());
                let mut manager =
                    MemoryManager::new(&memory_base_dir, Some(eff_ns.clone()), vec![]);
                // `manifest.memory_config.auto_purge` promises a purge pass when
                // the manager starts; this is the call that keeps the promise.
                if let Some(ref memory_config) = manifest.memory_config {
                    manager.start_auto_purge(memory_config, &eff_ns);
                }
                // Always attach the global __user__ store so every agent can
                let iface = MemoryInterface::new(manager, eff_ns, agent_name.to_string())?;
                iface.announce_shared_namespaces(&event_bus);
                Some(iface)
            });

        // Build user_context from UserMemoryRepository (chat mode only).
        let user_context = self
            .user_memory
            .get()
            .and_then(|opt| opt.as_ref())
            .and_then(|repo_mutex| {
                let repo = repo_mutex.lock().ok()?;
                build_user_context_from_repo(&repo)
            });

        let profile_interface = {
            let user_memory_db = self
                .data_dir
                .join(apollia_core::paths::DataFile::UserMemory.file_name());
            apollia_aip::profile::ProfileInterface::new(
                user_memory_db,
                agent_name.to_string(),
                manifest.user_memory_write,
                agent_name == "onboarding-agent",
            )
        };

        // Directory containing the agent .py, used to resolve datasources/
        // and templates/ files relative to the agent.
        let agent_dir = agent_path.parent().map(Path::to_path_buf);
        let datasources_declared = manifest.datasources.clone();
        let templates_declared = manifest.templates.clone();
        // Secrets allowlist + shared credential store.
        let secrets_declared = manifest.secrets.clone();
        let secret_store = open_secret_store(&self.data_dir);

        let built = Python::with_gil(|py| {
            let ctx = RuntimeContext::new_with_llm(
                llm_router,
                Arc::clone(&budget_view),
                tool_helper,
                Arc::new(ObservabilityConfig::default()),
                event_bus,
                agent_name.to_string().into(),
                tool_proxy,
                memory_interface,
                None, // mailbox: not available in chat runner context
                agent_name.to_string(),
                user_context,
                a2a_invoker,
                manifest.user_memory_write, // user_memory_writable: manifest-controlled
            )
            .with_profile(profile_interface)
            // Datasources YAML + templates Jinja2.
            .with_datasources(datasources_declared, agent_dir.as_deref())
            .with_templates(templates_declared, agent_dir.as_deref())
            // ctx.secrets read-only, gated by the manifest.
            .with_secrets(apollia_aip::secrets::SecretsInterface::new(
                secret_store,
                secrets_declared,
            ))
            // Bind the context to the task so ctx.log() labels the persisted
            // RuntimeEvent::AgentLog entries.
            .with_task_id(task.task_id.clone())
            .with_run_id(task.run_id.clone())
            .with_mailbox_capability(
                manifest.supports_mailbox,
                manifest.mailbox_allowlist.clone(),
                manifest
                    .tools_requiring_approval
                    .iter()
                    .any(|t| t == "mailbox:send"),
            );
            Py::new(py, ctx).map(|p| p.into_any())
        });
        let ctx: PyObject = match built {
            Ok(object) => object,
            Err(error) => return Err(format!("RuntimeContext construction failed: {error}")),
        };

        bridge.call_run(&task, ctx).await.map_err(|e| e.to_string())
    }
}

/// The live budget view a Chat Agent turn runs under on the CLI daemon.
///
/// [`direct_path_budget`] clamps what the manifest asks for against the runtime
/// ceiling, the same clamp the desktop applies to its own chat-agent turns and
/// the one task mode already applies through the engine. The returned view
/// shares the budget's counters, so the tool proxy and `ctx.llm` charge the very
/// budget they are checked against (principle #7, non-bypassable).
///
/// Only the two counted dimensions bite here: steps and tool calls. The
/// wall-clock dimension is supervised by the engine, which the chat path does
/// not go through.
pub(super) fn chat_agent_budget_view(manifest: &AgentManifest) -> StepBudgetView {
    direct_path_budget(&manifest.step_budget.clone().unwrap_or_default()).to_live_budget_view()
}

/// Builds the `user_context` dict from a [`UserMemoryRepository`].
///
/// Returns a `HashMap` with a single `"profile"` key mapping to the flat
/// list of `(key, value)` pairs from [`UserMemoryRepository::list_all`].
/// Returns `None` if the profile is empty.
pub(super) fn build_user_context_from_repo(
    repo: &apollia_memory::user_memory::UserMemoryRepository,
) -> Option<std::collections::HashMap<String, Vec<(String, String)>>> {
    let entries = repo.list_all().unwrap_or_default();
    if entries.is_empty() {
        return None;
    }
    let mut map = std::collections::HashMap::new();
    map.insert(
        "profile".to_string(),
        entries.into_iter().map(|e| (e.key, e.value)).collect(),
    );
    Some(map)
}

/// Build MCP tool executors for an agent dispatcher, or an empty `Vec` when no
/// MCP handle is wired.
///
/// Delegates to the canonical `apollia_mcp` assembly so the CLI standalone-agent
/// path stays in lockstep with the chat and desktop dispatchers. Without this,
/// the registry surfaces `mcp:<server>/<tool>` to the agent but the dispatcher
/// returns `UnknownTool` at call time.
pub(super) async fn mcp_executors_for(
    mcp_handle: &Option<apollia_mcp::manager::McpClientManagerHandle>,
) -> Vec<Box<dyn apollia_tools::executor::ToolExecutor>> {
    match mcp_handle {
        Some(handle) => apollia_mcp::executor::build_agent_tool_executors(handle).await,
        None => Vec::new(),
    }
}

/// Fallback backend, only used when agent loading fails at start time.
///
/// Returns a `Failed` result immediately without calling Python.
#[derive(Clone)]
pub(super) struct NoopBackend;

impl ExecutionBackend for NoopBackend {
    fn execute(
        &self,
        task: AIPTask,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>> {
        Box::pin(async move {
            Ok(AIPResult {
                task_id: task.task_id,
                status: TaskStatus::Failed,
                output: Vec::new(),
                error: Some(apollia_core::AIPError {
                    code: "NO_BACKEND".to_string(),
                    message: "no execution backend configured for this agent".to_string(),
                    details: None,
                }),
                artifacts: Vec::new(),
                input_required_data: None,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::StepBudgetConfig;

    /// Minimal manifest, budget dimension aside, matching the shape the chat
    /// runner reads out of a validated agent.
    fn manifest_with_budget(step_budget: Option<StepBudgetConfig>) -> AgentManifest {
        AgentManifest {
            format_version: 1,
            name: "budget-fixture".to_string(),
            version: "0.1.0".to_string(),
            description: "chat-agent budget fixture".to_string(),
            tools_required: vec![],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
            execution_mode: "direct".to_string(),
            supports_mailbox: false,
            mailbox_allowlist: None,
            system_prompt: None,
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

    #[test]
    fn test_chat_agent_budget_view_is_capped_when_the_manifest_is_silent() {
        // GIVEN a chat agent whose manifest declares no step budget
        let manifest = manifest_with_budget(None);

        // WHEN the chat runner builds the view it shares into ctx.tools and ctx.llm
        let view = chat_agent_budget_view(&manifest);

        // THEN the view carries the runtime ceiling, not the unlimited one
        let ceiling = StepBudgetConfig::default();
        assert_eq!(
            view.steps_remaining(),
            i64::from(ceiling.max_steps),
            "an unlimited view would leave u32::MAX steps on the table"
        );
        assert_eq!(
            view.tool_calls_remaining(),
            i64::from(ceiling.max_tool_calls),
            "an unlimited view would leave u32::MAX tool calls on the table"
        );
    }

    #[test]
    fn test_chat_agent_budget_view_caps_an_oversized_manifest() {
        // GIVEN a chat agent asking for far more than the runtime ceiling
        let manifest = manifest_with_budget(Some(StepBudgetConfig {
            max_steps: 100_000,
            max_tool_calls: 100_000,
            wall_clock_secs: 999_999,
        }));

        // WHEN the chat runner builds its budget view
        let view = chat_agent_budget_view(&manifest);

        // THEN the runtime ceiling wins on both counted dimensions
        let ceiling = StepBudgetConfig::default();
        assert_eq!(view.steps_remaining(), i64::from(ceiling.max_steps));
        assert_eq!(
            view.tool_calls_remaining(),
            i64::from(ceiling.max_tool_calls)
        );
    }

    #[test]
    fn test_chat_agent_budget_view_charges_the_same_counters_it_checks() {
        // GIVEN the view a chat turn hands to both the tool proxy and ctx.llm
        let view = chat_agent_budget_view(&manifest_with_budget(Some(StepBudgetConfig {
            max_steps: 2,
            max_tool_calls: 2,
            wall_clock_secs: 60,
        })));

        // WHEN the two chokepoints spend their allowance
        view.increment_steps();
        view.increment_steps();
        view.increment_tool_calls();
        view.increment_tool_calls();

        // THEN the budget reads as exhausted, which is what the proxies check
        assert!(view.is_exhausted(), "steps must run out");
        assert_eq!(view.tool_calls_remaining(), 0, "tool calls must run out");
    }
}
