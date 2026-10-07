//! The questions an agent asked the user and that wait for an answer:
//! `GET /api/v1/user-inputs`, `POST /api/v1/user-inputs/{id}/answer` and
//! `POST /api/v1/user-inputs/{id}/reject`.
//!
//! A pending `ask_user` request lives in the chat manager of the process that
//! runs the agent. These routes let a client of another process, the desktop
//! attached to a daemon first, list and settle them.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use super::{chat_error_to_response, ErrorResponse};
use crate::api::server::AppState;
use crate::chat::manager::ChatSessionManagerHandle;
use crate::chat::manager::PendingUserInputView;
use crate::coordinator::ExecutionBackend;

/// A refusal, already rendered: the error body of this module stays private.
type Refusal = Response;

fn refused(error: crate::chat::types::ChatError) -> Refusal {
    chat_error_to_response(error).into_response()
}

/// Request body for `POST /api/v1/user-inputs/{id}/answer`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct AnswerUserInputRequest {
    /// One answer per question, matched by the question `id`.
    #[schema(value_type = Vec<Object>)]
    pub answers: Vec<apollia_tools::tools::ask_user::UserAnswer>,
}

/// Request body for `POST /api/v1/user-inputs/{id}/reject`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RejectUserInputRequest {
    /// Why the user declined to answer.
    pub reason: String,
}

fn manager<B: ExecutionBackend + Clone>(state: &AppState<B>) -> Option<&ChatSessionManagerHandle> {
    state.chat_manager.as_ref()
}

fn unavailable() -> Refusal {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "chat subsystem not available".into(),
        }),
    )
        .into_response()
}

/// Handler for `GET /api/v1/user-inputs`.
#[utoipa::path(
    get,
    path = "/api/v1/user-inputs",
    tag = "chat",
    responses(
        (status = 200, description = "Questions waiting for the user", body = Vec<PendingUserInputView>),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_pending_user_inputs<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<Vec<PendingUserInputView>>, Refusal> {
    manager(&state)
        .ok_or_else(unavailable)?
        .list_pending_user_inputs()
        .await
        .map(Json)
        .map_err(refused)
}

/// Handler for `POST /api/v1/user-inputs/{id}/answer`.
#[utoipa::path(
    post,
    path = "/api/v1/user-inputs/{id}/answer",
    tag = "chat",
    params(("id" = String, Path, description = "Request id")),
    request_body = AnswerUserInputRequest,
    responses(
        (status = 200, description = "Answers delivered to the agent"),
        (status = 500, description = "No such pending request, or the agent stopped waiting", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn answer_user_input<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Json(request): Json<AnswerUserInputRequest>,
) -> Result<Json<serde_json::Value>, Refusal> {
    manager(&state)
        .ok_or_else(unavailable)?
        .resolve_user_input(id, request.answers)
        .await
        .map_err(refused)?;
    Ok(Json(serde_json::json!({ "status": "answered" })))
}

/// Handler for `POST /api/v1/user-inputs/{id}/reject`.
#[utoipa::path(
    post,
    path = "/api/v1/user-inputs/{id}/reject",
    tag = "chat",
    params(("id" = String, Path, description = "Request id")),
    request_body = RejectUserInputRequest,
    responses(
        (status = 200, description = "The agent is told the user declined"),
        (status = 500, description = "No such pending request, or the agent stopped waiting", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn reject_user_input<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Json(request): Json<RejectUserInputRequest>,
) -> Result<Json<serde_json::Value>, Refusal> {
    manager(&state)
        .ok_or_else(unavailable)?
        .reject_user_input(id, request.reason)
        .await
        .map_err(refused)?;
    Ok(Json(serde_json::json!({ "status": "rejected" })))
}

#[cfg(test)]
mod tests {

    use axum::http::StatusCode;

    use crate::api::test_support::{app_state, call};

    #[tokio::test]
    async fn test_without_a_chat_manager_the_three_routes_answer_503() {
        // GIVEN a daemon whose chat subsystem did not start
        let data = tempfile::tempdir().unwrap();
        let state = || app_state(data.path().to_path_buf());

        // WHEN each route is called through the router
        let (list, _) = call(state(), "GET", "/api/v1/user-inputs", None).await;
        let answers = serde_json::json!({ "answers": [{ "id": "q1", "value": "oui" }] });
        let (answer, _) = call(
            state(),
            "POST",
            "/api/v1/user-inputs/r-1/answer",
            Some(answers),
        )
        .await;
        let reason = serde_json::json!({ "reason": "pas maintenant" });
        let (reject, body) = call(
            state(),
            "POST",
            "/api/v1/user-inputs/r-1/reject",
            Some(reason),
        )
        .await;

        // THEN all three are mounted and say the subsystem is absent
        assert_eq!(list, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(answer, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(reject, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["error"], "chat subsystem not available");
    }
}
