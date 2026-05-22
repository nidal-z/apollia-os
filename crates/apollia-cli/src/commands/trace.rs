//! `apollia-os trace` — fetch the event-sourced trace for a task.
//!
//! Wraps `GET /api/v1/tasks/{id}/trace` (ADR-088). Renders either the raw
//! JSON or a compact human-readable timeline.

use std::path::PathBuf;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

/// Execute `apollia-os trace <task_id> [--format human|json]`.
pub async fn run(task_id: &str, format_json: bool, socket: Option<PathBuf>, json: bool) -> i32 {
    let path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
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
        Ok(resp) if resp.status == 404 => {
            if json {
                let out = serde_json::json!({"error": format!("task '{task_id}' not found")});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: task '{task_id}' not found");
            }
            exit_codes::GENERAL_ERROR
        }
        Ok(resp) => {
            eprintln!("Error: HTTP {}: {}", resp.status, resp.body);
            exit_codes::GENERAL_ERROR
        }
        Err(ClientError::ConnectionRefused) => {
            eprintln!("Error: runtime not started");
            exit_codes::RUNTIME_ERROR
        }
        Err(e) => {
            eprintln!("Error: {e}");
            exit_codes::GENERAL_ERROR
        }
    }
}

/// Render the trace as a chronological timeline.
///
/// The runtime event schema (ADR-088, `runtime_events.db`) exposes:
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
        assert_eq!(format_timestamp("2026-05-22T07:59:02.369Z"), "07:59:02.369");
    }

    #[test]
    fn timestamp_handles_missing_subseconds() {
        assert_eq!(format_timestamp("2026-05-22T12:34:56Z"), "12:34:56");
    }

    #[test]
    fn timestamp_empty_returns_blank_padding() {
        // Matches the column width so missing-ts rows still align.
        assert_eq!(format_timestamp(""), "            ");
    }

    #[test]
    fn summarize_tool_call_started_carries_tool_and_args() {
        let payload = serde_json::json!({
            "tool_name": "web_search",
            "args_json": "{\"query\":\"foo\"}",
        });
        let s = summarize_payload("tool_call_started", &payload);
        assert!(s.starts_with("web_search"), "got {s}");
        assert!(s.contains("\"query\":\"foo\""), "got {s}");
    }

    #[test]
    fn summarize_tool_call_completed_shows_duration_only_when_present() {
        let payload = serde_json::json!({"duration_ms": 1367, "exit_code": null});
        let s = summarize_payload("tool_call_completed", &payload);
        assert!(s.contains("1367ms"), "got {s}");
    }

    #[test]
    fn summarize_llm_call_completed_reports_tokens_and_cost() {
        let payload = serde_json::json!({
            "prompt_tokens": 120,
            "completion_tokens": 45,
            "duration_ms": 800,
            "cost_usd": 0.00123,
        });
        let s = summarize_payload("llm_call_completed", &payload);
        assert!(s.contains("120→45"), "got {s}");
        assert!(s.contains("800ms"), "got {s}");
        assert!(s.contains("$0.00123"), "got {s}");
    }

    #[test]
    fn summarize_agent_log_keeps_level_and_message() {
        let payload = serde_json::json!({"level": "warn", "message": "stale cache"});
        let s = summarize_payload("agent_log", &payload);
        assert_eq!(s, "[warn] stale cache");
    }

    #[test]
    fn summarize_unknown_kind_falls_back_to_serialized_payload() {
        let payload = serde_json::json!({"foo": 1, "bar": "baz"});
        let s = summarize_payload("custom_kind", &payload);
        assert!(s.contains("foo"));
        assert!(s.contains("baz"));
    }
}
