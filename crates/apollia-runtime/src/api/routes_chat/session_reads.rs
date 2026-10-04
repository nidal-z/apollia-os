//! Reads of a chat session the desktop needs in both of its modes:
//! `GET /api/v1/sessions/{id}/metrics`, the metrics panel,
//! `GET /api/v1/sessions/{id}/workspace`, the folder a relative path named in
//! the conversation is revealed against, and
//! `GET /api/v1/sessions/{id}/model-capabilities`, what the model of the
//! conversation lets the composer offer (a reasoning setting, images), and
//! `GET /api/v1/sessions/{id}/images/{file}`, an image a message of the
//! conversation carries, for its thumbnail.
//!
//! The first two had no route: only the desktop running the runtime in its own
//! process could read them. They answer what the embedded desktop answers,
//! through the same methods of the chat manager.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_llm::ReasoningProfile;

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::chat::manager::ChatSessionManagerHandle;
use crate::chat::{ChatError, SessionMetrics};
use crate::coordinator::ExecutionBackend;
use crate::local_engine::{model_capabilities, ModelCapabilities};

type ApiError = (StatusCode, Json<ApiErrorBody>);

/// The reasoning setting a model lets its caller fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningProfileDto {
    /// No setting: the model reasons or not on its own.
    None,
    /// Reasoning on or off (`enable_thinking`).
    Toggle,
    /// A level among low, medium and high (`reasoning_effort`).
    Levels,
}

impl From<ReasoningProfile> for ReasoningProfileDto {
    fn from(profile: ReasoningProfile) -> Self {
        match profile {
            ReasoningProfile::None => Self::None,
            ReasoningProfile::Toggle => Self::Toggle,
            ReasoningProfile::Levels => Self::Levels,
        }
    }
}

/// Answer of `GET /api/v1/sessions/{id}/model-capabilities`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct SessionModelCapabilities {
    /// The backend or model the conversation runs on, the default model of
    /// the machine when it names none; `null` when no model is configured.
    pub model: Option<String>,
    /// Whether a local engine serves the model.
    pub local: bool,
    /// The reasoning setting the model offers; `null` when it is not known
    /// (a remote server, a model not indexed yet), in which case no setting is
    /// offered.
    pub reasoning_profile: Option<ReasoningProfileDto>,
    /// Whether the model reads images.
    pub vision: bool,
    /// The image formats its engine decodes, empty without vision.
    pub image_formats: Vec<String>,
}

impl From<ModelCapabilities> for SessionModelCapabilities {
    fn from(found: ModelCapabilities) -> Self {
        let caps = found.capabilities.unwrap_or_default();
        Self {
            model: found.model,
            local: found.local,
            reasoning_profile: caps.reasoning_profile.map(Into::into),
            vision: caps.vision,
            image_formats: caps.image_formats,
        }
    }
}

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

/// Handler for `GET /api/v1/sessions/{id}/model-capabilities`, what the
/// model of a session can do.
///
/// The model is the one the next message of the session would run on: the
/// model of its declarative agent, else the backend the session prefers, else
/// the default model of the machine.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/{id}/model-capabilities",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    responses(
        (status = 200, description = "The model of the session and what it can do", body = SessionModelCapabilities),
        (status = 404, description = "Session not found", body = ApiErrorBody),
        (status = 503, description = "Chat subsystem or backend repository unavailable", body = ApiErrorBody),
    )
)]
pub async fn get_session_model_capabilities<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<SessionModelCapabilities>, ApiError> {
    let manager = chat_manager(&state)?;
    let model = manager.session_model(id).await.map_err(|e| match e {
        ChatError::SessionNotFound(_) => failure(StatusCode::NOT_FOUND, e.to_string()),
        other => failure(StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
    })?;
    let models_dir = crate::installed_models::models_dir(&state.data_dir);
    let repo = state.llm_backend_repo.as_ref().ok_or_else(|| {
        failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "LLM backend repository not available".to_owned(),
        )
    })?;
    let found = {
        let repo = repo.lock().map_err(|_| {
            failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "repository lock poisoned".to_owned(),
            )
        })?;
        model_capabilities(&repo, &models_dir, model.as_deref())
            .map_err(|e| failure(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };
    tracing::event!(
        tracing::Level::DEBUG,
        model = ?found.model,
        local = found.local,
        known = found.capabilities.is_some(),
        "chat.session.model_capabilities.read"
    );
    Ok(Json(found.into()))
}

/// Handler for `GET /api/v1/sessions/{id}/images/{file}`, an image a user
/// message of the session carries, as its metadata names it.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/{id}/images/{file}",
    tag = "chat",
    params(
        ("id" = String, Path, description = "Session id"),
        ("file" = String, Path, description = "File name of the image, as the message metadata lists it"),
    ),
    responses(
        (status = 200, description = "The image, its media type and its bytes in base64", body = crate::chat::images::ImageAttachment),
        (status = 400, description = "Not an image file name", body = ApiErrorBody),
        (status = 404, description = "No such image in the session", body = ApiErrorBody),
    )
)]
pub async fn get_session_image<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path((id, file)): Path<(String, String)>,
) -> Result<Json<crate::chat::images::ImageAttachment>, ApiError> {
    crate::chat::images::read_session_image(&state.data_dir, &id, &file)
        .map(Json)
        .map_err(|e| match e {
            ChatError::ImageRejected(_) => failure(StatusCode::BAD_REQUEST, e.to_string()),
            other => failure(StatusCode::NOT_FOUND, other.to_string()),
        })
}

fn failure(status: StatusCode, error: String) -> ApiError {
    (status, Json(ApiErrorBody { error }))
}

#[cfg(test)]
mod tests;
