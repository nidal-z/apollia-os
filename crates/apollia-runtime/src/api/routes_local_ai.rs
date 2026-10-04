//! The installation of the local AI over HTTP: `GET /api/v1/local-ai/system`,
//! `GET /api/v1/local-ai/models/gguf` and `.../models/whisper`, and
//! `POST /api/v1/local-ai/llm` and `.../dictation`.
//!
//! These steps of the onboarding had no route: only the desktop running the
//! runtime in its own process could take them. The routes serve the functions
//! of [`crate::local_ai`] to any client of the daemon, the attached desktop
//! first. The folders scanned and the files chosen are those of the machine
//! the daemon runs on, which loads the models.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::local_ai::{
    self, DictationConfigured, GgufModelInfo, LocalAiError, SetupLlmResult, SystemInfo,
    WhisperModelInfo,
};
use crate::profile::ProfileStore;

type ApiError = (StatusCode, Json<ApiErrorBody>);

/// Request body for `POST /api/v1/local-ai/llm`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetupLlmBody {
    /// Absolute path of the GGUF file chosen.
    pub gguf_path: String,
    /// The context window chosen, in tokens; absent keeps the stored one.
    #[serde(default)]
    pub context_window: Option<u32>,
}

/// Request body for `POST /api/v1/local-ai/dictation`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetupDictationBody {
    /// Absolute path of the Whisper model file chosen.
    pub model_path: String,
    /// The language of the interface, stored only when none is set yet.
    #[serde(default)]
    pub language: Option<String>,
}

/// Build the local AI installation router.
pub fn local_ai_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/local-ai/system", get(get_local_ai_system::<B>))
        .route("/api/v1/local-ai/models/gguf", get(scan_gguf_models::<B>))
        .route(
            "/api/v1/local-ai/models/whisper",
            get(scan_whisper_models::<B>),
        )
        .route("/api/v1/local-ai/llm", post(setup_local_llm::<B>))
        .route("/api/v1/local-ai/dictation", post(setup_dictation::<B>))
}

fn refusal(error: LocalAiError) -> ApiError {
    let status = match &error {
        LocalAiError::OnboardingUnavailable => StatusCode::SERVICE_UNAVAILABLE,
        e if e.is_invalid_request() => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(ApiErrorBody {
            error: error.to_string(),
        }),
    )
}

/// The home folder the daemon scans beside its data directory.
fn home() -> std::path::PathBuf {
    apollia_core::paths::home_dir().unwrap_or_default()
}

/// `GET /api/v1/local-ai/system`: memory, system and accelerator of the
/// machine the daemon runs on.
#[utoipa::path(
    get,
    path = "/api/v1/local-ai/system",
    tag = "local_ai",
    responses(
        (status = 200, description = "What this machine can run", body = SystemInfo),
    )
)]
pub(crate) async fn get_local_ai_system<B: ExecutionBackend + Clone>(
    State(_state): State<AppState<B>>,
) -> Result<Json<SystemInfo>, ApiError> {
    local_ai::system_info().await.map(Json).map_err(refusal)
}

/// `GET /api/v1/local-ai/models/gguf`: the GGUF models already on disk,
/// largest first, each graded for this machine.
#[utoipa::path(
    get,
    path = "/api/v1/local-ai/models/gguf",
    tag = "local_ai",
    responses(
        (status = 200, description = "The GGUF files found", body = [GgufModelInfo]),
    )
)]
pub(crate) async fn scan_gguf_models<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<Vec<GgufModelInfo>>, ApiError> {
    local_ai::gguf_models(home(), state.data_dir.clone())
        .await
        .map(Json)
        .map_err(refusal)
}

/// `GET /api/v1/local-ai/models/whisper`: the Whisper models already on disk,
/// each graded for this machine.
#[utoipa::path(
    get,
    path = "/api/v1/local-ai/models/whisper",
    tag = "local_ai",
    responses(
        (status = 200, description = "The Whisper files found", body = [WhisperModelInfo]),
    )
)]
pub(crate) async fn scan_whisper_models<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<Vec<WhisperModelInfo>>, ApiError> {
    local_ai::whisper_models(home(), state.data_dir.clone())
        .await
        .map(Json)
        .map_err(refusal)
}

/// `POST /api/v1/local-ai/llm`: copies a GGUF file into the models folder and
/// makes it the `local` backend.
///
/// The copy runs in its own task, so a client that stops waiting does not
/// leave half a model in the folder.
#[utoipa::path(
    post,
    path = "/api/v1/local-ai/llm",
    tag = "local_ai",
    request_body = SetupLlmBody,
    responses(
        (status = 200, description = "Where the model is stored", body = SetupLlmResult),
        (status = 400, description = "No such file, or not a GGUF file", body = ApiErrorBody),
    )
)]
pub(crate) async fn setup_local_llm<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<SetupLlmBody>,
) -> Result<Json<SetupLlmResult>, ApiError> {
    let data_dir = state.data_dir.clone();
    let setup = tokio::spawn(async move {
        local_ai::setup_local_llm(&data_dir, &body.gguf_path, body.context_window).await
    });
    match setup.await {
        Ok(result) => result.map(Json).map_err(refusal),
        Err(source) => Err(refusal(LocalAiError::Task {
            what: "local model setup",
            source,
        })),
    }
}

/// `POST /api/v1/local-ai/dictation`: makes a Whisper file the dictation model
/// and marks the onboarding progress as dictation-ready.
#[utoipa::path(
    post,
    path = "/api/v1/local-ai/dictation",
    tag = "local_ai",
    request_body = SetupDictationBody,
    responses(
        (status = 200, description = "The dictation model stored", body = DictationConfigured),
        (status = 503, description = "The onboarding store did not open", body = ApiErrorBody),
    )
)]
pub(crate) async fn setup_dictation<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<SetupDictationBody>,
) -> Result<Json<DictationConfigured>, ApiError> {
    local_ai::setup_dictation(
        &state.data_dir,
        state.user_memory.as_ref().map(ProfileStore),
        body.model_path,
        body.language,
    )
    .await
    .map(Json)
    .map_err(refusal)
}

#[cfg(test)]
mod tests;
