//! `GET /api/v1/sessions/{id}/prompt-template`, the prompt template the model
//! of a conversation receives.
//!
//! Two places hold that text. The preset the conversation chose carries one
//! when its author wrote it. Otherwise the engine applies the template of the
//! model file, and only the engine can say which: the model index records
//! that a file has a template, not its text. The local `llama-server` answers
//! it on `GET /props`, field `chat_template`, for the process serving the
//! model.
//!
//! Nothing is loaded to answer: a model that is not resident, a model served
//! by another engine or by a remote server, and an engine that leaves the
//! field out all read as a template that cannot be shown.

use std::path::{Path as FsPath, PathBuf};
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_core::{LlmBackendError, LlmBackendRepository, LlmProvider};
use apollia_llm::model_index::{ModelIndex, INDEX_FILE_NAME};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::chat::ChatError;
use crate::coordinator::ExecutionBackend;
use crate::llama_server::ModelKind;

type ApiError = (StatusCode, Json<ApiErrorBody>);

/// The key of a preset's load settings that holds a chat template.
const PRESET_TEMPLATE_KEY: &str = "chat_template";

/// How long the engine is given to answer its properties.
const PROPS_TIMEOUT: Duration = Duration::from_secs(2);

/// Most bytes read from the properties of the engine. They carry the
/// template and the default generation settings, a few tens of kilobytes.
const PROPS_MAX_BYTES: u64 = 2 * 1024 * 1024;

/// Where the template shown comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PromptTemplateSource {
    /// The preset the conversation chose carries it.
    Preset,
    /// The engine reports it for the process serving the model.
    Engine,
    /// The template is the one built into the model and cannot be read here:
    /// the model is not loaded, its engine does not report it, or it is
    /// served elsewhere.
    Unavailable,
}

/// Answer of `GET /api/v1/sessions/{id}/prompt-template`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, utoipa::ToSchema)]
pub struct SessionPromptTemplate {
    /// Where the template comes from.
    pub source: PromptTemplateSource,
    /// The template, `null` when `source` is `unavailable`.
    pub template: Option<String>,
}

impl SessionPromptTemplate {
    fn unavailable() -> Self {
        Self {
            source: PromptTemplateSource::Unavailable,
            template: None,
        }
    }
}

/// A template worth showing: text that is not blank.
fn shown(text: Option<&str>) -> Option<String> {
    text.filter(|t| !t.trim().is_empty()).map(str::to_owned)
}

/// The template the preset `preset` carries, `None` when the preset is gone
/// or carries none.
fn preset_template(
    repo: &LlmBackendRepository,
    preset: i64,
) -> Result<Option<String>, LlmBackendError> {
    Ok(repo.find_preset(preset)?.and_then(|p| {
        shown(
            p.load
                .get(PRESET_TEMPLATE_KEY)
                .and_then(serde_json::Value::as_str),
        )
    }))
}

/// The file the local `llama-server` loads for `model`, `None` naming the
/// default model of the machine. A model of another provider, and a name
/// nothing resolves, answer `None`.
fn local_model_path(
    repo: &LlmBackendRepository,
    models_dir: &FsPath,
    model: Option<&str>,
) -> Result<Option<PathBuf>, LlmBackendError> {
    let name = match model {
        Some(name) => name.to_owned(),
        None => match repo.find_default()? {
            Some(row) => row.name,
            None => return Ok(None),
        },
    };
    Ok(match repo.find_by_name(&name)? {
        Some(row) if row.provider == LlmProvider::LlamaCpp => Some(PathBuf::from(row.model)),
        Some(_) => None,
        None => ModelIndex::load(&models_dir.join(INDEX_FILE_NAME))
            .ok()
            .and_then(|index| index.get(&name).map(|m| m.path.clone())),
    })
}

/// The template the `llama-server` listening on `port` applies, read from its
/// properties. `None` when it does not answer in time, answers an error, or
/// leaves the field out.
pub(crate) async fn engine_template(port: u16) -> Option<String> {
    let client = apollia_core::net::loopback_client_builder()
        .timeout(PROPS_TIMEOUT)
        .build()
        .ok()?;
    let response = client
        .get(format!("http://127.0.0.1:{port}/props"))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let props: serde_json::Value = apollia_core::net::read_capped_json(response, PROPS_MAX_BYTES)
        .await
        .ok()?;
    shown(
        props
            .get(PRESET_TEMPLATE_KEY)
            .and_then(serde_json::Value::as_str),
    )
}

fn failure(status: StatusCode, error: impl std::fmt::Display) -> ApiError {
    (
        status,
        Json(ApiErrorBody {
            error: error.to_string(),
        }),
    )
}

/// Handler for `GET /api/v1/sessions/{id}/prompt-template`, the prompt
/// template the model of a session receives.
///
/// The template of the preset the session chose when that preset carries
/// one, else the template the local engine reports for the model of the
/// session when it is loaded. Reading it never loads a model.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/{id}/prompt-template",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    responses(
        (status = 200, description = "The prompt template of the session and where it comes from", body = SessionPromptTemplate),
        (status = 404, description = "Session not found", body = ApiErrorBody),
        (status = 503, description = "Chat subsystem or backend repository unavailable", body = ApiErrorBody),
    )
)]
pub async fn get_session_prompt_template<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<SessionPromptTemplate>, ApiError> {
    let manager = state.chat_manager.as_ref().ok_or_else(|| {
        failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "chat subsystem not available",
        )
    })?;
    let detail = manager
        .get_session(id.clone())
        .await
        .ok_or_else(|| failure(StatusCode::NOT_FOUND, format!("session not found: {id}")))?;
    let model = manager.session_model(id).await.map_err(|e| match e {
        ChatError::SessionNotFound(_) => failure(StatusCode::NOT_FOUND, e),
        other => failure(StatusCode::INTERNAL_SERVER_ERROR, other),
    })?;
    let repo = state.llm_backend_repo.as_ref().ok_or_else(|| {
        failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "LLM backend repository not available",
        )
    })?;
    let models_dir = crate::installed_models::models_dir(&state.data_dir);
    let (from_preset, path) = {
        let repo = repo.lock().map_err(|_| {
            failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "repository lock poisoned",
            )
        })?;
        let from_preset = match detail.session.settings.preset {
            Some(preset) => preset_template(&repo, preset)
                .map_err(|e| failure(StatusCode::INTERNAL_SERVER_ERROR, e))?,
            None => None,
        };
        let path = match from_preset {
            Some(_) => None,
            None => local_model_path(&repo, &models_dir, model.as_deref())
                .map_err(|e| failure(StatusCode::INTERNAL_SERVER_ERROR, e))?,
        };
        (from_preset, path)
    };
    if let Some(template) = from_preset {
        return Ok(Json(SessionPromptTemplate {
            source: PromptTemplateSource::Preset,
            template: Some(template),
        }));
    }
    let port = match (path, state.llama_server_supervisor.as_deref()) {
        (Some(path), Some(supervisor)) => {
            let path = path.to_string_lossy();
            supervisor
                .loaded_models()
                .await
                .into_iter()
                .find(|m| m.kind == ModelKind::Chat && m.model_path == path)
                .map(|m| m.port)
        }
        _ => None,
    };
    let answer = match port {
        Some(port) => match engine_template(port).await {
            Some(template) => SessionPromptTemplate {
                source: PromptTemplateSource::Engine,
                template: Some(template),
            },
            None => SessionPromptTemplate::unavailable(),
        },
        None => SessionPromptTemplate::unavailable(),
    };
    tracing::event!(
        tracing::Level::DEBUG,
        source = ?answer.source,
        loaded = port.is_some(),
        "chat.session.prompt_template.read"
    );
    Ok(Json(answer))
}

#[cfg(test)]
mod tests;
