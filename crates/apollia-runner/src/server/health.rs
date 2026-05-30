//! `GET /health` : health-check polled par le daemon toutes les 30s.

use axum::extract::State;
use axum::Json;

use crate::ipc::{HealthData, Response};

use super::AppState;

pub async fn handle(State(state): State<AppState>) -> Json<Response<HealthData>> {
    let uptime_secs = state
        .started_at
        .elapsed()
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let data = HealthData {
        uptime_secs,
        loaded_models: state.model_cache.loaded_ids(),
        memory_used_mb: state.model_cache.total_memory_mb(),
        // TODO: fetch total VRAM via the GPU backend.
        memory_total_mb: 0,
    };

    Json(Response::success_no_id(data))
}
