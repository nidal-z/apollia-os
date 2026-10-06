//! One backend per agent, the paths the runtime reads, and the port and socket probes.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use apollia_aip::bridge::AIPBridge;
use apollia_core::{AgentManifest, PendingApprovals};
use apollia_llm::LlmRouter;
use apollia_runtime::api::routes_agents::AgentBackendFactory;
use apollia_runtime::coordinator::DynBackend;
use apollia_runtime::eventbus::EventBusSender;
use apollia_runtime::registry::AgentRegistryHandle;
use apollia_runtime::router::TaskRouterHandle;
use apollia_tools::{AuditTrailHandle, TaskRepository, ToolRegistryHandle};

use super::chat_runner::NoopBackend;
use super::engine::AIPProductionBackend;

// ─────────────────────────────────────────────────────────────
// Factory: creates one AIPProductionBackend per agent at `agent start`
// ─────────────────────────────────────────────────────────────

/// Creates a real `AIPProductionBackend` per agent.
///
/// Called once from `POST /api/v1/agents`: loads Python, validates AIP duck typing,
/// and bakes an `AIPBridge` into a backend registered with the `TaskRouter`.
///
/// Uses `OnceLock` for `event_bus` and `llm_router` because they are created
/// inside `supervisor.start()`, which runs after this factory is constructed.
/// Both locks are populated before the first HTTP request arrives.
pub(super) struct ProductionBackendFactory {
    pub(super) event_bus: Arc<std::sync::OnceLock<EventBusSender>>,
    pub(super) llm_router: Arc<std::sync::OnceLock<Option<Arc<LlmRouter>>>>,
    pub(super) tool_registry: Arc<std::sync::OnceLock<ToolRegistryHandle>>,
    pub(super) audit_trail: Arc<std::sync::OnceLock<AuditTrailHandle>>,
    pub(super) pending_approvals: Arc<std::sync::OnceLock<Arc<PendingApprovals>>>,
    pub(super) plan_gates: Arc<std::sync::OnceLock<Arc<apollia_oria::PendingPlanGates>>>,
    #[allow(clippy::type_complexity)]
    pub(super) plan_cache: Arc<
        std::sync::OnceLock<Arc<std::sync::Mutex<apollia_oria::plan_cache::PlanCacheRepository>>>,
    >,
    pub(super) task_repository: Arc<std::sync::OnceLock<Arc<TaskRepository>>>,
    /// Agent registry handle, populated after supervisor.start().
    pub(super) registry: Arc<std::sync::OnceLock<AgentRegistryHandle>>,
    /// Task router handle, populated after supervisor.start().
    pub(super) router: Arc<std::sync::OnceLock<TaskRouterHandle<DynBackend>>>,
    /// Operator-supplied tools configuration loaded from `apollia.toml`, read
    /// at each run so a saved change applies to the next one.
    pub(super) tools_config: apollia_runtime::config_reload::Live<apollia_core::ToolsConfig>,
    /// Base data directory (`~/.apollia/`), used to open the shared
    /// [`ToolCredentialStore`] on each agent execution (`ctx.secrets`).
    pub(super) data_dir: PathBuf,
    /// MCP client manager handle, populated after `supervisor.start()`. Threaded
    /// into each `AIPProductionBackend` so agent dispatchers can execute MCP tools.
    pub(super) mcp_handle:
        Arc<std::sync::OnceLock<Option<apollia_mcp::manager::McpClientManagerHandle>>>,
    /// The governance actor, populated after `supervisor.start()`, read when a
    /// run starts.
    pub(super) governance: Arc<std::sync::OnceLock<apollia_runtime::governance::GovernanceHandle>>,
}

impl AgentBackendFactory for ProductionBackendFactory {
    fn create_for_agent(&self, agent_path: &Path, manifest: &AgentManifest) -> DynBackend {
        let agent_id = manifest.name.clone();

        // Retrieve the lazily-initialized event bus and LLM router.
        //
        // The OnceLocks are populated by start.rs *after* the Supervisor has
        // run its Phase 11 auto-load. That phase imports nothing, so this
        // factory only runs when a task first reaches an agent; should one
        // arrive before the locks are set, we return a placeholder
        // NoopBackend, and [`rewire_auto_loaded_agents`] replaces the
        // coordinator right after. The diagnostic is therefore DEBUG
        // (operator-relevant only when troubleshooting the rewire path), not
        // ERROR.
        let event_bus = match self.event_bus.get() {
            Some(bus) => bus.clone(),
            None => {
                tracing::debug!(
                    agent = %agent_id,
                    detail = "a placeholder no-op backend is emitted and rewired after start",
                    "agent.factory.premature"
                );
                return DynBackend::new(NoopBackend);
            }
        };
        let llm_router = self.llm_router.get().cloned().flatten();
        let tool_registry = self.tool_registry.get().cloned();
        let audit_trail = self.audit_trail.get().cloned();
        let pending_approvals = self.pending_approvals.get().cloned();
        let plan_gates = self.plan_gates.get().cloned();
        let plan_cache = self.plan_cache.get().cloned();
        let task_repository = self.task_repository.get().cloned();
        let mcp_handle = self.mcp_handle.get().cloned().flatten();
        let governance = self.governance.get().cloned();

        // Build the A2A invoker if registry + router are available.
        let a2a_invoker = match (self.registry.get().cloned(), self.router.get().cloned()) {
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
                    agent = %agent_id,
                    reason = "registry or router not initialised yet",
                    "agent.a2a.invoker.unavailable"
                );
                None
            }
        };

        let result: Result<AIPProductionBackend, String> = (|| {
            // Re-use the community helper so the ProductionBackendFactory
            // sees the same sys.path layering as `agent install / validate`.
            let extras = crate::community::validation_sys_paths(agent_path);
            let module = apollia_aip::loader::load_agent_module_with_sys_paths(agent_path, &extras)
                .map_err(|e| e.to_string())?;
            let validated =
                apollia_aip::validator::validate_agent(&module).map_err(|e| e.to_string())?;
            let access = apollia_tools::GrantedAccess::new(
                Some(apollia_core::paths::DataFile::Agents.path(&self.data_dir)),
                &validated.manifest,
            );
            let memory_namespace = validated.manifest.memory_namespace.clone();
            let user_memory_write = validated.manifest.user_memory_write;
            // Capture datasources/templates declarations + the agent's package
            // directory so the BridgeRunner can build ctx.datasources /
            // ctx.templates on every call_run.
            let datasources_declared = validated.manifest.datasources.clone();
            let templates_declared = validated.manifest.templates.clone();
            let agent_dir = agent_path.parent().map(Path::to_path_buf);
            // Capture the list of declared secrets.
            let secrets_declared = validated.manifest.secrets.clone();
            let bridge = Arc::new(AIPBridge::new(validated).map_err(|e| e.to_string())?);
            Ok(AIPProductionBackend {
                bridge,
                agent_id: agent_id.clone(),
                manifest: manifest.clone(),
                access,
                llm_router,
                event_bus,
                tool_registry,
                audit_trail,
                memory_namespace,
                memory_base_dir: default_memory_dir(),
                pending_approvals,
                plan_gates,
                plan_cache,
                task_repository,
                a2a_invoker,
                tools_config: self.tools_config.clone(),
                user_memory_write,
                datasources_declared,
                templates_declared,
                agent_dir,
                secrets_declared,
                secrets_data_dir: Some(self.data_dir.clone()),
                mcp_handle,
                governance,
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
                if let Some(bus) = self.event_bus.get() {
                    let _ = bus.send(apollia_core::RuntimeEvent::AgentLoadFailed {
                        name: agent_id.clone(),
                        error: e.to_string(),
                    });
                }
                DynBackend::new(NoopBackend)
            }
        }
    }
}

/// Returns the default memory directory (`~/.apollia/memory/`).
///
/// Matches the path convention used by `apollia-os memory inspect` and `MemoryManager`.
pub(super) fn default_memory_dir() -> PathBuf {
    let home = apollia_core::paths::home_dir_or_temp()
        .display()
        .to_string();
    apollia_core::paths::data_dir_under(home).join("memory")
}

/// The configuration file the runtime loads when no `--config` is given:
/// `~/.apollia/apollia.toml` when it exists, `None` otherwise.
///
/// The working directory and `~/.config/apollia/` are not searched: the first
/// is where an agent package keeps a manifest of the same name, and both made
/// the CLI, the daemon and the desktop disagree on which file was in force.
pub(crate) fn find_config_file() -> Option<PathBuf> {
    apollia_core::paths::config_file().filter(|p| p.is_file())
}

/// Returns `true` if a TCP listener is already bound on `port`.
pub(super) async fn port_is_in_use(port: u16) -> bool {
    tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .is_ok()
}

/// Returns `true` only when a live runtime is currently listening on `path`.
///
/// A previous crash can leave a stale socket file behind. To distinguish it
/// from a real running daemon we attempt a short-timeout connect: success
/// means a process is bound, `ConnectionRefused` means the file is stale and
/// safe to remove on the caller's side.
#[cfg(unix)]
pub(super) async fn socket_is_in_use(path: &std::path::Path) -> bool {
    if !path.exists() {
        return false;
    }
    match tokio::time::timeout(
        std::time::Duration::from_millis(200),
        tokio::net::UnixStream::connect(path),
    )
    .await
    {
        Ok(Ok(_)) => true,
        Ok(Err(_)) | Err(_) => false,
    }
}

/// On Windows there is no Unix socket, so we ask the named pipe instead.
///
/// Opening it answers the question directly: a pipe exists only while a
/// runtime holds it. `ERROR_PIPE_BUSY` counts as in use too, since a busy pipe
/// is a pipe someone is serving. This used to probe the TCP port, which stops
/// being the right question once the port is no longer how the CLI reaches the
/// runtime, and would have reported "free" on a running daemon that opened no
/// port.
#[cfg(windows)]
pub(super) async fn socket_is_in_use(_path: &std::path::Path) -> bool {
    /// `ERROR_PIPE_BUSY`: every instance is serving a client, so one exists.
    const ERROR_PIPE_BUSY: i32 = 231;
    let name = apollia_core::paths::named_pipe_name();
    match tokio::net::windows::named_pipe::ClientOptions::new().open(&name) {
        Ok(_) => true,
        Err(e) => e.raw_os_error() == Some(ERROR_PIPE_BUSY),
    }
}

/// Remove a stale Unix socket file left over from a previous crashed runtime.
///
/// Called only after [`socket_is_in_use`] returned `false` for an existing
/// path, so we know no live daemon is listening. Any error is logged but not
/// propagated: the subsequent bind will surface the real cause.
pub(super) fn cleanup_stale_socket(path: &std::path::Path) {
    if !path.exists() {
        return;
    }
    match std::fs::remove_file(path) {
        Ok(()) => {
            tracing::info!(
                socket = %path.display(),
                "api.socket.stale.removed"
            );
        }
        Err(e) => {
            tracing::warn!(
                socket = %path.display(),
                error = %e,
                "api.socket.stale.remove.failed"
            );
        }
    }
}
