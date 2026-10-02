//! `GET /api/v1/llm/embeddings`: the embeddings models of the registry, and
//! the one this machine should get.
//!
//! Answered from the table embedded in the binary and from the hardware
//! detection, with no request to the Hub, so it answers on a machine with no
//! network at all.

use std::path::Path;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_llm::hardware::HardwareProfile;
use apollia_llm::model_index::INDEX_FILE_NAME;
use apollia_llm::recommend::{EmbeddingCatalogue, EmbeddingModel, EmbeddingReason};
use apollia_llm::{ModelCategory, ModelIndex};

use super::{ErrorResponse, HardwareResponse};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Query for `GET /api/v1/llm/embeddings`.
#[derive(Debug, Deserialize)]
pub struct EmbeddingsQuery {
    /// Size in bytes of the language model the embeddings model has to fit
    /// beside, for example the file the onboarding is about to download. When
    /// absent, the largest language model installed on this machine.
    pub language_model_bytes: Option<u64>,
}

/// The language model the recommendation was made beside.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct CountedLanguageModel {
    /// `query` when the caller gave its size, `installed` when it is the
    /// largest language model of the model index.
    pub source: String,
    /// Identifier of the installed model; absent for a size given by query.
    pub id: Option<String>,
    /// Size of its weights in bytes.
    pub size_bytes: u64,
}

/// The embeddings category of the registry, and the entry for this machine.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EmbeddingModelsResponse {
    /// Every entry of the category: repository, file, size, digest,
    /// dimensions, context, pooling and prefixes.
    #[schema(value_type = Vec<Object>)]
    pub models: Vec<EmbeddingModel>,
    /// Identifier of the entry recommended for this machine.
    pub recommended: Option<String>,
    /// Why: `fits_beside` or `no_room_beside`, with the footprints compared,
    /// in gibibytes.
    #[schema(value_type = Option<Object>)]
    pub reason: Option<EmbeddingReason>,
    /// The language model counted; absent when none was.
    pub language_model: Option<CountedLanguageModel>,
    /// The machine the recommendation was made for.
    pub hardware: HardwareResponse,
}

/// `GET /api/v1/llm/embeddings?language_model_bytes=...`
#[utoipa::path(
    get,
    path = "/api/v1/llm/embeddings",
    tag = "model_hub",
    params(
        ("language_model_bytes" = Option<u64>, Query, description = "Size of the language model to fit beside; the largest installed one when absent"),
    ),
    responses(
        (status = 200, description = "The embeddings models and the one recommended for this machine", body = EmbeddingModelsResponse),
        (status = 500, description = "The shipped registry table is invalid", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_embedding_models<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(params): Query<EmbeddingsQuery>,
) -> Result<Json<EmbeddingModelsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let profile = tokio::task::spawn_blocking(apollia_llm::hardware::detect)
        .await
        .unwrap_or_else(|_| apollia_llm::hardware::detect());
    let models_dir = state.data_dir.join("models");
    let language = match params.language_model_bytes {
        Some(size_bytes) => Some(CountedLanguageModel {
            source: "query".to_owned(),
            id: None,
            size_bytes,
        }),
        None => tokio::task::spawn_blocking(move || installed_language_model(&models_dir))
            .await
            .unwrap_or(None),
    };
    embeddings_body(profile, language)
        .map(Json)
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error }),
            )
        })
}

/// The response for `profile`, the recommendation made beside `language`.
pub(crate) fn embeddings_body(
    profile: HardwareProfile,
    language: Option<CountedLanguageModel>,
) -> Result<EmbeddingModelsResponse, String> {
    let catalogue = EmbeddingCatalogue::embedded()
        .map_err(|e| format!("the shipped embeddings table is invalid: {e}"))?;
    let recommendation = catalogue.recommend(&profile, language.as_ref().map(|l| l.size_bytes));
    let (recommended, reason) = match recommendation {
        Some(r) => (Some(r.model), Some(r.reason)),
        None => (None, None),
    };
    Ok(EmbeddingModelsResponse {
        models: catalogue.models().to_vec(),
        recommended,
        reason,
        language_model: language,
        hardware: HardwareResponse::from(profile),
    })
}

/// The largest language model of the index under `models_dir`, read as it was
/// last written. `None` when there is no index or no language model in it.
pub(crate) fn installed_language_model(models_dir: &Path) -> Option<CountedLanguageModel> {
    let index = match ModelIndex::load(&models_dir.join(INDEX_FILE_NAME)) {
        Ok(index) => index,
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                path = %models_dir.display(),
                error = %e,
                "llm.embeddings.index.unreadable"
            );
            return None;
        }
    };
    index
        .models
        .iter()
        .filter(|m| m.effective_category() == ModelCategory::Language)
        .max_by_key(|m| m.size_bytes)
        .map(|m| CountedLanguageModel {
            source: "installed".to_owned(),
            id: Some(m.id.clone()),
            size_bytes: m.size_bytes,
        })
}

#[cfg(test)]
mod tests;
