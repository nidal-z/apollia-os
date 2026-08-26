//! Approval rendering and the small pure helpers the verbs share.

use apollia_oria::plan_repository::StepRecord;
use chrono::{DateTime, Utc};

use crate::client::{ClientError, RawResponse};
use crate::exit_codes;

use super::{MAX_OUTPUT_LEN, MAX_PROMPT_LEN};

/// Format the approvals list as a human-readable table.
///
/// Columns: `ID | TASK_ID | AGENT | DECISION | DATE`
pub(super) fn format_approvals_list(resp: &serde_json::Value, pending: bool) {
    let key = if pending { "pending" } else { "approvals" };
    let approvals = resp
        .get(key)
        .or_else(|| resp.get("approvals"))
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default();

    let header = if pending {
        "PENDING APPROVALS"
    } else {
        "RESOLVED APPROVALS"
    };
    println!("  {header}");
    println!(
        "  {:<36} {:<36} {:<20} {:<10} DATE",
        "ID", "TASK_ID", "AGENT", "DECISION"
    );

    if approvals.is_empty() {
        println!("  (none)");
        return;
    }

    for a in &approvals {
        print_approval_row(a, pending);
    }
}

/// Maps the approval decision to its display label.
pub(super) fn approval_decision(a: &serde_json::Value, pending: bool) -> &'static str {
    if pending {
        return "en attente";
    }
    match a.get("approved").and_then(|v| v.as_bool()) {
        Some(true) => "approved",
        Some(false) => "rejected",
        None => "?",
    }
}

/// Renders a single approvals-table row.
pub(super) fn print_approval_row(a: &serde_json::Value, pending: bool) {
    let id = a.get("id").and_then(|v| v.as_str()).unwrap_or("?");
    let task_id = a.get("task_id").and_then(|v| v.as_str()).unwrap_or("?");
    let agent = a.get("agent_id").and_then(|v| v.as_str()).unwrap_or("?");
    let decision = approval_decision(a, pending);
    let date = a
        .get("resolved_at")
        .or_else(|| a.get("requested_at"))
        .and_then(|v| v.as_str())
        .map(|s| if s.len() >= 19 { &s[..19] } else { s })
        .unwrap_or("?");
    println!(
        "  {:<36} {:<36} {:<20} {:<10} {}",
        id, task_id, agent, decision, date
    );
}

/// Extract the `tasks` array from a server response, defaulting to an empty vec.
pub(super) fn extract_tasks_array(resp: &serde_json::Value) -> Vec<serde_json::Value> {
    resp.get("tasks")
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default()
}

/// Extract an error message from a raw response body, with a fallback.
pub(super) fn extract_error_message(resp: &RawResponse, fallback: &str) -> String {
    serde_json::from_str::<serde_json::Value>(&resp.body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
        .unwrap_or_else(|| fallback.to_string())
}

/// Format a duration since an RFC3339 timestamp as a human-readable string.
///
/// Returns `"Xmin"` for durations under one hour, `"Xh"` otherwise.
/// Returns `"-"` if the timestamp cannot be parsed.
pub fn format_duration_since(input_required_at: &str) -> String {
    let Ok(dt) = input_required_at.parse::<DateTime<Utc>>() else {
        return "-".to_string();
    };
    let elapsed = Utc::now().signed_duration_since(dt);
    let mins = elapsed.num_minutes().max(0);
    if mins < 1 {
        "< 1min".to_string()
    } else if mins < 60 {
        format!("{mins}min")
    } else {
        format!("{}h", mins / 60)
    }
}

/// Compute elapsed seconds since an RFC3339 timestamp (for JSON `waiting_since_secs`).
///
/// Returns `0` if the timestamp cannot be parsed or is in the future.
pub fn elapsed_seconds(input_required_at: &str) -> u64 {
    let Ok(dt) = input_required_at.parse::<DateTime<Utc>>() else {
        return 0;
    };
    Utc::now().signed_duration_since(dt).num_seconds().max(0) as u64
}

/// Truncate a prompt string to [`MAX_PROMPT_LEN`] bytes, appending `"..."`.
pub fn truncate_prompt(prompt: &str) -> String {
    crate::commands::truncate_for_display(prompt, MAX_PROMPT_LEN, MAX_PROMPT_LEN, "...")
}

/// Return the Unicode status icon for a step status string.
pub(super) fn step_status_icon(status: &str) -> &'static str {
    match status {
        "completed" => "✔",
        "failed" => "✗",
        "running" => "●",
        "skipped" => "⏸",
        _ => "○",
    }
}

/// Compute a human-readable duration for a step, or `"-"` if unavailable.
pub(super) fn step_duration(step: &StepRecord) -> &'static str {
    if step.started_at.is_some() && step.completed_at.is_some() {
        "?"
    } else {
        "-"
    }
}

/// Truncate `output` to [`MAX_OUTPUT_LEN`] bytes, appending `"..."` if truncated.
pub(super) fn truncate_output(output: &str) -> String {
    crate::commands::truncate_for_display(output, MAX_OUTPUT_LEN, MAX_OUTPUT_LEN, "...")
}

/// Handle client errors uniformly.
pub(super) fn handle_error(err: ClientError, json: bool) -> i32 {
    match err {
        ClientError::ConnectionRefused => crate::output::emit_error(
            json,
            exit_codes::RUNTIME_ERROR,
            "runtime not started (connection refused)",
        ),
        other => crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &other.to_string()),
    }
}

/// Handle HTTP server errors.
pub(super) fn handle_server_error(status: u16, body: &str, json: bool) -> i32 {
    let error_msg = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
        .unwrap_or_else(|| format!("server error ({status})"));

    crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &error_msg.to_string())
}
