//! The in-app companion over HTTP: `GET` and `PUT /api/v1/companion/enabled`,
//! and `POST /api/v1/companion/sessions`.
//!
//! The companion had no route: only the desktop running the runtime in its own
//! process could read its preference or open the guide session. These routes
//! serve the functions of [`crate::companion`] to any client of the daemon,
//! the attached desktop first.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::companion::{create_session, read_enabled, write_enabled, CompanionError};
use crate::coordinator::ExecutionBackend;
use crate::profile::ProfileStore;

type ApiError = (StatusCode, Json<ApiErrorBody>);

/// Body of `PUT /api/v1/companion/enabled`, and answer of both verbs.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct CompanionEnabled {
    /// Whether the companion panel is on.
    pub enabled: bool,
}

/// Body of `POST /api/v1/companion/sessions`.
#[derive(Debug, Default, Deserialize, Serialize, utoipa::ToSchema)]
pub struct CreateCompanionSessionBody {
    /// The route the panel opened on, logged with the session.
    #[serde(default)]
    pub context: Option<String>,
}

/// Answer of `POST /api/v1/companion/sessions`.
#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
pub struct CompanionSession {
    /// Identifier of the guide session just opened.
    pub session_id: String,
}

/// Build the companion router.
pub fn companion_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/companion/enabled",
            get(get_companion_enabled::<B>).put(set_companion_enabled::<B>),
        )
        .route(
            "/api/v1/companion/sessions",
            post(create_companion_session::<B>),
        )
}

fn refusal(error: CompanionError) -> ApiError {
    let status = match error {
        CompanionError::MemoryUnavailable | CompanionError::ChatUnavailable => {
            StatusCode::SERVICE_UNAVAILABLE
        }
        CompanionError::GuideNotInstalled => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(ApiErrorBody {
            error: error.to_string(),
        }),
    )
}

/// `GET /api/v1/companion/enabled`: whether the companion is on.
#[utoipa::path(
    get,
    path = "/api/v1/companion/enabled",
    tag = "companion",
    responses(
        (status = 200, description = "The preference, off when never written", body = CompanionEnabled),
        (status = 503, description = "The user memory did not open", body = ApiErrorBody),
    )
)]
pub(crate) async fn get_companion_enabled<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<CompanionEnabled>, ApiError> {
    let enabled = read_enabled(state.user_memory.as_ref().map(ProfileStore))
        .await
        .map_err(refusal)?;
    Ok(Json(CompanionEnabled { enabled }))
}

/// `PUT /api/v1/companion/enabled`: turns the companion on or off.
#[utoipa::path(
    put,
    path = "/api/v1/companion/enabled",
    tag = "companion",
    request_body = CompanionEnabled,
    responses(
        (status = 200, description = "The preference as stored", body = CompanionEnabled),
        (status = 503, description = "The user memory did not open", body = ApiErrorBody),
    )
)]
pub(crate) async fn set_companion_enabled<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<CompanionEnabled>,
) -> Result<Json<CompanionEnabled>, ApiError> {
    write_enabled(state.user_memory.as_ref().map(ProfileStore), body.enabled)
        .await
        .map_err(refusal)?;
    Ok(Json(body))
}

/// `POST /api/v1/companion/sessions`: opens a session on the guide.
#[utoipa::path(
    post,
    path = "/api/v1/companion/sessions",
    tag = "companion",
    request_body = CreateCompanionSessionBody,
    responses(
        (status = 201, description = "The guide session opened", body = CompanionSession),
        (status = 404, description = "The guide is not installed", body = ApiErrorBody),
        (status = 503, description = "The chat subsystem did not start", body = ApiErrorBody),
        (status = 500, description = "The chat subsystem refused the session", body = ApiErrorBody),
    )
)]
pub(crate) async fn create_companion_session<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<CreateCompanionSessionBody>,
) -> Result<(StatusCode, Json<CompanionSession>), ApiError> {
    let session_id = create_session(
        state.chat_manager.as_ref(),
        state.user_memory.as_ref().map(ProfileStore),
        body.context,
    )
    .await
    .map_err(refusal)?;
    Ok((StatusCode::CREATED, Json(CompanionSession { session_id })))
}

#[cfg(test)]
mod tests;
