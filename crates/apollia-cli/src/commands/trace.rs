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
        let at = ev.get("at").and_then(|x| x.as_str()).unwrap_or("?");
        let kind = ev
            .get("kind")
            .or_else(|| ev.get("type"))
            .and_then(|x| x.as_str())
            .unwrap_or("?");
        let detail = ev
            .get("detail")
            .or_else(|| ev.get("payload"))
            .map(|x| x.to_string())
            .unwrap_or_default();
        println!("  {at}  {kind:<24}  {detail}");
    }
}
