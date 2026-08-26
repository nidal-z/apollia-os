//! APIServer, dual TCP + Unix socket HTTP server for the Apollia runtime.
//!
//! Listens on `localhost:<tcp_port>` and a Unix socket simultaneously,
//! sharing the same axum `Router` and `AppState`.
//!
//! Both listeners use a manual `hyper-util` accept loop (axum 0.7 serves only a
//! bare `TcpListener` natively, which cannot terminate TLS or accept a Unix
//! socket). The TCP loop optionally wraps each connection in a `rustls`
//! `TlsAcceptor` when a certificate is configured.

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use tokio::net::TcpListener;
#[cfg(unix)]
use tokio::net::UnixListener;
use tokio::sync::{watch, RwLock};
use tracing::info;

use apollia_core::{LlmBackendRepository, PendingApprovals, SttConfigRepository};
use apollia_llm::{LlmCallRepository, LlmRouter};
use apollia_mcp::manager::McpClientManagerHandle;
use apollia_memory::user_memory::UserMemoryRepository;
use apollia_notifications::{
    NotificationConfig, NotificationConfigRepository, NotificationEngineHandle,
};
use apollia_oria::plan_cache::PlanCacheRepository;
use apollia_tools::{AuditTrailHandle, TaskRepository, ToolRegistryHandle};
use apollia_triggers::{TriggerDefinitionRepository, TriggerEngineHandle};

use crate::mailbox::AgentMailboxHandle;

use crate::api::routes_agents::{AgentBackendFactory, AgentLoader};
use crate::chat::ChatSessionManagerHandle;
use crate::coordinator::{DynBackend, ExecutionBackend};
use crate::eventbus::EventBusSender;
use crate::registry::AgentRegistryHandle;
use crate::router::TaskRouterHandle;

pub mod listeners;
pub mod router;

use listeners::{build_tls_acceptor, is_loopback_addr, serve_tcp, serve_unix};
use router::build_router;

/// Shared, swappable handle to the active [`LlmRouter`].
///
/// Routes read a snapshot under a read-lock, then drop the lock before
/// awaiting on the backend. The `POST /api/v1/llm/reload` route takes the
/// write-lock briefly to swap a freshly-built router in place, without
/// having to restart the daemon.
///
/// `None` means no router is configured (no backends in `system.db`, or
/// rebuild failed). `Some(router)` means the router currently in use by
/// every reader (ping/chat/complete/status).
pub type SharedLlmRouter = Arc<RwLock<Option<Arc<LlmRouter>>>>;

/// Build an empty [`SharedLlmRouter`].
// TEST-ONLY: every boot path resolves a router (or its absence) through
// [`shared_llm_router_from`]; this constructor exists so a test can mount
// `AppState` without a backend.
pub fn empty_shared_llm_router() -> SharedLlmRouter {
    Arc::new(RwLock::new(None))
}

/// Build a [`SharedLlmRouter`] pre-loaded with a router (or `None` if absent).
pub fn shared_llm_router_from(initial: Option<Arc<LlmRouter>>) -> SharedLlmRouter {
    Arc::new(RwLock::new(initial))
}

/// Shared, swappable handle to the active STT engine actor.
///
/// Mirrors [`SharedLlmRouter`]: readers snapshot the handle under a read-lock,
/// the `POST /api/v1/stt/reload` route swaps a freshly-built engine in under a
/// write-lock. `None` means STT is not currently loaded (disabled, no model, or
/// runner unavailable). The same cell is shared by [`AppState`] and the embedded
/// runtime handle, so a mid-session reload is visible to every reader.
pub type SharedSttEngine = Arc<RwLock<Option<crate::stt::SttEngineHandle>>>;

/// Shared, swappable API-side STT transcription repository.
///
/// Rebuilt alongside [`SharedSttEngine`] so the transcription history endpoints
/// come online when a model is enabled mid-session.
pub type SharedSttRepository =
    Arc<RwLock<Option<Arc<std::sync::Mutex<apollia_stt::SttRepository>>>>>;

/// Build an empty [`SharedSttEngine`].
// TEST-ONLY: boot paths go through [`shared_stt_engine_from`]; this constructor
// exists so a test can mount `AppState` without an STT engine.
pub fn empty_shared_stt_engine() -> SharedSttEngine {
    Arc::new(RwLock::new(None))
}

/// Build a [`SharedSttEngine`] pre-loaded with an engine (or `None` if absent).
pub fn shared_stt_engine_from(initial: Option<crate::stt::SttEngineHandle>) -> SharedSttEngine {
    Arc::new(RwLock::new(initial))
}

/// Build an empty [`SharedSttRepository`].
// TEST-ONLY: boot paths go through [`shared_stt_repository_from`]; this
// constructor exists so a test can mount `AppState` without a repository.
pub fn empty_shared_stt_repository() -> SharedSttRepository {
    Arc::new(RwLock::new(None))
}

/// Build a [`SharedSttRepository`] pre-loaded with a repository (or `None`).
pub fn shared_stt_repository_from(
    initial: Option<Arc<std::sync::Mutex<apollia_stt::SttRepository>>>,
) -> SharedSttRepository {
    Arc::new(RwLock::new(initial))
}

/// Shared application state injected into all routes.
///
/// Contains handles to the runtime actors. Passed to axum via `with_state()`.
/// Routes extract it with `State<AppState<B>>`.
pub struct AppState<B: ExecutionBackend + Clone> {
    /// Handle to the task router actor.
    pub router_handle: TaskRouterHandle<B>,
    /// Handle to the agent registry actor.
    pub registry_handle: AgentRegistryHandle,
    /// Sender side of the runtime event bus.
    pub event_sender: EventBusSender,
    /// Agent loader for Python module loading.
    pub agent_loader: Arc<dyn AgentLoader>,
    /// Execution backend, cloned per coordinator on agent start.
    pub backend: B,
    /// Shared, hot-reloadable handle to the active [`LlmRouter`].
    ///
    /// Routes read a snapshot via `state.llm_router.read().await.clone()`
    /// and operate on that snapshot; the `POST /api/v1/llm/reload` route
    /// swaps a freshly-built router into the cell. `None` means no router
    /// is configured.
    pub llm_router: SharedLlmRouter,
    /// Handle to the TriggerEngine actor.
    ///
    /// Webhook route returns 503 Service Unavailable when this is `None`.
    pub trigger_engine: Option<TriggerEngineHandle>,
    /// Path to `apollia.toml`, used by `POST /api/v1/triggers/reload`.
    ///
    /// `None` when the runtime was started without a config file (e.g. in unit tests).
    /// The reload route returns 503 when this is `None`.
    pub config_path: Option<PathBuf>,
    /// HITL task repository, SQLite persistence for Human-in-the-Loop state.
    ///
    /// Opened by the Supervisor on startup from `~/.apollia/hitl.db`.
    /// `None` in unit tests or when HITL is not configured.
    /// The resume route returns 503 when this is `None`.
    pub task_repository: Option<Arc<TaskRepository>>,
    /// HITL registry of pending approvals, shared between routes and the ORIAEngine.
    ///
    /// `ResumeHandler` calls `pending_approvals.resolve()` to unblock
    /// `execute_direct()`, which is waiting on the oneshot channel.
    /// `None` when HITL is not configured; `resume_task` logs a warning.
    pub pending_approvals: Option<Arc<PendingApprovals>>,
    /// Plan-gate registry, shared between the plan-decision route and the ORIAEngine.
    ///
    /// The `plan-decision` route resolves a gate to unblock a run paused after
    /// plan generation. `None` when the plan gate is not configured.
    pub plan_gates: Option<Arc<apollia_oria::PendingPlanGates>>,
    /// Notification channel configuration loaded from `apollia.toml`.
    ///
    /// Used by `GET /api/v1/notifications/channels` and
    /// `POST /api/v1/notifications/test`.
    /// `None` when no `[notifications]` section is present in the config.
    pub notification_config: Option<NotificationConfig>,
    /// Factory for creating per-agent execution backends.
    ///
    /// `Some` in production, creates real `AIPBridge` backends with tool access.
    /// `None` in tests, falls back to `state.backend.clone()` (MockBackend/NoopBackend).
    pub backend_factory: Option<Arc<dyn AgentBackendFactory>>,
    /// Handle to the ToolRegistry actor, exposes the tool catalogue via REST.
    ///
    /// `Some` in production, populated by the Supervisor at startup.
    /// `None` in tests, the `/api/v1/tools` routes return 503 when `None`.
    pub tool_registry_handle: Option<ToolRegistryHandle>,
    /// Handle to the AuditTrail actor, exposes tool invocations via REST.
    ///
    /// `Some` in production, opened by the Supervisor from `~/.apollia/audit.db`.
    /// `None` in tests, the `/api/v1/audit` routes return 503 when `None`.
    pub audit_trail: Option<AuditTrailHandle>,
    /// Handle to the hash-chained audit journal actor, exposes `audit verify`.
    ///
    /// `Some` in production, opened by the Supervisor from
    /// `~/.apollia/audit_journal.db`. `None` in tests, the
    /// `/api/v1/audit/verify/:run_id` route returns 503 when `None`.
    pub audit_journal: Option<crate::audit_journal::AuditJournalHandle>,
    /// Truncation configuration for task observability.
    ///
    /// Passed to the `ExecutionCoordinator` for persisting input/output/transitions.
    pub obs_config: apollia_core::ObservabilityConfig,
    /// LLM call repository, aggregates cost and token usage.
    ///
    /// `Some` when an `LlmRouter` is configured and `llm_calls.db` is open.
    /// `None` in tests or when no LLM backend is configured.
    pub llm_call_repository: Option<Arc<std::sync::Mutex<LlmCallRepository>>>,
    /// CRUD repository for trigger definitions.
    ///
    /// Opened by the Supervisor from `data_dir/triggers.db`.
    /// Shared between boot (initial read) and the REST CRUD routes.
    /// `None` in unit tests.
    pub trigger_def_repo: Option<Arc<std::sync::Mutex<TriggerDefinitionRepository>>>,
    /// CRUD repository for the notification configuration.
    ///
    /// Opened by the Supervisor from `data_dir/notifications.db`.
    /// Shared between boot (initial read) and the REST CRUD routes.
    /// `None` in unit tests.
    pub notification_repo: Option<Arc<std::sync::Mutex<NotificationConfigRepository>>>,
    /// Handle to the [`NotificationEngine`] for hot-reload after CRUD.
    ///
    /// Lets the REST routes trigger a channel reload after a mutation in
    /// `notifications.db`. `None` in unit tests.
    pub notification_engine_handle: Option<NotificationEngineHandle>,
    /// Handle to the [`ChatSessionManager`] actor.
    ///
    /// `Some` after Phase 13 of the Supervisor startup sequence.
    /// `None` in tests or when the chat subsystem is not configured.
    pub chat_manager: Option<ChatSessionManagerHandle>,
    /// ORIA plan cache repository, stores cached execution plans.
    ///
    /// `Some` when plan caching is enabled (SQLite `plan_cache.db` opened).
    /// `None` in tests or when plan caching failed to initialize.
    pub plan_cache: Option<Arc<std::sync::Mutex<PlanCacheRepository>>>,
    /// Handle to the agent-to-agent mailbox actor.
    ///
    /// `Some` after the mailbox is spawned during startup.
    /// `None` in tests or when A2A messaging is disabled.
    pub mailbox_handle: Option<AgentMailboxHandle>,
    /// Repository for global user memory (preferences, habits, context).
    ///
    /// `Some` after the Supervisor opens `user_memory.db` on startup.
    /// `None` in tests or when user memory is not configured.
    pub user_memory: Option<Arc<std::sync::Mutex<UserMemoryRepository>>>,
    /// Swappable handle to the STT engine actor.
    ///
    /// Holds `Some` after Phase 15 of the Supervisor startup when
    /// `stt.enabled = true`; `None` in tests or when STT is disabled. Routes
    /// return 503 when `None`. The `POST /api/v1/stt/reload` route swaps a
    /// freshly-built engine in without restarting the daemon.
    pub stt_engine: SharedSttEngine,
    /// Swappable STT transcription repository, persists transcription history.
    ///
    /// Rebuilt alongside [`SharedSttEngine`]; `None` in tests or when STT is
    /// disabled.
    pub stt_repository: SharedSttRepository,
    /// Runtime data directory (`~/.apollia`), used by the STT reload route to
    /// locate `stt_transcriptions.db` when rebuilding the engine.
    pub data_dir: PathBuf,
    /// Handle to the MCP client manager actor.
    ///
    /// `Some` when at least one MCP server is configured in `mcp.db` and connected.
    /// `None` when the database is absent, empty, or all servers failed to start.
    /// MCP routes return 503 when `None`.
    pub mcp_handle: Option<McpClientManagerHandle>,
    /// SQLite-backed repository for MCP server configurations.
    ///
    /// Opened by the Supervisor at startup from `data_dir/mcp.db`.
    /// `None` in unit tests. Mutation routes return 503 when `None`.
    pub mcp_server_repo: Option<Arc<std::sync::Mutex<apollia_mcp::McpServerRepository>>>,
    /// CRUD repository for LLM backends.
    ///
    /// Opened by the Supervisor from `data_dir/system.db`.
    /// Shared between boot (loading the LlmRouter) and the REST CRUD routes.
    /// `None` in unit tests or when `system.db` could not be opened.
    pub llm_backend_repo: Option<Arc<std::sync::Mutex<LlmBackendRepository>>>,
    /// STT configuration repository, persists and reads the singleton `stt_config`
    /// row in `system.db`.
    ///
    /// `Some` after Phase 15 of the Supervisor startup sequence.
    /// `None` in tests or when `system.db` could not be opened.
    /// Config routes return 503 when `None`.
    pub stt_config_repo: Option<Arc<std::sync::Mutex<SttConfigRepository>>>,
    /// High-level A2A orchestrator, agent-to-agent invocations by skill ID.
    ///
    /// `Some` after the runtime is initialized with registry + router + event_bus.
    /// `None` in unit tests. The `/api/v1/a2a/skills` and
    /// `/api/v1/a2a/invoke` routes return 503 when `None`.
    pub a2a_invoker: Option<Arc<crate::a2a::A2AInvoker>>,
    /// Shared circuit-breaker registry observed by the runtime event subscriber.
    ///
    /// `Some` once the Supervisor builds the layer + spawns the
    /// `ToolCallCompleted` subscriber that mirrors per-tool successes /
    /// transient failures into circuit breakers. `None` in unit tests or
    /// when subscriber init failed; the resilience routes degrade to a
    /// stable empty snapshot instead of returning a 503.
    pub resilience_layer: Option<Arc<std::sync::Mutex<apollia_oria::ResilienceLayer>>>,
    /// Proxy onto the local sidecar runner.
    ///
    /// `Some` when the runtime started a runner at boot. The REST reload
    /// endpoint uses it to rebuild the LLM router with the same
    /// `LlamaCpp -> runner` override the supervisor applied; otherwise the
    /// reload would drop the local backend (the runner would become
    /// unreachable from agents and chat).
    ///
    /// `None` in unit tests and when no runner is available; the reload still
    /// works for cloud-only backends in that case.
    pub runner_proxy: Option<crate::runner_supervisor::RunnerProxy>,

    /// Managed embedded `llama-server` (local LLM engine), or `None` when the
    /// binary is absent or in unit tests. The reload path rebuilds the router
    /// with local `LlamaCpp` backends wired through this supervisor.
    pub llama_server_supervisor: Option<Arc<crate::llama_server::LlamaServerSupervisor>>,
}

impl<B: ExecutionBackend + Clone> Clone for AppState<B> {
    fn clone(&self) -> Self {
        Self {
            router_handle: self.router_handle.clone(),
            registry_handle: self.registry_handle.clone(),
            event_sender: self.event_sender.clone(),
            agent_loader: Arc::clone(&self.agent_loader),
            backend: self.backend.clone(),
            llm_router: self.llm_router.clone(),
            trigger_engine: self.trigger_engine.clone(),
            config_path: self.config_path.clone(),
            task_repository: self.task_repository.clone(),
            pending_approvals: self.pending_approvals.clone(),
            plan_gates: self.plan_gates.clone(),
            notification_config: self.notification_config.clone(),
            backend_factory: self.backend_factory.clone(),
            tool_registry_handle: self.tool_registry_handle.clone(),
            audit_trail: self.audit_trail.clone(),
            audit_journal: self.audit_journal.clone(),
            obs_config: self.obs_config.clone(),
            llm_call_repository: self.llm_call_repository.clone(),
            trigger_def_repo: self.trigger_def_repo.clone(),
            notification_repo: self.notification_repo.clone(),
            notification_engine_handle: self.notification_engine_handle.clone(),
            chat_manager: self.chat_manager.clone(),
            plan_cache: self.plan_cache.clone(),
            mailbox_handle: self.mailbox_handle.clone(),
            user_memory: self.user_memory.clone(),
            stt_engine: self.stt_engine.clone(),
            stt_repository: self.stt_repository.clone(),
            data_dir: self.data_dir.clone(),
            mcp_handle: self.mcp_handle.clone(),
            mcp_server_repo: self.mcp_server_repo.clone(),
            llm_backend_repo: self.llm_backend_repo.clone(),
            stt_config_repo: self.stt_config_repo.clone(),
            a2a_invoker: self.a2a_invoker.clone(),
            resilience_layer: self.resilience_layer.clone(),
            runner_proxy: self.runner_proxy.clone(),
            llama_server_supervisor: self.llama_server_supervisor.clone(),
        }
    }
}

/// Permission bits applied to the Unix socket right after `bind`: owner read
/// and write, nothing for the group and nothing for the rest of the machine.
#[cfg(unix)]
const SOCKET_MODE: u32 = 0o600;

/// Configuration for the APIServer.
pub struct APIServerConfig {
    /// Path to the Unix domain socket (e.g. `~/.apollia/runtime.sock`).
    pub socket_path: PathBuf,
    /// IP address to bind the TCP listener on.
    ///
    /// Defaults to `"127.0.0.1"` (loopback only). Set to `"0.0.0.0"` to accept
    /// connections from any interface (not recommended in production).
    pub bind_addr: String,
    /// TCP port to listen on (e.g. `7771`), or `None` to serve the Unix socket
    /// only.
    ///
    /// `Some(port)` binds a TCP listener on `bind_addr:port`. `None` skips the
    /// TCP listener entirely, so the runtime is reachable only through the Unix
    /// socket (local-trust). Embedded hosts default to `None`; the daemon sets
    /// `Some(port)`. Binding a non-loopback address without an `api_token` is
    /// refused at startup; a loopback bind without a token is allowed.
    pub tcp_port: Option<u16>,
    /// Bearer token required on TCP connections.
    ///
    /// `Some(token)`, every incoming TCP request must supply
    /// `Authorization: Bearer <token>`. Requests without it or with an incorrect
    /// value receive `401 Unauthorized`.
    ///
    /// `None`, no authentication check is performed (equivalent to
    /// `require_token = false` in `apollia.toml`).
    ///
    /// The Unix socket listener is never subject to token authentication.
    ///
    /// When a non-loopback `bind_addr` is combined with `None`, startup fails
    /// fast: the daemon refuses to serve a public unauthenticated API.
    pub api_token: Option<String>,
    /// PEM certificate chain for native TLS on the TCP listener.
    ///
    /// `Some` with [`tls_key_path`](Self::tls_key_path) enables TLS termination
    /// on the TCP listener. `None` keeps the listener cleartext, unchanged from
    /// prior behavior. Setting exactly one of the pair is a startup error. The
    /// Unix socket is never subject to TLS.
    pub tls_cert_path: Option<PathBuf>,
    /// PEM private key matching [`tls_cert_path`](Self::tls_cert_path).
    pub tls_key_path: Option<PathBuf>,
}

impl Default for APIServerConfig {
    fn default() -> Self {
        Self {
            socket_path: apollia_core::paths::socket_path_or_temp(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: None,
            api_token: None,
            tls_cert_path: None,
            tls_key_path: None,
        }
    }
}

/// Handle to control a running APIServer.
///
/// Obtained from [`APIServer::start`]. Call [`shutdown`](APIServerHandle::shutdown)
/// to trigger graceful shutdown of both listeners.
pub struct APIServerHandle {
    shutdown_tx: watch::Sender<bool>,
}

impl APIServerHandle {
    /// Signal the server to stop accepting new connections and drain existing ones.
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }
}

/// Errors that can occur when starting or running the APIServer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum APIServerError {
    /// TCP bind failed (port already in use, permission denied, etc.).
    #[error("failed to bind TCP on port {port}: {source}")]
    BindFailed {
        /// The TCP port that failed to bind.
        port: u16,
        /// The underlying IO error.
        source: std::io::Error,
    },

    /// Unix socket bind failed.
    #[error("failed to bind Unix socket at {path}: {source}")]
    SocketBindFailed {
        /// The socket path that failed to bind.
        path: String,
        /// The underlying IO error.
        source: std::io::Error,
    },

    /// TLS certificate or key could not be loaded.
    ///
    /// A daemon configured for TLS fails fast rather than falling back to
    /// cleartext when the certificate or key is missing or malformed.
    #[error("failed to load TLS material from {path}: {reason}")]
    TlsConfigLoad {
        /// The certificate or key path that failed to load.
        path: String,
        /// Human-readable cause (IO error or PEM parse failure).
        reason: String,
    },

    /// A non-loopback TCP bind was configured without a token.
    ///
    /// Serving a public interface with no authentication is refused at startup
    /// instead of degrading to an unauthenticated API.
    #[error(
        "refusing to bind non-loopback address {bind_addr} without an api_token: \
         set [api].require_token = true or bind a loopback address"
    )]
    InsecureBindWithoutToken {
        /// The non-loopback bind address that was rejected.
        bind_addr: String,
    },

    /// Generic server error.
    #[error("server error: {0}")]
    ServerError(String),
}

/// The Apollia APIServer, dual TCP + Unix socket HTTP server.
///
/// Built with [`APIServer::new`] and started with [`APIServer::start`].
/// Both listeners share the same axum `Router` and `AppState`.
pub struct APIServer {
    config: APIServerConfig,
    router: Router,
}

impl APIServer {
    /// Create a new APIServer with the given config and application state.
    pub fn new<B: ExecutionBackend + Clone + From<DynBackend>>(
        config: APIServerConfig,
        state: AppState<B>,
    ) -> Self {
        let router = build_router(state);
        Self { config, router }
    }

    /// Build the router from a state, for use in unit tests without starting a listener.
    #[cfg(test)]
    pub fn build_router_for_test<B: ExecutionBackend + Clone + From<DynBackend>>(
        state: AppState<B>,
    ) -> Router {
        build_router(state)
    }

    /// Start the server on both TCP and Unix socket listeners.
    ///
    /// Returns an [`APIServerHandle`] for graceful shutdown.
    /// Both listeners run as spawned Tokio tasks sharing the same router.
    /// Token authentication (when configured) is applied only to the TCP router.
    /// The stale Unix socket file is removed before binding if it exists.
    pub async fn start(self) -> Result<APIServerHandle, APIServerError> {
        use crate::api::middleware::TokenAuthLayer;

        let (shutdown_tx, _) = watch::channel(false);
        let Self { config, router } = self;

        // Make sure the socket's directory exists before binding: the default
        // path now sits under the data directory, which a first run has not
        // necessarily created yet.
        #[cfg(unix)]
        if let Some(parent) = config.socket_path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| APIServerError::SocketBindFailed {
                path: parent.display().to_string(),
                source,
            })?;
        }

        // Clean up stale Unix socket file if present.
        if config.socket_path.exists() {
            let _ = std::fs::remove_file(&config.socket_path);
        }

        // Bind Unix socket listener. Never token-authenticated: the socket is a
        // local-trust surface guarded by filesystem permissions.
        #[cfg(unix)]
        let unix_listener = UnixListener::bind(&config.socket_path).map_err(|source| {
            APIServerError::SocketBindFailed {
                path: config.socket_path.display().to_string(),
                source,
            }
        })?;

        // The socket is the one surface the API serves without a token, so its
        // permissions are the whole access control. `bind` applies the process
        // umask, which an operator can loosen; the mode is set explicitly here
        // so the file is owner-only whatever the umask says. A failure is fatal:
        // serving an unauthenticated socket that other accounts can open is the
        // exact posture this guards against.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                &config.socket_path,
                std::fs::Permissions::from_mode(SOCKET_MODE),
            )
            .map_err(|source| APIServerError::SocketBindFailed {
                path: config.socket_path.display().to_string(),
                source,
            })?;
        }

        // Conditionally bind the TCP listener. `None` serves the Unix socket
        // only, closing any unauthenticated TCP exposure for embedded hosts.
        if let Some(tcp_port) = config.tcp_port {
            // Fail-fast: refuse a non-loopback bind with no token
            // rather than silently serving a public unauthenticated API.
            if !is_loopback_addr(&config.bind_addr) && config.api_token.is_none() {
                return Err(APIServerError::InsecureBindWithoutToken {
                    bind_addr: config.bind_addr.clone(),
                });
            }

            // Build the TLS acceptor before binding so a bad certificate fails
            // fast instead of degrading to cleartext.
            let tls_acceptor = match (&config.tls_cert_path, &config.tls_key_path) {
                (Some(cert), Some(key)) => Some(build_tls_acceptor(cert, key)?),
                _ => None,
            };

            let tcp_addr = format!("{}:{}", config.bind_addr, tcp_port);
            let tcp_listener = TcpListener::bind(&tcp_addr).await.map_err(|source| {
                APIServerError::BindFailed {
                    port: tcp_port,
                    source,
                }
            })?;

            // Apply token authentication to the TCP-facing router only.
            let tcp_router = match &config.api_token {
                Some(token) => router.clone().layer(TokenAuthLayer::new(token.as_str())),
                None => router.clone(),
            };

            info!(
                tcp_port = %tcp_port,
                tls_enabled = tls_acceptor.is_some(),
                "api.tcp.listener_ready"
            );

            let mut tcp_shutdown_rx = shutdown_tx.subscribe();
            tokio::spawn(async move {
                serve_tcp(tcp_listener, tcp_router, tls_acceptor, &mut tcp_shutdown_rx).await;
            });
        }

        // Unix socket router: no authentication layer, filesystem permissions suffice.
        // Bound only where a Unix listener can consume it; off Unix the TCP
        // listener above is the whole surface.
        #[cfg(unix)]
        let unix_router = router;
        #[cfg(not(unix))]
        drop(router);

        info!(
            tcp_port = config.tcp_port.map(|p| p as i64).unwrap_or(-1),
            socket_path = %config.socket_path.display(),
            auth_enabled = config.api_token.is_some(),
            tcp_enabled = config.tcp_port.is_some(),
            "api.server.started"
        );

        // Spawn Unix socket listener task (manual accept loop with hyper-util).
        #[cfg(unix)]
        {
            let mut unix_shutdown_rx = shutdown_tx.subscribe();
            tokio::spawn(async move {
                serve_unix(unix_listener, unix_router, &mut unix_shutdown_rx).await;
            });
        }

        let handle = APIServerHandle { shutdown_tx };
        Ok(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tls_test_material::{TEST_TLS_CERT_PEM, TEST_TLS_KEY_PEM};
    use crate::coordinator::ExecutionBackend;
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use crate::router::TaskRouterHandle;
    use crate::test_support::{poll_until_async, reserve_port};
    use apollia_core::{AIPResult, AIPTask, TaskStatus};
    use std::future::Future;
    use std::pin::Pin;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Minimal ExecutionBackend for testing, never actually called.
    #[derive(Clone)]
    struct MockBackend;

    impl From<DynBackend> for MockBackend {
        fn from(_: DynBackend) -> Self {
            MockBackend
        }
    }

    impl ExecutionBackend for MockBackend {
        fn execute(
            &self,
            _task: AIPTask,
        ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
            Box::pin(async {
                Ok(AIPResult {
                    task_id: String::new(),
                    status: TaskStatus::Completed,
                    output: Vec::new(),
                    error: None,
                    artifacts: Vec::new(),
                    input_required_data: None,
                })
            })
        }
    }

    /// Build test AppState with real actors (minimal overhead).
    fn test_app_state() -> AppState<MockBackend> {
        let (event_tx, _event_rx) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);
        AppState {
            router_handle,
            registry_handle,
            event_sender: event_tx,
            agent_loader: Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: MockBackend,
            llm_router: empty_shared_llm_router(),
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            plan_gates: None,
            notification_config: None,
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: None,
            audit_journal: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            notification_repo: None,
            notification_engine_handle: None,
            chat_manager: None,
            plan_cache: None,
            mailbox_handle: None,
            user_memory: None,
            data_dir: std::path::PathBuf::new(),
            stt_engine: crate::api::server::empty_shared_stt_engine(),
            stt_repository: crate::api::server::empty_shared_stt_repository(),
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
            llama_server_supervisor: None,
        }
    }

    /// Injecting to an unknown recipient returns 404.
    #[tokio::test]
    async fn test_inject_unknown_recipient_returns_404() {
        // GIVEN a state with a mailbox but no registered agent
        let mut state = test_app_state();
        let mailbox = AgentMailboxHandle::spawn(
            None,
            state.event_sender.clone(),
            crate::mailbox::MailboxConfig::default(),
        )
        .await;
        state.mailbox_handle = Some(mailbox);

        // WHEN injecting a message to a non-existent recipient
        let result = crate::api::routes_messages::inject_agent_message(
            axum::extract::State(state),
            axum::extract::Path("ghost".to_string()),
            axum::Json(crate::api::routes_messages::InjectMessageBody {
                payload: serde_json::json!({"x": 1}),
                from: Some("op".to_string()),
            }),
        )
        .await;

        // THEN it is rejected with 404
        let (code, _) = result.err().expect("should be an error");
        assert_eq!(code, axum::http::StatusCode::NOT_FOUND);
    }

    /// Injecting when no mailbox is configured returns 503.
    #[tokio::test]
    async fn test_inject_without_mailbox_returns_503() {
        // GIVEN a state without a mailbox
        let state = test_app_state();

        // WHEN injecting a message
        let result = crate::api::routes_messages::inject_agent_message(
            axum::extract::State(state),
            axum::extract::Path("any".to_string()),
            axum::Json(crate::api::routes_messages::InjectMessageBody {
                payload: serde_json::json!({}),
                from: None,
            }),
        )
        .await;

        // THEN it is rejected with 503
        let (code, _) = result.err().expect("should be an error");
        assert_eq!(code, axum::http::StatusCode::SERVICE_UNAVAILABLE);
    }

    /// Create a unique temp socket path.
    fn temp_socket_path() -> PathBuf {
        let id = uuid::Uuid::new_v4();
        std::env::temp_dir().join(format!("apollia-test-{}.sock", id))
    }

    #[tokio::test]
    async fn test_health_endpoint_returns_ok() {
        // GIVEN an APIServer with a minimal router
        let state = test_app_state();
        let router = build_router(state);

        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        // WHEN GET /api/v1/health
        let req = Request::builder()
            .uri("/api/v1/health")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 200 {"status": "ok"}
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
    }

    #[tokio::test]
    async fn test_tcp_listener_binds_successfully() {
        // GIVEN a free port
        let reserved_port = reserve_port();
        let port = reserved_port.port();
        let socket_path = temp_socket_path();
        let state = test_app_state();
        let config = APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: Some(port),
            api_token: None,
            tls_cert_path: None,
            tls_key_path: None,
        };
        let server = APIServer::new(config, state);

        // WHEN start() is called
        // Release the probe listener only now, right before the bind it protects.
        reserved_port.release();
        let handle = server.start().await.unwrap();

        // THEN the server responds over TCP
        let resp = http_get_via_tcp(port).await;
        assert_eq!(resp, r#"{"status":"ok"}"#);

        // Cleanup
        handle.shutdown();
        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_tcp_port_none_serves_unix_only() {
        // GIVEN a config with no TCP port (embedded local-trust default)
        let socket_path = temp_socket_path();
        let reserved_port = reserve_port();
        let port = reserved_port.port();
        let state = test_app_state();
        let config = APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: None,
            api_token: None,
            tls_cert_path: None,
            tls_key_path: None,
        };
        let server = APIServer::new(config, state);

        // WHEN start() is called
        // Release the probe listener only now, right before the bind it protects.
        reserved_port.release();
        let handle = server.start().await.unwrap();

        // THEN the Unix socket serves requests
        let resp = http_get_via_unix(&socket_path).await;
        assert_eq!(resp, r#"{"status":"ok"}"#);

        // AND no TCP listener was bound: the port can be freshly bound. This is
        // the inverse requirement of every other port in this module, the number
        // must stay free rather than be taken, and it holds for the same reason:
        // reserve_port() draws outside the ephemeral pool, so nobody is handed
        // this port while the assertion runs.
        let rebind = TcpListener::bind(("127.0.0.1", port)).await;
        assert!(
            rebind.is_ok(),
            "no TCP listener must occupy the port when tcp_port is None"
        );

        // Cleanup
        handle.shutdown();
        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_tcp_token_required_when_configured() {
        // GIVEN a server bound on TCP with a bearer token
        let socket_path = temp_socket_path();
        let reserved_port = reserve_port();
        let port = reserved_port.port();
        let token = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
        let state = test_app_state();
        let config = APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: Some(port),
            api_token: Some(token.to_owned()),
            tls_cert_path: None,
            tls_key_path: None,
        };
        let server = APIServer::new(config, state);
        // Release the probe listener only now, right before the bind it protects.
        reserved_port.release();
        let handle = server.start().await.unwrap();

        // WHEN a request omits the token THEN it is rejected with 401
        let (status_no, _) = http_get_health_status_via_tcp(port, None).await;
        assert_eq!(status_no, 401, "TCP without token must be rejected");

        // WHEN a request carries the token THEN it succeeds
        let (status_ok, body) = http_get_health_status_via_tcp(port, Some(token)).await;
        assert_eq!(status_ok, 200, "TCP with token must be accepted");
        assert_eq!(body, r#"{"status":"ok"}"#);

        // Cleanup
        handle.shutdown();
        let _ = std::fs::remove_file(&socket_path);
    }

    /// Test-only cert verifier that accepts any server certificate, so the
    /// handshake test does not need a trust anchor for the self-signed fixture.
    #[derive(Debug)]
    struct AcceptAnyCert(Arc<tokio_rustls::rustls::crypto::CryptoProvider>);

    impl tokio_rustls::rustls::client::danger::ServerCertVerifier for AcceptAnyCert {
        fn verify_server_cert(
            &self,
            _end_entity: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
            _intermediates: &[tokio_rustls::rustls::pki_types::CertificateDer<'_>],
            _server_name: &tokio_rustls::rustls::pki_types::ServerName<'_>,
            _ocsp_response: &[u8],
            _now: tokio_rustls::rustls::pki_types::UnixTime,
        ) -> Result<
            tokio_rustls::rustls::client::danger::ServerCertVerified,
            tokio_rustls::rustls::Error,
        > {
            Ok(tokio_rustls::rustls::client::danger::ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            message: &[u8],
            cert: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
            dss: &tokio_rustls::rustls::DigitallySignedStruct,
        ) -> Result<
            tokio_rustls::rustls::client::danger::HandshakeSignatureValid,
            tokio_rustls::rustls::Error,
        > {
            tokio_rustls::rustls::crypto::verify_tls12_signature(
                message,
                cert,
                dss,
                &self.0.signature_verification_algorithms,
            )
        }

        fn verify_tls13_signature(
            &self,
            message: &[u8],
            cert: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
            dss: &tokio_rustls::rustls::DigitallySignedStruct,
        ) -> Result<
            tokio_rustls::rustls::client::danger::HandshakeSignatureValid,
            tokio_rustls::rustls::Error,
        > {
            tokio_rustls::rustls::crypto::verify_tls13_signature(
                message,
                cert,
                dss,
                &self.0.signature_verification_algorithms,
            )
        }

        fn supported_verify_schemes(&self) -> Vec<tokio_rustls::rustls::SignatureScheme> {
            self.0.signature_verification_algorithms.supported_schemes()
        }
    }

    #[tokio::test]
    async fn test_tcp_tls_handshake_serves_health() {
        // GIVEN a server configured with a self-signed cert + key
        let dir = std::env::temp_dir().join(format!("apollia-tls-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let cert_path = dir.join("cert.pem");
        let key_path = dir.join("key.pem");
        std::fs::write(&cert_path, TEST_TLS_CERT_PEM).unwrap();
        std::fs::write(&key_path, TEST_TLS_KEY_PEM).unwrap();

        let socket_path = temp_socket_path();
        let reserved_port = reserve_port();
        let port = reserved_port.port();
        let state = test_app_state();
        let config = APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: Some(port),
            api_token: None,
            tls_cert_path: Some(cert_path),
            tls_key_path: Some(key_path),
        };
        let server = APIServer::new(config, state);
        // Release the probe listener only now, right before the bind it protects.
        reserved_port.release();
        let handle = server.start().await.unwrap();

        // WHEN a TLS client completes the handshake and GETs /api/v1/health
        let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
        let client_config =
            tokio_rustls::rustls::ClientConfig::builder_with_provider(provider.clone())
                .with_safe_default_protocol_versions()
                .unwrap()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(AcceptAnyCert(provider)))
                .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(client_config));
        let server_name =
            tokio_rustls::rustls::pki_types::ServerName::try_from("localhost").unwrap();
        let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("TCP connect");
        let mut tls = connector
            .connect(server_name, tcp)
            .await
            .expect("TLS handshake");
        tls.write_all(
            b"GET /api/v1/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        )
        .await
        .unwrap();
        let mut response = Vec::new();
        tls.read_to_end(&mut response).await.unwrap();
        let response_str = String::from_utf8_lossy(&response);

        // THEN the health body is served over TLS
        let raw_body = response_str.split("\r\n\r\n").nth(1).unwrap_or("").trim();
        let body = match (raw_body.find('{'), raw_body.rfind('}')) {
            (Some(s), Some(e)) => &raw_body[s..=e],
            _ => raw_body,
        };
        assert_eq!(body, r#"{"status":"ok"}"#);

        // Cleanup
        handle.shutdown();
        let _ = std::fs::remove_file(&socket_path);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_non_loopback_bind_without_token_is_refused() {
        // GIVEN a non-loopback bind with no token
        let socket_path = temp_socket_path();
        let reserved_port = reserve_port();
        let port = reserved_port.port();
        let state = test_app_state();
        let config = APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "0.0.0.0".to_owned(),
            tcp_port: Some(port),
            api_token: None,
            tls_cert_path: None,
            tls_key_path: None,
        };
        let server = APIServer::new(config, state);

        // WHEN start() is called THEN it fails fast with InsecureBindWithoutToken
        // Release the probe listener only now, right before the bind it protects.
        reserved_port.release();
        let result = server.start().await;
        assert!(
            matches!(
                &result,
                Err(APIServerError::InsecureBindWithoutToken { .. })
            ),
            "expected InsecureBindWithoutToken, got: {:?}",
            result.as_ref().err()
        );

        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_loopback_bind_without_token_is_allowed() {
        // GIVEN a loopback bind with no token
        let socket_path = temp_socket_path();
        let reserved_port = reserve_port();
        let port = reserved_port.port();
        let state = test_app_state();
        let config = APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: Some(port),
            api_token: None,
            tls_cert_path: None,
            tls_key_path: None,
        };
        let server = APIServer::new(config, state);

        // WHEN start() is called THEN it starts (loopback is trusted)
        // Release the probe listener only now, right before the bind it protects.
        reserved_port.release();
        let handle = server.start().await.expect("loopback bind should start");

        // Cleanup
        handle.shutdown();
        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_non_loopback_bind_with_token_is_allowed() {
        // GIVEN a non-loopback bind with a token
        let socket_path = temp_socket_path();
        let reserved_port = reserve_port();
        let port = reserved_port.port();
        let state = test_app_state();
        let config = APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "0.0.0.0".to_owned(),
            tcp_port: Some(port),
            api_token: Some("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_owned()),
            tls_cert_path: None,
            tls_key_path: None,
        };
        let server = APIServer::new(config, state);

        // WHEN start() is called THEN it starts (token authenticates the surface)
        // Release the probe listener only now, right before the bind it protects.
        reserved_port.release();
        let handle = server.start().await.expect("token bind should start");

        // Cleanup
        handle.shutdown();
        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_unix_socket_listener_binds_successfully() {
        // GIVEN a temporary socket path
        let socket_path = temp_socket_path();
        let reserved_port = reserve_port();
        let port = reserved_port.port();
        let state = test_app_state();
        let config = APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: Some(port),
            api_token: None,
            tls_cert_path: None,
            tls_key_path: None,
        };
        let server = APIServer::new(config, state);

        // WHEN start() is called
        // Release the probe listener only now, right before the bind it protects.
        reserved_port.release();
        let handle = server.start().await.unwrap();

        // THEN the server responds over the Unix socket
        let resp = http_get_via_unix(&socket_path).await;
        assert_eq!(resp, r#"{"status":"ok"}"#);

        // Cleanup
        handle.shutdown();
        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_unix_socket_is_owner_only_after_bind() {
        // GIVEN a permissive umask, so the mode the socket ends up with is the
        //       one the server sets rather than the one the process inherits
        let socket_path = temp_socket_path();
        let reserved_port = reserve_port();
        let port = reserved_port.port();
        let state = test_app_state();
        let config = APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: Some(port),
            api_token: None,
            tls_cert_path: None,
            tls_key_path: None,
        };
        let server = APIServer::new(config, state);

        // WHEN the server binds the socket
        reserved_port.release();
        let handle = server.start().await.unwrap();

        // THEN the socket carries 0600: no group bit, no other bit, so no
        //      other account on the machine can open the unauthenticated API
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&socket_path)
            .expect("the socket exists once start() returned")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "socket mode is {mode:o}, expected 600");

        handle.shutdown();
        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_stale_socket_cleanup() {
        // GIVEN an existing (stale) socket file
        let socket_path = temp_socket_path();
        std::fs::write(&socket_path, b"stale").unwrap();
        assert!(socket_path.exists());

        let reserved_port = reserve_port();
        let port = reserved_port.port();
        let state = test_app_state();
        let config = APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: Some(port),
            api_token: None,
            tls_cert_path: None,
            tls_key_path: None,
        };
        let server = APIServer::new(config, state);

        // WHEN start() is called
        // Release the probe listener only now, right before the bind it protects.
        reserved_port.release();
        let handle = server.start().await.unwrap();

        // THEN the stale file is removed and the bind succeeds
        let resp = http_get_via_unix(&socket_path).await;
        assert_eq!(resp, r#"{"status":"ok"}"#);

        // Cleanup
        handle.shutdown();
        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_shutdown_stops_server() {
        // GIVEN a started APIServer
        let socket_path = temp_socket_path();
        let reserved_port = reserve_port();
        let port = reserved_port.port();
        let state = test_app_state();
        let config = APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: Some(port),
            api_token: None,
            tls_cert_path: None,
            tls_key_path: None,
        };
        let server = APIServer::new(config, state);
        // Release the probe listener only now, right before the bind it protects.
        reserved_port.release();
        let handle = server.start().await.unwrap();

        // Verify it's serving
        let resp = http_get_via_tcp(port).await;
        assert_eq!(resp, r#"{"status":"ok"}"#);

        // WHEN shutdown() is called
        handle.shutdown();

        // THEN the server stops accepting connections. Graceful shutdown is
        // asynchronous (a watch channel drives the listener task), so poll the
        // connect until it is refused rather than sleeping a fixed delay.
        let addr = format!("127.0.0.1:{}", port);
        let stopped = poll_until_async(Duration::from_secs(5), || async {
            tokio::net::TcpStream::connect(&addr).await.is_err()
        })
        .await;
        assert!(stopped, "server should no longer accept connections");

        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_unknown_route_returns_404() {
        // GIVEN an APIServer with the router
        let state = test_app_state();
        let router = build_router(state);

        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        // WHEN GET /api/v1/unknown
        let req = Request::builder()
            .uri("/api/v1/unknown")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 404 Not Found
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// Helper: send HTTP GET /api/v1/health via TCP and return the JSON body.
    async fn http_get_via_tcp(port: u16) -> String {
        let stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .expect("failed to connect to TCP");
        raw_http_get_health(stream).await
    }

    /// GET `/api/v1/health` over TCP with an optional `Bearer` token, returning
    /// the HTTP status code and trimmed body.
    async fn http_get_health_status_via_tcp(port: u16, auth: Option<&str>) -> (u16, String) {
        let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .expect("failed to connect to TCP");
        let auth_line = match auth {
            Some(t) => format!("Authorization: Bearer {t}\r\n"),
            None => String::new(),
        };
        let request = format!(
            "GET /api/v1/health HTTP/1.1\r\nHost: localhost\r\n{auth_line}Connection: close\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let response_str = String::from_utf8_lossy(&response);
        let status = response_str
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(0);
        let body = response_str
            .split("\r\n\r\n")
            .nth(1)
            .unwrap_or("")
            .trim()
            .to_string();
        (status, body)
    }

    /// Helper: send HTTP GET /api/v1/health via Unix socket and return the JSON body.
    async fn http_get_via_unix(path: &std::path::Path) -> String {
        let stream = tokio::net::UnixStream::connect(path)
            .await
            .expect("failed to connect to Unix socket");
        raw_http_get_health(stream).await
    }

    /// Send a raw HTTP/1.1 GET request and extract the JSON body from the response.
    async fn raw_http_get_health<S>(mut stream: S) -> String
    where
        S: AsyncReadExt + AsyncWriteExt + Unpin,
    {
        let request =
            b"GET /api/v1/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
        stream.write_all(request).await.unwrap();

        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let response_str = String::from_utf8_lossy(&response);

        // Extract body after headers (blank line separator).
        // May be chunked, extract the JSON object.
        let raw_body = response_str.split("\r\n\r\n").nth(1).unwrap_or("").trim();

        // Handle chunked transfer encoding: find JSON payload in body.
        if let Some(start) = raw_body.find('{') {
            if let Some(end) = raw_body.rfind('}') {
                return raw_body[start..=end].to_string();
            }
        }
        raw_body.to_string()
    }
}
