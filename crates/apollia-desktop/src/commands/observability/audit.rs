//! The daily LLM cost breakdown and the hooks in force.

use crate::attached::RuntimeLink;
use serde::Serialize;
use tauri::State;

// ---------------------------------------------------------------------------
// LLM Daily Costs
// ---------------------------------------------------------------------------

/// Daily per-backend cost entry for the SVG chart.
#[derive(Debug, Serialize)]
pub struct LlmDailyCostEntry {
    /// Date in `YYYY-MM-DD` format.
    pub date: String,
    /// Backend name.
    pub backend: String,
    /// Estimated total cost in USD for that day.
    pub cost_usd: f64,
}

/// Daily LLM costs response.
#[derive(Debug, Serialize)]
pub struct LlmDailyCostsResponse {
    /// Entries per day and backend.
    pub entries: Vec<LlmDailyCostEntry>,
    /// Number of requested days.
    pub days: u32,
}

/// Fetches LLM costs broken down by day and backend.
///
/// Delegates to `GET /api/v1/llm/costs/daily?days=N`.
#[tauri::command]
pub async fn get_llm_daily_costs(
    state: State<'_, RuntimeLink>,
    days: Option<u32>,
) -> Result<LlmDailyCostsResponse, String> {
    let d = days.unwrap_or(7);
    let path = format!("/api/v1/llm/costs/daily?days={d}");
    let json = state.get_json(&path).await;

    match json {
        Ok(resp) => {
            let entries = resp
                .get("entries")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|e| LlmDailyCostEntry {
                    date: e
                        .get("date")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    backend: e
                        .get("backend")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    cost_usd: e.get("cost_usd").and_then(|v| v.as_f64()).unwrap_or(0.0),
                })
                .collect();

            Ok(LlmDailyCostsResponse { entries, days: d })
        }
        Err(_) => Ok(LlmDailyCostsResponse {
            entries: vec![],
            days: d,
        }),
    }
}

// ---------------------------------------------------------------------------
// Lifecycle Hooks
// ---------------------------------------------------------------------------

/// Lists the lifecycle hook handlers registered at startup.
///
/// Calls `GET /api/v1/hooks` and returns the raw JSON array of handler
/// summaries (`{ id, type, events, timeout_ms, target }`) for the Builder hooks
/// view. An empty array is a valid configuration, surfaced as a clean state.
#[tauri::command]
pub async fn get_active_hooks(state: State<'_, RuntimeLink>) -> Result<serde_json::Value, String> {
    state.get_json("/api/v1/hooks").await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_daily_costs_response_serializes() {
        // GIVEN an LlmDailyCostsResponse
        let resp = LlmDailyCostsResponse {
            entries: vec![
                LlmDailyCostEntry {
                    date: "2026-03-12".to_string(),
                    backend: "anthropic".to_string(),
                    cost_usd: 0.15,
                },
                LlmDailyCostEntry {
                    date: "2026-03-13".to_string(),
                    backend: "local".to_string(),
                    cost_usd: 0.0,
                },
            ],
            days: 7,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&resp).expect("serialize");

        // THEN entries and days are correct
        assert_eq!(json["days"], 7);
        let entries = json["entries"].as_array().expect("entries is array");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["date"], "2026-03-12");
        assert_eq!(entries[0]["backend"], "anthropic");
    }
}
