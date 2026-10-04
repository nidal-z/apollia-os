//! `GET /api/v1/llm/hardware/usage`: what the machine is using right now.
//!
//! One measurement per request. Nothing here samples on its own: a screen
//! that shows the figures asks again while it is open and stops when it
//! closes, so a closed screen costs nothing.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_llm::resource_usage::{ResourceUsage, VramUsage};

use super::ErrorResponse;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// The memory and the processor in use at the time of the request.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ResourceUsageResponse {
    /// System memory in use, in GB.
    pub ram_used_gb: f64,
    /// System memory installed, in GB.
    pub ram_total_gb: f64,
    /// Processor load over every core, from 0 to 100.
    pub cpu_percent: f32,
    /// Video memory in use: `{"kind":"dedicated","used_gb":..,"total_gb":..}`
    /// on an NVIDIA card, `{"kind":"unified"}` where the graphics processor
    /// shares the system memory, `{"kind":"unmeasured"}` anywhere else.
    #[schema(value_type = Object)]
    pub vram: VramUsage,
}

impl From<ResourceUsage> for ResourceUsageResponse {
    fn from(u: ResourceUsage) -> Self {
        Self {
            ram_used_gb: u.ram_used_gb,
            ram_total_gb: u.ram_total_gb,
            cpu_percent: u.cpu_percent,
            vram: u.vram,
        }
    }
}

/// `GET /api/v1/llm/hardware/usage`, measure the machine once.
#[utoipa::path(
    get,
    path = "/api/v1/llm/hardware/usage",
    tag = "model_hub",
    responses(
        (status = 200, description = "Memory, processor and video memory in use", body = ResourceUsageResponse),
        (status = 500, description = "The measurement did not complete", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_resource_usage<B: ExecutionBackend + Clone>(
    State(_state): State<AppState<B>>,
) -> Result<Json<ResourceUsageResponse>, (StatusCode, Json<ErrorResponse>)> {
    apollia_llm::resource_usage::measure()
        .await
        .map(|usage| Json(ResourceUsageResponse::from(usage)))
        .map_err(|error| {
            tracing::event!(tracing::Level::WARN, error = %error, "hardware.usage.failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: error.to_string(),
                }),
            )
        })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::http::StatusCode;

    use crate::api::test_support::{app_state, call, StemLoader};

    #[tokio::test]
    async fn test_the_usage_route_states_memory_processor_and_video_memory() {
        // GIVEN a runtime
        let tmp = tempfile::tempdir().expect("tempdir");
        let state = app_state(
            Arc::new(StemLoader { refusal: None }),
            tmp.path().to_path_buf(),
        );

        // WHEN the usage is asked
        let (status, body) = call(state, "GET", "/api/v1/llm/hardware/usage", None).await;

        // THEN it answers the memory in use within what is installed, a load
        //      that is a percentage, and the video memory by its kind
        assert_eq!(status, StatusCode::OK, "{body}");
        let used = body["ram_used_gb"].as_f64().expect("ram_used_gb");
        let total = body["ram_total_gb"].as_f64().expect("ram_total_gb");
        assert!(used > 0.0 && used <= total, "{body}");
        let cpu = body["cpu_percent"].as_f64().expect("cpu_percent");
        assert!((0.0..=100.0).contains(&cpu), "{body}");
        let kind = body["vram"]["kind"].as_str().expect("vram.kind");
        assert!(
            ["dedicated", "unified", "unmeasured"].contains(&kind),
            "{body}"
        );
    }
}
