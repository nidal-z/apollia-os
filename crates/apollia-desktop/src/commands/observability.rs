//! Tauri IPC commands for the Observability view.
//!
//! Three commands covering the view's three tabs:
//! - `get_global_timeline`: runtime events aggregated across tasks
//! - `get_tool_audit_trail`: tool invocations with details
//! - `get_llm_daily_costs`: LLM costs broken down by day and backend

use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

/// The stores the timeline reads live in `scanners`; the view's three other
/// tabs live in `audit`, `plan_cache` and `mailbox`.
pub mod audit;
pub mod mailbox;
pub mod plan_cache;
pub mod scanners;

use scanners::{
    scan_audit_db, scan_chat_approvals, scan_chat_sessions, scan_hitl_approvals, scan_hitl_tasks,
    scan_llm_calls_db, scan_runtime_events, scan_trigger_history,
};

// ---------------------------------------------------------------------------
// Global Timeline
// ---------------------------------------------------------------------------

/// Global timeline event for display.
#[derive(Debug, Serialize)]
pub struct GlobalTimelineEvent {
    /// Event type: task, tool, llm, trigger, hitl.
    pub event_type: String,
    /// ISO 8601 timestamp.
    pub timestamp: String,
    /// Event summary.
    pub summary: String,
    /// Expandable JSON details.
    pub detail: serde_json::Value,
}

/// Parameters for `get_global_timeline`.
#[derive(Debug, Deserialize)]
pub struct TimelineParams {
    /// Time window in minutes (30, 60, 360, 720, 1440).
    pub window_minutes: u32,
}

/// Fetches an exhaustive global timeline by scanning the SQLite databases by
/// time window, **regardless of the originating task_id**.
///
/// The previous task-centric scan missed any operation not attached to a
/// persisted task: chat, triggers, runtime events outside a task. This version
/// queries each source directly by timestamp, surfacing 100% of the activity
/// visible in the window.
///
/// Scanned sources:
/// - `audit.db tool_invocations` → tool
/// - `llm_calls.db llm_calls` → llm
/// - `hitl.db tasks` (transitions_json) → task
/// - `hitl.db task_approvals` → hitl
/// - `chat.db chat_sessions` + `chat_approval_log` → task / hitl
/// - `triggers.db trigger_history` → task (trigger fire)
/// - `runtime_events.db runtime_events` (kinds: thought, agent_log, action_parse_error) → memory / task / error
#[tauri::command]
pub async fn get_global_timeline(
    state: State<'_, RuntimeHandle>,
    params: TimelineParams,
) -> Result<Vec<GlobalTimelineEvent>, String> {
    let cutoff = chrono::Utc::now() - chrono::Duration::minutes(i64::from(params.window_minutes));
    // ISO 8601 (UTC, no fractional secs), compatible with the canonical format
    // stored across all our SQLite tables and lexicographically comparable.
    let cutoff_str = cutoff.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // Resolve data_dir the same way the desktop bootstrapper does (main.rs).
    let data_dir = {
        let home = apollia_core::paths::home_dir_or_temp()
            .display()
            .to_string();
        apollia_core::paths::data_dir_under(home)
    };

    // Build agent_id/name → human-readable label map once. Used to humanise the
    // [prefix] of each event summary (e.g. `[veille-ia-agent]` instead of UUID).
    // We map both UUID id → name AND name → name so source rows storing either
    // form resolve identically.
    let mut agent_labels: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    if let Ok(agents) = state.registry_handle.list_agents().await {
        for entry in agents {
            let name = entry.manifest.name.clone();
            agent_labels.insert(entry.id.to_string(), name.clone());
            agent_labels.insert(name.clone(), name);
        }
    }

    // SQLite is sync, so push the entire scan onto a blocking thread.
    let result = tokio::task::spawn_blocking(move || {
        let mut events = Vec::<GlobalTimelineEvent>::new();
        scan_audit_db(&data_dir, &cutoff_str, &agent_labels, &mut events);
        scan_llm_calls_db(&data_dir, &cutoff_str, &agent_labels, &mut events);
        scan_hitl_tasks(&data_dir, &cutoff_str, &mut events);
        scan_hitl_approvals(&data_dir, &cutoff_str, &mut events);
        scan_chat_sessions(&data_dir, &cutoff_str, &mut events);
        scan_chat_approvals(&data_dir, &cutoff_str, &mut events);
        scan_trigger_history(&data_dir, &cutoff_str, &mut events);
        scan_runtime_events(&data_dir, &cutoff_str, &agent_labels, &mut events);
        events
    })
    .await
    .map_err(|e| format!("join error: {e}"))?;

    // Sort DESC by timestamp (most recent first).
    let mut sorted = result;
    sorted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(sorted)
}

/// Resolves a human-readable label for a raw identifier (agent_id, name, ...).
fn label_for(agent_key: &str, labels: &std::collections::HashMap<String, String>) -> String {
    labels
        .get(agent_key)
        .cloned()
        .unwrap_or_else(|| agent_key.to_string())
}

/// Cleanly truncates a string for summaries (max `max` chars plus an ellipsis).
fn trim_for_summary(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_timeline_event_serializes() {
        // GIVEN a GlobalTimelineEvent
        let event = GlobalTimelineEvent {
            event_type: "task".to_string(),
            timestamp: "2026-03-13T10:00:00Z".to_string(),
            summary: "[agent-1] Task → working".to_string(),
            detail: serde_json::json!({"status": "working"}),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&event).expect("serialize");

        // THEN all fields are correct
        assert_eq!(json["event_type"], "task");
        assert_eq!(json["summary"], "[agent-1] Task → working");
        assert_eq!(json["detail"]["status"], "working");
    }
}
