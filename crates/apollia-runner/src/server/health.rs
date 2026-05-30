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
        // FUTURE: total VRAM is not yet exposed by the GPU backend, so we
        // report `0` as a sentinel. Clients should treat `memory_total_mb == 0`
        // as "unknown" rather than "no memory".
        memory_total_mb: 0,
    };

    Json(Response::success_no_id(data))
}
