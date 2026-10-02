//! `GET /api/v1/llm/dictation`: the dictation models of the registry, whether
//! each fits this machine, and the one this machine should get.
//!
//! Answered from the table embedded in the binary and from the hardware
//! detection, with no request to the Hub, so it answers on a machine with no
//! network at all.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_llm::hardware::HardwareProfile;
use apollia_llm::recommend::{dictation_fits, DictationCatalogue, DictationModel, DictationReason};

use super::embeddings::{installed_language_model, CountedLanguageModel};
use super::{ErrorResponse, HardwareResponse};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Query for `GET /api/v1/llm/dictation`.
#[derive(Debug, Deserialize)]
pub struct DictationQuery {
    /// Size in bytes of the language model the dictation model has to fit
    /// beside. When absent, the largest language model installed here.
    pub language_model_bytes: Option<u64>,
}

/// One entry of the category, with the verdict for this machine.
#[derive(Debug, Clone, Serialize)]
pub struct DictationEntry {
    /// Repository, file, quantisation, size, digest, card and source.
    #[serde(flatten)]
    pub model: DictationModel,
    /// Its footprint, the file plus the fixed margin, in gibibytes.
    pub footprint_gb: f64,
    /// Whether that footprint fits this machine on its own.
    pub fits: bool,
}

/// The dictation category of the registry, and the entry for this machine.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DictationModelsResponse {
    /// Every entry of the category, with its footprint and whether it fits.
    #[schema(value_type = Vec<Object>)]
    pub models: Vec<DictationEntry>,
    /// Identifier of the entry recommended for this machine.
    pub recommended: Option<String>,
    /// Why: `fits_beside` or `no_room_beside`, with the footprints compared,
    /// in gibibytes.
    #[schema(value_type = Option<Object>)]
    pub reason: Option<DictationReason>,
    /// The language model counted; absent when none was.
    pub language_model: Option<CountedLanguageModel>,
    /// The machine the recommendation was made for.
    pub hardware: HardwareResponse,
}

/// `GET /api/v1/llm/dictation?language_model_bytes=...`
#[utoipa::path(
    get,
    path = "/api/v1/llm/dictation",
    tag = "model_hub",
    params(
        ("language_model_bytes" = Option<u64>, Query, description = "Size of the language model to fit beside; the largest installed one when absent"),
    ),
    responses(
        (status = 200, description = "The dictation models, whether each fits, and the one recommended for this machine", body = DictationModelsResponse),
        (status = 500, description = "The shipped registry table is invalid", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_dictation_models<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(params): Query<DictationQuery>,
) -> Result<Json<DictationModelsResponse>, (StatusCode, Json<ErrorResponse>)> {
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
    dictation_body(profile, language)
        .map(Json)
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error }),
            )
        })
}

/// The response for `profile`, the recommendation made beside `language`.
pub(crate) fn dictation_body(
    profile: HardwareProfile,
    language: Option<CountedLanguageModel>,
) -> Result<DictationModelsResponse, String> {
    let catalogue = DictationCatalogue::embedded()
        .map_err(|e| format!("the shipped dictation table is invalid: {e}"))?;
    let recommendation = catalogue.recommend(&profile, language.as_ref().map(|l| l.size_bytes));
    let (recommended, reason) = match recommendation {
        Some(r) => (Some(r.model), Some(r.reason)),
        None => (None, None),
    };
    let models = catalogue
        .models()
        .iter()
        .map(|model| DictationEntry {
            footprint_gb: model.footprint_gb(),
            fits: dictation_fits(model.size_bytes, &profile),
            model: model.clone(),
        })
        .collect();
    Ok(DictationModelsResponse {
        models,
        recommended,
        reason,
        language_model: language,
        hardware: HardwareResponse::from(profile),
    })
}

#[cfg(test)]
mod tests;
