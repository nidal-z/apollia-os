//! LLM routes — `GET /api/v1/llm/status`, `POST /api/v1/llm/ping`, `POST /api/v1/llm/chat`.
//!
//! These handlers expose the `LlmRouter` state through the HTTP API so the CLI
//! can diagnose backends without starting a full agent.

use std::sync::Arc;
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
// Cost stats types & handler
// ─────────────────────────────────────────────

/// Query parameters for `GET /api/v1/llm/costs`.
#[derive(Debug, Deserialize)]
pub struct CostsQuery {
    /// Number of days to aggregate costs for (default: 7).
    #[serde(default = "default_cost_days")]
    pub days: u32,
}

/// Default number of days for cost aggregation.
fn default_cost_days() -> u32 {
    7
}

/// A single backend/model cost summary row.
#[derive(Debug, Serialize)]
pub struct CostSummaryRow {
    /// Backend name.
    pub backend: String,
    /// Model identifier.
    pub model: String,
    /// Number of LLM calls.
    pub call_count: u64,
    /// Total tokens (prompt + completion).
    pub total_tokens: u64,
    /// Estimated total cost in USD.
    pub total_cost_usd: f64,
}

/// Response body for `GET /api/v1/llm/costs`.
#[derive(Debug, Serialize)]
pub struct CostsResponse {
    /// Per-backend/model cost breakdown.
    pub rows: Vec<CostSummaryRow>,
    /// Number of days aggregated.
    pub days: u32,
}

/// Handler for `GET /api/v1/llm/costs`.
///
/// Aggregates LLM call costs and token usage from `llm_calls.db` over the
/// requested time window. Returns 503 if no `LlmCallRepository` is available.
pub async fn get_llm_costs<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    axum::extract::Query(query): axum::extract::Query<CostsQuery>,
) -> Result<Json<CostsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let repo = state.llm_call_repository.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "no LLM call repository configured"})),
        )
    })?;

    let days = query.days;
    let since = chrono::Utc::now() - chrono::Duration::days(i64::from(days));
    let since_str = since.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let repo = Arc::clone(repo);
    let summaries = tokio::task::spawn_blocking(move || {
        let guard = repo
            .lock()
            .map_err(|e| format!("failed to lock repository: {e}"))?;
        guard
            .costs_by_backend_model_since(&since_str)
            .map_err(|e| format!("query failed: {e}"))
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("join error: {e}")})),
        )
    })?
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )
    })?;

    let rows = summaries
        .into_iter()
        .map(|s| CostSummaryRow {
            backend: s.backend,
            model: s.model,
            call_count: s.call_count,
            total_tokens: s.total_tokens,
            total_cost_usd: s.total_cost_usd,
        })
        .collect();

    Ok(Json(CostsResponse { rows, days }))
}

// ─────────────────────────────────────────────
// Daily costs types & handler
// ─────────────────────────────────────────────

/// A single day+backend cost entry for the daily chart.
#[derive(Debug, Serialize)]
pub struct DailyCostEntry {
    /// Date au format `YYYY-MM-DD`.
    pub date: String,
    /// Nom du backend.
    pub backend: String,
    /// Coût total estimé en USD pour ce jour.
    pub cost_usd: f64,
}

/// Response body for `GET /api/v1/llm/costs/daily`.
#[derive(Debug, Serialize)]
pub struct DailyCostsResponse {
    /// Per-day/backend cost entries.
    pub entries: Vec<DailyCostEntry>,
    /// Number of days requested.
    pub days: u32,
}

/// Handler for `GET /api/v1/llm/costs/daily`.
///
/// Returns LLM costs broken down by day and backend for the requested
/// time window. Used by the Observability LLM Costs chart.
pub async fn get_llm_daily_costs<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    axum::extract::Query(query): axum::extract::Query<CostsQuery>,
) -> Result<Json<DailyCostsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let repo = state.llm_call_repository.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "no LLM call repository configured"})),
        )
    })?;

    let days = query.days;
    let since = chrono::Utc::now() - chrono::Duration::days(i64::from(days));
    let since_str = since.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let repo = Arc::clone(repo);
    let summaries = tokio::task::spawn_blocking(move || {
        let guard = repo
            .lock()
            .map_err(|e| format!("failed to lock repository: {e}"))?;
        guard
            .costs_by_day_backend_since(&since_str)
            .map_err(|e| format!("query failed: {e}"))
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("join error: {e}")})),
        )
    })?
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )
    })?;

    let entries = summaries
        .into_iter()
        .map(|s| DailyCostEntry {
            date: s.date,
            backend: s.backend,
            cost_usd: s.cost_usd,
        })
        .collect();

    Ok(Json(DailyCostsResponse { entries, days }))
}

// ─────────────────────────────────────────────
// Sub-router builder
// ─────────────────────────────────────────────

/// Build the axum sub-router for LLM diagnostic endpoints.
///
/// Routes registered:
/// - `GET  /api/v1/llm/status`      — list all backends
/// - `POST /api/v1/llm/ping`        — measure backend latency
/// - `POST /api/v1/llm/chat`        — send a one-shot prompt
/// - `GET  /api/v1/llm/costs`       — aggregate cost/token stats
/// - `GET  /api/v1/llm/costs/daily`  — daily cost breakdown per backend
pub fn llm_routes<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/llm/status", get(get_llm_status::<B>))
        .route("/api/v1/llm/ping", post(ping_llm_backend::<B>))
        .route("/api/v1/llm/chat", post(llm_chat::<B>))
        .route("/api/v1/llm/costs", get(get_llm_costs::<B>))
        .route("/api/v1/llm/costs/daily", get(get_llm_daily_costs::<B>))
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
