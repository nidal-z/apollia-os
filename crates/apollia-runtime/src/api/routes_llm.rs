//! LLM routes — `GET /api/v1/llm/status`, `POST /api/v1/llm/ping`, `POST /api/v1/llm/chat`.
//!
//! These handlers expose the `LlmRouter` state through the HTTP API so the CLI
//! can diagnose backends without starting a full agent.

use std::time::Instant;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use apollia_llm::router::ObservabilityConfig;
use apollia_llm::{BackendInfo, ChatMessage, CompletionRequest};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

// ─────────────────────────────────────────────
// Response / Request types
// ─────────────────────────────────────────────

/// Response body for `GET /api/v1/llm/status`.
#[derive(Debug, Serialize)]
pub struct LlmStatusResponse {
    /// All configured LLM backends with their current availability state.
    pub backends: Vec<BackendInfo>,
}

/// Request body for `POST /api/v1/llm/ping`.
#[derive(Debug, Deserialize)]
pub struct PingRequest {
    /// Backend name to ping; uses the router default if `null` or omitted.
    pub backend: Option<String>,
}

/// Response body for `POST /api/v1/llm/ping`.
#[derive(Debug, Serialize)]
pub struct PingResponse {
    /// Name of the backend that was pinged.
    pub backend: String,
    /// `true` if the backend responded successfully.
    pub available: bool,
    /// Round-trip latency in milliseconds (only set when `available` is `true`).
    pub latency_ms: Option<u64>,
    /// Human-readable error message when `available` is `false`.
    pub error: Option<String>,
}

/// Token usage included in chat responses.
#[derive(Debug, Serialize)]
pub struct TokenUsageResponse {
    /// Number of tokens in the prompt.
    pub prompt_tokens: u32,
    /// Number of tokens in the completion.
    pub completion_tokens: u32,
    /// Estimated cost in USD (cloud backends only; `None` for local inference).
    pub cost_usd: Option<f64>,
}

/// Request body for `POST /api/v1/llm/chat`.
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    /// User prompt text to send to the LLM.
    pub prompt: String,
    /// Backend to use; falls back to the router default if omitted.
    pub backend: Option<String>,
}

/// Response body for `POST /api/v1/llm/chat`.
#[derive(Debug, Serialize)]
pub struct ChatResponse {
    /// LLM-generated response text.
    pub content: String,
    /// Token usage statistics for this call.
    pub usage: TokenUsageResponse,
    /// Total round-trip latency in milliseconds.
    pub latency_ms: u64,
}

// ─────────────────────────────────────────────
// Handlers
// ─────────────────────────────────────────────

/// Handler for `GET /api/v1/llm/status`.
///
/// Returns the list of all configured backends with their availability state.
/// Returns `{"backends": []}` if no `LlmRouter` is configured in `AppState`.
pub async fn get_llm_status<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> (StatusCode, Json<LlmStatusResponse>) {
    let backends = state
        .llm_router
        .as_ref()
        .map(|router| router.list())
        .unwrap_or_default();

    (StatusCode::OK, Json(LlmStatusResponse { backends }))
}

/// Handler for `POST /api/v1/llm/ping`.
///
/// Sends a trivial completion request (`"ping"`) to the specified backend (or the
/// router default) and measures the round-trip latency.
///
/// Returns `503 Service Unavailable` with `available: false` when:
/// - No `LlmRouter` is configured, or
/// - The backend call fails (key missing, network error, etc.).
pub async fn ping_llm_backend<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(req): Json<PingRequest>,
) -> (StatusCode, Json<PingResponse>) {
    let backend_name = req.backend.as_deref();

    let Some(router) = state.llm_router.as_ref() else {
        let name = backend_name.unwrap_or("default").to_owned();
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(PingResponse {
                backend: name,
                available: false,
                latency_ms: None,
                error: Some("no LLM router configured".into()),
            }),
        );
    };

    let resolved_name = backend_name
        .unwrap_or_else(|| router.default_name())
        .to_owned();

    let ping_req = CompletionRequest {
        messages: vec![ChatMessage::user("ping")],
        ..Default::default()
    };
    let obs = ObservabilityConfig::default();
    let started = Instant::now();

    let result = router
        .complete_with_observability(backend_name, ping_req, Some(&state.event_sender), &obs)
        .await;

    let latency_ms = started.elapsed().as_millis() as u64;

    match result {
        Ok(_) => (
            StatusCode::OK,
            Json(PingResponse {
                backend: resolved_name,
                available: true,
                latency_ms: Some(latency_ms),
                error: None,
            }),
        ),
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(PingResponse {
                backend: resolved_name,
                available: false,
                latency_ms: None,
                error: Some(e.to_string()),
            }),
        ),
    }
}

/// Handler for `POST /api/v1/llm/chat`.
///
/// Builds a single-turn `CompletionRequest` from the prompt and dispatches it
/// via `LlmRouter::complete_with_observability`. Observability events
/// (`LlmCallCompleted`) are emitted on the `EventBus` automatically.
///
/// Returns `503 Service Unavailable` if no router is configured or the call fails.
pub async fn llm_chat<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(req): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, (StatusCode, Json<serde_json::Value>)> {
    let router = state.llm_router.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "no LLM router configured"})),
        )
    })?;

    let backend_name = req.backend.as_deref();
    let completion_req = CompletionRequest {
        messages: vec![ChatMessage::user(req.prompt)],
        ..Default::default()
    };
    let obs = ObservabilityConfig::default();

    let response = router
        .complete_with_observability(
            backend_name,
            completion_req,
            Some(&state.event_sender),
            &obs,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        })?;

    Ok(Json(ChatResponse {
        content: response.content,
        usage: TokenUsageResponse {
            prompt_tokens: response.usage.prompt_tokens,
            completion_tokens: response.usage.completion_tokens,
            cost_usd: response.usage.cost_usd,
        },
        latency_ms: response.latency_ms,
    }))
}

// ─────────────────────────────────────────────
// Sub-router builder
// ─────────────────────────────────────────────

/// Build the axum sub-router for LLM diagnostic endpoints.
///
/// Routes registered:
/// - `GET  /api/v1/llm/status` — list all backends
/// - `POST /api/v1/llm/ping`   — measure backend latency
/// - `POST /api/v1/llm/chat`   — send a one-shot prompt
pub fn llm_routes<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/llm/status", get(get_llm_status::<B>))
        .route("/api/v1/llm/ping", post(ping_llm_backend::<B>))
        .route("/api/v1/llm/chat", post(llm_chat::<B>))
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinator::ExecutionBackend;
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use crate::router::TaskRouterHandle;
    use apollia_core::{AIPResult, AIPTask, TaskStatus};
    use apollia_llm::LlmRouter;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use tower::ServiceExt;

    /// Minimal `ExecutionBackend` for tests — never actually executed.
    #[derive(Clone)]
    struct MockBackend;

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

    /// Build a minimal `AppState` with `llm_router = None`.
    fn test_app_state_no_llm() -> AppState<MockBackend> {
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
        }
    }

    // GIVEN an AppState with llm_router = None
    // WHEN GET /api/v1/llm/status
    // THEN 200 {"backends": []}
    #[tokio::test]
    async fn test_get_llm_status_no_router_returns_empty_list() {
        // GIVEN
        let app_router = llm_routes::<MockBackend>().with_state(test_app_state_no_llm());

        // WHEN
        let req = Request::builder()
            .uri("/api/v1/llm/status")
            .body(Body::empty())
            .unwrap();
        let resp = app_router.oneshot(req).await.unwrap();

        // THEN
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["backends"], serde_json::json!([]));
    }

    // GIVEN an AppState with llm_router = None
    // WHEN POST /api/v1/llm/ping {"backend": null}
    // THEN 503 with available = false
    #[tokio::test]
    async fn test_ping_no_router_returns_503_unavailable() {
        // GIVEN
        let app_router = llm_routes::<MockBackend>().with_state(test_app_state_no_llm());

        // WHEN
        let body_json = serde_json::json!({"backend": null});
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/llm/ping")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body_json).unwrap()))
            .unwrap();
        let resp = app_router.oneshot(req).await.unwrap();

        // THEN
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        let bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["available"], false);
        assert!(json["error"].is_string());
    }

    // GIVEN an AppState with llm_router = None
    // WHEN POST /api/v1/llm/chat {"prompt": "hello", "backend": null}
    // THEN 503 {"error": "no LLM router configured"}
    #[tokio::test]
    async fn test_chat_no_router_returns_503() {
        // GIVEN
        let app_router = llm_routes::<MockBackend>().with_state(test_app_state_no_llm());

        // WHEN
        let body_json = serde_json::json!({"prompt": "hello", "backend": null});
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/llm/chat")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body_json).unwrap()))
            .unwrap();
        let resp = app_router.oneshot(req).await.unwrap();

        // THEN
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        let bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["error"].as_str().is_some());
    }

    // GIVEN an AppState with llm_router = Some(LlmRouter::empty())
    // WHEN GET /api/v1/llm/status
    // THEN 200 with a "backends" array (empty for LlmRouter::empty())
    #[tokio::test]
    async fn test_get_llm_status_with_router_returns_backends_field() {
        // GIVEN
        let mut state = test_app_state_no_llm();
        state.llm_router = Some(Arc::new(LlmRouter::empty()));
        let app_router = llm_routes::<MockBackend>().with_state(state);

        // WHEN
        let req = Request::builder()
            .uri("/api/v1/llm/status")
            .body(Body::empty())
            .unwrap();
        let resp = app_router.oneshot(req).await.unwrap();

        // THEN
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(
            json["backends"].is_array(),
            "response must contain a 'backends' array"
        );
    }
}
