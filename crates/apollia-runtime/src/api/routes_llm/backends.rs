//! CRUD over the configured LLM backends, `/api/v1/llm/backends`.
//!
//! The repository is the source of truth; every mutation mirrors itself into
//! `config.toml` and, for the hot paths, reloads the live router.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_core::{LlmBackendConfig, LlmBackendError, LlmBackendRepository, LlmProvider};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

// ─────────────────────────────────────────────
// CRUD types
// ─────────────────────────────────────────────

/// Request body for `POST /api/v1/llm/backends`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateLlmBackendRequest {
    /// Unique name, pattern `^[a-z0-9_-]+$`.
    pub name: String,
    /// Provider identifier: `"llama-cpp"`, `"openai"`, `"mistral"`, `"anthropic"`, `"ollama"`.
    pub provider: String,
    /// Model identifier (e.g. `"gpt-4o"`, `"mistral-small-latest"`).
    pub model: String,
    /// Provider-specific configuration object (must be a JSON object, not null or primitive).
    #[schema(value_type = Object)]
    pub config_json: serde_json::Value,
    /// Whether this backend is active (default: `true`).
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Mark this backend as the default (default: `false`).
    #[serde(default)]
    pub is_default: bool,
}

/// Request body for `PUT /api/v1/llm/backends/:name`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateLlmBackendRequest {
    /// Provider identifier.
    pub provider: String,
    /// Model identifier.
    pub model: String,
    /// Provider-specific configuration object.
    #[schema(value_type = Object)]
    pub config_json: serde_json::Value,
    /// Whether this backend is active.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Whether this backend is the default.
    #[serde(default)]
    pub is_default: bool,
}

/// Response body for a single backend.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct LlmBackendResponse {
    /// Unique backend name.
    pub name: String,
    /// Provider identifier string.
    pub provider: String,
    /// Model identifier.
    pub model: String,
    /// Provider-specific configuration.
    #[schema(value_type = Object)]
    pub config_json: serde_json::Value,
    /// Whether this backend is enabled.
    pub enabled: bool,
    /// Whether this is the default backend.
    pub is_default: bool,
}

/// Response body for `GET /api/v1/llm/backends`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct LlmBackendsListResponse {
    /// All configured backends.
    pub backends: Vec<LlmBackendResponse>,
}

/// Response body for `DELETE /api/v1/llm/backends/:name`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DeleteBackendResponse {
    /// Name of the deleted backend.
    pub deleted: String,
}

/// Response body for `POST /api/v1/llm/backends/:name/set-default`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SetDefaultResponse {
    /// The backend that is now the default.
    pub default: String,
}

/// Response body for `POST /api/v1/llm/reload`.
///
/// Carries the list of backends that are live in the freshly-swapped router,
/// so callers can confirm at a glance what is now available without a
/// follow-up `GET /api/v1/llm/status` call.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ReloadRouterResponse {
    /// Backends now reachable via the active router.
    #[schema(value_type = Vec<Object>)]
    pub backends: Vec<apollia_llm::BackendInfo>,
    /// Default backend name reported by the router (empty when no backends).
    pub default: String,
    /// Whether the rebuilt router reaches agents that are already running.
    ///
    /// `false` today, and not a transient state: the agent execution path reads
    /// its router from a `OnceLock` populated at boot, a different cell from the
    /// one this route rewrites. A reload therefore reaches chat and this API,
    /// and an already-running agent keeps the router it started with until the
    /// daemon restarts. Reported rather than hidden, because the failure it
    /// produces on the Python side, `'NoneType' object has no attribute
    /// 'complete'`, names none of this.
    pub reaches_running_agents: bool,
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
        LlmBackendError::Db(_) | LlmBackendError::Io(_) | LlmBackendError::Schema(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
        }
        // The enum is #[non_exhaustive]: a variant added upstream reaches the
        // client as a 500 carrying its own message, never as a compile break.
        _ => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
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
            tracing::warn!(error = %e, "llm.backends.sync.failed");
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
#[utoipa::path(
    get,
    path = "/api/v1/llm/backends",
    tag = "llm",
    responses(
        (status = 200, description = "Configured backends", body = LlmBackendsListResponse),
        (status = 500, description = "Repository error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Backend repository not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
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
#[utoipa::path(
    get,
    path = "/api/v1/llm/backends/{name}",
    tag = "llm",
    params(("name" = String, Path, description = "Backend name")),
    responses(
        (status = 200, description = "Backend detail", body = LlmBackendResponse),
        (status = 404, description = "Backend not found", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Repository error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Backend repository not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
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
#[utoipa::path(
    post,
    path = "/api/v1/llm/backends",
    tag = "llm",
    request_body = CreateLlmBackendRequest,
    responses(
        (status = 201, description = "Backend created", body = LlmBackendResponse),
        (status = 409, description = "A default backend already exists", body = crate::api::openapi::ApiErrorBody),
        (status = 422, description = "Validation error", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Repository error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Backend repository not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
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
#[utoipa::path(
    put,
    path = "/api/v1/llm/backends/{name}",
    tag = "llm",
    params(("name" = String, Path, description = "Backend name")),
    request_body = UpdateLlmBackendRequest,
    responses(
        (status = 200, description = "Backend updated", body = LlmBackendResponse),
        (status = 404, description = "Backend not found", body = crate::api::openapi::ApiErrorBody),
        (status = 422, description = "Validation error", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Repository error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Backend repository not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
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
#[utoipa::path(
    delete,
    path = "/api/v1/llm/backends/{name}",
    tag = "llm",
    params(("name" = String, Path, description = "Backend name")),
    responses(
        (status = 200, description = "Backend deleted", body = DeleteBackendResponse),
        (status = 404, description = "Backend not found", body = crate::api::openapi::ApiErrorBody),
        (status = 409, description = "Cannot delete the default backend", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Repository error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Backend repository not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
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
#[utoipa::path(
    post,
    path = "/api/v1/llm/backends/{name}/set-default",
    tag = "llm",
    params(("name" = String, Path, description = "Backend name")),
    responses(
        (status = 200, description = "Backend marked as default", body = SetDefaultResponse),
        (status = 404, description = "Backend not found", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Repository error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Backend repository not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
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
#[utoipa::path(
    post,
    path = "/api/v1/llm/reload",
    tag = "llm",
    responses(
        (status = 200, description = "Router reloaded with the live backend list", body = ReloadRouterResponse),
        (status = 500, description = "Router rebuild failed", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Backend repository unavailable or no default backend", body = crate::api::openapi::ApiErrorBody),
    )
)]
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

    // Re-inject the `LlamaCpp -> llama-server` override so the local backend
    // stays wired through the managed server; otherwise the rebuild would drop
    // it and local inference would become unreachable from agents and chat.
    // Shares the same factory used by the supervisor at boot.
    let factory =
        crate::llama_server_backend::llama_server_override(state.llama_server_supervisor.clone());
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
        "llm.router.reloaded"
    );

    Ok(Json(ReloadRouterResponse {
        backends,
        default,
        reaches_running_agents: false,
    }))
}
