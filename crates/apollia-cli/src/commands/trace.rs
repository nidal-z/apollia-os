//! `apollia-os trace`: fetch the event-sourced trace for a task.
//!
//! Wraps `GET /api/v1/tasks/{id}/trace`. Renders either the raw JSON or a
//! compact human-readable timeline.

use std::path::PathBuf;

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;

/// Execute `apollia-os trace <task_id> [--format human|json]`.
pub async fn run(task_id: &str, format_json: bool, socket: Option<PathBuf>, json: bool) -> i32 {
    let path = socket.unwrap_or_else(default_socket_path);
    let client = RuntimeClient::new(path);
    let uri = format!("/api/v1/tasks/{task_id}/trace");
    match client.get(&uri).await {
        Ok(resp) if resp.status < 400 => {
            if json || format_json {
                println!("{}", resp.body);
            } else {
                render_trace_human(&resp.body);
            }
            exit_codes::SUCCESS
        }
        Ok(resp) if resp.status == 404 => crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("task '{task_id}' not found"),
        ),
        Ok(resp) => crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("HTTP {}: {}", resp.status, resp.body),
        ),
        Err(ClientError::ConnectionRefused) => {
            crate::output::emit_error(json, exit_codes::RUNTIME_ERROR, "runtime not started")
        }
        Err(e) => crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string()),
    }
}

/// Render the trace as a chronological timeline.
///
/// The runtime event schema (`runtime_events.db`) exposes:
///   `ts` (RFC3339 timestamp), `kind`, `agent_id`, `step_num`,
///   `payload_json` (kind-specific stringified JSON).
///
/// Per-kind summary is extracted from `payload_json` so the operator can
/// read what each event did at a glance, not just its type.
fn render_trace_human(body: &str) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
        println!("{body}");
        return;
    };
    let events = v
        .get("events")
        .and_then(|e| e.as_array())
        .cloned()
        .unwrap_or_default();
    if events.is_empty() {
        println!("(no events)");
        return;
    }
    for ev in &events {
        let ts_full = ev.get("ts").and_then(|x| x.as_str()).unwrap_or("");
        let ts = format_timestamp(ts_full);
        let kind = ev.get("kind").and_then(|x| x.as_str()).unwrap_or("?");
        let step = ev
            .get("step_num")
            .and_then(|v| v.as_u64())
            .map(|n| format!("#{n}"))
            .unwrap_or_else(|| "  ".to_string());
        let payload = ev
            .get("payload_json")
            .and_then(|x| x.as_str())
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());
        let detail = payload
            .as_ref()
            .map(|p| summarize_payload(kind, p))
            .unwrap_or_default();
        println!("  {ts}  {step:<4}  {kind:<22}  {detail}");
    }
}

/// Truncate an RFC3339 timestamp to `HH:MM:SS.mmm` for compact display.
///
/// Falls back to the raw value when parsing fails. `"2026-05-22T07:59:02.369Z"`
/// → `"07:59:02.369"`. Empty input → empty string (caller pads).
fn format_timestamp(rfc3339: &str) -> String {
    if rfc3339.is_empty() {
        return "            ".to_string();
    }
    if let Some((_, time_part)) = rfc3339.split_once('T') {
        let trimmed = time_part.trim_end_matches('Z');
        let cut = trimmed
            .char_indices()
            .nth(12)
            .map(|(i, _)| i)
            .unwrap_or(trimmed.len());
        return trimmed[..cut].to_string();
    }
    rfc3339.to_string()
}

/// Extract a one-line summary from the kind-specific payload.
fn summarize_payload(kind: &str, payload: &serde_json::Value) -> String {
    match kind {
        "agent_log" => {
            let level = payload
                .get("level")
                .and_then(|v| v.as_str())
                .unwrap_or("info");
            let msg = payload
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            format!("[{level}] {}", truncate(msg, 96))
        }
        "tool_call_started" => {
            let tool = payload
                .get("tool_name")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let args = payload
                .get("args_json")
                .and_then(|v| v.as_str())
                .map(|s| truncate(s, 80))
                .unwrap_or_default();
            format!("{tool}  {args}")
        }
        "tool_call_completed" => {
            let duration = payload
                .get("duration_ms")
                .and_then(|v| v.as_u64())
                .map(|d| format!("{d}ms"))
                .unwrap_or_default();
            let exit = payload
                .get("exit_code")
                .and_then(|v| v.as_i64())
                .map(|c| format!("exit={c}"))
                .unwrap_or_default();
            let error = payload
                .get("error")
                .and_then(|v| v.as_str())
                .map(|e| format!(" error={}", truncate(e, 64)))
                .unwrap_or_default();
            format!("{duration} {exit}{error}").trim().to_string()
        }
        "llm_call_started" => {
            let backend = payload
                .get("backend")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let model = payload.get("model").and_then(|v| v.as_str()).unwrap_or("");
            format!("{backend} {model}")
        }
        "llm_call_completed" => {
            let prompt = payload
                .get("prompt_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let completion = payload
                .get("completion_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let duration = payload
                .get("duration_ms")
                .and_then(|v| v.as_u64())
                .map(|d| format!("{d}ms"))
                .unwrap_or_default();
            let cost = payload
                .get("cost_usd")
                .and_then(|v| v.as_f64())
                .map(|c| format!(" ${c:.5}"))
                .unwrap_or_default();
            format!("{prompt}→{completion} tok  {duration}{cost}")
        }
        _ => {
            // Fallback: serialise the payload to a single line.
            truncate(&payload.to_string(), 100)
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_keeps_hh_mm_ss_ms_from_rfc3339() {
        // GIVEN a full timestamp with milliseconds
        // WHEN it is formatted for the trace column
        // THEN only the time of day is kept, milliseconds included
        assert_eq!(format_timestamp("2026-05-22T07:59:02.369Z"), "07:59:02.369");
    }

    #[test]
    fn timestamp_handles_missing_subseconds() {
        // GIVEN a timestamp with no subsecond part
        // WHEN it is formatted for the trace column
        // THEN the time of day is kept, with no invented milliseconds
        assert_eq!(format_timestamp("2026-05-22T12:34:56Z"), "12:34:56");
    }

    #[test]
    fn timestamp_empty_returns_blank_padding() {
        // GIVEN a row carrying no timestamp
        // Matches the column width so missing-ts rows still align.
        // WHEN it is formatted for the trace column
        // THEN the width is filled with spaces so the columns still line up
        assert_eq!(format_timestamp(""), "            ");
    }

    #[test]
    fn summarize_tool_call_started_carries_tool_and_args() {
        // GIVEN a tool call start carrying its tool name and its arguments
        let payload = serde_json::json!({
            "tool_name": "web_search",
            "args_json": "{\"query\":\"foo\"}",
        });
        // WHEN it is summarised for the trace
        let s = summarize_payload("tool_call_started", &payload);
        // THEN the line opens on the tool and carries the arguments
        assert!(s.starts_with("web_search"), "got {s}");
        assert!(s.contains("\"query\":\"foo\""), "got {s}");
    }

    #[test]
    fn summarize_tool_call_completed_shows_duration_only_when_present() {
        // GIVEN a tool call completion carrying a duration and no exit code
        let payload = serde_json::json!({"duration_ms": 1367, "exit_code": null});
        // WHEN it is summarised for the trace
        let s = summarize_payload("tool_call_completed", &payload);
        // THEN the duration is on the line
        assert!(s.contains("1367ms"), "got {s}");
    }

    #[test]
    fn summarize_llm_call_completed_reports_tokens_and_cost() {
        // GIVEN an LLM call completion carrying tokens, duration and cost
        let payload = serde_json::json!({
            "prompt_tokens": 120,
            "completion_tokens": 45,
            "duration_ms": 800,
            "cost_usd": 0.00123,
        });
        // WHEN it is summarised for the trace
        let s = summarize_payload("llm_call_completed", &payload);
        // THEN the line carries the token move, the duration and the cost
        assert!(s.contains("120→45"), "got {s}");
        assert!(s.contains("800ms"), "got {s}");
        assert!(s.contains("$0.00123"), "got {s}");
    }

    #[test]
    fn summarize_agent_log_keeps_level_and_message() {
        // GIVEN an agent log line carrying a level and a message
        let payload = serde_json::json!({"level": "warn", "message": "stale cache"});
        // WHEN it is summarised for the trace
        let s = summarize_payload("agent_log", &payload);
        // THEN both are kept, the level in front
        assert_eq!(s, "[warn] stale cache");
    }

    #[test]
    fn summarize_unknown_kind_falls_back_to_serialized_payload() {
        // GIVEN an event of a kind the summariser knows nothing about
        let payload = serde_json::json!({"foo": 1, "bar": "baz"});
        // WHEN it is summarised for the trace
        let s = summarize_payload("custom_kind", &payload);
        // THEN the payload is printed as it is rather than the line being dropped
        assert!(s.contains("foo"));
        assert!(s.contains("baz"));
    }
}
