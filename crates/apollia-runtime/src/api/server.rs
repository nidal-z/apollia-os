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
use axum::response::Redirect;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use tokio::net::TcpListener;
#[cfg(unix)]
use tokio::net::UnixListener;
use tokio::sync::watch;
use tracing::info;

use apollia_core::PendingApprovals;
use apollia_llm::LlmRouter;
use apollia_notifications::NotificationConfig;
use apollia_pipelines::PipelineEngineHandle;
use apollia_tools::{AuditTrailHandle, TaskRepository, ToolRegistryHandle};
use apollia_triggers::TriggerEngineHandle;

use crate::api::routes_agents::{AgentBackendFactory, AgentLoader};
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
    /// Injected into each agent's `RuntimeContext` via `ctx.llm` (STORY-059).
    /// Agents receive `ctx.llm = None` and an `AgentDegraded` event if absent.
    pub llm_router: Option<Arc<LlmRouter>>,
    /// Handle to the TriggerEngine actor — `None` before STORY-072 (Supervisor integration).
    ///
    /// Webhook route returns 503 Service Unavailable when this is `None` (AC-6).
    pub trigger_engine: Option<TriggerEngineHandle>,
    /// Path to `apollia.toml` — used by `POST /api/v1/triggers/reload` (STORY-073).
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
    /// `execute_direct()` qui attend sur le oneshot channel (STORY-096).
    /// `None` quand le HITL n'est pas configuré — `resume_task` logue un warning.
    pub pending_approvals: Option<Arc<PendingApprovals>>,
    /// Configuration des canaux de notification chargée depuis `apollia.toml`.
    ///
    /// Utilisée par `GET /api/v1/notifications/channels` et
    /// `POST /api/v1/notifications/test` (STORY-104).
    /// `None` si aucune section `[notifications]` n'est présente dans la config.
    pub notification_config: Option<NotificationConfig>,
    /// Handle vers le `PipelineEngine` actor (STORY-119).
    ///
    /// `None` quand aucun `[[pipelines]]` n'est déclaré dans `apollia.toml`.
    /// Les routes REST pipelines (STORY-120) retournent 503 quand `None`.
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
    use super::routes_audit::{get_audit_stats, list_audit};
    use super::routes_tools::{describe_tool, list_tools};
    use super::routes_dashboard::{
        dashboard_stream, get_dashboard, get_dashboard_partial, get_dashboard_state, get_htmx_js,
    };
    use super::routes_llm::llm_routes;
    use super::routes_notifications::{list_channels, notification_logs, test_channels};
    use super::routes_pipelines::{
        get_run, get_run_by_id, list_pipelines, list_runs, run_pipeline,
    };
    use super::routes_sse::stream_task;
    use super::routes_tasks::{cancel_task, get_task, list_tasks, resume_task, submit_task};
    use super::routes_triggers::{
        disable_trigger, enable_trigger, fire_trigger, get_trigger, get_trigger_logs,
        list_triggers, reload_triggers,
    };
    use super::routes_webhooks::handle_webhook;

    Router::new()
        // Redirect root to the dashboard (browser convenience)
        .route("/", get(|| async { Redirect::permanent("/dashboard") }))
        .route("/api/v1/health", get(health_handler))
        .route("/api/v1/shutdown", post(shutdown_handler::<B>))
        .route("/api/v1/tasks", get(list_tasks::<B>).post(submit_task::<B>))
        .route(
            "/api/v1/tasks/:id",
            get(get_task::<B>).delete(cancel_task::<B>),
        )
        .route("/api/v1/tasks/:id/stream", get(stream_task::<B>))
        .route("/api/v1/tasks/:id/resume", post(resume_task::<B>))
        // Tool routes (STORY-011 Tool Registry)
        .route("/api/v1/tools", get(list_tools::<B>))
        .route("/api/v1/tools/:name", get(describe_tool::<B>))
        // Audit trail routes (STORY-016 AuditTrail)
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
        .route("/webhooks/:id", post(handle_webhook::<B>))
        // Trigger routes (STORY-073 reload + STORY-074 CRUD)
        .route("/api/v1/triggers", get(list_triggers::<B>))
        .route("/api/v1/triggers/reload", post(reload_triggers::<B>))
        .route("/api/v1/triggers/:id", get(get_trigger::<B>))
        .route("/api/v1/triggers/:id/fire", post(fire_trigger::<B>))
        .route("/api/v1/triggers/:id/enable", post(enable_trigger::<B>))
        .route("/api/v1/triggers/:id/disable", post(disable_trigger::<B>))
        .route("/api/v1/triggers/:id/logs", get(get_trigger_logs::<B>))
        // Static assets — HTMX served from binary (STORY-077, Principle #2)
        .route("/static/htmx.min.js", get(get_htmx_js))
        // Dashboard routes (STORY-075/076/077) — /dashboard has no /api/v1 prefix (browser navigation)
        .route("/dashboard", get(get_dashboard))
        .route("/api/v1/dashboard/state", get(get_dashboard_state::<B>))
        .route(
            "/api/v1/dashboard/partials/:section",
            get(get_dashboard_partial::<B>),
        )
        .route("/api/v1/dashboard/stream", get(dashboard_stream::<B>))
        // Notification routes (STORY-104)
        .route("/api/v1/notifications/channels", get(list_channels::<B>))
        .route("/api/v1/notifications/test", post(test_channels::<B>))
        .route("/api/v1/notifications/logs", get(notification_logs::<B>))
        .merge(llm_routes::<B>())
        // Pipeline routes (STORY-120 + STORY-121)
        .route("/api/v1/pipelines", get(list_pipelines::<B>))
        .route("/api/v1/pipelines/:id/run", post(run_pipeline::<B>))
        .route("/api/v1/pipelines/:id/runs", get(list_runs::<B>))
        .route("/api/v1/pipelines/:id/runs/:run_id", get(get_run::<B>))
        .route("/api/v1/runs/:run_id", get(get_run_by_id::<B>))
        .with_state(state)
}

impl APIServer {
    /// Create a new APIServer with the given config and application state.
    pub fn new<B: ExecutionBackend + Clone + From<DynBackend>>(config: APIServerConfig, state: AppState<B>) -> Self {
        let router = build_router(state);
        Self { config, router }
    }

    /// Build the router from a state, for use in unit tests without starting a listener.
    #[cfg(test)]
    pub fn build_router_for_test<B: ExecutionBackend + Clone + From<DynBackend>>(state: AppState<B>) -> Router {
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
        fn from(_: DynBackend) -> Self { MockBackend }
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
