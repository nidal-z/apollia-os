//! `POST /api/v1/sessions/:id/pause`, stop the turn in flight.
//!
//! The pause is cooperative: the ReAct loop stops at its next checkpoint, the
//! partial answer is persisted, and no completion event follows. A session
//! with no turn in flight answers the same success, so a client that races the
//! end of a turn never sees an error for a pause that had nothing to stop.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use super::ErrorResponse;
use crate::api::server::AppState;
use crate::chat::PauseError;
use crate::coordinator::ExecutionBackend;

/// Handler for `POST /api/v1/sessions/:id/pause`, pause the turn in flight.
#[utoipa::path(
    post,
    path = "/api/v1/sessions/{id}/pause",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    responses(
        (status = 200, description = "Turn paused, or no turn was in flight"),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn pause_session<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let Some(manager) = &state.chat_manager else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "chat subsystem not available".into(),
            }),
        )
            .into_response();
    };

    match manager.pause_session(&id).await {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({ "status": "paused" })),
        )
            .into_response(),
        Err(error @ PauseError::UnknownSession { .. }) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}
