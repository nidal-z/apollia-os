//! APIServer — dual TCP + Unix socket HTTP server for the Apollia runtime.
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
use tokio::sync::watch;
use tracing::info;

use apollia_core::{LlmBackendRepository, PendingApprovals, SttConfigRepository};
use apollia_llm::{LlmCallRepository, LlmRouter};
use apollia_mcp::manager::McpClientManagerHandle;
use apollia_memory::user_memory::UserMemoryRepository;
use apollia_notifications::{
    NotificationConfig, NotificationConfigRepository, NotificationEngineHandle,
};
use apollia_oria::plan_cache::PlanCacheRepository;
use apollia_pipelines::{PipelineDefinitionRepository, PipelineEngineHandle};
use apollia_tools::{AuditTrailHandle, TaskRepository, ToolRegistryHandle};
use apollia_triggers::{TriggerDefinitionRepository, TriggerEngineHandle};

use crate::mailbox::AgentMailboxHandle;

use crate::api::routes_agents::{AgentBackendFactory, AgentLoader};
use crate::chat::ChatSessionManagerHandle;
use crate::coordinator::{DynBackend, ExecutionBackend};
use crate::eventbus::EventBusSender;
use crate::registry::AgentRegistryHandle;
use crate::router::TaskRouterHandle;

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
    /// Agent loader for Python module loading (ADR-019).
    pub agent_loader: Arc<dyn AgentLoader>,
    /// Execution backend — cloned per coordinator on agent start.
    pub backend: B,
    /// LLM router — `None` if no LLM backend was configured or available.
    ///
    /// Injected into each agent's `RuntimeContext` via `ctx.llm`.
    /// Agents receive `ctx.llm = None` and an `AgentDegraded` event if absent.
    pub llm_router: Option<Arc<LlmRouter>>,
    /// Handle to the TriggerEngine actor.
    ///
    /// Webhook route returns 503 Service Unavailable when this is `None`.
    pub trigger_engine: Option<TriggerEngineHandle>,
    /// Path to `apollia.toml` — used by `POST /api/v1/triggers/reload`.
    ///
    /// `None` when the runtime was started without a config file (e.g. in unit tests).
    /// The reload route returns 503 when this is `None`.
    pub config_path: Option<PathBuf>,
    /// HITL task repository — SQLite persistence for Human-in-the-Loop state.
    ///
    /// Opened by the Supervisor on startup from `~/.apollia/hitl.db`.
    /// `None` in unit tests or when HITL is not configured.
    /// The resume route returns 503 when this is `None`.
    pub task_repository: Option<Arc<TaskRepository>>,
    /// Registre HITL des approbations en attente — partagé entre routes et ORIAEngine.
    ///
    /// `ResumeHandler` appelle `pending_approvals.resolve()` pour débloquer
    /// `execute_direct()` qui attend sur le oneshot channel.
    /// `None` quand le HITL n'est pas configuré — `resume_task` logue un warning.
    pub pending_approvals: Option<Arc<PendingApprovals>>,
    /// Configuration des canaux de notification chargée depuis `apollia.toml`.
    ///
    /// Utilisée par `GET /api/v1/notifications/channels` et
    /// `POST /api/v1/notifications/test`.
    /// `None` si aucune section `[notifications]` n'est présente dans la config.
    pub notification_config: Option<NotificationConfig>,
    /// Handle vers le `PipelineEngine` actor.
    ///
    /// `None` quand aucun `[[pipelines]]` n'est déclaré dans `apollia.toml`.
    /// Les routes REST pipelines retournent 503 quand `None`.
    pub pipeline_engine: Option<PipelineEngineHandle>,
    /// Factory for creating per-agent execution backends (ADR-019 extension).
    ///
    /// `Some` in production — creates real `AIPBridge` backends with tool access.
    /// `None` in tests — falls back to `state.backend.clone()` (MockBackend/NoopBackend).
    pub backend_factory: Option<Arc<dyn AgentBackendFactory>>,
    /// Handle to the ToolRegistry actor — exposes the tool catalogue via REST.
    ///
    /// `Some` in production — populated by the Supervisor at startup.
    /// `None` in tests — the `/api/v1/tools` routes return 503 when `None`.
    pub tool_registry_handle: Option<ToolRegistryHandle>,
    /// Handle to the AuditTrail actor — exposes tool invocations via REST.
    ///
    /// `Some` in production — opened by the Supervisor from `~/.apollia/audit.db`.
    /// `None` in tests — the `/api/v1/audit` routes return 503 when `None`.
    pub audit_trail: Option<AuditTrailHandle>,
    /// Configuration de troncature pour l'observabilité des tâches.
    ///
    /// Passée aux `ExecutionCoordinator` pour la persistance input/output/transitions.
    pub obs_config: apollia_core::ObservabilityConfig,
    /// Repository des appels LLM — agrégation coûts/tokens.
    ///
    /// `Some` quand un `LlmRouter` est configuré et que `llm_calls.db` est ouvert.
    /// `None` en tests ou quand aucun backend LLM n'est configuré.
    pub llm_call_repository: Option<Arc<std::sync::Mutex<LlmCallRepository>>>,
    /// Repository CRUD des définitions de triggers.
    ///
    /// Ouvert par le Supervisor depuis `data_dir/triggers.db`.
    /// Partagé entre le boot (lecture initiale) et les routes REST CRUD.
    /// `None` en tests unitaires.
    pub trigger_def_repo: Option<Arc<std::sync::Mutex<TriggerDefinitionRepository>>>,
    /// Repository CRUD des définitions de pipelines.
    ///
    /// Ouvert par le Supervisor depuis `data_dir/pipelines.db`.
    /// Partagé entre le boot (lecture initiale) et les routes REST CRUD.
    /// `None` en tests unitaires.
    pub pipeline_def_repo: Option<Arc<std::sync::Mutex<PipelineDefinitionRepository>>>,
    /// Repository CRUD de la configuration des notifications.
    ///
    /// Ouvert par le Supervisor depuis `data_dir/notifications.db`.
    /// Partagé entre le boot (lecture initiale) et les routes REST CRUD.
    /// `None` en tests unitaires.
    pub notification_repo: Option<Arc<std::sync::Mutex<NotificationConfigRepository>>>,
    /// Handle vers le [`NotificationEngine`] pour hot-reload après CRUD.
    ///
    /// Permet aux routes REST de déclencher un rechargement des canaux après
    /// une mutation dans `notifications.db`. `None` en tests unitaires.
    pub notification_engine_handle: Option<NotificationEngineHandle>,
    /// Handle to the [`ChatSessionManager`] actor.
    ///
    /// `Some` after Phase 13 of the Supervisor startup sequence.
    /// `None` in tests or when the chat subsystem is not configured.
    pub chat_manager: Option<ChatSessionManagerHandle>,
    /// ORIA plan cache repository — stores cached execution plans.
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
    /// STT transcription repository — persists transcription history.
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
    /// Repository CRUD des backends LLM.
    ///
    /// Ouvert par le Supervisor depuis `data_dir/system.db`.
    /// Partagé entre le boot (chargement du LlmRouter) et les routes REST CRUD.
    /// `None` en tests unitaires ou quand `system.db` n'a pas pu être ouvert.
    pub llm_backend_repo: Option<Arc<std::sync::Mutex<LlmBackendRepository>>>,
    /// STT configuration repository — persists and reads the singleton `stt_config`
    /// row in `system.db`.
    ///
    /// `Some` after Phase 15 of the Supervisor startup sequence.
    /// `None` in tests or when `system.db` could not be opened.
    /// Config routes return 503 when `None`.
    pub stt_config_repo: Option<Arc<std::sync::Mutex<SttConfigRepository>>>,
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
            notification_config: self.notification_config.clone(),
            pipeline_engine: self.pipeline_engine.clone(),
            backend_factory: self.backend_factory.clone(),
            tool_registry_handle: self.tool_registry_handle.clone(),
            audit_trail: self.audit_trail.clone(),
            obs_config: self.obs_config.clone(),
            llm_call_repository: self.llm_call_repository.clone(),
            trigger_def_repo: self.trigger_def_repo.clone(),
            pipeline_def_repo: self.pipeline_def_repo.clone(),
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
        }
    }
}

/// Configuration for the APIServer.
pub struct APIServerConfig {
    /// Path to the Unix domain socket (e.g. `/tmp/apollia.sock`).
    pub socket_path: PathBuf,
    /// TCP port to listen on (e.g. `7771`).
    pub tcp_port: u16,
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

/// The Apollia APIServer — dual TCP + Unix socket HTTP server.
///
/// Built with [`APIServer::new`] and started with [`APIServer::start`].
/// Both listeners share the same axum `Router` and `AppState`.
pub struct APIServer {
    config: APIServerConfig,
    router: Router,
}

/// Response body for the health endpoint.
#[derive(Serialize)]
struct HealthResponse {
    status: String,
}

/// Response body for the shutdown endpoint.
#[derive(Serialize)]
struct ShutdownResponse {
    status: String,
}

/// Handler for `GET /api/v1/health`.
async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".into(),
    })
}

/// Handler for `POST /api/v1/shutdown`.
///
/// Emits [`RuntimeEvent::ShutdownRequested`] on the EventBus. The caller
/// (typically `apollia-os start`) listens for this event to trigger
/// graceful shutdown (ADR-018).
async fn shutdown_handler<B: ExecutionBackend + Clone>(
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
    use super::routes_agents::{get_agent, list_agents, start_agent, stop_agent};
    use super::routes_approvals::{list_pending_approvals, list_resolved_approvals};
    use super::routes_audit::{get_audit_stats, list_audit};
    use super::routes_chat::{
        authorize_tool as chat_authorize_tool, close_session, create_session,
        get_session as chat_get_session, list_sessions, send_message, stream_session,
    };
    use super::routes_llm::llm_routes;
    use super::routes_mcp::mcp_router;
    use super::routes_messages::list_agent_messages;
    use super::routes_notifications::{
        create_channel, delete_channel, get_events, list_channels, notification_logs, set_events,
        test_channels, update_channel,
    };
    use super::routes_pipelines::{
        create_pipeline, delete_pipeline, get_pipeline, get_run, get_run_by_id, list_pipelines,
        list_runs, run_pipeline, update_pipeline,
    };
    use super::routes_plan_cache::{clear_plan_cache, get_plan_cache_stats};
    use super::routes_sse::stream_task;
    use super::routes_stt::{
        delete_transcription, get_stt_config, list_models, list_transcriptions, stt_status,
        transcribe_audio, update_stt_config,
    };
    use super::routes_tasks::{cancel_task, get_task, list_tasks, resume_task, submit_task};
    use super::routes_timeline::get_task_timeline;
    use super::routes_tools::{describe_tool, list_tools};
    use super::routes_triggers::{
        create_trigger, delete_trigger, disable_trigger, enable_trigger, fire_trigger,
        get_trigger_by_id, get_trigger_logs, list_triggers, reload_triggers, update_trigger,
    };
    use super::routes_user::{forget_memory, get_memory, get_profile, update_profile};
    use super::routes_webhooks::handle_webhook;

    Router::new()
        .route("/api/v1/health", get(health_handler))
        .route("/api/v1/shutdown", post(shutdown_handler::<B>))
        .route("/api/v1/tasks", get(list_tasks::<B>).post(submit_task::<B>))
        .route(
            "/api/v1/tasks/:id",
            get(get_task::<B>).delete(cancel_task::<B>),
        )
        .route("/api/v1/tasks/:id/stream", get(stream_task::<B>))
        .route("/api/v1/tasks/:id/resume", post(resume_task::<B>))
        // Timeline route
        .route("/api/v1/tasks/:id/timeline", get(get_task_timeline::<B>))
        // Tool routes
        .route("/api/v1/tools", get(list_tools::<B>))
        .route("/api/v1/tools/:name", get(describe_tool::<B>))
        // Audit trail routes
        .route("/api/v1/audit", get(list_audit::<B>))
        .route("/api/v1/audit/stats", get(get_audit_stats::<B>))
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
            get(list_agent_messages::<B>),
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
        // Pipeline routes (CRUD + run management)
        .route(
            "/api/v1/pipelines",
            get(list_pipelines::<B>).post(create_pipeline::<B>),
        )
        .route(
            "/api/v1/pipelines/:id",
            get(get_pipeline::<B>)
                .put(update_pipeline::<B>)
                .delete(delete_pipeline::<B>),
        )
        .route("/api/v1/pipelines/:id/run", post(run_pipeline::<B>))
        .route("/api/v1/pipelines/:id/runs", get(list_runs::<B>))
        .route("/api/v1/pipelines/:id/runs/:run_id", get(get_run::<B>))
        .route("/api/v1/runs/:run_id", get(get_run_by_id::<B>))
        // Chat session routes
        .route(
            "/api/v1/sessions",
            get(list_sessions::<B>).post(create_session::<B>),
        )
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
        // User profile + memory routes
        .route(
            "/api/v1/user/profile",
            get(get_profile::<B>).put(update_profile::<B>),
        )
        .route("/api/v1/user/memory", get(get_memory::<B>))
        .route(
            "/api/v1/user/memory/:key",
            axum::routing::delete(forget_memory::<B>),
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
    /// The stale Unix socket file is removed before binding if it exists.
    pub async fn start(self) -> Result<APIServerHandle, APIServerError> {
        let (shutdown_tx, _) = watch::channel(false);

        // Bind TCP listener
        let tcp_addr = format!("127.0.0.1:{}", self.config.tcp_port);
        let tcp_listener =
            TcpListener::bind(&tcp_addr)
                .await
                .map_err(|source| APIServerError::BindFailed {
                    port: self.config.tcp_port,
                    source,
                })?;

        // Clean up stale Unix socket file if present
        if self.config.socket_path.exists() {
            let _ = std::fs::remove_file(&self.config.socket_path);
        }

        // Bind Unix socket listener
        #[cfg(unix)]
        let unix_listener = UnixListener::bind(&self.config.socket_path).map_err(|source| {
            APIServerError::SocketBindFailed {
                path: self.config.socket_path.display().to_string(),
                source,
            }
        })?;

        info!(
            tcp_port = %self.config.tcp_port,
            socket_path = %self.config.socket_path.display(),
            "APIServer started on TCP and Unix socket"
        );

        // Spawn TCP listener task with graceful shutdown
        let tcp_router = self.router.clone();
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

        // Spawn Unix socket listener task (manual accept loop with hyper-util)
        #[cfg(unix)]
        {
            let unix_router = self.router;
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
                                // "error shutting down connection" is benign — the CLI client
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

    /// Minimal ExecutionBackend for testing — never actually called.
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
            llm_router: None,
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            notification_config: None,
            pipeline_engine: None,
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            pipeline_def_repo: None,
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
        }
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
        // GIVEN un APIServer avec un router minimal
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
        // GIVEN un port libre
        let port = free_port().await;
        let socket_path = temp_socket_path();
        let state = test_app_state();
        let config = APIServerConfig {
            socket_path: socket_path.clone(),
            tcp_port: port,
        };
        let server = APIServer::new(config, state);

        // WHEN start() est appele
        let handle = server.start().await.unwrap();

        // THEN le serveur repond sur TCP
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let resp = http_get_via_tcp(port).await;
        assert_eq!(resp, r#"{"status":"ok"}"#);

        // Cleanup
        handle.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_unix_socket_listener_binds_successfully() {
        // GIVEN un chemin socket temporaire
        let socket_path = temp_socket_path();
        let port = free_port().await;
        let state = test_app_state();
        let config = APIServerConfig {
            socket_path: socket_path.clone(),
            tcp_port: port,
        };
        let server = APIServer::new(config, state);

        // WHEN start() est appele
        let handle = server.start().await.unwrap();

        // THEN le serveur repond sur Unix socket
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
        // GIVEN un fichier socket existant (stale)
        let socket_path = temp_socket_path();
        std::fs::write(&socket_path, b"stale").unwrap();
        assert!(socket_path.exists());

        let port = free_port().await;
        let state = test_app_state();
        let config = APIServerConfig {
            socket_path: socket_path.clone(),
            tcp_port: port,
        };
        let server = APIServer::new(config, state);

        // WHEN start() est appele
        let handle = server.start().await.unwrap();

        // THEN le fichier stale est supprime et le bind reussit
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
        // GIVEN un APIServer demarre
        let socket_path = temp_socket_path();
        let port = free_port().await;
        let state = test_app_state();
        let config = APIServerConfig {
            socket_path: socket_path.clone(),
            tcp_port: port,
        };
        let server = APIServer::new(config, state);
        let handle = server.start().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Verify it's serving
        let resp = http_get_via_tcp(port).await;
        assert_eq!(resp, r#"{"status":"ok"}"#);

        // WHEN shutdown() est appele
        handle.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // THEN le serveur ne repond plus
        let result = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port)).await;
        assert!(
            result.is_err(),
            "server should no longer accept connections"
        );

        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_unknown_route_returns_404() {
        // GIVEN un APIServer avec le router
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
        // May be chunked — extract the JSON object.
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
