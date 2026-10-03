//! Two reads of a chat session the desktop needs in both of its modes:
//! `GET /api/v1/sessions/{id}/metrics`, the metrics panel, and
//! `GET /api/v1/sessions/{id}/workspace`, the folder a relative path named in
//! the conversation is revealed against.
//!
//! Neither had a route: only the desktop running the runtime in its own process
//! could read them. Both answer what the embedded desktop answers, through the
//! same methods of the chat manager.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::chat::manager::ChatSessionManagerHandle;
use crate::chat::SessionMetrics;
use crate::coordinator::ExecutionBackend;

type ApiError = (StatusCode, Json<ApiErrorBody>);

/// Answer of `GET /api/v1/sessions/{id}/workspace`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct SessionWorkspace {
    /// The working directory of the session, `null` when none resolves (an
    /// unknown session, or a free chat with no default workspace and no data
    /// directory).
    pub workspace: Option<String>,
}

fn chat_manager<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Result<&ChatSessionManagerHandle, ApiError> {
    state.chat_manager.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiErrorBody {
                error: "chat subsystem not available".into(),
            }),
        )
    })
}

/// Handler for `GET /api/v1/sessions/{id}/metrics`, the metrics of a session.
///
/// Zero-filled when no exchange has completed yet, as in the embedded desktop.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/{id}/metrics",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    responses(
        (status = 200, description = "Session metrics", body = SessionMetrics),
        (status = 503, description = "Chat subsystem unavailable", body = ApiErrorBody),
    )
)]
pub async fn get_session_metrics<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<SessionMetrics>, ApiError> {
    let manager = chat_manager(&state)?;
    Ok(Json(manager.session_metrics_or_empty(id).await))
}

/// Handler for `GET /api/v1/sessions/{id}/workspace`, the working directory of
/// a session.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/{id}/workspace",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    responses(
        (status = 200, description = "Session working directory", body = SessionWorkspace),
        (status = 503, description = "Chat subsystem unavailable", body = ApiErrorBody),
    )
)]
pub async fn get_session_workspace<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<SessionWorkspace>, ApiError> {
    let manager = chat_manager(&state)?;
    let workspace = manager
        .resolve_session_workspace(id)
        .await
        .map(|path| path.to_string_lossy().into_owned());
    Ok(Json(SessionWorkspace { workspace }))
}

#[cfg(test)]
mod tests;
