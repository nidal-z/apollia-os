//! Model cards and READMEs.
//!
//! - `GET /api/v1/llm/registry/cards?lang=`  , the catalogue opens: the signed
//!   index is read (main address, mirror, kept copy) and every card is
//!   returned in one language
//! - `GET /api/v1/llm/registry/card?id=&lang=`, one card, from the kept copy
//!   only, so it answers offline once the catalogue has been opened
//! - `GET /api/v1/llm/registry/readme?repo=`  , the README of a repository, as
//!   text, read on demand and kept nowhere
//!
//! The registry address is the `[models]` section of `apollia.toml`, read when
//! the catalogue opens, with the registry settings the organisation policy
//! fixes laid over it: a policy that replaces or cuts the registry wins over
//! the file, and a catalogue it cut says so.

use std::path::{Path, PathBuf};

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_llm::model_cards::fetch::{open_catalogue, RegistrySource};
use apollia_llm::model_cards::readme::{fetch_readme, ReadmeError, HUGGINGFACE_BASE};
use apollia_llm::model_cards::{
    cached_index, compiled_public_key, default_cache_dir, LocalizedCard, PublisherLists,
};

use super::ErrorResponse;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Query for the card routes.
#[derive(Debug, Deserialize)]
pub struct CardsQuery {
    /// Language of the texts: `fr` or `en`. English when absent or unknown.
    pub lang: Option<String>,
}

/// Query for `GET /api/v1/llm/registry/card`.
#[derive(Debug, Deserialize)]
pub struct CardQuery {
    /// Identifier of the card, for example `qwen3/Qwen3-8B`.
    pub id: String,
    /// Language of the texts: `fr` or `en`. English when absent or unknown.
    pub lang: Option<String>,
}

/// Query for `GET /api/v1/llm/registry/readme`.
#[derive(Debug, Deserialize)]
pub struct ReadmeQuery {
    /// The repository, `owner/name`.
    pub repo: String,
}

/// The catalogue's cards, or why there are none.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ModelCardsResponse {
    /// `ok`, `offline` (nothing answered and no copy is kept), `disabled`
    /// (turned off by configuration or policy) or `no_trusted_key` (this build
    /// carries no distribution key).
    pub status: String,
    /// Where the index came from when `status` is `ok`: `primary`, `mirror`
    /// or `cache`.
    pub origin: Option<String>,
    /// A sentence for the user when the catalogue is empty.
    pub message: Option<String>,
    /// Every card, in the asked language.
    #[schema(value_type = Vec<Object>)]
    pub cards: Vec<LocalizedCard>,
    /// The official publishers and the recognised converters.
    #[schema(value_type = Object)]
    pub publishers: PublisherLists,
}

/// A README, as text.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ReadmeResponse {
    /// The repository, `owner/name`.
    pub repo: String,
    /// Its owner.
    pub author: String,
    /// The licence its front matter declares.
    pub license: Option<String>,
    /// The page of the repository.
    pub source_url: String,
    /// The text.
    pub text: String,
}

type RouteError = (StatusCode, Json<ErrorResponse>);

fn error(status: StatusCode, message: impl Into<String>) -> RouteError {
    (
        status,
        Json(ErrorResponse {
            error: message.into(),
        }),
    )
}

/// The registry source `apollia.toml` configures: the file the runtime was
/// started with, else the one in its data directory, under the settings the
/// organisation policy fixes.
pub(crate) fn configured_source(
    config_path: Option<&Path>,
    data_dir: &Path,
    imposed: &apollia_core::config::PolicySettings,
) -> RegistrySource {
    let path = config_path.map_or_else(
        || data_dir.join(apollia_core::paths::CONFIG_FILE_NAME),
        Path::to_path_buf,
    );
    RegistrySource::from_config_file(&path).under_policy(imposed)
}

/// The body of the cards route, for a source, a cache and a key.
pub(crate) async fn cards_body(
    source: &RegistrySource,
    cache_dir: &Path,
    public_key: Option<&str>,
    lang: &str,
) -> Result<ModelCardsResponse, RouteError> {
    let view = open_catalogue(source, cache_dir, public_key, lang)
        .await
        .map_err(|e| error(StatusCode::BAD_GATEWAY, e.to_string()))?;
    let origin = view
        .origin
        .and_then(|o| serde_json::to_value(o).ok())
        .and_then(|v| v.as_str().map(str::to_owned));
    Ok(ModelCardsResponse {
        status: view.status,
        origin,
        message: view.message,
        cards: view.cards,
        publishers: view.publishers,
    })
}

/// `GET /api/v1/llm/registry/cards?lang=...`
#[utoipa::path(
    get,
    path = "/api/v1/llm/registry/cards",
    tag = "model_hub",
    params(
        ("lang" = Option<String>, Query, description = "Language of the texts, fr or en; English when absent"),
    ),
    responses(
        (status = 200, description = "The cards of the catalogue, or an empty catalogue with the reason", body = ModelCardsResponse),
        (status = 502, description = "The index was refused: its signature does not hold or it is not an index", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_model_cards<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(params): Query<CardsQuery>,
) -> Result<Json<ModelCardsResponse>, RouteError> {
    let source = configured_source(
        state.config_path.as_deref(),
        &state.data_dir,
        &state.inference_governance.imposed_settings(),
    );
    let cache_dir = default_cache_dir(&state.data_dir);
    let lang = params.lang.unwrap_or_default();
    cards_body(&source, &cache_dir, compiled_public_key(), &lang)
        .await
        .map(Json)
}

/// The body of the single card route, read from the kept copy only.
pub(crate) fn card_body(
    cache_dir: &Path,
    public_key: Option<&str>,
    id: &str,
    lang: &str,
) -> Result<LocalizedCard, RouteError> {
    let index = cached_index(cache_dir, public_key)
        .map_err(|e| error(StatusCode::BAD_GATEWAY, e.to_string()))?
        .ok_or_else(|| {
            error(
                StatusCode::NOT_FOUND,
                "no model catalogue is kept yet; open the catalogue first",
            )
        })?;
    index
        .card(id)
        .map(|card| card.localized(lang))
        .ok_or_else(|| error(StatusCode::NOT_FOUND, format!("no card {id}")))
}

/// `GET /api/v1/llm/registry/card?id=...&lang=...`
#[utoipa::path(
    get,
    path = "/api/v1/llm/registry/card",
    tag = "model_hub",
    params(
        ("id" = String, Query, description = "Identifier of the card, for example qwen3/Qwen3-8B"),
        ("lang" = Option<String>, Query, description = "Language of the texts, fr or en; English when absent"),
    ),
    responses(
        (status = 200, description = "The card in the asked language, English when that language is not written", body = Object),
        (status = 404, description = "No kept catalogue, or no card with that identifier", body = crate::api::openapi::ApiErrorBody),
        (status = 502, description = "The kept copy was refused", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_model_card<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(params): Query<CardQuery>,
) -> Result<Json<LocalizedCard>, RouteError> {
    let cache_dir: PathBuf = default_cache_dir(&state.data_dir);
    let lang = params.lang.unwrap_or_default();
    card_body(&cache_dir, compiled_public_key(), &params.id, &lang).map(Json)
}

/// The body of the README route, for a hub address.
pub(crate) async fn readme_body(base: &str, repo: &str) -> Result<ReadmeResponse, RouteError> {
    let readme = fetch_readme(base, repo).await.map_err(|e| {
        let status = match &e {
            ReadmeError::InvalidRepo(_) => StatusCode::BAD_REQUEST,
            ReadmeError::Offline => StatusCode::SERVICE_UNAVAILABLE,
            ReadmeError::NotFound(_) => StatusCode::NOT_FOUND,
            ReadmeError::TooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            _ => StatusCode::BAD_GATEWAY,
        };
        error(status, e.to_string())
    })?;
    Ok(ReadmeResponse {
        repo: readme.repo,
        author: readme.author,
        license: readme.license,
        source_url: readme.source_url,
        text: readme.text,
    })
}

/// `GET /api/v1/llm/registry/readme?repo=...`
#[utoipa::path(
    get,
    path = "/api/v1/llm/registry/readme",
    tag = "model_hub",
    params(
        ("repo" = String, Query, description = "The repository, owner/name"),
    ),
    responses(
        (status = 200, description = "The README as text, with the repository, its owner and its licence", body = ReadmeResponse),
        (status = 400, description = "The repository identifier is not owner/name", body = crate::api::openapi::ApiErrorBody),
        (status = 404, description = "The repository has no README", body = crate::api::openapi::ApiErrorBody),
        (status = 413, description = "The README is larger than 512 KiB", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "README unavailable offline", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_model_readme<B: ExecutionBackend + Clone>(
    State(_state): State<AppState<B>>,
    Query(params): Query<ReadmeQuery>,
) -> Result<Json<ReadmeResponse>, RouteError> {
    readme_body(HUGGINGFACE_BASE, &params.repo).await.map(Json)
}

#[cfg(test)]
mod tests;
