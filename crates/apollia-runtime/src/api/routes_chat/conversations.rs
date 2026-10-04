//! The conversations of the chat as the list manages them: change a session's
//! configuration, rename it, name it from its first message, and erase it.
//!
//! - `PATCH  /api/v1/sessions/{id}`                , instructions, tools, server
//! - `PUT    /api/v1/sessions/{id}/title`          , the title the user typed
//! - `POST   /api/v1/sessions/{id}/title/generate` , a title drawn by the model
//! - `POST   /api/v1/sessions/{id}/erase`          , the session and its messages
//!
//! `DELETE /api/v1/sessions/{id}` closes a session and keeps its history;
//! erasing is a separate action, so the two never share a verb.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use super::chat_error_to_response;
use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::chat::manager::ChatSessionManagerHandle;
use crate::chat::title::{name_session, rename_session, TitleError};
use crate::coordinator::ExecutionBackend;

/// Request body for `PATCH /api/v1/sessions/{id}`. An absent field keeps its
/// value.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct UpdateSessionBody {
    /// New system prompt.
    pub system_prompt: Option<String>,
    /// New list of tools available in the session.
    pub tools: Option<Vec<String>>,
    /// Inference server of the session.
    #[schema(value_type = Option<String>)]
    pub llm_backend: Option<Option<String>>,
}

/// Request body for `PUT /api/v1/sessions/{id}/title`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct RenameSessionBody {
    /// The title, trimmed and capped at 100 characters.
    pub title: String,
}

/// Request body for `POST /api/v1/sessions/{id}/title/generate`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct GenerateTitleBody {
    /// The first user message the title is drawn from.
    pub first_message: String,
}

/// Answer of `POST /api/v1/sessions/{id}/title/generate`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct GeneratedTitle {
    /// The title stored on the session.
    pub title: String,
}

/// Build the router of these routes. `PATCH /api/v1/sessions/{id}` is mounted
/// with the other methods of that path, in the root router.
pub fn conversations_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/sessions/:id/title", put(rename_chat_session::<B>))
        .route(
            "/api/v1/sessions/:id/title/generate",
            post(generate_session_title::<B>),
        )
        .route("/api/v1/sessions/:id/erase", post(erase_session::<B>))
}

type Refusal = (StatusCode, Json<ApiErrorBody>);

fn chat_refusal(error: crate::chat::ChatError) -> Refusal {
    let (status, Json(body)) = chat_error_to_response(error);
    (status, Json(ApiErrorBody { error: body.error }))
}

fn manager<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Result<&ChatSessionManagerHandle, Refusal> {
    state.chat_manager.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiErrorBody {
                error: "chat subsystem not available".into(),
            }),
        )
    })
}

fn title_refusal(error: TitleError) -> Refusal {
    let status = match error {
        TitleError::Chat(chat) => return chat_refusal(chat),
        TitleError::EmptyTitle | TitleError::EmptyMessage => StatusCode::BAD_REQUEST,
        TitleError::NoRouter => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::BAD_GATEWAY,
    };
    (
        status,
        Json(ApiErrorBody {
            error: error.to_string(),
        }),
    )
}

fn done(status: &str) -> axum::response::Response {
    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": status })),
    )
        .into_response()
}

/// Handler for `PATCH /api/v1/sessions/{id}`, change the configuration.
#[utoipa::path(
    patch,
    path = "/api/v1/sessions/{id}",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    request_body = UpdateSessionBody,
    responses(
        (status = 200, description = "Session updated"),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn update_session<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateSessionBody>,
) -> Result<axum::response::Response, Refusal> {
    manager(&state)?
        .update_session(id, body.system_prompt, body.tools, body.llm_backend)
        .await
        .map_err(chat_refusal)?;
    Ok(done("updated"))
}

/// Handler for `PUT /api/v1/sessions/{id}/title`, rename.
#[utoipa::path(
    put,
    path = "/api/v1/sessions/{id}/title",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    request_body = RenameSessionBody,
    responses(
        (status = 200, description = "Session renamed"),
        (status = 400, description = "Blank title", body = crate::api::openapi::ApiErrorBody),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn rename_chat_session<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Json(body): Json<RenameSessionBody>,
) -> Result<axum::response::Response, Refusal> {
    rename_session(manager(&state)?, id, &body.title)
        .await
        .map_err(title_refusal)?;
    Ok(done("renamed"))
}

/// Handler for `POST /api/v1/sessions/{id}/title/generate`, name the session
/// from its first message with the configured model.
#[utoipa::path(
    post,
    path = "/api/v1/sessions/{id}/title/generate",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    request_body = GenerateTitleBody,
    responses(
        (status = 200, description = "The title stored on the session", body = GeneratedTitle),
        (status = 400, description = "Blank message", body = crate::api::openapi::ApiErrorBody),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 502, description = "The model failed or answered no title", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem or inference server unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn generate_session_title<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Json(body): Json<GenerateTitleBody>,
) -> Result<Json<GeneratedTitle>, Refusal> {
    let manager = manager(&state)?;
    let llm = state.llm_router.read().await.clone();
    let title = name_session(manager, llm.as_deref(), id, body.first_message)
        .await
        .map_err(title_refusal)?;
    Ok(Json(GeneratedTitle { title }))
}

/// Handler for `POST /api/v1/sessions/{id}/erase`, delete the session and
/// every message it holds.
#[utoipa::path(
    post,
    path = "/api/v1/sessions/{id}/erase",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    responses(
        (status = 200, description = "Session erased"),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn erase_session<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, Refusal> {
    manager(&state)?
        .delete_session(id)
        .await
        .map_err(chat_refusal)?;
    Ok(done("erased"))
}

#[cfg(test)]
mod tests;
