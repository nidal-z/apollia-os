//! Driving the compatible server from the native API: `GET /api/v1/server`,
//! `POST /api/v1/server/start` and `POST /api/v1/server/stop`, and its tokens
//! under `/api/v1/server/tokens` (see [`tokens`]).
//!
//! The compatible server is a second listener with its own port (see
//! [`crate::compat_server`]); these routes are the only part of it the native
//! API knows. Starting and stopping it writes `[server] enabled`, so the state
//! survives a restart of the runtime.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::compat_server::{CompatServerError, CompatServerHandle, CompatServerStatus};
use crate::coordinator::ExecutionBackend;

type ApiError = (StatusCode, Json<ApiErrorBody>);

/// Builds the router of the server routes and of its tokens.
pub fn server_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/server", get(server_status::<B>))
        .route("/api/v1/server/start", post(start_server::<B>))
        .route("/api/v1/server/stop", post(stop_server::<B>))
        .route(
            "/api/v1/server/tokens",
            get(tokens::list_server_tokens::<B>).post(tokens::create_server_token::<B>),
        )
        .route(
            "/api/v1/server/tokens/:token",
            delete(tokens::revoke_server_token::<B>),
        )
}

fn handle<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Result<&CompatServerHandle, ApiError> {
    state.compat_server.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiErrorBody {
                error: "the compatible server is not available in this runtime".to_owned(),
            }),
        )
    })
}

fn refusal(error: CompatServerError) -> ApiError {
    let status = match error {
        CompatServerError::PortInUse { .. } => StatusCode::CONFLICT,
        CompatServerError::ActorGone => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(ApiErrorBody {
            error: error.to_string(),
        }),
    )
}

/// `GET /api/v1/server`: whether the compatible server listens, its port, why
/// its last start failed, and how many of its tokens are in use.
#[utoipa::path(
    get,
    path = "/api/v1/server",
    tag = "server",
    responses(
        (status = 200, description = "State of the compatible server", body = CompatServerStatus),
        (status = 503, description = "No compatible server in this runtime", body = ApiErrorBody),
    )
)]
pub(crate) async fn server_status<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<CompatServerStatus>, ApiError> {
    let status = handle(&state)?.status().await.map_err(refusal)?;
    Ok(Json(status))
}

/// `POST /api/v1/server/start`: opens the compatible server on its configured
/// port and writes `[server] enabled = true`.
#[utoipa::path(
    post,
    path = "/api/v1/server/start",
    tag = "server",
    responses(
        (status = 200, description = "The compatible server listens", body = CompatServerStatus),
        (status = 409, description = "Another program holds the port", body = ApiErrorBody),
        (status = 500, description = "The listener or the setting failed", body = ApiErrorBody),
        (status = 503, description = "No compatible server in this runtime", body = ApiErrorBody),
    )
)]
pub(crate) async fn start_server<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<CompatServerStatus>, ApiError> {
    let status = handle(&state)?.start().await.map_err(refusal)?;
    Ok(Json(status))
}

/// `POST /api/v1/server/stop`: closes the compatible server and writes
/// `[server] enabled = false`. The native API keeps answering.
#[utoipa::path(
    post,
    path = "/api/v1/server/stop",
    tag = "server",
    responses(
        (status = 200, description = "The compatible server is stopped", body = CompatServerStatus),
        (status = 500, description = "The setting could not be written", body = ApiErrorBody),
        (status = 503, description = "No compatible server in this runtime", body = ApiErrorBody),
    )
)]
pub(crate) async fn stop_server<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<CompatServerStatus>, ApiError> {
    let status = handle(&state)?.stop().await.map_err(refusal)?;
    Ok(Json(status))
}

pub(crate) mod tokens;

#[cfg(test)]
mod tests;
