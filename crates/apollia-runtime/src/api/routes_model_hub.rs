//! Model Hub routes, hardware detection, HF registry search.
//!
//! - `GET /api/v1/llm/hardware`                 , hardware profile (RAM, CPU, GPU)
//! - `GET /api/v1/llm/registry/search`          , search HuggingFace GGUF models
//! - `GET /api/v1/llm/registry/model/:org/:repo`, model metadata + file list

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use apollia_llm::hardware::HardwareProfile;
use apollia_llm::AcceleratorProfile;

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

// ─────────────────────────────────────────────
// Response types
// ─────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct HardwareResponse {
    pub total_ram_gb: f64,
    pub available_ram_gb: f64,
    pub cpu_model: String,
    pub cpu_cores: u32,
    pub accelerator: AcceleratorProfile,
    pub memory_budget_gb: f64,
}

impl From<HardwareProfile> for HardwareResponse {
    fn from(p: HardwareProfile) -> Self {
        Self {
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

// ─────────────────────────────────────────────
// Handlers
// ─────────────────────────────────────────────

/// `GET /api/v1/llm/hardware`, detect and return the hardware profile.
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

// ─────────────────────────────────────────────
// Router
// ─────────────────────────────────────────────

/// Build the axum sub-router for Model Hub endpoints.
pub fn model_hub_routes<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    let router = Router::new().route("/api/v1/llm/hardware", get(get_hardware::<B>));

    #[cfg(feature = "cloud")]
    let router = router
        .route("/api/v1/llm/registry/search", get(search_registry::<B>))
        .route(
            "/api/v1/llm/registry/model/:org/:repo",
            get(get_registry_model::<B>),
        );

    router
}
