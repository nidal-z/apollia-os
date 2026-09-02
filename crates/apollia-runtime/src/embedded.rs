//! Embedded runtime, starts the full Supervisor inside a dedicated thread.
//!
//! Designed for Tauri v2 integration: the desktop process calls
//! [`init_embedded()`] once at startup, receives a [`RuntimeHandle`] with all
//! actor handles, and passes it to `tauri::Builder::manage()`.
//!
//! The Tokio runtime lives in a separate OS thread so the Tauri main thread
//! (which drives the native event loop) is never blocked after init.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use apollia_core::{HitlConfig, ObservabilityConfig, PendingApprovals, RuntimeConfig};
use apollia_llm::LlmRouter;
use apollia_notifications::NotificationEngineHandle;
use apollia_tools::{AgentRepository, AuditTrailHandle, ProjectRepository, TaskRepository};

use crate::api::routes_agents::{AgentBackendFactory, AgentLoader, StubAgentLoader};
use crate::api::{APIServerConfig, APIServerHandle};
use crate::coordinator::{DynBackend, ExecutionBackend};
use crate::eventbus::EventBusSender;
use crate::registry::AgentRegistryHandle;
use crate::router::TaskRouterHandle;
use crate::supervisor::{Supervisor, SupervisorConfig, SupervisorError};
use apollia_tools::ToolRegistryHandle;
use apollia_triggers::TriggerEngineHandle;

/// Default timeout (in seconds) to wait for the Supervisor to emit `AllReady`.
///
/// 300 s (5 min) accommodates large local models (70B-400B, multi-shard).
/// Override via `[runtime] startup_timeout_secs` in `apollia.toml`.
const DEFAULT_STARTUP_TIMEOUT_SECS: u64 = 300;

/// Handle to the embedded runtime, holding every actor handle.
///
/// Passed to Tauri via `manage()` so it is reachable from IPC commands.
/// All fields are `Clone + Send + Sync`. Handles that do not implement
/// `Clone` natively are wrapped in `Arc`.
#[derive(Clone)]
pub struct RuntimeHandle {
    /// Sender for publishing events on the EventBus.
    pub event_sender: EventBusSender,
    /// Handle to the AgentRegistry.
    pub registry_handle: AgentRegistryHandle,
    /// Handle to the ToolRegistry.
    pub tool_registry_handle: ToolRegistryHandle,
    /// Handle to the TaskRouter.
    pub router_handle: TaskRouterHandle<DynBackend>,
    /// Handle to the APIServer (for shutdown). Wrapped in `Arc` because
    /// `APIServerHandle` does not implement `Clone` (internal watch::Sender).
    pub api_handle: Arc<APIServerHandle>,
    /// Optional LLM router.
    pub llm_router: Option<Arc<LlmRouter>>,
    /// Handle to the TriggerEngine.
    pub trigger_engine: TriggerEngineHandle,
    /// Optional handle to the AuditTrail.
    pub audit_trail: Option<AuditTrailHandle>,
    /// Read access to the TaskRepository.
    pub task_repository: Option<Arc<TaskRepository>>,
    /// Pending approvals.
    pub pending_approvals: Option<Arc<PendingApprovals>>,
    /// Plan-gate registry, shared with the per-task `ORIAEngine`.
    ///
    /// `Some` after a successful startup. Lets the desktop resolve a pending
    /// plan gate (approve, reject, or submit an edited plan) via
    /// [`crate::plan_approval::PlanApprovalHandle`].
    pub plan_gates: Option<Arc<apollia_oria::PendingPlanGates>>,
    /// Optional handle to the NotificationEngine. Wrapped in `Arc` because
    /// `NotificationEngineHandle` does not implement `Clone`.
    pub notification_engine: Option<Arc<NotificationEngineHandle>>,
    /// LLM call repository, aggregates costs and tokens.
    ///
    /// `Some` when an `LlmRouter` is configured and `llm_calls.db` is open.
    pub llm_call_repository: Option<Arc<std::sync::Mutex<apollia_llm::LlmCallRepository>>>,
    /// Handle to the [`ChatSessionManager`] actor.
    ///
    /// `Some` after the Supervisor startup sequence.
    /// `None` when the chat subsystem failed to start.
    pub chat_manager: Option<crate::chat::ChatSessionManagerHandle>,
    /// ORIA plan cache repository.
    ///
    /// `Some` when `plan_cache.db` opened successfully.
    pub plan_cache: Option<Arc<std::sync::Mutex<apollia_oria::plan_cache::PlanCacheRepository>>>,
    /// Handle to the agent-to-agent mailbox actor.
    ///
    /// `Some` after the mailbox is spawned during startup.
    pub mailbox_handle: Option<crate::mailbox::AgentMailboxHandle>,
    /// Repository for global user memory (preferences, habits, context).
    ///
    /// `Some` when `user_memory.db` opened successfully on startup.
    /// `None` when the open failed (warning logged, user memory disabled).
    pub user_memory:
        Option<Arc<std::sync::Mutex<apollia_memory::user_memory::UserMemoryRepository>>>,
    /// Swappable handle to the SttEngine actor.
    ///
    /// Shares the same cell as [`AppState`](crate::api::AppState), so the STT
    /// reload command brings a model online mid-session for both the in-process
    /// desktop readers and the axum routes. Holds `None` when STT is disabled,
    /// the model is absent, or loading failed.
    pub stt_engine: crate::api::server::SharedSttEngine,
    /// Swappable STT transcription repository for API routes.
    ///
    /// Separate connection for read operations. Holds `None` when STT is disabled.
    pub stt_repository: crate::api::server::SharedSttRepository,
    /// Project repository, manages per-project workspace contexts.
    ///
    /// `Some` when `projects.db` opened successfully on startup.
    pub project_repository: Option<Arc<ProjectRepository>>,
    /// Handle to the MCP client manager.
    ///
    /// `Some` when the supervisor started the manager (always in v0.1.1+
    /// even without servers installed). Consumed by the agent runners to
    /// build one `McpToolExecutor` per registered MCP tool and inject it
    /// into the agent's ToolDispatcher.
    pub mcp_handle: Option<apollia_mcp::manager::McpClientManagerHandle>,
    /// Native tools configuration (web_search, web_read, disabled, http_allowlist).
    ///
    /// Comes from the `[tools]` section of `apollia.toml` (or default if absent).
    /// Consumed by the agent runners to configure the `NativeDispatcherConfig`
    /// and to merge in statically disabled tools.
    pub tools_config: apollia_core::ToolsConfig,
    /// TCP port of the APIServer.
    pub api_port: u16,
    /// Supervisor of the local sidecar runner.
    ///
    /// Held for the entire lifetime of the embedded runtime: it owns the
    /// runner child process (`kill_on_drop(true)`). Without this reference the
    /// runner would be killed after boot and local inference calls would fail
    /// with connection-refused. Also exposes `.proxy()` so the reload commands
    /// can rebuild the router with the local backend wired through.
    pub runner_supervisor: Option<Arc<crate::runner_supervisor::RunnerSupervisor>>,

    /// Managed embedded `llama-server` (local LLM engine). Exposed so the desktop
    /// reload command can rebuild the router with local backends wired through it.
    pub llama_server_supervisor: Option<Arc<crate::llama_server::LlamaServerSupervisor>>,

    /// Default plan-mode state for new chat sessions, read from the `[chat]`
    /// section of `apollia.toml` at boot.
    ///
    /// The runtime is the single source of truth for this default; the desktop
    /// reads it to seed its own per-user preference rather than inventing one.
    pub plan_mode_default: bool,

    /// Default working directory for free-chat sessions, read from the
    /// `[chat] default_workspace` key of `apollia.toml` at boot. `None` falls
    /// back to `~/.apollia`. Surfaced to the desktop Settings page.
    pub chat_default_workspace: Option<String>,

    /// Paths an agent may read and write without an approval prompt, from the
    /// `[filesystem] trusted_paths` key of `apollia.toml`, `~` already
    /// resolved. Read once at boot.
    ///
    /// It is a friction boundary, not a wall: a path outside every entry is
    /// classified one level higher, which asks the user rather than refusing.
    /// An empty list therefore means every write outside the working directory
    /// is asked about. Default: the user's home directory.
    pub filesystem_trusted_paths: Vec<std::path::PathBuf>,

    /// Temperature applied to a chat turn that advertises tools, read from the
    /// `[chat] tool_turn_temperature` key of `apollia.toml` at boot. `None`
    /// resolves to the agent default.
    pub chat_tool_turn_temperature: Option<f32>,
}

impl RuntimeHandle {
    /// Kill every child process the runtime owns, before the host exits.
    ///
    /// `kill_on_drop(true)` on each child is not enough here: a host that calls
    /// `AppHandle::exit` terminates the process before the managed handle is
    /// dropped, so the destructors never run and the children are orphaned.
    ///
    /// Both supervisors, deliberately. The exit hook used to stop the runner
    /// only, which left `llama-server` alive with its VRAM and its loopback
    /// port held after the application had quit.
    pub async fn stop_child_processes(&self) {
        stop_supervisors(
            self.runner_supervisor.as_deref(),
            self.llama_server_supervisor.as_deref(),
        )
        .await;
    }
}

/// The teardown itself, over the two supervisors rather than the whole handle,
/// so it is reachable from a test that cannot assemble a full [`RuntimeHandle`].
///
/// Deliberately not named after the method above: `stop_child_processes` is
/// guarded by a claim whose whole point is that a caller outside this file
/// exists, and a same-named inner call would satisfy that check on its own.
pub(crate) async fn stop_supervisors(
    runner: Option<&crate::runner_supervisor::RunnerSupervisor>,
    llama_server: Option<&crate::llama_server::LlamaServerSupervisor>,
) {
    if let Some(runner) = runner {
        runner.shutdown_in_place().await;
    }
    if let Some(llama_server) = llama_server {
        llama_server.shutdown_in_place().await;
    }
}

/// Error returned by [`init_embedded()`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EmbeddedError {
    /// The Supervisor failed to start.
    #[error("supervisor startup failed: {0}")]
    SupervisorFailed(#[from] SupervisorError),
    /// Timed out waiting for `AllReady`.
    #[error("runtime did not become ready within {0}s")]
    StartupTimeout(u64),
    /// The runtime thread panicked.
    #[error("runtime thread panicked")]
    RuntimeThreadPanicked,
}

/// Configuration for [`init_embedded()`].
///
/// Allows overriding the TCP port, the Unix socket path, and the timeouts.
/// The default values match the standard behavior of `apollia-os start`.
pub struct EmbeddedConfig {
    /// TCP port for the APIServer, or `None` to serve the Unix socket only.
    ///
    /// Defaults to `None`: an embedded host is reachable through the Unix
    /// socket (local-trust) and does not expose a TCP port. A host that needs
    /// TCP (for a same-machine REST bridge, or a remote driver) sets
    /// `Some(port)` and MUST also set [`api_token`](EmbeddedConfig::api_token)
    /// so the port is authenticated.
    pub tcp_port: Option<u16>,
    /// Bearer token honored on the TCP listener, or `None` for no TCP auth.
    ///
    /// Only meaningful when [`tcp_port`](EmbeddedConfig::tcp_port) is `Some`.
    /// When a TCP port is bound with `None` here, the port is unauthenticated
    /// and the server logs a warning. The Unix socket is never token-gated.
    pub api_token: Option<String>,
    /// Unix socket path (default: `~/.apollia/runtime.sock`).
    pub socket_path: PathBuf,
    /// Runtime data directory (default: `~/.apollia/`).
    pub data_dir: PathBuf,
    /// Supervisor startup timeout in seconds (default: 30).
    pub startup_timeout_secs: u64,
    /// Observability configuration.
    pub obs_config: ObservabilityConfig,
    /// Agent loader used to load Python agents.
    pub agent_loader: Arc<dyn AgentLoader>,
    /// Backend factory used to create per-agent execution backends.
    pub backend_factory: Option<Arc<dyn AgentBackendFactory>>,
    /// Optional LLM configuration parsed from `apollia.toml`.
    pub llm_config: Option<apollia_llm::LlmConfig>,
    /// Path of the loaded `apollia.toml` file, required for trigger hot reload.
    pub config_path: Option<PathBuf>,
    /// Repository of installed agents, required for auto-load at boot.
    pub agent_repository: Option<AgentRepository>,
    /// Directory of bundled agents for auto-install on first boot.
    ///
    /// If `None` or if `manifest.json` is absent, auto-install is skipped.
    pub bundled_agents_path: Option<PathBuf>,
    /// Chat Agent runner, enables Chat Agent mode in the ChatSessionManager.
    /// When `None`, Agent mode sessions will fail at message time.
    pub chat_agent_runner: Option<Arc<dyn crate::chat::ChatAgentRunner>>,

    /// Core runtime configuration (EventBus, mailbox).
    ///
    /// Maps to the `[runtime]` section in `apollia.toml`.
    /// Populated by [`EmbeddedConfig::apply_toml`].
    pub runtime_config: RuntimeConfig,

    /// Human-in-the-Loop configuration (HITL timeout, scan interval).
    ///
    /// Maps to the `[hitl]` section in `apollia.toml`.
    /// Populated by [`EmbeddedConfig::apply_toml`].
    pub hitl_config: HitlConfig,

    /// Native tools configuration (web_search, web_read, disabled).
    ///
    /// Maps to the `[tools]` section in `apollia.toml`.
    /// Populated by [`EmbeddedConfig::apply_toml`].
    pub tools_config: apollia_core::ToolsConfig,

    /// MCP module configuration (tool loading strategy, search limit).
    ///
    /// Maps to the `[mcp]` section in `apollia.toml`.
    /// Populated by [`EmbeddedConfig::apply_toml`].
    pub mcp_config: apollia_core::McpConfig,

    /// Lifecycle hooks configuration (command/http handlers).
    ///
    /// Maps to the `[hooks]` section in `apollia.toml`.
    /// Populated by [`EmbeddedConfig::apply_toml`].
    pub hooks_config: apollia_core::HooksConfig,

    /// Chat subsystem configuration (session-level defaults).
    ///
    /// Maps to the `[chat]` section in `apollia.toml`.
    /// Populated by [`EmbeddedConfig::apply_toml`].
    pub chat_config: apollia_core::ChatConfig,

    /// Filesystem configuration: reversible journal, and the paths an agent may
    /// work in without being asked.
    ///
    /// Maps to the `[filesystem]` section in `apollia.toml`.
    /// Populated by [`EmbeddedConfig::apply_toml`].
    pub filesystem_config: apollia_core::FilesystemConfig,
}

impl Default for EmbeddedConfig {
    fn default() -> Self {
        let home = apollia_core::paths::home_dir_or_temp();
        Self {
            tcp_port: None,
            api_token: None,
            socket_path: apollia_core::paths::socket_path_under(&home),
            data_dir: apollia_core::paths::data_dir_under(home),
            startup_timeout_secs: DEFAULT_STARTUP_TIMEOUT_SECS,
            obs_config: ObservabilityConfig::default(),
            agent_loader: Arc::new(StubAgentLoader),
            backend_factory: None,
            llm_config: None,
            config_path: None,
            agent_repository: None,
            bundled_agents_path: None,
            chat_agent_runner: None,
            runtime_config: RuntimeConfig::default(),
            hitl_config: HitlConfig::default(),
            tools_config: apollia_core::ToolsConfig::default(),
            mcp_config: apollia_core::McpConfig::default(),
            hooks_config: apollia_core::HooksConfig::default(),
            chat_config: apollia_core::ChatConfig::default(),
            filesystem_config: apollia_core::FilesystemConfig::default(),
        }
    }
}

impl EmbeddedConfig {
    /// Applies every parsable section of `apollia.toml` to this config.
    ///
    /// Parses `[llm]`, `[runtime]`, `[hitl]`, `[a2a]`, and `[api]` in a single pass.
    /// Parse errors are silently ignored: the runtime starts with default
    /// values when a section is absent or invalid.
    pub fn apply_toml(mut self, content: &str) -> Self {
        #[derive(serde::Deserialize)]
        struct TomlSections {
            llm: Option<apollia_llm::LlmConfig>,
            runtime: Option<RuntimeConfig>,
            hitl: Option<HitlConfig>,
            api: Option<apollia_core::ApiConfig>,
            tools: Option<apollia_core::ToolsConfig>,
            mcp: Option<apollia_core::McpConfig>,
            hooks: Option<apollia_core::HooksConfig>,
            chat: Option<apollia_core::ChatConfig>,
            filesystem: Option<apollia_core::FilesystemConfig>,
            observability: Option<ObservabilityConfig>,
        }
        if let Ok(s) = toml::from_str::<TomlSections>(content) {
            self.llm_config = s.llm;
            if let Some(rc) = s.runtime {
                // Propagate startup_timeout_secs from [runtime] to EmbeddedConfig
                // so large local models (70B+) don't hit the hardcoded default limit.
                self.startup_timeout_secs = rc.startup_timeout_secs;
                self.runtime_config = rc;
            }
            if let Some(hc) = s.hitl {
                self.hitl_config = hc;
            }
            if let Some(api) = s.api {
                // Update socket_path from [api].unix_socket when explicitly configured.
                self.socket_path = api.unix_socket;
            }
            if let Some(tc) = s.tools {
                self.tools_config = tc;
            }
            if let Some(mc) = s.mcp {
                self.mcp_config = mc;
            }
            if let Some(hooks) = s.hooks {
                self.hooks_config = hooks;
            }
            if let Some(chat) = s.chat {
                self.chat_config = chat;
            }
            if let Some(fs) = s.filesystem {
                self.filesystem_config = fs;
            }
            if let Some(obs) = s.observability {
                self.obs_config = obs;
            }
        }

        // triggers and notifications are now loaded from SQLite by the Supervisor.
        // TOML sections for these are ignored.

        self
    }
}

/// Starts the Apollia runtime in embedded mode.
///
/// Spawns a dedicated thread with a Tokio runtime, starts the Supervisor,
/// waits for the `AllReady` event, then returns a [`RuntimeHandle`].
///
/// The Unix socket stays active so the CLI can be used concurrently.
///
/// # Errors
///
/// - [`EmbeddedError::SupervisorFailed`] if the Supervisor fails to start.
/// - [`EmbeddedError::StartupTimeout`] if `AllReady` is not received in time.
/// - [`EmbeddedError::RuntimeThreadPanicked`] if the runtime thread panics.
///
/// Process-global handle to the apollia-worker Tokio runtime.
///
/// Stored so the PyO3 async bridge (`apollia-aip`) can pin
/// `pyo3-async-runtimes` onto this exact runtime instead of letting it spawn a
/// second one. Populated by the first [`init_embedded`] call.
static WORKER_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

/// Returns the apollia-worker Tokio runtime once the embedded runtime has
/// started, or `None` before [`init_embedded`] has run.
///
/// Kept crate-neutral (no PyO3 types) so `apollia-runtime` stays Python-free;
/// the PyO3 bridge fetches this handle and pins itself to it.
#[must_use]
pub fn worker_runtime() -> Option<&'static tokio::runtime::Runtime> {
    WORKER_RUNTIME.get()
}

pub fn init_embedded(config: EmbeddedConfig) -> Result<RuntimeHandle, EmbeddedError> {
    let (result_tx, result_rx) = std::sync::mpsc::channel::<Result<RuntimeHandle, EmbeddedError>>();
    let startup_timeout_secs = config.startup_timeout_secs;

    std::thread::Builder::new()
        .name("apollia-runtime".to_string())
        .spawn(move || {
            let built = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("apollia-worker")
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = result_tx.send(Err(EmbeddedError::SupervisorFailed(
                        SupervisorError::ActorStartFailed {
                            actor: "tokio-runtime".to_string(),
                            reason: e.to_string(),
                        },
                    )));
                    return;
                }
            };

            // Publish the runtime process-globally so the PyO3 async bridge can
            // pin itself to it. A second init_embedded in the same process
            // reuses the first runtime (the freshly built one is dropped here,
            // harmless as it holds no tasks yet).
            let _ = WORKER_RUNTIME.set(built);
            let rt = match WORKER_RUNTIME.get() {
                Some(rt) => rt,
                None => {
                    let _ = result_tx.send(Err(EmbeddedError::RuntimeThreadPanicked));
                    return;
                }
            };

            rt.block_on(async move {
                let result = start_supervisor_and_wait(config).await;
                let _ = result_tx.send(result);

                // Keep the Tokio runtime alive so actors continue running.
                // The thread parks here indefinitely, shutdown is driven by
                // the ShutdownController via the EventBus.
                std::future::pending::<()>().await;
            });
        })
        .map_err(|_| EmbeddedError::RuntimeThreadPanicked)?;

    let timeout = Duration::from_secs(startup_timeout_secs);
    match result_rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            Err(EmbeddedError::StartupTimeout(startup_timeout_secs))
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err(EmbeddedError::RuntimeThreadPanicked)
        }
    }
}

/// Internal: starts the Supervisor and waits for `AllReady`.
async fn start_supervisor_and_wait(config: EmbeddedConfig) -> Result<RuntimeHandle, EmbeddedError> {
    let tcp_port = config.tcp_port;

    let tools_config = config.tools_config.clone();
    let mcp_loading = apollia_mcp::session::LoadingMode::from(config.mcp_config.tool_loading);
    let tool_search_limit = config.mcp_config.tool_search_limit;
    let plan_mode_default = config.chat_config.plan_mode_default;
    let chat_default_workspace = config.chat_config.default_workspace.clone();
    let filesystem_trusted_paths = config.filesystem_config.resolved_trusted_paths();
    let chat_tool_turn_temperature = config.chat_config.tool_turn_temperature;
    let supervisor_config = SupervisorConfig {
        api_config: APIServerConfig {
            socket_path: config.socket_path,
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port,
            api_token: config.api_token,
            // Embedded hosts are loopback-only; TLS is a daemon-only concern.
            ..APIServerConfig::default()
        },
        startup_timeout_secs: config.startup_timeout_secs,
        llm_config: config.llm_config,
        config_path: config.config_path,
        runtime_config: config.runtime_config,
        hitl_config: config.hitl_config,
        data_dir: config.data_dir,
        obs_config: config.obs_config,
        agent_repository: config.agent_repository,
        package_repository: None,
        bundled_agents_path: config.bundled_agents_path,
        tools_config: tools_config.clone(),
        mcp_loading,
        tool_search_limit,
        hooks_config: config.hooks_config,
        plan_mode_default: config.chat_config.plan_mode_default,
        chat_default_workspace: chat_default_workspace.clone(),
        chat_tool_turn_temperature,
        filesystem_trusted_paths: filesystem_trusted_paths.clone(),
    };

    let supervisor = Supervisor::new(supervisor_config);

    let noop = DynBackend::new(NoopBackend);
    let handles = supervisor
        .start(
            noop,
            config.agent_loader,
            config.backend_factory,
            config.chat_agent_runner,
        )
        .await?;

    Ok(RuntimeHandle {
        event_sender: handles.event_sender,
        registry_handle: handles.registry_handle,
        tool_registry_handle: handles.tool_registry_handle,
        router_handle: handles.router_handle,
        api_handle: Arc::new(handles.api_handle),
        llm_router: handles.llm_router,
        trigger_engine: handles.trigger_engine,
        audit_trail: handles.audit_trail,
        task_repository: handles.task_repository,
        pending_approvals: handles.pending_approvals,
        plan_gates: handles.plan_gates,
        notification_engine: handles.notification_engine.map(Arc::new),
        llm_call_repository: handles.llm_call_repository,
        chat_manager: handles.chat_manager,
        plan_cache: handles.plan_cache,
        mailbox_handle: handles.mailbox_handle,
        user_memory: handles.user_memory,
        stt_engine: handles.stt_engine,
        stt_repository: handles.stt_repository,
        project_repository: handles.project_repository,
        mcp_handle: handles.mcp_handle,
        tools_config,
        api_port: tcp_port.unwrap_or(0),
        runner_supervisor: handles.runner_supervisor,
        llama_server_supervisor: handles.llama_server_supervisor,
        plan_mode_default,
        chat_default_workspace,
        chat_tool_turn_temperature,
        filesystem_trusted_paths,
    })
}

/// Fallback backend, returns a `Failed` result immediately.
///
/// Used as the default execution backend when no Python agent is configured.
#[derive(Clone)]
struct NoopBackend;

impl ExecutionBackend for NoopBackend {
    fn execute(
        &self,
        task: apollia_core::AIPTask,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<apollia_core::AIPResult, String>> + Send>,
    > {
        Box::pin(async move {
            Ok(apollia_core::AIPResult {
                task_id: task.task_id,
                status: apollia_core::TaskStatus::Failed,
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

    #[test]
    fn test_embedded_config_default_values() {
        // GIVEN the default EmbeddedConfig
        let config = EmbeddedConfig::default();

        // WHEN its fields are read
        // THEN reasonable defaults are set: Unix socket only, no TCP exposure
        assert_eq!(config.tcp_port, None);
        assert_eq!(config.api_token, None);
        assert_eq!(
            config.socket_path,
            apollia_core::paths::socket_path_under(apollia_core::paths::home_dir_or_temp())
        );
        assert_eq!(config.startup_timeout_secs, DEFAULT_STARTUP_TIMEOUT_SECS);
    }

    #[test]
    fn test_embedded_error_display() {
        // GIVEN various EmbeddedError variants
        let timeout_err = EmbeddedError::StartupTimeout(30);
        let panic_err = EmbeddedError::RuntimeThreadPanicked;

        // WHEN each is rendered for a human
        // THEN display messages are informative
        assert!(timeout_err.to_string().contains("30s"));
        assert!(panic_err.to_string().contains("panicked"));
    }

    #[test]
    fn test_runtime_handle_is_clone() {
        // Compile-time check: RuntimeHandle must be Clone.
        // GIVEN the handle the embedded runtime hands out
        // WHEN a second owner is asked for
        // THEN the type supplies one, or this file stops compiling
        fn assert_clone<T: Clone>() {}
        assert_clone::<RuntimeHandle>();
    }

    #[test]
    fn test_embedded_error_from_supervisor_error() {
        // GIVEN a SupervisorError
        let sup_err = SupervisorError::ConfigError("test".to_string());

        // WHEN converted to EmbeddedError
        let embedded_err: EmbeddedError = sup_err.into();

        // THEN it wraps correctly
        assert!(embedded_err.to_string().contains("test"));
    }

    #[tokio::test]
    async fn test_teardown_stops_the_inference_engine_too() {
        // GIVEN a runtime that owns both child processes
        let runner = crate::runner_supervisor::RunnerSupervisor::for_tests();
        let llama_server = crate::llama_server::LlamaServerSupervisor::for_tests();

        // WHEN the host tears its children down on exit
        stop_supervisors(Some(&runner), Some(&llama_server)).await;

        // THEN neither survives. The engine half is the regression this guards:
        // the exit hook stopped the runner only, and `llama-server` stayed
        // resident after the application had quit.
        assert!(
            runner.is_shutting_down().await,
            "the runner was left running after exit"
        );
        assert!(
            llama_server.is_shutting_down().await,
            "llama-server was left running after exit"
        );
    }

    #[tokio::test]
    async fn test_teardown_tolerates_a_runtime_without_children() {
        // GIVEN a runtime that started neither supervisor (boot failure, or a
        // build with no local engine)
        // WHEN the host tears its children down, on a task of its own so the
        // panic this guards against is a value rather than an aborted test
        let torn_down = tokio::spawn(async { stop_supervisors(None, None).await }).await;
        // THEN it is a no-op rather than a panic on exit
        assert!(
            torn_down.is_ok(),
            "tearing down a runtime without children panicked: {:?}",
            torn_down.err()
        );
    }
}
