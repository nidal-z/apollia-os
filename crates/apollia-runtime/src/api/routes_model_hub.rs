//! Model Hub routes, hardware detection, HF registry search.
//!
//! - `GET /api/v1/llm/hardware`                 , hardware profile (RAM, CPU, GPU)
//! - `GET /api/v1/llm/hardware/usage`           , memory, processor and video memory in use
//! - `GET /api/v1/llm/registry/search`          , search the official publishers and the recognised converters
//! - `GET /api/v1/llm/registry/model/:org/:repo`, model metadata + file list
//! - `GET /api/v1/llm/recommend`                 , models ranked for this machine
//! - `GET /api/v1/llm/recommendations`           , the recommendation outcome the desktop shows
//! - `GET /api/v1/llm/embeddings`                , embeddings models, and the one for this machine
//! - `GET|PUT /api/v1/llm/embeddings/default`    , the embeddings model search uses
//! - `GET /api/v1/llm/dictation`                 , dictation models, whether each fits, and the one for this machine
//! - `GET /api/v1/llm/registry/cards`            , the signed model cards, when the catalogue opens
//! - `GET /api/v1/llm/registry/card`             , one model card, from the kept copy
//! - `GET /api/v1/llm/registry/readme`           , the README of a repository, as text
//! - `GET /api/v1/llm/installed-models`          , the models installed on this machine
//! - `DELETE /api/v1/llm/installed-models`       , removes an installed model, unless in use

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use apollia_llm::hardware::HardwareProfile;
use apollia_llm::AcceleratorProfile;

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

#[cfg(feature = "cloud")]
pub mod cards;
pub mod dictation;
pub mod embedding_default;
pub mod embeddings;
pub mod installed;
pub mod usage;

// ─────────────────────────────────────────────
// Response types
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct HardwareResponse {
    pub total_ram_gb: f64,
    pub available_ram_gb: f64,
    pub cpu_model: String,
    pub cpu_cores: u32,
    #[schema(value_type = Object)]
    pub accelerator: AcceleratorProfile,
    pub memory_budget_gb: f64,
    /// What a model can occupy on this machine, in GB rounded to the tenth:
    /// the card plus the usable share of system memory on a discrete
    /// accelerator, the memory budget otherwise. A model under it is one the
    /// estimator does not call too large.
    pub total_capacity_gb: f64,
}

impl From<HardwareProfile> for HardwareResponse {
    fn from(p: HardwareProfile) -> Self {
        let total_capacity_gb = apollia_llm::recommend::estimated_capacity_gb(&p);
        Self {
            total_capacity_gb,
            total_ram_gb: p.total_ram_gb,
            available_ram_gb: p.available_ram_gb,
            cpu_model: p.cpu_model,
            cpu_cores: p.cpu_cores,
            accelerator: p.accelerator,
            memory_budget_gb: p.memory_budget_gb,
        }
    }
}

/// Query for `GET /api/v1/llm/registry/search`.
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Words of a model name; never an owner, the organisations are the lists'.
    pub q: Option<String>,
    /// `gguf` (default) or `mlx`, the latter on Apple Silicon only.
    pub format: Option<apollia_llm::SearchFormat>,
    pub limit: Option<u32>,
    /// `best_match` (default), `likes`, `downloads` or `updated`.
    pub sort: Option<String>,
    /// Keep only the results one of whose variants is recommended here.
    #[serde(default)]
    pub fits_only: bool,
    /// Optional HuggingFace token for gated models (passed as query param).
    pub hf_token: Option<String>,
}

/// Query for `GET /api/v1/llm/registry/model/:org/:repo`.
#[derive(Debug, Deserialize)]
pub struct ModelQuery {
    /// Optional HuggingFace token for gated models (passed as query param).
    pub hf_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// Query for `GET /api/v1/llm/recommend`.
#[derive(Debug, Deserialize)]
pub struct RecommendQuery {
    /// Maximum number of recommendations to return.
    pub limit: Option<usize>,
    /// Context window the key/value cache is sized against. Defaults to what
    /// the runtime itself launches `llama-server` with.
    pub n_ctx: Option<u32>,
    /// HuggingFace token, for gated repositories.
    pub hf_token: Option<String>,
}

// ─────────────────────────────────────────────
// Handlers
// ─────────────────────────────────────────────

/// `GET /api/v1/llm/hardware`, detect and return the hardware profile.
#[utoipa::path(
    get,
    path = "/api/v1/llm/hardware",
    tag = "model_hub",
    responses(
        (status = 200, description = "Detected hardware profile", body = HardwareResponse),
    )
)]
pub async fn get_hardware<B: ExecutionBackend + Clone>(
    State(_state): State<AppState<B>>,
) -> Json<HardwareResponse> {
    let profile = tokio::task::spawn_blocking(apollia_llm::hardware::detect)
        .await
        .unwrap_or_else(|_| apollia_llm::hardware::detect());

    Json(HardwareResponse::from(profile))
}

/// `GET /api/v1/llm/registry/search?q=...&format=...&sort=...&fits_only=...`
///
/// Asks each organisation of the two publisher lists, by its `author=`, and
/// nothing else. Answers `503` with `search is unavailable offline` when no
/// organisation can be reached, never an empty list.
#[cfg(feature = "cloud")]
#[utoipa::path(
    get,
    path = "/api/v1/llm/registry/search",
    tag = "model_hub",
    params(
        ("q" = Option<String>, Query, description = "Words of a model name, searched within the official publishers and the recognised converters only"),
        ("format" = Option<String>, Query, description = "gguf (default) or mlx; mlx only on Apple Silicon"),
        ("limit" = Option<u32>, Query, description = "Maximum number of results"),
        ("sort" = Option<String>, Query, description = "best_match (default), likes, downloads or updated"),
        ("fits_only" = Option<bool>, Query, description = "Keep only the results with a variant recommended for this machine"),
        ("hf_token" = Option<String>, Query, description = "HuggingFace token for gated models"),
    ),
    responses(
        (status = 200, description = "Results with their generated cards, and the estimated capacity of this machine"),
        (status = 400, description = "Unknown sort, or MLX asked off Apple Silicon", body = crate::api::openapi::ApiErrorBody),
        (status = 502, description = "Upstream HuggingFace error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Search unavailable offline", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn search_registry<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let hardware = tokio::task::spawn_blocking(apollia_llm::hardware::detect)
        .await
        .ok();
    let cache_dir = apollia_llm::model_cards::default_cache_dir(&state.data_dir);
    let inputs = tokio::task::spawn_blocking(move || apollia_llm::SearchInputs::load(&cache_dir))
        .await
        .map_err(|e| search_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let client = apollia_llm::HfRegistryClient::new(params.hf_token.clone());
    search_body(&client, &params, &inputs, hardware.as_ref())
        .await
        .map(Json)
}

/// The body of the search route, for a client, the inputs read from this
/// machine, and its hardware.
#[cfg(feature = "cloud")]
pub(crate) async fn search_body(
    client: &apollia_llm::HfRegistryClient,
    params: &SearchQuery,
    inputs: &apollia_llm::SearchInputs,
    hardware: Option<&HardwareProfile>,
) -> Result<serde_json::Value, (StatusCode, Json<ErrorResponse>)> {
    use apollia_llm::{BoundedSearch, BoundedSearchError, SearchSort};

    let sort = match params.sort.as_deref() {
        None => SearchSort::default(),
        Some(key) => SearchSort::from_key(key)
            .ok_or_else(|| search_error(StatusCode::BAD_REQUEST, format!("unknown sort: {key}")))?,
    };
    let search = BoundedSearch {
        query: params.q.clone().unwrap_or_default(),
        format: params.format.unwrap_or_default(),
        sort,
        limit: params.limit,
        fits_only: params.fits_only,
    };
    let page = client
        .search_bounded(&search, &inputs.context(hardware))
        .await
        .map_err(|e| {
            let status = match e {
                BoundedSearchError::Offline => StatusCode::SERVICE_UNAVAILABLE,
                BoundedSearchError::MlxNeedsAppleSilicon => StatusCode::BAD_REQUEST,
                _ => StatusCode::BAD_GATEWAY,
            };
            search_error(status, e.to_string())
        })?;
    serde_json::to_value(page)
        .map_err(|e| search_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[cfg(feature = "cloud")]
fn search_error(status: StatusCode, error: String) -> (StatusCode, Json<ErrorResponse>) {
    (status, Json(ErrorResponse { error }))
}

/// `GET /api/v1/llm/registry/model/:org/:repo?hf_token=...`
#[cfg(feature = "cloud")]
#[utoipa::path(
    get,
    path = "/api/v1/llm/registry/model/{org}/{repo}",
    tag = "model_hub",
    params(
        ("org" = String, Path, description = "HuggingFace organization or user"),
        ("repo" = String, Path, description = "HuggingFace repository name"),
        ("hf_token" = Option<String>, Query, description = "HuggingFace token for gated models"),
    ),
    responses(
        (status = 200, description = "Model metadata and file list (HuggingFace model card)"),
        (status = 403, description = "Model is gated", body = crate::api::openapi::ApiErrorBody),
        (status = 404, description = "Model not found", body = crate::api::openapi::ApiErrorBody),
        (status = 502, description = "Upstream HuggingFace error", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_registry_model<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path((org, repo)): Path<(String, String)>,
    Query(params): Query<ModelQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    use apollia_llm::HfRegistryClient;

    let hardware = tokio::task::spawn_blocking(apollia_llm::hardware::detect)
        .await
        .ok();

    let repo_id = format!("{org}/{repo}");
    let client = HfRegistryClient::new(params.hf_token);

    let mut card = client
        .get_model(&repo_id, hardware.as_ref(), None)
        .await
        .map_err(|e| {
            let status = match &e {
                apollia_llm::HfError::NotFound(_) => StatusCode::NOT_FOUND,
                apollia_llm::HfError::Gated(_) => StatusCode::FORBIDDEN,
                _ => StatusCode::BAD_GATEWAY,
            };
            (
                status,
                Json(ErrorResponse {
                    error: format!("failed to fetch model {repo_id}: {e}"),
                }),
            )
        })?;

    // Fetch generation_config.json for recommended params (best-effort).
    if let Some(gen_config) = client.get_generation_config(&repo_id).await {
        card.generation_config = Some(gen_config);
    }

    // Which variants are already on this machine: read from the model index,
    // best-effort, since a card is still worth showing without it.
    let dir = crate::installed_models::models_dir(&state.data_dir);
    if let Ok(Ok(index)) =
        tokio::task::spawn_blocking(move || crate::installed_models::current(&dir)).await
    {
        apollia_llm::recommend::mark_downloaded(&mut card.variants, &index);
    }

    Ok(Json(serde_json::to_value(card).unwrap_or_default()))
}

/// `GET /api/v1/llm/recommend?limit=...&n_ctx=...`
///
/// Ranks the models this machine should run: plans against the embedded
/// generation table, resolves the survivors on HuggingFace, reads each
/// finalist's GGUF header over a range request, and orders what is left.
///
/// Answers `503` when the Hub cannot be reached, which is deliberately not the
/// same as an empty `200`. The catalogue is fetched live, so no network means
/// no catalogue; reporting that as "no models fit" would be a different and
/// false claim about the operator's machine.
#[cfg(feature = "cloud")]
#[utoipa::path(
    get,
    path = "/api/v1/llm/recommend",
    tag = "model_hub",
    params(
        ("limit" = Option<usize>, Query, description = "Maximum number of recommendations"),
        ("n_ctx" = Option<u32>, Query, description = "Context window the cache is sized against"),
        ("hf_token" = Option<String>, Query, description = "HuggingFace token for gated models"),
    ),
    responses(
        (status = 200, description = "Models ranked for this machine, best first"),
        (status = 503, description = "HuggingFace unreachable, so there is no catalogue to rank", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn recommend_models<B: ExecutionBackend + Clone>(
    State(_state): State<AppState<B>>,
    Query(params): Query<RecommendQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    use apollia_llm::recommend::{
        resolve, FamilyManifest, ResolveError, ResolveOptions, RuntimeShape,
    };

    let profile = tokio::task::spawn_blocking(apollia_llm::hardware::detect)
        .await
        .unwrap_or_else(|_| apollia_llm::hardware::detect());

    let manifest = FamilyManifest::embedded().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("the shipped model table is invalid: {e}"),
            }),
        )
    })?;

    let mut options = ResolveOptions {
        hf_token: params.hf_token,
        ..ResolveOptions::default()
    };
    if let Some(n_ctx) = params.n_ctx {
        options.shape = RuntimeShape {
            n_ctx,
            ..RuntimeShape::default()
        };
    }

    let models = match resolve(&manifest, &profile, &options).await {
        Ok(models) => models,
        Err(ResolveError::HubUnreachable(detail)) => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: format!("huggingface unreachable: {detail}"),
                }),
            ));
        }
        Err(err) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: err.to_string(),
                }),
            ));
        }
    };

    let mut models = models;
    if let Some(limit) = params.limit {
        models.truncate(limit.max(1));
    }

    Ok(Json(serde_json::json!({
        "models": models,
        "hardware": HardwareResponse::from(profile),
    })))
}

/// `GET /api/v1/llm/recommendations?limit=...&n_ctx=...&fits_only=...`
///
/// The recommendation as the desktop shows it: the ranked list, or the
/// unreachable Hub and the empty ranking kept apart from it as a `status`,
/// each with the hardware and the editorial selection. Unlike
/// `GET /api/v1/llm/recommend`, an unreachable Hub is a `200` carrying the
/// selection, since the selection needs no network.
#[cfg(feature = "cloud")]
#[utoipa::path(
    get,
    path = "/api/v1/llm/recommendations",
    tag = "model_hub",
    params(
        ("limit" = Option<usize>, Query, description = "Maximum number of ranked models, at most 10"),
        ("n_ctx" = Option<u32>, Query, description = "Context window the cache is sized against"),
        ("fits_only" = Option<bool>, Query, description = "Keep, in the editorial selection, only the sizes that fit this machine"),
        ("hf_token" = Option<String>, Query, description = "HuggingFace token for gated models"),
    ),
    responses(
        (status = 200, description = "The outcome: status ok, unreachable or empty, with the hardware and the editorial selection"),
        (status = 400, description = "A malformed query parameter", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "The hardware probe or the shipped model table failed", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_recommendations<B: ExecutionBackend + Clone>(
    State(_state): State<AppState<B>>,
    Query(params): Query<crate::recommendation::RecommendParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let outcome = crate::recommendation::recommend(params)
        .await
        .map_err(|e| internal_error(e.to_string()))?;
    serde_json::to_value(outcome)
        .map(Json)
        .map_err(|e| internal_error(e.to_string()))
}

fn internal_error(error: String) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse { error }),
    )
}

// ─────────────────────────────────────────────
// Router
// ─────────────────────────────────────────────

/// Build the axum sub-router for Model Hub endpoints.
pub fn model_hub_routes<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    let router = Router::new()
        .route("/api/v1/llm/hardware", get(get_hardware::<B>))
        .route(
            "/api/v1/llm/hardware/usage",
            get(usage::get_resource_usage::<B>),
        )
        .route(
            "/api/v1/llm/embeddings",
            get(embeddings::list_embedding_models::<B>),
        )
        .route(
            "/api/v1/llm/embeddings/default",
            get(embedding_default::get_embedding_default::<B>)
                .put(embedding_default::set_embedding_default::<B>),
        )
        .route(
            "/api/v1/llm/dictation",
            get(dictation::list_dictation_models::<B>),
        )
        .route(
            "/api/v1/llm/installed-models",
            get(installed::list_installed_models::<B>)
                .delete(installed::delete_installed_model::<B>),
        );

    #[cfg(feature = "cloud")]
    let router = router
        .route("/api/v1/llm/registry/search", get(search_registry::<B>))
        .route(
            "/api/v1/llm/registry/model/:org/:repo",
            get(get_registry_model::<B>),
        )
        .route("/api/v1/llm/recommend", get(recommend_models::<B>))
        .route("/api/v1/llm/recommendations", get(get_recommendations::<B>))
        .route(
            "/api/v1/llm/registry/cards",
            get(cards::list_model_cards::<B>),
        )
        .route("/api/v1/llm/registry/card", get(cards::get_model_card::<B>))
        .route(
            "/api/v1/llm/registry/readme",
            get(cards::get_model_readme::<B>),
        );

    router
}

#[cfg(test)]
mod route_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hardware_body_carries_the_capacity_the_estimator_grades_against() {
        // GIVEN a machine with a 12 GB dedicated card and 64 GB of system memory
        let profile = HardwareProfile {
            total_ram_gb: 64.0,
            available_ram_gb: 40.0,
            cpu_model: "test".to_owned(),
            cpu_cores: 16,
            accelerator: AcceleratorProfile::Cuda {
                device_name: "test".to_owned(),
                vram_gb: 12.0,
                compute_capability: (8, 9),
            },
            memory_budget_gb: 12.0,
        };
        let expected =
            apollia_llm::recommend::MemoryArchitecture::of(&profile).total_capacity_gb(&profile);

        // WHEN the route's body is built and serialized as the route sends it
        let body =
            serde_json::to_value(HardwareResponse::from(profile)).expect("the body serializes");

        // THEN the capacity is the card plus the usable share of system memory
        // (12 + 64 x 0.6 = 50.4), the figure `total_capacity_gb` computes,
        // rounded to the tenth
        assert_eq!(body["total_capacity_gb"], serde_json::json!(50.4));
        assert!((expected - 50.4).abs() < 0.05);
        assert_eq!(body["memory_budget_gb"], serde_json::json!(12.0));
    }

    #[cfg(feature = "cloud")]
    fn linux_box() -> HardwareProfile {
        HardwareProfile {
            total_ram_gb: 32.0,
            available_ram_gb: 16.0,
            cpu_model: "test".to_owned(),
            cpu_cores: 8,
            accelerator: AcceleratorProfile::None,
            memory_budget_gb: 19.2,
        }
    }

    #[cfg(feature = "cloud")]
    fn query(format: Option<apollia_llm::SearchFormat>, sort: Option<&str>) -> SearchQuery {
        SearchQuery {
            q: Some("Qwen3".to_owned()),
            format,
            limit: None,
            sort: sort.map(str::to_owned),
            fits_only: false,
            hf_token: None,
        }
    }

    #[cfg(feature = "cloud")]
    #[tokio::test]
    async fn offline_the_search_route_says_so_rather_than_an_empty_list() {
        // GIVEN a Hub address nothing can listen on and the embedded lists
        let client = apollia_llm::HfRegistryClient::with_hub_base(None, "http://127.0.0.1:0");
        let inputs = apollia_llm::SearchInputs::load(std::path::Path::new("/nonexistent"));
        let profile = linux_box();

        // WHEN the route body is built
        let outcome = search_body(&client, &query(None, None), &inputs, Some(&profile)).await;

        // THEN it is a 503 naming the offline search
        let Err((status, Json(body))) = outcome else {
            panic!("an offline search is an error");
        };
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body.error, "search is unavailable offline");
    }

    #[cfg(feature = "cloud")]
    #[tokio::test]
    async fn the_search_route_refuses_mlx_off_apple_silicon_and_an_unknown_sort() {
        // GIVEN a Linux machine and a Hub that is never reached
        let client = apollia_llm::HfRegistryClient::with_hub_base(None, "http://127.0.0.1:0");
        let inputs = apollia_llm::SearchInputs::load(std::path::Path::new("/nonexistent"));
        let profile = linux_box();

        for params in [
            query(Some(apollia_llm::SearchFormat::Mlx), None),
            query(None, Some("trending")),
        ] {
            // WHEN the route body is built
            let outcome = search_body(&client, &params, &inputs, Some(&profile)).await;

            // THEN it is a 400 with its reason
            let Err((status, Json(body))) = outcome else {
                panic!("the request is refused");
            };
            assert_eq!(status, StatusCode::BAD_REQUEST);
            assert!(
                body.error.contains("Apple Silicon") || body.error.contains("unknown sort"),
                "{}",
                body.error
            );
        }
    }
}
