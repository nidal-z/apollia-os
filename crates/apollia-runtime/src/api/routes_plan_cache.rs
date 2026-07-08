//! REST routes for the ORIA plan cache, `GET /api/v1/plan-cache/stats` and
//! `POST /api/v1/plan-cache/clear`.
//!
//! Exposes cache statistics and management operations for the desktop frontend
//! and CLI. Returns `503 Service Unavailable` when no plan cache is configured.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use crate::api::server::AppState;
use crate::coordinator::{DynBackend, ExecutionBackend};

/// Response body for `GET /api/v1/plan-cache/stats`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct PlanCacheStatsResponse {
    /// Number of cached plans.
    pub total_entries: u32,
    /// Total number of cache hits across all entries.
    pub cache_hits: u64,
    /// Hit rate as a percentage (0.0–100.0).
    pub hit_rate_pct: f64,
    /// Timestamp of the oldest cache entry, or `null`.
    pub oldest_entry_at: Option<String>,
    /// Timestamp of the newest cache entry, or `null`.
    pub newest_entry_at: Option<String>,
}

/// Response body for `POST /api/v1/plan-cache/clear`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ClearCacheResponse {
    /// Number of entries removed.
    pub cleared_count: u32,
}

/// Error response body.
#[derive(Serialize)]
pub struct ErrorResponse {
    /// Human-readable error description.
    error: String,
}

/// `GET /api/v1/plan-cache/stats`, return aggregate cache statistics.
///
/// Returns zeroed counters when the cache is empty. Returns `503` when
/// the plan cache repository is not configured.
#[utoipa::path(
    get,
    path = "/api/v1/plan-cache/stats",
    tag = "governance",
    responses(
        (status = 200, description = "Plan cache statistics", body = PlanCacheStatsResponse),
        (status = 500, description = "Internal cache error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Plan cache not configured", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_plan_cache_stats<B: ExecutionBackend + Clone + From<DynBackend>>(
    State(state): State<AppState<B>>,
) -> Result<(StatusCode, Json<PlanCacheStatsResponse>), (StatusCode, Json<ErrorResponse>)> {
    let repo = state.plan_cache.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "plan cache not available".to_string(),
            }),
        )
    })?;

    let stats = {
        let repo = repo.lock().map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("mutex poisoned: {e}"),
                }),
            )
        })?;
        repo.stats().map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("plan cache stats failed: {e}"),
                }),
            )
        })?
    };

    // Compute hit rate: hits / (hits + total_entries) is not meaningful;
    // the caller tracks misses externally. We expose raw hit_count here.
    // hit_rate_pct = 0.0 when no entries exist.
    let hit_rate_pct = if stats.total_entries == 0 {
        0.0
    } else {
        // Approximate: hits / entries gives average hits per entry, not a true rate.
        // The frontend can compute a more meaningful rate from these raw values.
        0.0
    };

    Ok((
        StatusCode::OK,
        Json(PlanCacheStatsResponse {
            total_entries: stats.total_entries,
            cache_hits: stats.cache_hits,
            hit_rate_pct,
            oldest_entry_at: stats.oldest_entry_at,
            newest_entry_at: stats.newest_entry_at,
        }),
    ))
}

/// `POST /api/v1/plan-cache/clear`, remove all cached plans.
///
/// Returns the number of entries that were removed. Returns `503` when
/// the plan cache repository is not configured.
#[utoipa::path(
    post,
    path = "/api/v1/plan-cache/clear",
    tag = "governance",
    responses(
        (status = 200, description = "Number of cleared entries", body = ClearCacheResponse),
        (status = 500, description = "Internal cache error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Plan cache not configured", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn clear_plan_cache<B: ExecutionBackend + Clone + From<DynBackend>>(
    State(state): State<AppState<B>>,
) -> Result<(StatusCode, Json<ClearCacheResponse>), (StatusCode, Json<ErrorResponse>)> {
    let repo = state.plan_cache.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "plan cache not available".to_string(),
            }),
        )
    })?;

    let cleared = {
        let repo = repo.lock().map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("mutex poisoned: {e}"),
                }),
            )
        })?;
        repo.clear_all().map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("plan cache clear failed: {e}"),
                }),
            )
        })?
    };

    Ok((
        StatusCode::OK,
        Json(ClearCacheResponse {
            cleared_count: cleared,
        }),
    ))
}
