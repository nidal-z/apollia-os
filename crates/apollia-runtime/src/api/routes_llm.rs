//! LLM routes, status/ping/chat diagnostics + CRUD backends.
//!
//! - `GET  /api/v1/llm/status`                    , list backends
//! - `POST /api/v1/llm/ping`                      , measure backend latency
//! - `POST /api/v1/llm/chat`                      , one-shot prompt
//! - `POST /api/v1/llm/complete`                  , multi-turn completion
//! - `GET  /api/v1/llm/costs`                     , aggregate cost/token stats
//! - `GET  /api/v1/llm/costs/daily`               , daily cost breakdown
//! - `GET    /api/v1/llm/backends`                , list configured backends
//! - `GET    /api/v1/llm/backends/:name`          , get one backend
//! - `POST   /api/v1/llm/backends`                , create backend
//! - `PUT    /api/v1/llm/backends/:name`          , update backend
//! - `DELETE /api/v1/llm/backends/:name`          , delete backend
//! - `POST   /api/v1/llm/backends/:name/set-default`, mark as default

use std::sync::Arc;
use std::time::Instant;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use apollia_core::{LlmBackendConfig, LlmBackendError, LlmBackendRepository, LlmProvider};
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
    /// Accumulated session cost in USD. `None` when no router is configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// Hybrid routing cost ceiling in USD. `None` when hybrid routing is not
    /// configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ceiling_usd: Option<f64>,
    /// Whether the cost ceiling has been reached. Always `false` when hybrid
    /// routing is not configured.
    pub ceiling_reached: bool,
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

/// Response body for `POST /api/v1/llm/chat` and `POST /api/v1/llm/complete`.
#[derive(Debug, Serialize)]
pub struct ChatResponse {
    /// LLM-generated response text.
    pub content: String,
    /// Token usage statistics for this call.
    pub usage: TokenUsageResponse,
    /// Total round-trip latency in milliseconds.
    pub latency_ms: u64,
}

/// A single message in a multi-turn conversation request.
///
/// `role` accepts `"system"`, `"user"`, `"assistant"`, or `"tool"`.
#[derive(Debug, Deserialize)]
pub struct MessageDto {
    /// Role of the message sender.
    pub role: String,
    /// Text content of the message.
    pub content: String,
}

/// Request body for `POST /api/v1/llm/complete`.
#[derive(Debug, Deserialize)]
pub struct CompleteRequest {
    /// Ordered list of messages forming the conversation history.
    pub messages: Vec<MessageDto>,
    /// Backend to use; falls back to the router default if omitted.
    pub backend: Option<String>,
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
    let snapshot = state.llm_router.read().await.clone();
    let backends = snapshot
        .as_ref()
        .map(|router| router.list())
        .unwrap_or_default();
    // Surface the hybrid cost ceiling and the accumulated session cost so the
    // CLI can show budget headroom. `ceiling_usd` is absent without hybrid
    // routing; `cost_usd` is absent only when no router is configured.
    let cost_usd = snapshot.as_ref().map(|router| router.session_cost_usd());
    let ceiling_usd = snapshot.as_ref().and_then(|router| router.cost_ceiling_usd());
    let ceiling_reached = snapshot
        .as_ref()
        .is_some_and(|router| router.is_ceiling_reached());

    (
        StatusCode::OK,
        Json(LlmStatusResponse {
            backends,
            cost_usd,
            ceiling_usd,
            ceiling_reached,
        }),
    )
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

    let snapshot = state.llm_router.read().await.clone();
    let Some(router) = snapshot else {
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
    let router = state.llm_router.read().await.clone().ok_or_else(|| {
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

/// Handler for `POST /api/v1/llm/complete`.
///
/// Accepts a multi-turn conversation history and dispatches it through
/// `LlmRouter::complete_with_observability`. Each `MessageDto` is mapped
/// to a `ChatMessage` based on its `role` field:
///
/// - `"system"`    → `ChatMessage::system`
/// - `"user"`      → `ChatMessage::user`
/// - `"assistant"` → `ChatMessage::assistant`
/// - `"tool"`      → `ChatMessage::tool_result` (empty call id)
///
/// Returns `400 Bad Request` when an unknown role is encountered.
/// Returns `503 Service Unavailable` when no router is configured or the call fails.
pub async fn llm_complete<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(req): Json<CompleteRequest>,
) -> Result<Json<ChatResponse>, (StatusCode, Json<serde_json::Value>)> {
    let router = state.llm_router.read().await.clone().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "no LLM router configured"})),
        )
    })?;

    let mut messages = Vec::with_capacity(req.messages.len());
    for dto in req.messages {
        let msg = match dto.role.as_str() {
            "system" => ChatMessage::system(dto.content),
            "user" => ChatMessage::user(dto.content),
            "assistant" => ChatMessage::assistant(dto.content),
            "tool" => ChatMessage::tool_result("", &dto.content),
            other => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": format!(
                            "unknown role '{}'; expected system|user|assistant|tool",
                            other
                        )
                    })),
                ));
            }
        };
        messages.push(msg);
    }

    let completion_req = CompletionRequest {
        messages,
        ..Default::default()
    };
    let obs = ObservabilityConfig::default();

    let response = router
        .complete_with_observability(
            req.backend.as_deref(),
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
    /// Backend name.
    pub backend: String,
    /// Total estimated cost in USD for this day.
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
// CRUD types
// ─────────────────────────────────────────────

/// Request body for `POST /api/v1/llm/backends`.
#[derive(Debug, Deserialize)]
pub struct CreateLlmBackendRequest {
    /// Unique name, pattern `^[a-z0-9_-]+$`.
    pub name: String,
    /// Provider identifier: `"llama-cpp"`, `"openai"`, `"mistral"`, `"anthropic"`, `"ollama"`.
    pub provider: String,
    /// Model identifier (e.g. `"gpt-4o"`, `"mistral-small-latest"`).
    pub model: String,
    /// Provider-specific configuration object (must be a JSON object, not null or primitive).
    pub config_json: serde_json::Value,
    /// Whether this backend is active (default: `true`).
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Mark this backend as the default (default: `false`).
    #[serde(default)]
    pub is_default: bool,
}

/// Request body for `PUT /api/v1/llm/backends/:name`.
#[derive(Debug, Deserialize)]
pub struct UpdateLlmBackendRequest {
    /// Provider identifier.
    pub provider: String,
    /// Model identifier.
    pub model: String,
    /// Provider-specific configuration object.
    pub config_json: serde_json::Value,
    /// Whether this backend is active.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Whether this backend is the default.
    #[serde(default)]
    pub is_default: bool,
}

/// Response body for a single backend.
#[derive(Debug, Serialize)]
pub struct LlmBackendResponse {
    /// Unique backend name.
    pub name: String,
    /// Provider identifier string.
    pub provider: String,
    /// Model identifier.
    pub model: String,
    /// Provider-specific configuration.
    pub config_json: serde_json::Value,
    /// Whether this backend is enabled.
    pub enabled: bool,
    /// Whether this is the default backend.
    pub is_default: bool,
}

/// Response body for `GET /api/v1/llm/backends`.
#[derive(Debug, Serialize)]
pub struct LlmBackendsListResponse {
    /// All configured backends.
    pub backends: Vec<LlmBackendResponse>,
}

/// Response body for `DELETE /api/v1/llm/backends/:name`.
#[derive(Debug, Serialize)]
pub struct DeleteBackendResponse {
    /// Name of the deleted backend.
    pub deleted: String,
}

/// Response body for `POST /api/v1/llm/backends/:name/set-default`.
#[derive(Debug, Serialize)]
pub struct SetDefaultResponse {
    /// The backend that is now the default.
    pub default: String,
}

/// Response body for `POST /api/v1/llm/reload`.
///
/// Carries the list of backends that are live in the freshly-swapped router,
/// so callers can confirm at a glance what is now available without a
/// follow-up `GET /api/v1/llm/status` call.
#[derive(Debug, Serialize)]
pub struct ReloadRouterResponse {
    /// Backends now reachable via the active router.
    pub backends: Vec<apollia_llm::BackendInfo>,
    /// Default backend name reported by the router (empty when no backends).
    pub default: String,
}

/// CRUD error response body.
#[derive(Debug, Serialize)]
pub struct BackendErrorResponse {
    /// Human-readable error message.
    pub error: String,
}

fn default_true() -> bool {
    true
}

/// Convert an [`LlmBackendConfig`] to [`LlmBackendResponse`].
fn config_to_response(cfg: LlmBackendConfig) -> LlmBackendResponse {
    LlmBackendResponse {
        name: cfg.name,
        provider: cfg.provider.to_string(),
        model: cfg.model,
        config_json: cfg.config_json,
        enabled: cfg.enabled,
        is_default: cfg.is_default,
    }
}

/// Map [`LlmBackendError`] to an HTTP status + error body.
fn map_backend_error(err: LlmBackendError) -> (StatusCode, Json<BackendErrorResponse>) {
    let (status, msg) = match &err {
        LlmBackendError::NotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        LlmBackendError::DefaultAlreadyExists(_) | LlmBackendError::CannotDeleteDefault => {
            (StatusCode::CONFLICT, err.to_string())
        }
        LlmBackendError::InvalidName(_) | LlmBackendError::Serialization(_) => {
            (StatusCode::UNPROCESSABLE_ENTITY, err.to_string())
        }
        LlmBackendError::Db(_) | LlmBackendError::Io(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
        }
    };
    (status, Json(BackendErrorResponse { error: msg }))
}

/// Sync backends from DB to `apollia.toml` after a mutation (best-effort, never fails the request).
fn sync_toml_after_mutation<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    guard: &LlmBackendRepository,
) {
    if let Some(path) = &state.config_path {
        if let Err(e) = guard.sync_to_toml(path) {
            tracing::warn!(error = %e, "failed to sync llm backends to apollia.toml");
        }
    }
}

/// Extract the `llm_backend_repo` from state, returning 503 if absent.
fn require_backend_repo<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Result<&Arc<std::sync::Mutex<LlmBackendRepository>>, (StatusCode, Json<BackendErrorResponse>)>
{
    state.llm_backend_repo.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(BackendErrorResponse {
                error: "LLM backend repository not available".into(),
            }),
        )
    })
}

/// Validate that `config_json` is a JSON object.
fn validate_config_json(
    value: &serde_json::Value,
) -> Result<(), (StatusCode, Json<BackendErrorResponse>)> {
    if !value.is_object() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(BackendErrorResponse {
                error: "config_json must be a JSON object".into(),
            }),
        ));
    }
    Ok(())
}

// ─────────────────────────────────────────────
// CRUD Handlers
// ─────────────────────────────────────────────

/// Handler for `GET /api/v1/llm/backends`.
///
/// Lists all configured backends from `system.db`.
/// Returns 503 if the repository is not available.
pub async fn list_llm_backends<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<LlmBackendsListResponse>, (StatusCode, Json<BackendErrorResponse>)> {
    let repo = require_backend_repo(&state)?;
    let guard = repo.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(BackendErrorResponse {
                error: "repository lock poisoned".into(),
            }),
        )
    })?;
    let configs = guard.list().map_err(map_backend_error)?;
    let backends = configs.into_iter().map(config_to_response).collect();
    Ok(Json(LlmBackendsListResponse { backends }))
}

/// Handler for `GET /api/v1/llm/backends/:name`.
///
/// Returns the backend with the given name or 404 if not found.
pub async fn get_llm_backend<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
) -> Result<Json<LlmBackendResponse>, (StatusCode, Json<BackendErrorResponse>)> {
    let repo = require_backend_repo(&state)?;
    let guard = repo.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(BackendErrorResponse {
                error: "repository lock poisoned".into(),
            }),
        )
    })?;
    let cfg = guard
        .find_by_name(&name)
        .map_err(map_backend_error)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(BackendErrorResponse {
                    error: format!("backend '{name}' not found"),
                }),
            )
        })?;
    Ok(Json(config_to_response(cfg)))
}

/// Handler for `POST /api/v1/llm/backends`.
///
/// Creates a new backend. Returns 201 on success, 409 if a default already exists
/// when `is_default` is set, 422 on validation error.
pub async fn create_llm_backend<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<CreateLlmBackendRequest>,
) -> Result<(StatusCode, Json<LlmBackendResponse>), (StatusCode, Json<BackendErrorResponse>)> {
    validate_config_json(&body.config_json)?;

    let provider = LlmProvider::try_from(body.provider.as_str()).map_err(|_| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(BackendErrorResponse {
                error: format!(
                    "unknown provider '{}'; valid values: llama-cpp, openai, mistral, anthropic, ollama",
                    body.provider
                ),
            }),
        )
    })?;

    let cfg = LlmBackendConfig {
        name: body.name.clone(),
        provider,
        model: body.model,
        config_json: body.config_json,
        enabled: body.enabled,
        is_default: body.is_default,
    };

    let repo = require_backend_repo(&state)?;
    let guard = repo.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(BackendErrorResponse {
                error: "repository lock poisoned".into(),
            }),
        )
    })?;
    guard.save(&cfg).map_err(map_backend_error)?;
    sync_toml_after_mutation(&state, &guard);
    let created = guard
        .find_by_name(&cfg.name)
        .map_err(map_backend_error)?
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(BackendErrorResponse {
                    error: "backend saved but not found".into(),
                }),
            )
        })?;
    Ok((StatusCode::CREATED, Json(config_to_response(created))))
}

/// Handler for `PUT /api/v1/llm/backends/:name`.
///
/// Replaces an existing backend configuration. Returns 404 if the backend does not exist.
pub async fn update_llm_backend<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
    Json(body): Json<UpdateLlmBackendRequest>,
) -> Result<Json<LlmBackendResponse>, (StatusCode, Json<BackendErrorResponse>)> {
    validate_config_json(&body.config_json)?;

    let provider = LlmProvider::try_from(body.provider.as_str()).map_err(|_| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(BackendErrorResponse {
                error: format!(
                    "unknown provider '{}'; valid values: llama-cpp, openai, mistral, anthropic, ollama",
                    body.provider
                ),
            }),
        )
    })?;

    let repo = require_backend_repo(&state)?;
    let guard = repo.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(BackendErrorResponse {
                error: "repository lock poisoned".into(),
            }),
        )
    })?;

    // Verify existence before updating.
    guard
        .find_by_name(&name)
        .map_err(map_backend_error)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(BackendErrorResponse {
                    error: format!("backend '{name}' not found"),
                }),
            )
        })?;

    let cfg = LlmBackendConfig {
        name: name.clone(),
        provider,
        model: body.model,
        config_json: body.config_json,
        enabled: body.enabled,
        is_default: body.is_default,
    };
    guard.save(&cfg).map_err(map_backend_error)?;
    sync_toml_after_mutation(&state, &guard);
    let updated = guard
        .find_by_name(&name)
        .map_err(map_backend_error)?
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(BackendErrorResponse {
                    error: "backend updated but not found".into(),
                }),
            )
        })?;
    Ok(Json(config_to_response(updated)))
}

/// Handler for `DELETE /api/v1/llm/backends/:name`.
///
/// Deletes a backend. Returns 409 if the backend is the current default, 404 if not found.
pub async fn delete_llm_backend<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
) -> Result<Json<DeleteBackendResponse>, (StatusCode, Json<BackendErrorResponse>)> {
    let repo = require_backend_repo(&state)?;
    let guard = repo.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(BackendErrorResponse {
                error: "repository lock poisoned".into(),
            }),
        )
    })?;
    guard.delete(&name).map_err(map_backend_error)?;
    sync_toml_after_mutation(&state, &guard);
    Ok(Json(DeleteBackendResponse { deleted: name }))
}

/// Handler for `POST /api/v1/llm/backends/:name/set-default`.
///
/// Marks the given backend as the default. Returns 404 if not found.
pub async fn set_default_llm_backend<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
) -> Result<Json<SetDefaultResponse>, (StatusCode, Json<BackendErrorResponse>)> {
    let repo = require_backend_repo(&state)?;
    let guard = repo.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(BackendErrorResponse {
                error: "repository lock poisoned".into(),
            }),
        )
    })?;
    guard.set_default(&name).map_err(map_backend_error)?;
    sync_toml_after_mutation(&state, &guard);
    Ok(Json(SetDefaultResponse { default: name }))
}

/// Handler for `POST /api/v1/llm/reload`.
///
/// Rebuilds the active `LlmRouter` from `system.db` and swaps it into the
/// shared cell exposed by [`AppState::llm_router`], without restarting the
/// daemon. The new router becomes visible to every subsequent reader
/// (`ping`, `chat`, `complete`, `status`); in-flight requests that already
/// hold a snapshot of the previous router finish against the old router and
/// are not interrupted.
///
/// The route also forwards the freshly-built router to the
/// [`ChatSessionManager`] via its `ReloadLlm` actor command so live chat
/// sessions pick up the new model on their next turn.
///
/// Returns:
/// - `200 OK` with the list of backends now active.
/// - `503 Service Unavailable` when `llm_backend_repo` is `None` (the runtime
///   was started without `system.db`, typically a unit test).
/// - `500 Internal Server Error` when the repository is reachable but
///   building the router fails (invalid config_json, model file missing for
///   a local backend, etc.).
pub async fn reload_llm_router<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<ReloadRouterResponse>, (StatusCode, Json<BackendErrorResponse>)> {
    let repo = require_backend_repo(&state)?;

    // Snapshot the persisted backends under the synchronous mutex and drop
    // the guard *before* awaiting. Holding a `std::sync::MutexGuard` across
    // an await makes the future `!Send` and rejects the axum handler trait.
    // The async work (instantiating each backend, possibly loading a GGUF
    // model) then runs lock-free.
    let (all_configs, default_name) = {
        let guard = repo.lock().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(BackendErrorResponse {
                    error: "repository lock poisoned".into(),
                }),
            )
        })?;
        let all = guard.list().map_err(map_backend_error)?;
        let default = guard
            .find_default()
            .map_err(map_backend_error)?
            .map(|cfg| cfg.name)
            .ok_or_else(|| {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(BackendErrorResponse {
                        error: "no default LLM backend configured (run `apollia-os llm backends set-default <name>`)".into(),
                    }),
                )
            })?;
        (all, default)
    };

    // Re-inject the `LlamaCpp -> runner` override so the local backend stays
    // wired through the sidecar; otherwise the rebuild would drop it and the
    // runner would become unreachable from agents and chat. Shares the same
    // factory used by the supervisor at boot.
    let factory =
        crate::runner_supervisor::runner_llm_override(state.runner_proxy.clone());
    let new_router = apollia_llm::LlmRouter::from_backend_configs_with_override(
        all_configs,
        default_name,
        factory,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(BackendErrorResponse {
                error: format!("rebuild failed: {e}"),
            }),
        )
    })?;

    let backends = new_router.list();
    let default = new_router.default_name().to_owned();
    let arc_router = Arc::new(new_router);

    {
        let mut cell = state.llm_router.write().await;
        *cell = Some(arc_router.clone());
    }

    if let Some(chat) = state.chat_manager.as_ref() {
        chat.reload_llm(Some(arc_router.clone())).await;
    }

    tracing::info!(
        backend_count = backends.len(),
        default = %default,
        "LLM router reloaded"
    );

    Ok(Json(ReloadRouterResponse { backends, default }))
}

// ─────────────────────────────────────────────
// Sub-router builder
// ─────────────────────────────────────────────

/// Build the axum sub-router for LLM diagnostic and CRUD endpoints.
///
/// Routes registered:
/// - `GET  /api/v1/llm/status`                         , list all backends
/// - `POST /api/v1/llm/ping`                           , measure backend latency
/// - `POST /api/v1/llm/chat`                           , send a one-shot prompt
/// - `GET  /api/v1/llm/costs`                          , aggregate cost/token stats
/// - `GET  /api/v1/llm/costs/daily`                    , daily cost breakdown per backend
/// - `GET    /api/v1/llm/backends`                     , list configured backends
/// - `GET    /api/v1/llm/backends/:name`               , get one backend
/// - `POST   /api/v1/llm/backends`                     , create backend
/// - `PUT    /api/v1/llm/backends/:name`               , update backend
/// - `DELETE /api/v1/llm/backends/:name`               , delete backend
/// - `POST   /api/v1/llm/backends/:name/set-default`   , mark as default
pub fn llm_routes<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/llm/status", get(get_llm_status::<B>))
        .route("/api/v1/llm/ping", post(ping_llm_backend::<B>))
        .route("/api/v1/llm/chat", post(llm_chat::<B>))
        .route("/api/v1/llm/complete", post(llm_complete::<B>))
        .route("/api/v1/llm/costs", get(get_llm_costs::<B>))
        .route("/api/v1/llm/costs/daily", get(get_llm_daily_costs::<B>))
        .route(
            "/api/v1/llm/backends",
            get(list_llm_backends::<B>).post(create_llm_backend::<B>),
        )
        .route(
            "/api/v1/llm/backends/:name",
            get(get_llm_backend::<B>)
                .put(update_llm_backend::<B>)
                .delete(delete_llm_backend::<B>),
        )
        .route(
            "/api/v1/llm/backends/:name/set-default",
            post(set_default_llm_backend::<B>),
        )
        .route("/api/v1/llm/reload", post(reload_llm_router::<B>))
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

    /// Minimal `ExecutionBackend` for tests, never actually executed.
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
            llm_router: crate::api::server::empty_shared_llm_router(),
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

    #[test]
    fn test_llm_status_response_includes_cost_and_ceiling() {
        // GIVEN a status response with a session cost and a configured ceiling
        let resp = LlmStatusResponse {
            backends: vec![],
            cost_usd: Some(0.23),
            ceiling_usd: Some(2.0),
            ceiling_reached: false,
        };

        // WHEN serializing
        let json = serde_json::to_value(&resp).expect("serialize");

        // THEN all three fields are present
        assert_eq!(json["cost_usd"], 0.23);
        assert_eq!(json["ceiling_usd"], 2.0);
        assert_eq!(json["ceiling_reached"], false);
    }

    #[test]
    fn test_llm_status_response_omits_ceiling_when_absent() {
        // GIVEN a status response with no hybrid ceiling
        let resp = LlmStatusResponse {
            backends: vec![],
            cost_usd: Some(0.1),
            ceiling_usd: None,
            ceiling_reached: false,
        };

        // WHEN serializing
        let json = serde_json::to_value(&resp).expect("serialize");

        // THEN ceiling_usd is omitted but ceiling_reached stays present and false
        assert!(json.get("ceiling_usd").is_none());
        assert_eq!(json["ceiling_reached"], false);
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

    // GIVEN an AppState with llm_router = None
    // WHEN POST /api/v1/llm/complete {"messages": [{"role": "user", "content": "bonjour"}]}
    // THEN 503 {"error": "no LLM router configured"}
    #[tokio::test]
    async fn test_complete_no_router_returns_503() {
        // GIVEN
        let app_router = llm_routes::<MockBackend>().with_state(test_app_state_no_llm());

        // WHEN
        let body_json = serde_json::json!({
            "messages": [{"role": "user", "content": "bonjour"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/llm/complete")
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
    // WHEN POST /api/v1/llm/complete with an unknown role
    // THEN 400 {"error": "unknown role '...'"}
    #[tokio::test]
    async fn test_complete_unknown_role_returns_400() {
        // GIVEN
        let mut state = test_app_state_no_llm();
        *state.llm_router.write().await = Some(Arc::new(LlmRouter::empty()));
        let app_router = llm_routes::<MockBackend>().with_state(state);

        // WHEN
        let body_json = serde_json::json!({
            "messages": [{"role": "unknown_role", "content": "test"}]
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/llm/complete")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body_json).unwrap()))
            .unwrap();
        let resp = app_router.oneshot(req).await.unwrap();

        // THEN
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["error"]
            .as_str()
            .map(|s| s.contains("unknown_role"))
            .unwrap_or(false));
    }

    // GIVEN an AppState with llm_router = Some(LlmRouter::empty())
    // WHEN GET /api/v1/llm/status
    // THEN 200 with a "backends" array (empty for LlmRouter::empty())
    #[tokio::test]
    async fn test_get_llm_status_with_router_returns_backends_field() {
        // GIVEN
        let mut state = test_app_state_no_llm();
        *state.llm_router.write().await = Some(Arc::new(LlmRouter::empty()));
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

    // ── CRUD backend tests ────────────────────────────────────────────────

    /// Build a test `AppState` with a real in-memory `LlmBackendRepository`.
    fn test_app_state_with_repo(dir: &tempfile::TempDir) -> AppState<MockBackend> {
        let db_path = dir.path().join("system.db");
        let repo = LlmBackendRepository::open(&db_path).expect("open system.db");
        let mut state = test_app_state_no_llm();
        state.llm_backend_repo = Some(Arc::new(std::sync::Mutex::new(repo)));
        state
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), 16 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    // GIVEN a runtime with 2 backends registered
    // WHEN GET /api/v1/llm/backends
    // THEN 200 + JSON array of 2 backends
    #[tokio::test]
    async fn test_list_returns_registered_backends() {
        // GIVEN
        let dir = tempfile::tempdir().unwrap();
        let state = test_app_state_with_repo(&dir);
        {
            let repo = state.llm_backend_repo.as_ref().unwrap().lock().unwrap();
            repo.save(&LlmBackendConfig {
                name: "alpha".into(),
                provider: apollia_core::LlmProvider::OpenAi,
                model: "gpt-4o".into(),
                config_json: serde_json::json!({"api_key": "k1"}),
                enabled: true,
                is_default: true,
            })
            .unwrap();
            repo.save(&LlmBackendConfig {
                name: "beta".into(),
                provider: apollia_core::LlmProvider::Mistral,
                model: "mistral-small-latest".into(),
                config_json: serde_json::json!({}),
                enabled: true,
                is_default: false,
            })
            .unwrap();
        }
        let app_router = llm_routes::<MockBackend>().with_state(state);

        // WHEN
        let req = Request::builder()
            .uri("/api/v1/llm/backends")
            .body(Body::empty())
            .unwrap();
        let resp = app_router.oneshot(req).await.unwrap();

        // THEN
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let backends = json["backends"].as_array().unwrap();
        assert_eq!(backends.len(), 2);
    }

    // GIVEN a runtime without "test-backend"
    // WHEN POST /api/v1/llm/backends with valid body
    // THEN 201 + backend is in the list
    #[tokio::test]
    async fn test_create_persists_new_backend() {
        // GIVEN
        let dir = tempfile::tempdir().unwrap();
        let state = test_app_state_with_repo(&dir);
        let app_router = llm_routes::<MockBackend>().with_state(state);

        // WHEN
        let body = serde_json::json!({
            "name": "test-backend",
            "provider": "openai",
            "model": "gpt-4o",
            "config_json": {"api_key": "sk-test"},
            "enabled": true,
            "is_default": true
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/llm/backends")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app_router.oneshot(req).await.unwrap();

        // THEN
        assert_eq!(resp.status(), StatusCode::CREATED);
        let json = body_json(resp).await;
        assert_eq!(json["name"], "test-backend");
        assert_eq!(json["provider"], "openai");
    }

    // GIVEN backend "a" marked is_default=true
    // WHEN DELETE /api/v1/llm/backends/a
    // THEN 409 Conflict
    #[tokio::test]
    async fn test_delete_default_returns_409() {
        // GIVEN
        let dir = tempfile::tempdir().unwrap();
        let state = test_app_state_with_repo(&dir);
        {
            let repo = state.llm_backend_repo.as_ref().unwrap().lock().unwrap();
            repo.save(&LlmBackendConfig {
                name: "a".into(),
                provider: apollia_core::LlmProvider::OpenAi,
                model: "gpt-4o".into(),
                config_json: serde_json::json!({}),
                enabled: true,
                is_default: true,
            })
            .unwrap();
        }
        let app_router = llm_routes::<MockBackend>().with_state(state);

        // WHEN
        let req = Request::builder()
            .method("DELETE")
            .uri("/api/v1/llm/backends/a")
            .body(Body::empty())
            .unwrap();
        let resp = app_router.oneshot(req).await.unwrap();

        // THEN
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    // GIVEN no backend named "ghost"
    // WHEN GET /api/v1/llm/backends/ghost
    // THEN 404
    #[tokio::test]
    async fn test_unknown_backend_returns_404() {
        // GIVEN
        let dir = tempfile::tempdir().unwrap();
        let state = test_app_state_with_repo(&dir);
        let app_router = llm_routes::<MockBackend>().with_state(state);

        // WHEN
        let req = Request::builder()
            .uri("/api/v1/llm/backends/ghost")
            .body(Body::empty())
            .unwrap();
        let resp = app_router.oneshot(req).await.unwrap();

        // THEN
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // GIVEN no `system.db` available in AppState
    // WHEN POST /api/v1/llm/reload
    // THEN 503, repository is required to rebuild the router
    #[tokio::test]
    async fn test_reload_without_repo_returns_503() {
        let state = test_app_state_no_llm();
        let app_router = llm_routes::<MockBackend>().with_state(state);

        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/llm/reload")
            .body(Body::empty())
            .unwrap();
        let resp = app_router.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    // GIVEN a repo with no default backend
    // WHEN POST /api/v1/llm/reload
    // THEN 503 with an explicit "no default" message, the rebuild needs a
    //       default to pick.
    #[tokio::test]
    async fn test_reload_without_default_returns_503() {
        let dir = tempfile::tempdir().unwrap();
        let state = test_app_state_with_repo(&dir);
        let app_router = llm_routes::<MockBackend>().with_state(state);

        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/llm/reload")
            .body(Body::empty())
            .unwrap();
        let resp = app_router.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        let json = body_json(resp).await;
        let err = json["error"].as_str().unwrap_or_default();
        assert!(err.contains("default"), "expected 'default' in error, got {err}");
    }

    // GIVEN an AppState whose llm_router cell still holds the boot router,
    //       and a repo with no default backend
    // WHEN POST /api/v1/llm/reload fails
    // THEN the boot router is preserved (failed reload does not blank the cell)
    #[tokio::test]
    async fn test_reload_failure_preserves_existing_router() {
        let dir = tempfile::tempdir().unwrap();
        let state = test_app_state_with_repo(&dir);
        *state.llm_router.write().await = Some(Arc::new(LlmRouter::empty()));
        let cell = state.llm_router.clone();
        let app_router = llm_routes::<MockBackend>().with_state(state);

        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/llm/reload")
            .body(Body::empty())
            .unwrap();
        let resp = app_router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

        assert!(
            cell.read().await.is_some(),
            "a failed reload must not drop the previously-loaded router"
        );
    }
}
