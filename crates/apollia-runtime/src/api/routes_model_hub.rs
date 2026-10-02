//! Model Hub routes, hardware detection, HF registry search.
//!
//! - `GET /api/v1/llm/hardware`                 , hardware profile (RAM, CPU, GPU)
//! - `GET /api/v1/llm/registry/search`          , search HuggingFace GGUF models
//! - `GET /api/v1/llm/registry/model/:org/:repo`, model metadata + file list
//! - `GET /api/v1/llm/recommend`                 , models ranked for this machine
//! - `GET /api/v1/llm/embeddings`                , embeddings models, and the one for this machine

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use apollia_llm::hardware::HardwareProfile;
use apollia_llm::AcceleratorProfile;

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

pub mod embeddings;

// ─────────────────────────────────────────────
// Response types
// ─────────────────────────────────────────────

#[derive(Debug, Serialize, utoipa::ToSchema)]
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

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub limit: Option<u32>,
    pub sort: Option<String>,
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

/// `GET /api/v1/llm/registry/search?q=...&limit=...&sort=...`
#[cfg(feature = "cloud")]
#[utoipa::path(
    get,
    path = "/api/v1/llm/registry/search",
    tag = "model_hub",
    params(
        ("q" = Option<String>, Query, description = "Search query"),
        ("limit" = Option<u32>, Query, description = "Maximum number of results"),
        ("sort" = Option<String>, Query, description = "Sort key"),
        ("hf_token" = Option<String>, Query, description = "HuggingFace token for gated models"),
    ),
    responses(
        (status = 200, description = "Matching GGUF models (HuggingFace model cards)"),
        (status = 502, description = "Upstream HuggingFace error", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn search_registry<B: ExecutionBackend + Clone>(
    State(_state): State<AppState<B>>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    use apollia_llm::{HfRegistryClient, HfSearchFilter};

    let hardware = tokio::task::spawn_blocking(apollia_llm::hardware::detect)
        .await
        .ok();

    let client = HfRegistryClient::new(params.hf_token);
    let query = params.q.as_deref().unwrap_or("");
    let filter = HfSearchFilter {
        filter: Some("gguf".to_string()),
        sort: params.sort,
        limit: params.limit,
        pipeline_tag: Some("text-generation".to_string()),
        language: None,
        next_cursor: None,
    };

    let models = client
        .search(query, filter, hardware.as_ref())
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
        })?;

    Ok(Json(serde_json::to_value(models).unwrap_or_default()))
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
    State(_state): State<AppState<B>>,
    Path((org, repo)): Path<(String, String)>,
    Query(params): Query<SearchQuery>,
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
                    error: e.to_string(),
                }),
            )
        })?;

    // Fetch generation_config.json for recommended params (best-effort).
    if let Some(gen_config) = client.get_generation_config(&repo_id).await {
        card.generation_config = Some(gen_config);
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

// ─────────────────────────────────────────────
// Router
// ─────────────────────────────────────────────

/// Build the axum sub-router for Model Hub endpoints.
pub fn model_hub_routes<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    let router = Router::new()
        .route("/api/v1/llm/hardware", get(get_hardware::<B>))
        .route(
            "/api/v1/llm/embeddings",
            get(embeddings::list_embedding_models::<B>),
        );

    #[cfg(feature = "cloud")]
    let router = router
        .route("/api/v1/llm/registry/search", get(search_registry::<B>))
        .route(
            "/api/v1/llm/registry/model/:org/:repo",
            get(get_registry_model::<B>),
        )
        .route("/api/v1/llm/recommend", get(recommend_models::<B>));

    router
}

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
}
