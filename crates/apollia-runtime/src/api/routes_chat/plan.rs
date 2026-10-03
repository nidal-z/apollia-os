//! The plan mode of a chat session, over HTTP.
//!
//! - `GET  /api/v1/sessions/plan-mode-default`    , the state a new session starts in
//! - `PUT  /api/v1/sessions/:id/plan-mode`        , turn plan mode on or off
//! - `GET  /api/v1/sessions/:id/plan`             , the plan and its phase
//! - `GET  /api/v1/sessions/:id/plan/mutations`   , the plan's revision history
//! - `POST /api/v1/sessions/:id/plan/approve`     , approve the plan awaiting approval
//! - `POST /api/v1/sessions/:id/plan/reject`      , reject it, with an optional reason
//!
//! Each route calls the same [`ChatSessionManagerHandle`] method the desktop
//! calls when the runtime runs in its process, and a refusal carries the
//! manager's own message, so both modes answer alike.
//!
//! [`ChatSessionManagerHandle`]: crate::chat::manager::ChatSessionManagerHandle

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use super::{chat_error_to_response, ErrorResponse};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Response body for `GET /api/v1/sessions/plan-mode-default`.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PlanModeDefault {
    /// Whether a new session starts in plan mode (`[chat] plan_mode_default`).
    pub enabled: bool,
}

/// Request body for `PUT /api/v1/sessions/:id/plan-mode`.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SetPlanModeRequest {
    /// `true` turns plan mode on (phase `discovery`), `false` turns it off.
    pub enabled: bool,
}

/// Request body for `POST /api/v1/sessions/:id/plan/reject`.
#[derive(Debug, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RejectPlanRequest {
    /// Guidance the agent reads when it revises the plan.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Response body of the plan-mode writes.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PlanActionResponse {
    /// Session the action applied to.
    pub session_id: String,
    /// What was done: `plan_mode_on`, `plan_mode_off`, `approved` or `rejected`.
    pub status: String,
}

/// The 503 every route of this module answers without the chat manager, in
/// the words of the embedded commands.
fn unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "chat subsystem not available".into(),
        }),
    )
        .into_response()
}

fn done(session_id: String, status: &str) -> Response {
    (
        StatusCode::OK,
        Json(PlanActionResponse {
            session_id,
            status: status.to_owned(),
        }),
    )
        .into_response()
}

/// Handler for `GET /api/v1/sessions/plan-mode-default`.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/plan-mode-default",
    tag = "chat",
    responses(
        (status = 200, description = "The plan-mode state a new session starts in", body = PlanModeDefault),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_plan_mode_default<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Response {
    match state.chat_manager.as_ref() {
        Some(manager) => Json(PlanModeDefault {
            enabled: manager.plan_mode_default(),
        })
        .into_response(),
        None => unavailable(),
    }
}

/// Handler for `PUT /api/v1/sessions/:id/plan-mode`.
#[utoipa::path(
    put,
    path = "/api/v1/sessions/{id}/plan-mode",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    request_body = SetPlanModeRequest,
    responses(
        (status = 200, description = "Plan mode set", body = PlanActionResponse),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn set_plan_mode<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Json(body): Json<SetPlanModeRequest>,
) -> Response {
    let Some(manager) = state.chat_manager.as_ref() else {
        return unavailable();
    };
    match manager.set_plan_mode(id.clone(), body.enabled).await {
        Ok(()) if body.enabled => done(id, "plan_mode_on"),
        Ok(()) => done(id, "plan_mode_off"),
        Err(error) => chat_error_to_response(error).into_response(),
    }
}

/// Handler for `GET /api/v1/sessions/:id/plan`.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/{id}/plan",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    responses(
        (status = 200, description = "The plan, absent before the first one, and the session's plan phase", body = crate::chat::ChatPlanSnapshot),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_session_plan<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Response {
    let Some(manager) = state.chat_manager.as_ref() else {
        return unavailable();
    };
    match manager.get_plan(id).await {
        Ok(snapshot) => Json(snapshot).into_response(),
        Err(error) => chat_error_to_response(error).into_response(),
    }
}

/// Handler for `GET /api/v1/sessions/:id/plan/mutations`, oldest first.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/{id}/plan/mutations",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    responses(
        (status = 200, description = "The plan mutations in the order they were recorded, removals included", body = [Object]),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_plan_mutations<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Response {
    let Some(manager) = state.chat_manager.as_ref() else {
        return unavailable();
    };
    match manager.read_plan_mutations(id).await {
        Ok(mutations) => Json(mutations).into_response(),
        Err(error) => chat_error_to_response(error).into_response(),
    }
}

/// Handler for `POST /api/v1/sessions/:id/plan/approve`.
#[utoipa::path(
    post,
    path = "/api/v1/sessions/{id}/plan/approve",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    responses(
        (status = 200, description = "Plan approved, execution resumes", body = PlanActionResponse),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 409, description = "The session is not awaiting approval", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn approve_plan<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Response {
    let Some(manager) = state.chat_manager.as_ref() else {
        return unavailable();
    };
    match manager.approve_plan(&id).await {
        Ok(()) => done(id, "approved"),
        Err(error) => chat_error_to_response(error).into_response(),
    }
}

/// Handler for `POST /api/v1/sessions/:id/plan/reject`.
#[utoipa::path(
    post,
    path = "/api/v1/sessions/{id}/plan/reject",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    request_body = RejectPlanRequest,
    responses(
        (status = 200, description = "Plan rejected, the agent revises it", body = PlanActionResponse),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 409, description = "The session is not awaiting approval", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn reject_plan<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Json(body): Json<RejectPlanRequest>,
) -> Response {
    let Some(manager) = state.chat_manager.as_ref() else {
        return unavailable();
    };
    match manager.reject_plan(&id, body.reason).await {
        Ok(()) => done(id, "rejected"),
        Err(error) => chat_error_to_response(error).into_response(),
    }
}

#[cfg(test)]
mod tests;
