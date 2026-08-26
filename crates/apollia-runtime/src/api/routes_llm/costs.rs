//! Cost and token accounting endpoints, `GET /api/v1/llm/costs[/daily]`.
//!
//! Both read the observability store the LLM router writes to; the daily
//! breakdown windows on the local calendar day, not on UTC.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

// ─────────────────────────────────────────────
// Cost stats types & handler
// ─────────────────────────────────────────────

/// Query parameters for `GET /api/v1/llm/costs`.
#[derive(Debug, Deserialize)]
pub struct CostsQuery {
    /// Number of days to aggregate costs for (default: 7).
    #[serde(default = "default_cost_days")]
    pub days: u32,
}

/// Default number of days for cost aggregation.
fn default_cost_days() -> u32 {
    7
}

/// A single backend/model cost summary row.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CostSummaryRow {
    /// Backend name.
    pub backend: String,
    /// Model identifier.
    pub model: String,
    /// Number of LLM calls.
    pub call_count: u64,
    /// Total tokens (prompt + completion).
    pub total_tokens: u64,
    /// Estimated total cost in USD.
    pub total_cost_usd: f64,
}

/// Response body for `GET /api/v1/llm/costs`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CostsResponse {
    /// Per-backend/model cost breakdown.
    pub rows: Vec<CostSummaryRow>,
    /// Number of days aggregated.
    pub days: u32,
}

/// Handler for `GET /api/v1/llm/costs`.
///
/// Aggregates LLM call costs and token usage from `llm_calls.db` over the
/// requested time window. Returns 503 if no `LlmCallRepository` is available.
#[utoipa::path(
    get,
    path = "/api/v1/llm/costs",
    tag = "llm",
    params(("days" = Option<u32>, Query, description = "Number of days to aggregate (default 7)")),
    responses(
        (status = 200, description = "Aggregated LLM costs by backend and model", body = CostsResponse),
        (status = 500, description = "Query failed", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "No LLM call repository configured", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_llm_costs<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    axum::extract::Query(query): axum::extract::Query<CostsQuery>,
) -> Result<Json<CostsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let repo = state.llm_call_repository.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "no LLM call repository configured"})),
        )
    })?;

    let days = query.days;
    let since = chrono::Utc::now() - chrono::Duration::days(i64::from(days));
    let since_str = since.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let repo = Arc::clone(repo);
    let summaries = tokio::task::spawn_blocking(move || {
        let guard = repo
            .lock()
            .map_err(|e| format!("failed to lock repository: {e}"))?;
        guard
            .costs_by_backend_model_since(&since_str)
            .map_err(|e| format!("query failed: {e}"))
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("join error: {e}")})),
        )
    })?
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )
    })?;

    let rows = summaries
        .into_iter()
        .map(|s| CostSummaryRow {
            backend: s.backend,
            model: s.model,
            call_count: s.call_count,
            total_tokens: s.total_tokens,
            total_cost_usd: s.total_cost_usd,
        })
        .collect();

    Ok(Json(CostsResponse { rows, days }))
}

// ─────────────────────────────────────────────
// Daily costs types & handler
// ─────────────────────────────────────────────

/// A single day+backend cost entry for the daily chart.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DailyCostEntry {
    /// Local calendar day of the host, in `YYYY-MM-DD` format.
    pub date: String,
    /// Backend name.
    pub backend: String,
    /// Total estimated cost in USD for this day.
    pub cost_usd: f64,
}

/// Response body for `GET /api/v1/llm/costs/daily`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DailyCostsResponse {
    /// Per-day/backend cost entries.
    pub entries: Vec<DailyCostEntry>,
    /// Number of days requested.
    pub days: u32,
}

/// UTC instant at which a window of `days` local calendar days opens.
///
/// The last day of the window is today, so the window opens at local midnight
/// `days - 1` days ago. Returned in the `YYYY-MM-DDTHH:MM:SSZ` form the
/// repository compares against `created_at`.
fn local_window_start(days: u32) -> String {
    let span = i64::from(days.max(1)) - 1;
    let start_day = chrono::Local::now().date_naive() - chrono::Duration::days(span);
    let start = start_day
        .and_hms_opt(0, 0, 0)
        .and_then(|naive| chrono::TimeZone::from_local_datetime(&chrono::Local, &naive).earliest())
        .map(|local| local.with_timezone(&chrono::Utc))
        // A DST forward jump can delete local midnight itself. Falling back on
        // a plain 24-hour count then opens the window earlier, never later, so
        // no day the axis draws is left out.
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(i64::from(days.max(1))));
    start.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Handler for `GET /api/v1/llm/costs/daily`.
///
/// Returns LLM costs broken down by day and backend for the requested
/// time window. Used by the Observability LLM Costs chart.
///
/// Both the day of each entry and the bounds of the window are the host's
/// local calendar, not UTC: `days` counts calendar days ending today, and the
/// window opens at local midnight of the first of them. A window measured in
/// 24-hour slices from `now` reaches into a day the chart draws no bar for,
/// and the spend of that fraction then shows in a total no bar carries.
#[utoipa::path(
    get,
    path = "/api/v1/llm/costs/daily",
    tag = "llm",
    params(("days" = Option<u32>, Query, description = "Number of local calendar days to aggregate, ending today (default 7)")),
    responses(
        (status = 200, description = "Per-day LLM costs by backend", body = DailyCostsResponse),
        (status = 500, description = "Query failed", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "No LLM call repository configured", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_llm_daily_costs<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    axum::extract::Query(query): axum::extract::Query<CostsQuery>,
) -> Result<Json<DailyCostsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let repo = state.llm_call_repository.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "no LLM call repository configured"})),
        )
    })?;

    let days = query.days;
    let since_str = local_window_start(days);

    let repo = Arc::clone(repo);
    let summaries = tokio::task::spawn_blocking(move || {
        let guard = repo
            .lock()
            .map_err(|e| format!("failed to lock repository: {e}"))?;
        guard
            .costs_by_day_backend_since(&since_str)
            .map_err(|e| format!("query failed: {e}"))
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("join error: {e}")})),
        )
    })?
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )
    })?;

    let entries = summaries
        .into_iter()
        .map(|s| DailyCostEntry {
            date: s.date,
            backend: s.backend,
            cost_usd: s.cost_usd,
        })
        .collect();

    Ok(Json(DailyCostsResponse { entries, days }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daily_cost_window_opens_at_local_midnight_of_its_first_day() {
        // GIVEN a 7 calendar day window, the default of the LLM Costs chart
        let days = 7_u32;

        // WHEN computing the instant the window opens
        let since = local_window_start(days);

        // THEN it is local midnight of the day the chart draws first, so the
        // window covers the axis exactly and no fraction of an eighth day
        // enters a total that no bar carries
        let expected = (chrono::Local::now().date_naive() - chrono::Duration::days(6))
            .and_hms_opt(0, 0, 0)
            .and_then(|naive| {
                chrono::TimeZone::from_local_datetime(&chrono::Local, &naive).earliest()
            })
            .expect("local midnight exists on this date")
            .with_timezone(&chrono::Utc)
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        assert_eq!(since, expected);
    }
}
