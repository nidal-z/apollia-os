//! A turn of the conversation played again:
//! `POST /api/v1/sessions/{id}/messages/{message_id}/regenerate` and
//! `POST /api/v1/sessions/{id}/messages/{message_id}/resend`.
//!
//! Both truncate the history in place, in the same session, then run the turn
//! again; the answer streams through the event bus as for a new message. They
//! serve the desktop attached to a daemon, which reaches the chat manager only
//! through these routes.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use super::{chat_error_to_response, ErrorResponse, SendMessageResponse};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Request body for `POST /api/v1/sessions/{id}/messages/{message_id}/resend`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ResendMessageRequest {
    /// The new text of the user message.
    pub content: String,
}

fn unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "chat subsystem not available".into(),
        }),
    )
        .into_response()
}

/// Handler for `POST /api/v1/sessions/{id}/messages/{message_id}/regenerate`.
///
/// `message_id` is the assistant message to regenerate: it and every later
/// message are dropped, then the user turn it answered runs again.
#[utoipa::path(
    post,
    path = "/api/v1/sessions/{id}/messages/{message_id}/regenerate",
    tag = "chat",
    params(
        ("id" = String, Path, description = "Session id"),
        ("message_id" = String, Path, description = "Assistant message to regenerate"),
    ),
    responses(
        (status = 202, description = "Turn running again"),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 409, description = "Session closed or busy", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Message not found or no user turn before it", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn regenerate_response<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path((id, message_id)): Path<(String, String)>,
) -> Response {
    let Some(manager) = &state.chat_manager else {
        return unavailable();
    };
    match manager.regenerate_response(id, message_id).await {
        Ok(()) => (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({ "status": "processing" })),
        )
            .into_response(),
        Err(e) => chat_error_to_response(e).into_response(),
    }
}

/// Handler for `POST /api/v1/sessions/{id}/messages/{message_id}/resend`.
///
/// `message_id` is the user message to edit: it and every later message are
/// dropped, then `content` is sent as a fresh user turn.
#[utoipa::path(
    post,
    path = "/api/v1/sessions/{id}/messages/{message_id}/resend",
    tag = "chat",
    params(
        ("id" = String, Path, description = "Session id"),
        ("message_id" = String, Path, description = "User message to replace"),
    ),
    request_body = ResendMessageRequest,
    responses(
        (status = 202, description = "New user message accepted", body = SendMessageResponse),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 409, description = "Session closed or busy", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Message not found or not a user message", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn resend_message<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path((id, message_id)): Path<(String, String)>,
    Json(body): Json<ResendMessageRequest>,
) -> Response {
    let Some(manager) = &state.chat_manager else {
        return unavailable();
    };
    match manager.edit_and_resend(id, message_id, body.content).await {
        Ok(message_id) => (
            StatusCode::ACCEPTED,
            Json(SendMessageResponse {
                message_id,
                status: "processing".into(),
            }),
        )
            .into_response(),
        Err(e) => chat_error_to_response(e).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::{json, Value};

    use crate::api::test_support::{call, seeded_chat};

    /// Calls `uri` on a daemon holding the session `s-1`, resumed into memory.
    async fn on_seeded_session(uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        let data = tempfile::tempdir().unwrap();
        let state = seeded_chat(data.path());
        let (resumed, _) = call(state.clone(), "POST", "/api/v1/sessions/s-1/resume", None).await;
        assert_eq!(resumed, StatusCode::OK);
        call(state, "POST", uri, body).await
    }

    #[tokio::test]
    async fn test_an_answer_is_regenerated_and_a_user_message_resent_in_the_same_session() {
        // GIVEN a daemon holding a session with one user turn and its answer
        let regenerate = "/api/v1/sessions/s-1/messages/a-1/regenerate";
        let resend = "/api/v1/sessions/s-1/messages/u-1/resend";

        // WHEN the answer is regenerated, and, on another copy, the user
        // message is edited and sent again
        let (regenerated, regenerated_body) = on_seeded_session(regenerate, None).await;
        let (resent, resent_body) =
            on_seeded_session(resend, Some(json!({ "content": "Bonsoir" }))).await;

        // THEN both turns run again, the edit under a new user message
        assert_eq!(regenerated, StatusCode::ACCEPTED, "{regenerated_body}");
        assert_eq!(regenerated_body["status"], "processing");
        assert_eq!(resent, StatusCode::ACCEPTED, "{resent_body}");
        assert_ne!(resent_body["message_id"], "u-1");
        assert!(resent_body["message_id"].is_string());
    }

    #[tokio::test]
    async fn test_editing_an_answer_or_a_missing_session_is_refused_with_the_manager_words() {
        // GIVEN a daemon holding one session
        let edit_answer = "/api/v1/sessions/s-1/messages/a-1/resend";
        let ghost = "/api/v1/sessions/ghost/messages/a-1/regenerate";

        // WHEN an assistant message is edited, and a session it never opened
        // is regenerated
        let (edit, edit_body) =
            on_seeded_session(edit_answer, Some(json!({ "content": "Bonsoir" }))).await;
        let (missing, missing_body) = on_seeded_session(ghost, None).await;

        // THEN each is refused with the message the embedded command shows
        assert_eq!(edit, StatusCode::INTERNAL_SERVER_ERROR);
        let edit_error = edit_body["error"].as_str().unwrap();
        assert!(
            edit_error.contains("can only edit a user message"),
            "{edit_error}"
        );
        assert_eq!(missing, StatusCode::NOT_FOUND);
        assert!(missing_body["error"].as_str().unwrap().contains("ghost"));
    }
}
