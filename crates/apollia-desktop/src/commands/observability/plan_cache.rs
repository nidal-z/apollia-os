//! The ORIA plan cache as the observability view reads it: its statistics, and
//! the command that empties it.

use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::{http_get_json, http_post_json};

// ---------------------------------------------------------------------------
// Plan Cache Stats
// ---------------------------------------------------------------------------

/// ORIA plan cache statistics.
#[derive(Debug, Serialize, Deserialize)]
pub struct PlanCacheStatsView {
    /// Total number of cached entries.
    pub total_entries: u32,
    /// Total number of cache hits.
    pub cache_hits: u64,
    /// Total number of cache misses.
    pub cache_misses: u64,
    /// Hit rate as a percentage (0.0 to 100.0).
    pub hit_rate_pct: f64,
    /// Timestamp of the oldest entry, or `null`.
    pub oldest_entry_at: Option<String>,
    /// Timestamp of the newest entry, or `null`.
    pub newest_entry_at: Option<String>,
}

/// Result of the cache clearing operation.
#[derive(Debug, Serialize, Deserialize)]
pub struct ClearCacheResult {
    /// Number of removed entries.
    pub cleared_count: u32,
}

/// Returns the ORIA plan cache statistics.
///
/// Returns zeroed counters if the cache is empty or disabled.
/// Delegates to `GET /api/v1/plan-cache/stats`.
#[tauri::command]
pub async fn get_plan_cache_stats(
    state: State<'_, RuntimeHandle>,
) -> Result<PlanCacheStatsView, String> {
    get_plan_cache_stats_inner(state.api_port).await
}

/// Inner logic for `get_plan_cache_stats`, testable without Tauri State.
async fn get_plan_cache_stats_inner(port: u16) -> Result<PlanCacheStatsView, String> {
    let json = http_get_json(port, "/api/v1/plan-cache/stats")
        .await
        .map_err(|e| format!("get_plan_cache_stats: {e}"))?;

    Ok(PlanCacheStatsView {
        total_entries: json
            .get("total_entries")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        cache_hits: json.get("cache_hits").and_then(|v| v.as_u64()).unwrap_or(0),
        cache_misses: 0,
        hit_rate_pct: json
            .get("hit_rate_pct")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        oldest_entry_at: json
            .get("oldest_entry_at")
            .and_then(|v| v.as_str())
            .map(String::from),
        newest_entry_at: json
            .get("newest_entry_at")
            .and_then(|v| v.as_str())
            .map(String::from),
    })
}

/// Clears the plan cache and returns the number of removed entries.
///
/// Delegates to `POST /api/v1/plan-cache/clear`.
#[tauri::command]
pub async fn clear_plan_cache(state: State<'_, RuntimeHandle>) -> Result<ClearCacheResult, String> {
    clear_plan_cache_inner(state.api_port).await
}

/// Inner logic for `clear_plan_cache`, testable without Tauri State.
async fn clear_plan_cache_inner(port: u16) -> Result<ClearCacheResult, String> {
    let json = http_post_json(port, "/api/v1/plan-cache/clear", &serde_json::json!({}))
        .await
        .map_err(|e| format!("clear_plan_cache: {e}"))?;

    Ok(ClearCacheResult {
        cleared_count: json
            .get("cleared_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_cache_stats_view_serializes_populated() {
        // GIVEN a PlanCacheStatsView with data
        let stats = PlanCacheStatsView {
            total_entries: 42,
            cache_hits: 128,
            cache_misses: 30,
            hit_rate_pct: 81.01,
            oldest_entry_at: Some("2026-03-01T10:00:00Z".to_string()),
            newest_entry_at: Some("2026-03-24T15:00:00Z".to_string()),
        };

        // WHEN serialized
        let json = serde_json::to_value(&stats).expect("serialize");

        // THEN all fields are present
        assert_eq!(json["total_entries"], 42);
        assert_eq!(json["cache_hits"], 128);
        assert_eq!(json["cache_misses"], 30);
        assert_eq!(json["hit_rate_pct"], 81.01);
        assert_eq!(json["oldest_entry_at"], "2026-03-01T10:00:00Z");
        assert_eq!(json["newest_entry_at"], "2026-03-24T15:00:00Z");
    }

    #[test]
    fn test_plan_cache_stats_view_serializes_empty() {
        // GIVEN an empty stats view
        let stats = PlanCacheStatsView {
            total_entries: 0,
            cache_hits: 0,
            cache_misses: 0,
            hit_rate_pct: 0.0,
            oldest_entry_at: None,
            newest_entry_at: None,
        };

        // WHEN serialized
        let json = serde_json::to_value(&stats).expect("serialize");

        // THEN zero counters and null dates
        assert_eq!(json["total_entries"], 0);
        assert_eq!(json["cache_hits"], 0);
        assert!(json["oldest_entry_at"].is_null());
        assert!(json["newest_entry_at"].is_null());
    }

    #[test]
    fn test_clear_cache_result_serializes() {
        // GIVEN a clear result
        let result = ClearCacheResult { cleared_count: 15 };

        // WHEN serialized
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN cleared_count is correct
        assert_eq!(json["cleared_count"], 15);
    }
}
