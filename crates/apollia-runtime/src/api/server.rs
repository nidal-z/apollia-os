//! APIServer, dual TCP + Unix socket HTTP server for the Apollia runtime.
//!
//! Listens on `localhost:<tcp_port>` and a Unix socket simultaneously,
//! sharing the same axum `Router` and `AppState`.
//!
//! TCP uses `axum::serve` directly. Unix socket uses a manual accept loop
//! with `hyper-util` since axum 0.7 only supports `TcpListener` natively.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
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

/// Build an empty [`SharedLlmRouter`] for tests and "no LLM" boot paths.
pub fn empty_shared_llm_router() -> SharedLlmRouter {
    Arc::new(RwLock::new(None))
}

/// Build a [`SharedLlmRouter`] pre-loaded with a router (or `None` if absent).
pub fn shared_llm_router_from(initial: Option<Arc<LlmRouter>>) -> SharedLlmRouter {
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
    /// Handle to the STT engine actor.
    ///
    /// `Some` after Phase 14 of the Supervisor startup when `stt.enabled = true`.
    /// `None` in tests or when STT is disabled. Routes return 503 when `None`.
    pub stt_engine: Option<crate::stt::SttEngineHandle>,
    /// STT transcription repository, persists transcription history.
    ///
    /// `Some` when the STT subsystem is initialized.
    /// `None` in tests or when STT is disabled.
    pub stt_repository: Option<Arc<std::sync::Mutex<apollia_stt::SttRepository>>>,
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
            mcp_handle: self.mcp_handle.clone(),
            mcp_server_repo: self.mcp_server_repo.clone(),
            llm_backend_repo: self.llm_backend_repo.clone(),
            stt_config_repo: self.stt_config_repo.clone(),
            a2a_invoker: self.a2a_invoker.clone(),
            resilience_layer: self.resilience_layer.clone(),
            runner_proxy: self.runner_proxy.clone(),
        }
    }
}

/// Configuration for the APIServer.
pub struct APIServerConfig {
    /// Path to the Unix domain socket (e.g. `/tmp/apollia.sock`).
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
    /// `Some(port)`. When a TCP port is bound without an `api_token`, a warning
    /// is emitted because the port is then unauthenticated.
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
    pub api_token: Option<String>,
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

/// Response body for the health endpoint.
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct HealthResponse {
    status: String,
}

/// Response body for the shutdown endpoint.
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct ShutdownResponse {
    status: String,
}

/// Handler for `GET /api/v1/health`.
#[utoipa::path(
    get,
    path = "/api/v1/health",
    tag = "health",
    responses((status = 200, description = "Runtime is up", body = HealthResponse))
)]
pub(crate) async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".into(),
    })
}

/// Handler for `POST /api/v1/shutdown`.
///
/// Emits [`RuntimeEvent::ShutdownRequested`] on the EventBus. The caller
/// (typically `apollia-os start`) listens for this event to trigger
/// graceful shutdown.
#[utoipa::path(
    post,
    path = "/api/v1/shutdown",
    tag = "health",
    responses((status = 200, description = "Shutdown initiated", body = ShutdownResponse))
)]
pub(crate) async fn shutdown_handler<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Json<ShutdownResponse> {
    info!("Shutdown requested via API");
    let _ = state
        .event_sender
        .send(apollia_core::RuntimeEvent::ShutdownRequested);
    Json(ShutdownResponse {
        status: "shutting_down".into(),
    })
}

/// Build the axum Router with all routes and shared state.
fn build_router<B: ExecutionBackend + Clone + From<DynBackend>>(state: AppState<B>) -> Router {
    use super::routes_a2a::{
        delegate, get_task_sidechains, invoke_by_skill, list_a2a_agents, list_a2a_skills,
    };
    use super::routes_agents::{get_agent, list_agents, start_agent, stop_agent};
    use super::routes_approvals::{list_pending_approvals, list_resolved_approvals};
    use super::routes_audit::{
        get_audit_anchor, get_audit_stats, list_audit, post_replay_run, show_audit_run,
        verify_audit_journal, verify_audit_run,
    };
    use super::routes_chat::{
        authorize_tool as chat_authorize_tool, close_session, create_session,
        fork_session as chat_fork_session, get_session as chat_get_session,
        get_session_todo as chat_get_session_todo, list_recent_sessions, list_session_children,
        list_sessions, resume_session as chat_resume_session, send_message, stream_session,
    };
    use super::routes_hooks::list_hooks;
    use super::routes_llm::llm_routes;
    use super::routes_mcp::mcp_router;
    use super::routes_model_hub::model_hub_routes;
    use super::routes_notifications::{
        create_channel, delete_channel, get_events, list_channels, notification_logs, set_events,
        test_channels, update_channel,
    };
    use super::routes_plan_cache::{clear_plan_cache, get_plan_cache_stats};
    use super::routes_resilience::resilience_router;
    use super::routes_review::post_review;
    use super::routes_sse::stream_task;
    use super::routes_stt::{
        delete_transcription, get_stt_config, list_models, list_transcriptions, stt_status,
        transcribe_audio, update_stt_config,
    };
    use super::routes_tasks::{
        cancel_task, get_task, list_tasks, resume_task, submit_plan_decision, submit_task,
    };
    use super::routes_timeline::get_task_timeline;
    use super::routes_tools::{describe_tool, list_tools};
    use super::routes_trace::get_task_trace;
    use super::routes_triggers::{
        create_trigger, delete_trigger, disable_trigger, enable_trigger, fire_trigger,
        get_trigger_by_id, get_trigger_logs, list_triggers, reload_triggers, update_trigger,
    };
    use super::routes_webhooks::handle_webhook;
    use crate::api::routes_messages::{inject_agent_message, list_agent_messages, stream_mailbox};

    Router::new()
        .route("/api/v1/health", get(health_handler))
        .route(
            "/api/v1/openapi.json",
            get(crate::api::openapi::openapi_json),
        )
        .route("/api/v1/shutdown", post(shutdown_handler::<B>))
        .route("/api/v1/tasks", get(list_tasks::<B>).post(submit_task::<B>))
        .route(
            "/api/v1/tasks/:id",
            get(get_task::<B>).delete(cancel_task::<B>),
        )
        .route("/api/v1/tasks/:id/stream", get(stream_task::<B>))
        .route("/api/v1/tasks/:id/resume", post(resume_task::<B>))
        .route(
            "/api/v1/tasks/:id/plan-decision",
            post(submit_plan_decision::<B>),
        )
        .route("/api/v1/tasks/:id/review", post(post_review::<B>))
        // Timeline route (legacy, deprecation candidate)
        .route("/api/v1/tasks/:id/timeline", get(get_task_timeline::<B>))
        // Event-sourced trace
        .route("/api/v1/tasks/:id/trace", get(get_task_trace::<B>))
        // Tool routes
        .route("/api/v1/tools", get(list_tools::<B>))
        .route("/api/v1/tools/:name", get(describe_tool::<B>))
        // Audit trail routes
        .route("/api/v1/audit", get(list_audit::<B>))
        .route("/api/v1/audit/stats", get(get_audit_stats::<B>))
        .route("/api/v1/audit/verify", get(verify_audit_journal::<B>))
        .route("/api/v1/audit/verify/:run_id", get(verify_audit_run::<B>))
        .route("/api/v1/audit/anchor", get(get_audit_anchor::<B>))
        .route("/api/v1/audit/journal/:run_id", get(show_audit_run::<B>))
        .route("/api/v1/audit/replay/:run_id", post(post_replay_run::<B>))
        // Lifecycle hooks route
        .route("/api/v1/hooks", get(list_hooks::<B>))
        .route(
            "/api/v1/agents",
            get(list_agents::<B>).post(start_agent::<B>),
        )
        .route(
            "/api/v1/agents/:id",
            get(get_agent::<B>).delete(stop_agent::<B>),
        )
        .route(
            "/api/v1/agents/:name/messages",
            get(list_agent_messages::<B>).post(inject_agent_message::<B>),
        )
        .route("/api/v1/mailbox/stream", get(stream_mailbox::<B>))
        // A2A routing routes
        .route("/api/v1/a2a/agents", get(list_a2a_agents::<B>))
        .route("/api/v1/a2a/delegate", post(delegate::<B>))
        .route("/api/v1/a2a/skills", get(list_a2a_skills::<B>))
        .route("/api/v1/a2a/invoke", post(invoke_by_skill::<B>))
        // Sidechain traceability
        .route(
            "/api/v1/tasks/:id/sidechains",
            get(get_task_sidechains::<B>),
        )
        // Plan cache routes
        .route("/api/v1/plan-cache/stats", get(get_plan_cache_stats::<B>))
        .route("/api/v1/plan-cache/clear", post(clear_plan_cache::<B>))
        .route("/webhooks/:id", post(handle_webhook::<B>))
        // HITL approval routes
        .route(
            "/api/v1/approvals/pending",
            get(list_pending_approvals::<B>),
        )
        .route(
            "/api/v1/approvals/resolved",
            get(list_resolved_approvals::<B>),
        )
        // Trigger routes (reload + status + CRUD)
        .route(
            "/api/v1/triggers",
            get(list_triggers::<B>).post(create_trigger::<B>),
        )
        .route("/api/v1/triggers/reload", post(reload_triggers::<B>))
        .route(
            "/api/v1/triggers/:id",
            get(get_trigger_by_id::<B>)
                .put(update_trigger::<B>)
                .delete(delete_trigger::<B>),
        )
        .route("/api/v1/triggers/:id/fire", post(fire_trigger::<B>))
        .route("/api/v1/triggers/:id/enable", post(enable_trigger::<B>))
        .route("/api/v1/triggers/:id/disable", post(disable_trigger::<B>))
        .route("/api/v1/triggers/:id/logs", get(get_trigger_logs::<B>))
        // Notification routes (CRUD)
        .route(
            "/api/v1/notifications/channels",
            get(list_channels::<B>).post(create_channel::<B>),
        )
        .route(
            "/api/v1/notifications/channels/:id",
            axum::routing::put(update_channel::<B>).delete(delete_channel::<B>),
        )
        .route(
            "/api/v1/notifications/events",
            get(get_events::<B>).put(set_events::<B>),
        )
        .route(
            "/api/v1/notifications/channels/:id/test",
            post(test_channels::<B>),
        )
        .route("/api/v1/notifications/test", post(test_channels::<B>))
        .route("/api/v1/notifications/logs", get(notification_logs::<B>))
        .merge(llm_routes::<B>())
        .merge(model_hub_routes::<B>())
        .merge(resilience_router::<B>())
        // Chat session routes
        .route(
            "/api/v1/sessions",
            get(list_sessions::<B>).post(create_session::<B>),
        )
        // /recent must be registered before /:id to avoid the path param capturing "recent"
        .route("/api/v1/sessions/recent", get(list_recent_sessions::<B>))
        .route(
            "/api/v1/sessions/:id",
            get(chat_get_session::<B>).delete(close_session::<B>),
        )
        .route("/api/v1/sessions/:id/messages", post(send_message::<B>))
        .route(
            "/api/v1/sessions/:id/authorize",
            post(chat_authorize_tool::<B>),
        )
        .route("/api/v1/sessions/:id/stream", get(stream_session::<B>))
        .route("/api/v1/sessions/:id/todo", get(chat_get_session_todo::<B>))
        .route(
            "/api/v1/sessions/:id/resume",
            post(chat_resume_session::<B>),
        )
        .route("/api/v1/sessions/:id/fork", post(chat_fork_session::<B>))
        .route(
            "/api/v1/sessions/:id/children",
            get(list_session_children::<B>),
        )
        // STT routes
        .route("/api/v1/stt/status", get(stt_status::<B>))
        .route("/api/v1/stt/transcribe", post(transcribe_audio::<B>))
        .route("/api/v1/stt/transcriptions", get(list_transcriptions::<B>))
        .route(
            "/api/v1/stt/transcriptions/:id",
            axum::routing::delete(delete_transcription::<B>),
        )
        .route("/api/v1/stt/models", get(list_models::<B>))
        .route(
            "/api/v1/stt/config",
            get(get_stt_config::<B>).put(update_stt_config::<B>),
        )
        // MCP routes
        .merge(mcp_router::<B>())
        .with_state(state)
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

        // Conditionally bind the TCP listener. `None` serves the Unix socket
        // only, closing any unauthenticated TCP exposure for embedded hosts.
        if let Some(tcp_port) = config.tcp_port {
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
                None => {
                    tracing::warn!(
                        tcp_port = %tcp_port,
                        "api.tcp.unauthenticated"
                    );
                    router.clone()
                }
            };

            let mut tcp_shutdown_rx = shutdown_tx.subscribe();
            tokio::spawn(async move {
                let result = axum::serve(tcp_listener, tcp_router)
                    .with_graceful_shutdown(async move {
                        let _ = tcp_shutdown_rx.wait_for(|v| *v).await;
                    })
                    .await;
                if let Err(e) = result {
                    tracing::error!(error = %e, "TCP listener error");
                }
            });
        }

        // Unix socket router: no authentication layer, filesystem permissions suffice.
        let unix_router = router;

        info!(
            tcp_port = config.tcp_port.map(|p| p as i64).unwrap_or(-1),
            socket_path = %config.socket_path.display(),
            auth_enabled = config.api_token.is_some(),
            tcp_enabled = config.tcp_port.is_some(),
            "APIServer started"
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

/// Serve HTTP requests over a Unix domain socket using hyper-util.
///
/// Runs an accept loop that converts each incoming `UnixStream` into
/// a hyper connection via `TokioIo`, then dispatches to the axum router
/// through `TowerToHyperService`.
#[cfg(unix)]
async fn serve_unix(
    listener: UnixListener,
    router: Router,
    shutdown_rx: &mut watch::Receiver<bool>,
) {
    use hyper_util::rt::TokioIo;
    use hyper_util::server::conn::auto::Builder as ServerBuilder;
    use hyper_util::service::TowerToHyperService;

    let builder = ServerBuilder::new(hyper_util::rt::TokioExecutor::new());

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, _addr)) => {
                        let io = TokioIo::new(stream);
                        let svc = TowerToHyperService::new(router.clone());
                        let conn_builder = builder.clone();
                        tokio::spawn(async move {
                            if let Err(e) = conn_builder.serve_connection(io, svc).await {
                                // "error shutting down connection" is benign, the CLI client
                                // closed its end before hyper completed the graceful shutdown.
                                // Note: hyper emits "shutting down" (not "shutdown") in this message,
                                // so we match on "shut" to cover both variants.
                                let msg = e.to_string();
                                if msg.contains("shut") || msg.contains("broken pipe") || msg.contains("connection reset") {
                                    tracing::debug!(error = %e, "Unix socket connection closed by client");
                                } else {
                                    tracing::error!(error = %e, "Unix socket connection error");
                                }
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Unix socket accept error");
                    }
                }
            }
            _ = shutdown_rx.wait_for(|v| *v) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinator::ExecutionBackend;
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use crate::router::TaskRouterHandle;
    use apollia_core::{AIPResult, AIPTask, TaskStatus};
    use std::future::Future;
    use std::pin::Pin;
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
            stt_engine: None,
            stt_repository: None,
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
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

    /// Find a free TCP port by binding to port 0.
    async fn free_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        listener.local_addr().unwrap().port()
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
        let port = free_port().await;
        let socket_path = temp_socket_path();
        let state = test_app_state();
        let config = APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: Some(port),
            api_token: None,
        };
        let server = APIServer::new(config, state);

        // WHEN start() is called
        let handle = server.start().await.unwrap();

        // THEN the server responds over TCP
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let resp = http_get_via_tcp(port).await;
        assert_eq!(resp, r#"{"status":"ok"}"#);

        // Cleanup
        handle.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_tcp_port_none_serves_unix_only() {
        // GIVEN a config with no TCP port (embedded local-trust default)
        let socket_path = temp_socket_path();
        let port = free_port().await;
        let state = test_app_state();
        let config = APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: None,
            api_token: None,
        };
        let server = APIServer::new(config, state);

        // WHEN start() is called
        let handle = server.start().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // THEN the Unix socket serves requests
        let resp = http_get_via_unix(&socket_path).await;
        assert_eq!(resp, r#"{"status":"ok"}"#);

        // AND no TCP listener was bound: the port can be freshly bound
        let rebind = TcpListener::bind(("127.0.0.1", port)).await;
        assert!(
            rebind.is_ok(),
            "no TCP listener must occupy the port when tcp_port is None"
        );

        // Cleanup
        handle.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_tcp_token_required_when_configured() {
        // GIVEN a server bound on TCP with a bearer token
        let socket_path = temp_socket_path();
        let port = free_port().await;
        let token = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
        let state = test_app_state();
        let config = APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: Some(port),
            api_token: Some(token.to_owned()),
        };
        let server = APIServer::new(config, state);
        let handle = server.start().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // WHEN a request omits the token THEN it is rejected with 401
        let (status_no, _) = http_get_health_status_via_tcp(port, None).await;
        assert_eq!(status_no, 401, "TCP without token must be rejected");

        // WHEN a request carries the token THEN it succeeds
        let (status_ok, body) = http_get_health_status_via_tcp(port, Some(token)).await;
        assert_eq!(status_ok, 200, "TCP with token must be accepted");
        assert_eq!(body, r#"{"status":"ok"}"#);

        // Cleanup
        handle.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_unix_socket_listener_binds_successfully() {
        // GIVEN a temporary socket path
        let socket_path = temp_socket_path();
        let port = free_port().await;
        let state = test_app_state();
        let config = APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: Some(port),
            api_token: None,
        };
        let server = APIServer::new(config, state);

        // WHEN start() is called
        let handle = server.start().await.unwrap();

        // THEN the server responds over the Unix socket
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let resp = http_get_via_unix(&socket_path).await;
        assert_eq!(resp, r#"{"status":"ok"}"#);

        // Cleanup
        handle.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_stale_socket_cleanup() {
        // GIVEN an existing (stale) socket file
        let socket_path = temp_socket_path();
        std::fs::write(&socket_path, b"stale").unwrap();
        assert!(socket_path.exists());

        let port = free_port().await;
        let state = test_app_state();
        let config = APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: Some(port),
            api_token: None,
        };
        let server = APIServer::new(config, state);

        // WHEN start() is called
        let handle = server.start().await.unwrap();

        // THEN the stale file is removed and the bind succeeds
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let resp = http_get_via_unix(&socket_path).await;
        assert_eq!(resp, r#"{"status":"ok"}"#);

        // Cleanup
        handle.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_shutdown_stops_server() {
        // GIVEN a started APIServer
        let socket_path = temp_socket_path();
        let port = free_port().await;
        let state = test_app_state();
        let config = APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: Some(port),
            api_token: None,
        };
        let server = APIServer::new(config, state);
        let handle = server.start().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Verify it's serving
        let resp = http_get_via_tcp(port).await;
        assert_eq!(resp, r#"{"status":"ok"}"#);

        // WHEN shutdown() is called
        handle.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // THEN the server no longer responds
        let result = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port)).await;
        assert!(
            result.is_err(),
            "server should no longer accept connections"
        );

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
