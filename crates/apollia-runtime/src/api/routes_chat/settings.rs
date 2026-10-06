//! The settings and the notes of one conversation.
//!
//! - `GET /api/v1/sessions/{id}/settings` , what the conversation changes from its agent
//! - `PUT /api/v1/sessions/{id}/settings` , replace them
//! - `GET /api/v1/sessions/{id}/notes`    , the notes kept beside the conversation
//! - `PUT /api/v1/sessions/{id}/notes`    , replace them
//!
//! The settings hold for the conversation alone: nothing is written to its
//! agent nor to governance. A field left out is inherited from the agent, and
//! a field the settings do not declare is refused. A closed conversation is
//! read and no longer written.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use super::conversations::{chat_refusal, manager, Refusal};
use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::chat::SessionSettings;
use crate::coordinator::ExecutionBackend;

/// The notes of a conversation, as `GET` answers them and `PUT` takes them.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionNotes {
    /// Free text the user keeps beside the conversation. It is never sent to
    /// a model.
    pub notes: String,
}

/// Build the router of these routes.
pub fn settings_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/sessions/:id/settings",
            get(get_session_settings::<B>).put(put_session_settings::<B>),
        )
        .route(
            "/api/v1/sessions/:id/notes",
            get(get_session_notes::<B>).put(put_session_notes::<B>),
        )
}

fn not_found(id: &str) -> Refusal {
    (
        StatusCode::NOT_FOUND,
        Json(ApiErrorBody {
            error: format!("session not found: {id}"),
        }),
    )
}

/// Reads `body` as `T`, a closed type: a field it does not declare, or a
/// value of the wrong shape, is answered 400 with the name at fault.
fn closed_body<T: serde::de::DeserializeOwned>(body: serde_json::Value) -> Result<T, Refusal> {
    serde_json::from_value(body).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiErrorBody {
                error: error.to_string(),
            }),
        )
    })
}

/// Handler for `GET /api/v1/sessions/{id}/settings`.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/{id}/settings",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    responses(
        (status = 200, description = "The settings of the conversation", body = SessionSettings),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_session_settings<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<SessionSettings>, Refusal> {
    let detail = manager(&state)?
        .get_session(id.clone())
        .await
        .ok_or_else(|| not_found(&id))?;
    Ok(Json(detail.session.settings))
}

/// Handler for `PUT /api/v1/sessions/{id}/settings`.
#[utoipa::path(
    put,
    path = "/api/v1/sessions/{id}/settings",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    request_body = SessionSettings,
    responses(
        (status = 200, description = "The settings as stored", body = SessionSettings),
        (status = 400, description = "Unknown field, or a value that cannot be applied", body = crate::api::openapi::ApiErrorBody),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 409, description = "Session closed", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn put_session_settings<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<SessionSettings>, Refusal> {
    let settings: SessionSettings = closed_body(body)?;
    let stored = manager(&state)?
        .set_session_settings(id, settings)
        .await
        .map_err(chat_refusal)?;
    Ok(Json(stored))
}

/// Handler for `GET /api/v1/sessions/{id}/notes`.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/{id}/notes",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    responses(
        (status = 200, description = "The notes of the conversation", body = SessionNotes),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_session_notes<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<SessionNotes>, Refusal> {
    let detail = manager(&state)?
        .get_session(id.clone())
        .await
        .ok_or_else(|| not_found(&id))?;
    Ok(Json(SessionNotes {
        notes: detail.session.notes,
    }))
}

/// Handler for `PUT /api/v1/sessions/{id}/notes`.
#[utoipa::path(
    put,
    path = "/api/v1/sessions/{id}/notes",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    request_body = SessionNotes,
    responses(
        (status = 200, description = "The notes as stored", body = SessionNotes),
        (status = 400, description = "Unknown field, or notes too long", body = crate::api::openapi::ApiErrorBody),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 409, description = "Session closed", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn put_session_notes<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<SessionNotes>, Refusal> {
    let body: SessionNotes = closed_body(body)?;
    let notes = manager(&state)?
        .set_session_notes(id, body.notes)
        .await
        .map_err(chat_refusal)?;
    Ok(Json(SessionNotes { notes }))
}

#[cfg(test)]
mod tests;
