//! The audit tab: the tool invocations recorded in the journal, the daily LLM
//! cost breakdown, the journal's own integrity check, and the hooks in force.

use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::http_get_json;

// ---------------------------------------------------------------------------
// Audit Trail
// ---------------------------------------------------------------------------

/// Audit trail entry for display in the UI.
#[derive(Debug, Serialize)]
pub struct AuditTrailEntry {
    /// Unique invocation identifier.
    pub id: String,
    /// Name of the invoked tool.
    pub tool_name: String,
    /// Agent UUID (used for filtering).
    pub agent_id: String,
    /// Human-readable agent name, resolved from the registry (e.g. "standup-scribe").
    /// Falls back to agent_id if the agent is no longer registered.
    pub agent_name: String,
    /// ISO 8601 timestamp.
    pub timestamp: String,
    /// Execution duration in milliseconds.
    pub duration_ms: Option<u64>,
    /// Process exit code.
    pub exit_code: Option<i32>,
    /// Full JSON arguments of the invocation.
    pub args_json: Option<String>,
    /// Tool standard output.
    pub stdout: Option<String>,
    /// Tool error output.
    pub stderr: Option<String>,
}

/// Fetches the latest tool invocations via the audit REST API.
///
/// Delegates to `GET /api/v1/audit?limit=N` and returns the parsed entries for
/// display in the AuditTrail table.
#[tauri::command]
pub async fn get_tool_audit_trail(
    state: State<'_, RuntimeHandle>,
    limit: Option<u32>,
) -> Result<Vec<AuditTrailEntry>, String> {
    let l = limit.unwrap_or(50);
    let path = format!("/api/v1/audit?limit={l}");
    let json = http_get_json(state.api_port, &path).await?;

    let events = json
        .get("events")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // Resolve agent names asynchronously from the registry so the UI shows
    // "standup-scribe" instead of a raw UUID. Falls back to the UUID when the
    // agent is no longer registered (e.g. stopped between runs).
    let mut entries: Vec<AuditTrailEntry> = Vec::with_capacity(events.len());
    for e in events {
        let agent_id = e
            .get("agent_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let agent_name = state
            .registry_handle
            .get_agent(&agent_id)
            .await
            .ok()
            .flatten()
            .map(|entry| entry.manifest.name.clone())
            .unwrap_or_else(|| agent_id.clone()); // agent_id is already String here

        entries.push(AuditTrailEntry {
            id: e
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            tool_name: e
                .get("tool_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            agent_id,
            agent_name,
            timestamp: e
                .get("started_at")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            duration_ms: e.get("duration_ms").and_then(|v| v.as_u64()),
            exit_code: e
                .get("exit_code")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32),
            args_json: e
                .get("args_json")
                .and_then(|v| v.as_str())
                .map(String::from),
            stdout: e.get("stdout").and_then(|v| v.as_str()).map(String::from),
            stderr: e.get("stderr").and_then(|v| v.as_str()).map(String::from),
        });
    }

    Ok(entries)
}

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
    state: State<'_, RuntimeHandle>,
    days: Option<u32>,
) -> Result<LlmDailyCostsResponse, String> {
    let d = days.unwrap_or(7);
    let path = format!("/api/v1/llm/costs/daily?days={d}");
    let json = http_get_json(state.api_port, &path).await;

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
// Audit Stats
// ---------------------------------------------------------------------------

/// Fetches the aggregated audit trail statistics.
///
/// Calls `GET /api/v1/audit/stats` and returns the raw JSON to avoid
/// duplicating the data structure on the Tauri side.
#[tauri::command]
pub async fn get_audit_stats(state: State<'_, RuntimeHandle>) -> Result<serde_json::Value, String> {
    http_get_json(state.api_port, "/api/v1/audit/stats").await
}

// ---------------------------------------------------------------------------
// Audit Chain Verification
// ---------------------------------------------------------------------------

/// Integrity verdict for a run's hash-chained audit journal.
///
/// Flattens the runtime `VerifyChainReport` into the shape consumed by the
/// desktop verify panel: a boolean verdict, the broken link identifier (the
/// sequence number of the first tampered entry, `None` when intact), and a
/// short human-readable message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditVerifyResult {
    /// True when the whole chain verifies (no broken link, signatures valid).
    pub ok: bool,
    /// Sequence number of the first broken entry as a string, `None` when `ok`.
    pub broken_at: Option<String>,
    /// Short status detail, surfaced under the verdict in the UI.
    pub message: String,
}

/// Verifies the hash chain of a run's audit journal.
///
/// Delegates to `GET /api/v1/audit/verify/:run_id` on the in-process runtime
/// and maps the report to [`AuditVerifyResult`]. A missing run surfaces as an
/// `Err` (the HTTP layer answers 404), so the UI shows an explicit error rather
/// than a silent "ok" verdict.
#[tauri::command]
pub async fn verify_audit_run(
    state: State<'_, RuntimeHandle>,
    run_id: String,
) -> Result<AuditVerifyResult, String> {
    let path = format!("/api/v1/audit/verify/{run_id}");
    let json = http_get_json(state.api_port, &path).await?;

    let ok = json.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    let broken_at = json
        .get("first_broken_link")
        .and_then(|v| v.get("seq"))
        .and_then(serde_json::Value::as_u64)
        .map(|seq| seq.to_string());
    let entries_checked = json
        .get("entries_checked")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);

    let message = if ok {
        format!("{entries_checked} entries verified")
    } else {
        match &broken_at {
            Some(seq) => format!("integrity broken at entry {seq} of {entries_checked} checked"),
            None => format!("integrity check failed after {entries_checked} entries"),
        }
    };

    Ok(AuditVerifyResult {
        ok,
        broken_at,
        message,
    })
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
pub async fn get_active_hooks(
    state: State<'_, RuntimeHandle>,
) -> Result<serde_json::Value, String> {
    http_get_json(state.api_port, "/api/v1/hooks").await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_trail_entry_serializes() {
        // GIVEN an AuditTrailEntry with both agent_id (UUID) and agent_name
        let entry = AuditTrailEntry {
            id: "inv-001".to_string(),
            tool_name: "file_io".to_string(),
            agent_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            agent_name: "standup-scribe".to_string(),
            timestamp: "2026-03-13T10:00:00Z".to_string(),
            duration_ms: Some(42),
            exit_code: Some(0),
            args_json: Some(r#"{"path": "/tmp/test"}"#.to_string()),
            stdout: Some("ok".to_string()),
            stderr: None,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&entry).expect("serialize");

        // THEN all fields are present including agent_name
        assert_eq!(json["tool_name"], "file_io");
        assert_eq!(json["agent_name"], "standup-scribe");
        assert_eq!(json["duration_ms"], 42);
        assert_eq!(json["exit_code"], 0);
        assert!(json["stderr"].is_null());
    }

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
