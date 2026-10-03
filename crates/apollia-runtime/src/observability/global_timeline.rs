//! The global timeline of the Observability view: every store of the data
//! directory scanned by time window, regardless of the originating task.
//!
//! A task-centric scan misses any operation not attached to a persisted task:
//! chat, triggers, runtime events outside a task. This reader queries each
//! source directly by timestamp, surfacing all the activity visible in the
//! window. It is served by `GET /api/v1/observability/timeline`, which the
//! desktop calls in both of its modes.
//!
//! Scanned sources:
//! - `audit.db tool_invocations` → tool
//! - `llm_calls.db llm_calls` → llm
//! - `hitl.db tasks` (transitions_json) → task
//! - `hitl.db task_approvals` → hitl
//! - `chat.db chat_sessions` + `chat_approval_log` → task / hitl
//! - `triggers.db trigger_history` → task (trigger fire)
//! - `runtime_events.db runtime_events` (kinds: thought, agent_log, action_parse_error) → memory / task / error

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::registry::AgentRegistryHandle;

/// The readers the timeline aggregates, one per store.
mod scanners;

use scanners::{
    scan_audit_db, scan_chat_approvals, scan_chat_sessions, scan_hitl_approvals, scan_hitl_tasks,
    scan_llm_calls_db, scan_runtime_events, scan_trigger_history,
};

/// Global timeline event for display.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct GlobalTimelineEvent {
    /// Event type: task, tool, llm, trigger, hitl, memory, error.
    pub event_type: String,
    /// ISO 8601 timestamp.
    pub timestamp: String,
    /// Event summary.
    pub summary: String,
    /// Expandable JSON details.
    #[schema(value_type = Object)]
    pub detail: serde_json::Value,
}

/// Errors raised while reading the timeline.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GlobalTimelineError {
    /// The blocking scan task failed to join.
    #[error("timeline scan task failed: {0}")]
    Join(String),
}

/// Reads the timeline of the last `window_minutes` minutes, most recent first.
///
/// Agent identifiers are humanised from `registry` (both the UUID and the name
/// resolve to the name), so a row reads `[veille-ia-agent]` rather than a UUID.
///
/// # Errors
///
/// [`GlobalTimelineError::Join`] when the blocking scan does not complete. A
/// store that is absent, locked or from an older schema yields no rows.
pub async fn read_global_timeline(
    registry: &AgentRegistryHandle,
    data_dir: PathBuf,
    window_minutes: u32,
) -> Result<Vec<GlobalTimelineEvent>, GlobalTimelineError> {
    let cutoff = chrono::Utc::now() - chrono::Duration::minutes(i64::from(window_minutes));
    // ISO 8601 (UTC, no fractional secs), compatible with the canonical format
    // stored across all the SQLite tables and lexicographically comparable.
    let cutoff_str = cutoff.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let mut agent_labels: HashMap<String, String> = HashMap::new();
    if let Ok(agents) = registry.list_agents().await {
        for entry in agents {
            let name = entry.manifest.name.clone();
            agent_labels.insert(entry.id.to_string(), name.clone());
            agent_labels.insert(name.clone(), name);
        }
    }

    let mut events = tokio::task::spawn_blocking(move || {
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
    .map_err(|e| GlobalTimelineError::Join(e.to_string()))?;

    events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(events)
}

/// Resolves a human-readable label for a raw identifier (agent_id, name, ...).
fn label_for(agent_key: &str, labels: &HashMap<String, String>) -> String {
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
