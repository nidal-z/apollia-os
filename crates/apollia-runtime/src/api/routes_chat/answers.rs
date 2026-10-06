//! The actions on one message of a conversation, beside playing a turn again:
//! `GET /api/v1/sessions/{id}/continuation`,
//! `POST /api/v1/sessions/{id}/messages/{message_id}/continue`,
//! `PUT /api/v1/sessions/{id}/messages/{message_id}` and
//! `DELETE /api/v1/sessions/{id}/messages/{message_id}`.
//!
//! Continuing an answer runs its turn again with the answer as the prefill of
//! the assistant message; the rest streams through the event bus as for a new
//! message. It is refused where the engine of the model does not continue an
//! answer, with the reason the read reports. Rewriting an answer and removing
//! a message change the history in place and play nothing.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use super::{chat_error_to_response, ErrorResponse};
use crate::api::server::AppState;
use crate::chat::manager::Continuation;
use crate::coordinator::ExecutionBackend;

/// Request body for `PUT /api/v1/sessions/{id}/messages/{message_id}`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct EditAnswerRequest {
    /// The new text of the answer.
    pub content: String,
}

/// Answer of `DELETE /api/v1/sessions/{id}/messages/{message_id}`.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeletedMessages {
    /// Messages removed: the one named and, for an answer, the tool messages
    /// of its turn.
    pub deleted: u32,
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

/// Handler for `GET /api/v1/sessions/{id}/continuation`: whether the last
/// answer can be continued on the model the next turn runs on, and why not.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/{id}/continuation",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    responses(
        (status = 200, description = "Whether the last answer can be continued", body = Continuation),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_continuation<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Response {
    let Some(manager) = &state.chat_manager else {
        return unavailable();
    };
    match manager.continuation(id).await {
        Ok(found) => Json(found).into_response(),
        Err(e) => chat_error_to_response(e).into_response(),
    }
}

/// Handler for `POST /api/v1/sessions/{id}/messages/{message_id}/continue`.
///
/// `message_id` is the last answer of the session. Its turn runs again with
/// the answer as the prefill, and the answer that lands replaces it under the
/// same id.
#[utoipa::path(
    post,
    path = "/api/v1/sessions/{id}/messages/{message_id}/continue",
    tag = "chat",
    params(
        ("id" = String, Path, description = "Session id"),
        ("message_id" = String, Path, description = "The last answer of the session"),
    ),
    responses(
        (status = 202, description = "Answer being continued"),
        (status = 400, description = "Not the last answer, or an empty one", body = crate::api::openapi::ApiErrorBody),
        (status = 404, description = "Session or message not found", body = crate::api::openapi::ApiErrorBody),
        (status = 409, description = "Session closed or busy", body = crate::api::openapi::ApiErrorBody),
        (status = 422, description = "The model does not continue an answer", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn continue_answer<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path((id, message_id)): Path<(String, String)>,
) -> Response {
    let Some(manager) = &state.chat_manager else {
        return unavailable();
    };
    match manager.continue_answer(id, message_id).await {
        Ok(()) => (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({ "status": "processing" })),
        )
            .into_response(),
        Err(e) => chat_error_to_response(e).into_response(),
    }
}

/// Handler for `PUT /api/v1/sessions/{id}/messages/{message_id}`: replace the
/// text of an answer, marked edited. No turn is played again.
#[utoipa::path(
    put,
    path = "/api/v1/sessions/{id}/messages/{message_id}",
    tag = "chat",
    params(
        ("id" = String, Path, description = "Session id"),
        ("message_id" = String, Path, description = "Answer to rewrite"),
    ),
    request_body = EditAnswerRequest,
    responses(
        (status = 200, description = "Answer rewritten"),
        (status = 400, description = "A user message, or an empty text", body = crate::api::openapi::ApiErrorBody),
        (status = 404, description = "Session or message not found", body = crate::api::openapi::ApiErrorBody),
        (status = 409, description = "Session closed or busy", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn edit_answer<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path((id, message_id)): Path<(String, String)>,
    Json(body): Json<EditAnswerRequest>,
) -> Response {
    let Some(manager) = &state.chat_manager else {
        return unavailable();
    };
    match manager.edit_answer(id, message_id, body.content).await {
        Ok(()) => Json(serde_json::json!({ "status": "edited" })).into_response(),
        Err(e) => chat_error_to_response(e).into_response(),
    }
}

/// Handler for `DELETE /api/v1/sessions/{id}/messages/{message_id}`: remove a
/// message and, for an answer, the tool messages of its turn.
#[utoipa::path(
    delete,
    path = "/api/v1/sessions/{id}/messages/{message_id}",
    tag = "chat",
    params(
        ("id" = String, Path, description = "Session id"),
        ("message_id" = String, Path, description = "Message to remove"),
    ),
    responses(
        (status = 200, description = "Messages removed", body = DeletedMessages),
        (status = 404, description = "Session or message not found", body = crate::api::openapi::ApiErrorBody),
        (status = 409, description = "Session closed or busy", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn delete_message<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path((id, message_id)): Path<(String, String)>,
) -> Response {
    let Some(manager) = &state.chat_manager else {
        return unavailable();
    };
    match manager.delete_message(id, message_id).await {
        Ok(deleted) => Json(DeletedMessages { deleted }).into_response(),
        Err(e) => chat_error_to_response(e).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::{json, Value};

    use crate::api::server::AppState;
    use crate::api::test_support::{call, seeded_chat, MockBackend};

    /// A daemon holding the session `s-1`, one user turn and its answer,
    /// resumed into memory. Its router holds no model.
    async fn seeded(data: &tempfile::TempDir) -> AppState<MockBackend> {
        let state = seeded_chat(data.path());
        let (resumed, _) = call(state.clone(), "POST", "/api/v1/sessions/s-1/resume", None).await;
        assert_eq!(resumed, StatusCode::OK);
        state
    }

    async fn history(state: &AppState<MockBackend>) -> Vec<Value> {
        let (_, body) = call(state.clone(), "GET", "/api/v1/sessions/s-1", None).await;
        body["session"]["history"].as_array().cloned().unwrap()
    }

    #[tokio::test]
    async fn test_without_a_model_that_continues_the_read_says_why_and_the_action_is_refused() {
        // GIVEN a daemon whose router holds no model that continues an answer
        let data = tempfile::tempdir().unwrap();
        let state = seeded(&data).await;

        // WHEN the continuation is read, then asked for
        let (read, support) = call(
            state.clone(),
            "GET",
            "/api/v1/sessions/s-1/continuation",
            None,
        )
        .await;
        let (asked, refusal) = call(
            state.clone(),
            "POST",
            "/api/v1/sessions/s-1/messages/a-1/continue",
            None,
        )
        .await;

        // THEN the read reports it unavailable with its reason, the action is
        // refused with that reason, and the answer is as it was
        assert_eq!(read, StatusCode::OK);
        assert_eq!(support, json!({ "available": false, "reason": "engine" }));
        assert_eq!(asked, StatusCode::UNPROCESSABLE_ENTITY, "{refusal}");
        assert!(refusal["error"].as_str().unwrap().contains("engine"));
        assert_eq!(
            history(&state).await[1]["content"],
            "Hello, how can I help?"
        );
    }

    #[tokio::test]
    async fn test_an_answer_is_rewritten_in_place_and_a_user_message_is_not() {
        // GIVEN a daemon holding one user turn and its answer
        let data = tempfile::tempdir().unwrap();
        let state = seeded(&data).await;
        let body = Some(json!({ "content": "Good evening." }));

        // WHEN the answer is rewritten, then the user message
        let (answer, _) = call(
            state.clone(),
            "PUT",
            "/api/v1/sessions/s-1/messages/a-1",
            body.clone(),
        )
        .await;
        let (user, refusal) = call(
            state.clone(),
            "PUT",
            "/api/v1/sessions/s-1/messages/u-1",
            body,
        )
        .await;

        // THEN the answer reads the new text, marked edited, under the same
        // id, and the user message is refused and unchanged
        assert_eq!(answer, StatusCode::OK);
        assert_eq!(user, StatusCode::BAD_REQUEST, "{refusal}");
        let messages = history(&state).await;
        assert_eq!(messages[0]["content"], "Bonjour");
        assert_eq!(messages[1]["id"], "a-1");
        assert_eq!(messages[1]["content"], "Good evening.");
        assert_eq!(messages[1]["metadata"]["edited"], true);
    }

    #[tokio::test]
    async fn test_a_message_is_deleted_once_and_an_unknown_one_is_not_found() {
        // GIVEN a daemon holding one user turn and its answer
        let data = tempfile::tempdir().unwrap();
        let state = seeded(&data).await;
        let answer = "/api/v1/sessions/s-1/messages/a-1";

        // WHEN the answer is deleted, twice, then a message of a session the
        // daemon never opened
        let (first, removed) = call(state.clone(), "DELETE", answer, None).await;
        let (second, _) = call(state.clone(), "DELETE", answer, None).await;
        let (ghost, _) = call(
            state.clone(),
            "DELETE",
            "/api/v1/sessions/ghost/messages/a-1",
            None,
        )
        .await;

        // THEN it is removed once, the user message stays alone, and both
        // other calls answer not found
        assert_eq!(first, StatusCode::OK);
        assert_eq!(removed, json!({ "deleted": 1 }));
        assert_eq!(second, StatusCode::NOT_FOUND);
        assert_eq!(ghost, StatusCode::NOT_FOUND);
        let messages = history(&state).await;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["id"], "u-1");
    }
}
