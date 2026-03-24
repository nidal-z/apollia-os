//! `apollia-os run <agent> <input>` — submit a task and wait for the result.
//!
//! Supports `--stream` for real-time SSE streaming of task progress.
//! In orchestrated mode, the stream displays the execution plan, step-by-step
//! progression, and replanning events. In direct mode, the pre-Sprint 10
//! behaviour is preserved.

use std::path::PathBuf;
use std::time::Instant;

use futures::StreamExt;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

// ─── SSE types ───────────────────────────────────────────────────────────────

/// A parsed Server-Sent Event received on the task stream.
pub struct SseEvent {
    /// The `event` field extracted from the JSON payload (e.g. `"plan_generated"`).
    pub event_type: String,
    /// The full parsed JSON data payload.
    pub data: serde_json::Value,
    /// The original raw JSON string as received on the wire.
    pub raw_json: String,
}

// ─── Display state ────────────────────────────────────────────────────────────

/// Display state maintained across SSE events for an orchestrated run.
pub struct RunDisplayState {
    /// ID of the current execution plan, if one was received.
    pub plan_id: Option<String>,
    /// Total number of steps in the current plan.
    pub step_count: usize,
    /// Sequential number of the step currently in progress.
    pub current_num: usize,
    /// Whether `--json` raw mode is active.
    pub json_mode: bool,
    /// When `true`, suppress all intermediate events — only terminal events produce output.
    ///
    /// Used by the default (non-`--stream`) `run` invocation to display only the final
    /// result while still using SSE internally to receive the agent output.
    pub quiet: bool,
}

impl RunDisplayState {
    /// Create a new display state.
    pub fn new(json_mode: bool, quiet: bool) -> Self {
        Self {
            plan_id: None,
            step_count: 0,
            current_num: 0,
            json_mode,
            quiet,
        }
    }
}

// ─── Event handler ────────────────────────────────────────────────────────────

/// Handle one SSE event and update the display accordingly.
///
/// Returns `true` when the event is terminal (stream should be closed).
///
/// In `--json` mode every event is printed as a raw JSON line; no human
/// formatting is applied.  In TTY mode, orchestrated plan events render the
/// plan tree, step progress (`●`/`✔`/`✗`), replanning notices, and final
/// result.  Direct-mode events (`step`, `completed`, `failed`, `canceled`)
/// fall through to their original handlers.
pub fn handle_sse_event(event: &SseEvent, state: &mut RunDisplayState) -> bool {
    if state.json_mode {
        println!("{}", event.raw_json);
        return matches!(
            event.event_type.as_str(),
            "completed" | "canceled" | "failed" | "plan_failed"
        );
    }

    // In quiet mode only terminal events produce output — intermediate plan/step
    // events are silently consumed.  This is used by the default (non-`--stream`)
    // invocation so that the final agent output is still surfaced cleanly.
    if state.quiet {
        return match event.event_type.as_str() {
            "completed" => {
                if let Some(result) = event.data["result"].as_str() {
                    println!("{result}");
                }
                true
            }
            "failed" => {
                let error = event.data["error"].as_str().unwrap_or("unknown error");
                eprintln!("  x Échec : {error}");
                true
            }
            "plan_failed" => {
                let reason = event.data["reason"].as_str().unwrap_or("Erreur inconnue");
                eprintln!("  ✗ Plan échoué : {reason}");
                true
            }
            "canceled" => {
                eprintln!("  Tâche annulée.");
                true
            }
            _ => false,
        };
    }

    match event.event_type.as_str() {
        // ── Orchestrated: plan generated ──────────────────────────────────
        "plan_generated" => {
            let step_count = event.data["step_count"].as_u64().unwrap_or(0) as usize;
            state.step_count = step_count;
            state.plan_id = event.data["plan_id"].as_str().map(String::from);
            eprintln!();
            println!("  Plan généré ({step_count} étapes) :");
            if let Some(steps) = event.data["steps"].as_array() {
                let last = steps.len().saturating_sub(1);
                for (i, step) in steps.iter().enumerate() {
                    let id = step["step_id"].as_str().unwrap_or("?");
                    let desc = step["description"].as_str().unwrap_or("?");
                    let tool = step["tool_hint"].as_str().unwrap_or("llm");
                    let deps = step["depends_on"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();
                    let deps_str = if deps.is_empty() {
                        String::new()
                    } else {
                        format!("  (attend {deps})")
                    };
                    let branch = if i == last { "└──" } else { "├──" };
                    println!("  {branch} [{id}] {desc}  → {tool}{deps_str}");
                }
            }
            eprintln!();
            false
        }

        // ── Orchestrated: individual step started ─────────────────────────
        "step_started" => {
            let num = event.data["num"].as_u64().unwrap_or(0);
            let total = event.data["total"]
                .as_u64()
                .unwrap_or(state.step_count as u64);
            let desc = event.data["desc"].as_str().unwrap_or("?");
            state.current_num = num as usize;
            print!("  ● [{num}/{total}] {desc}...");
            false
        }

        // ── Orchestrated: individual step completed ───────────────────────
        "step_completed" => {
            let duration_ms = event.data["duration_ms"].as_u64().unwrap_or(0);
            let secs = duration_ms as f64 / 1000.0;
            println!(
                "\r  ✔ [{}/{}] (complété)  {:.1}s",
                state.current_num, state.step_count, secs
            );
            false
        }

        // ── Orchestrated: individual step failed (not necessarily fatal) ──
        "step_failed" => {
            let duration_ms = event.data["duration_ms"].as_u64().unwrap_or(0);
            let error = event.data["error"].as_str().unwrap_or("?");
            let retryable = event.data["retryable"].as_bool().unwrap_or(false);
            let secs = duration_ms as f64 / 1000.0;
            println!(
                "\r  ✗ [{}/{}] {error}  {:.1}s",
                state.current_num, state.step_count, secs
            );
            if !retryable {
                eprintln!("  Erreur non-récupérable.");
            }
            false
        }

        // ── Orchestrated: replanning notice ───────────────────────────────
        "replanning" => {
            let attempt = event.data["attempt"].as_u64().unwrap_or(1);
            let failed_step = event.data["failed_step"].as_str().unwrap_or("?");
            let reason = event.data["reason"].as_str().unwrap_or("?");
            println!("  ↻ Replanification ({attempt}/2) — step {failed_step} échoué : {reason}");
            false
        }

        // ── Orchestrated: plan completed (all steps done) ─────────────────
        "plan_completed" => {
            let step_count = event.data["step_count"].as_u64().unwrap_or(0);
            let duration_ms = event.data["duration_ms"].as_u64().unwrap_or(0);
            let secs = duration_ms as f64 / 1000.0;
            println!();
            println!("  ✔ Plan complété — {step_count} steps en {secs:.1}s");
            false
        }

        // ── Orchestrated: plan failed (unrecoverable) — terminal ──────────
        "plan_failed" => {
            let reason = event.data["reason"].as_str().unwrap_or("Erreur inconnue");
            eprintln!();
            eprintln!("  ✗ Plan échoué : {reason}");
            true
        }

        // ── Common: task completed (direct or orchestrated) — terminal ────
        "completed" => {
            if let Some(result) = event.data["result"].as_str() {
                println!();
                println!("{result}");
            }
            println!();
            true
        }

        // ── Common: task failed (direct mode) — terminal ──────────────────
        "failed" => {
            let error = event.data["error"].as_str().unwrap_or("unknown error");
            eprintln!("  x Échec : {error}");
            true
        }

        // ── Common: task canceled — terminal ──────────────────────────────
        "canceled" => {
            eprintln!("  Tâche annulée.");
            true
        }

        // ── Common: task picked up by executor — shows the stream is live ──
        "started" => {
            let agent = event.data["agent_id"].as_str().unwrap_or("?");
            println!("  ~ Running on {agent}...");
            false
        }

        // ── Direct mode legacy: step progress ─────────────────────────────
        "step" => {
            let step = event.data["step"].as_u64().unwrap_or(0);
            let tool = event.data["tool"]
                .as_str()
                .map(|t| format!(" tool_call {t}"))
                .unwrap_or_default();
            println!("  ~ Step {step}:{tool}");
            false
        }

        _ => false,
    }
}

// ─── Public entry point ───────────────────────────────────────────────────────

/// Execute the `run` command.
///
/// Submits a task to the specified agent and waits for the result.
/// With `--detach`, returns immediately after submission and prints the task ID.
/// Returns the process exit code.
pub async fn run(
    agent_id: &str,
    input: &str,
    socket: Option<PathBuf>,
    json: bool,
    stream: bool,
    detach: bool,
) -> i32 {
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let client = RuntimeClient::new(socket_path);
    let start = Instant::now();

    // Submit the task using the A2A-aligned AIPInput format so Python agents can read
    // parts[0]["text"] directly (see AIPPart::Text serialisation in apollia-core).
    let input_value = serde_json::json!({
        "parts": [{"type": "text", "text": input}]
    });
    let submit_result = client.submit_task(agent_id, input_value).await;

    let task_json = match submit_result {
        Ok(j) => j,
        Err(ClientError::ConnectionRefused) => {
            return output_error(
                "runtime not started (connection refused)",
                json,
                exit_codes::RUNTIME_ERROR,
            );
        }
        Err(e) => {
            return output_error(&e.to_string(), json, exit_codes::GENERAL_ERROR);
        }
    };

    let task_id = match task_json.get("task_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => {
            return output_error(
                "missing task_id in response",
                json,
                exit_codes::GENERAL_ERROR,
            );
        }
    };

    // --detach: fire-and-forget — print task_id and return immediately.
    if detach {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"task_id": task_id, "agent_id": agent_id, "status": "submitted"})
                )
                .unwrap_or_default()
            );
        } else {
            println!("  -> Task {task_id} submitted to {agent_id}");
            println!("     Track with: apollia-os task status {task_id}");
        }
        return exit_codes::SUCCESS;
    }

    if !json {
        println!("  -> Task {task_id} submitted to {agent_id}");
    }

    // Default path uses polling: GET /api/v1/tasks/:id until completion.
    // The router now stores task output alongside status (see router.rs), so polling
    // correctly surfaces the agent result without SSE race conditions.
    //
    // With `--stream`: SSE streaming shows plan/step events in real time.
    if stream {
        stream_task(&client, &task_id, json, start, false).await
    } else {
        poll_task(&client, &task_id, json, start).await
    }
}

// ─── Private helpers ──────────────────────────────────────────────────────────

/// Poll task status until completion.
async fn poll_task(client: &RuntimeClient, task_id: &str, json: bool, start: Instant) -> i32 {
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let result = client.get_task(task_id).await;
        match result {
            Ok(task_json) => {
                let status = task_json
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                match status {
                    "completed" => {
                        let elapsed = start.elapsed();
                        if json {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&task_json).unwrap_or_default()
                            );
                        } else {
                            if let Some(text) = task_json.get("result").and_then(|v| v.as_str()) {
                                println!("{text}");
                            }
                            println!("  * Completed in {:.1}s", elapsed.as_secs_f64());
                        }
                        return exit_codes::SUCCESS;
                    }
                    "failed" => {
                        let elapsed = start.elapsed();
                        let error_msg = task_json
                            .get("error")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown error");
                        if json {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&task_json).unwrap_or_default()
                            );
                        } else {
                            eprintln!("  x Failed in {:.1}s: {error_msg}", elapsed.as_secs_f64());
                        }
                        return exit_codes::TASK_FAILED;
                    }
                    "canceled" => {
                        if json {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&task_json).unwrap_or_default()
                            );
                        } else {
                            println!("  Task {task_id} was canceled");
                        }
                        return exit_codes::GENERAL_ERROR;
                    }
                    _ => continue,
                }
            }
            Err(e) => {
                return output_error(&e.to_string(), json, exit_codes::GENERAL_ERROR);
            }
        }
    }
}

/// Stream task events via SSE and display them using [`handle_sse_event`].
///
/// Connects to `GET /api/v1/tasks/{id}/stream` using [`RuntimeClient::stream_sse_lines`],
/// which reads the HTTP body incrementally — one line at a time as the server flushes it.
/// Each SSE frame is parsed and dispatched to `handle_sse_event` immediately, producing
/// real-time output instead of waiting for the full response to buffer.
///
/// Returns when a terminal event is received or falls back to polling if the
/// stream closes without a terminal event (e.g. race on task already completed).
///
/// When `quiet` is `true`, intermediate events are suppressed and only the final
/// agent output is printed (used by the default non-`--stream` path).
async fn stream_task(
    client: &RuntimeClient,
    task_id: &str,
    json: bool,
    start: Instant,
    quiet: bool,
) -> i32 {
    let uri = format!("/api/v1/tasks/{task_id}/stream");
    let mut line_stream = match client.stream_sse_lines(&uri).await {
        Ok(s) => s,
        Err(ClientError::ConnectionRefused) => {
            return output_error(
                "runtime not started (connection refused)",
                json,
                exit_codes::RUNTIME_ERROR,
            );
        }
        Err(e) => {
            return output_error(&e.to_string(), json, exit_codes::GENERAL_ERROR);
        }
    };

    let mut state = RunDisplayState::new(json, quiet);
    let mut terminal_event_type = String::new();

    // Each line arrives as soon as the server flushes it — true streaming.
    while let Some(line_result) = line_stream.next().await {
        let line = match line_result {
            Ok(l) => l,
            Err(e) => {
                eprintln!("  x Stream error: {e}");
                break;
            }
        };

        if let Some(data) = line.strip_prefix("data: ") {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                let event_type = parsed
                    .get("event")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let sse_event = SseEvent {
                    event_type: event_type.clone(),
                    data: parsed,
                    raw_json: data.to_string(),
                };

                if handle_sse_event(&sse_event, &mut state) {
                    terminal_event_type = event_type;
                    break;
                }
            }
        }
    }

    // Map terminal event type to exit code
    match terminal_event_type.as_str() {
        "completed" => exit_codes::SUCCESS,
        "failed" => exit_codes::TASK_FAILED,
        "plan_failed" | "canceled" => exit_codes::GENERAL_ERROR,
        // No terminal event — stream closed early (task already done) → fall back to polling
        _ => poll_task(client, task_id, json, start).await,
    }
}

/// Output an error and return the given exit code.
fn output_error(msg: &str, json: bool, code: i32) -> i32 {
    if json {
        let err = serde_json::json!({"error": msg});
        println!("{}", serde_json::to_string_pretty(&err).unwrap_or_default());
    } else {
        eprintln!("Error: {msg}");
    }
    code
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(event_type: &str, data: serde_json::Value) -> SseEvent {
        SseEvent {
            event_type: event_type.to_string(),
            raw_json: data.to_string(),
            data,
        }
    }

    // plan_generated updates state and is NOT terminal
    #[test]
    fn test_ac1_plan_generated_handler() {
        // GIVEN
        let event = make_event(
            "plan_generated",
            serde_json::json!({
                "plan_id": "p-001",
                "step_count": 3,
                "agent_name": "test",
                "steps": []
            }),
        );
        let mut state = RunDisplayState::new(false, false);

        // WHEN
        let terminal = handle_sse_event(&event, &mut state);

        // THEN
        assert!(!terminal);
        assert_eq!(state.step_count, 3);
        assert_eq!(state.plan_id.as_deref(), Some("p-001"));
    }

    // plan tree rendered with dependencies
    #[test]
    fn test_ac1_plan_generated_with_steps() {
        // GIVEN — 2 steps, second depends on first
        let event = make_event(
            "plan_generated",
            serde_json::json!({
                "plan_id": "p-002",
                "step_count": 2,
                "steps": [
                    {"step_id": "s1", "description": "fetch data", "tool_hint": "file_io", "depends_on": []},
                    {"step_id": "s2", "description": "summarise", "tool_hint": "llm", "depends_on": ["s1"]}
                ]
            }),
        );
        let mut state = RunDisplayState::new(false, false);

        // WHEN
        let terminal = handle_sse_event(&event, &mut state);

        // THEN
        assert!(!terminal);
        assert_eq!(state.step_count, 2);
    }

    // step_started updates current_num and is NOT terminal
    #[test]
    fn test_ac2_step_started_not_terminal() {
        // GIVEN
        let event = make_event(
            "step_started",
            serde_json::json!({"num": 1, "total": 4, "step_id": "s1", "desc": "fetch data"}),
        );
        let mut state = RunDisplayState::new(false, false);

        // WHEN
        let terminal = handle_sse_event(&event, &mut state);

        // THEN
        assert!(!terminal);
        assert_eq!(state.current_num, 1);
    }

    // replanning is NOT terminal
    #[test]
    fn test_ac3_replanning_not_terminal() {
        // GIVEN
        let event = make_event(
            "replanning",
            serde_json::json!({"attempt": 1, "failed_step": "s3", "reason": "timeout"}),
        );
        let mut state = RunDisplayState::new(false, false);

        // WHEN
        let terminal = handle_sse_event(&event, &mut state);

        // THEN — replanning is informational, not terminal
        assert!(!terminal);
    }

    // plan_failed is terminal
    #[test]
    fn test_ac4_plan_failed_est_terminal() {
        // GIVEN
        let event = make_event("plan_failed", serde_json::json!({"reason": "MAX_REPLAN"}));
        let mut state = RunDisplayState::new(false, false);

        // WHEN
        let terminal = handle_sse_event(&event, &mut state);

        // THEN
        assert!(terminal);
    }

    // completed is terminal
    #[test]
    fn test_ac5_completed_est_terminal() {
        // GIVEN
        let event = make_event("completed", serde_json::json!({"result": "Résultat final"}));
        let mut state = RunDisplayState::new(false, false);

        // WHEN
        let terminal = handle_sse_event(&event, &mut state);

        // THEN
        assert!(terminal);
    }

    // direct-mode events (no plan_* events) still work
    #[test]
    fn test_ac5_direct_mode_step_not_terminal() {
        // GIVEN — legacy direct-mode step event
        let event = make_event("step", serde_json::json!({"step": 1, "tool": "file_io"}));
        let mut state = RunDisplayState::new(false, false);

        // WHEN
        let terminal = handle_sse_event(&event, &mut state);

        // THEN — not terminal, state untouched (step_count stays 0)
        assert!(!terminal);
        assert_eq!(state.step_count, 0);
    }

    // json_mode prints raw_json; step_started is NOT terminal in json mode
    #[test]
    fn test_ac6_json_mode_passe_en_brut() {
        // GIVEN
        let event = SseEvent {
            event_type: "step_started".into(),
            data: serde_json::json!({}),
            raw_json: r#"{"event":"step_started"}"#.into(),
        };
        let mut state = RunDisplayState::new(true, false); // json_mode = true

        // WHEN — just verify it doesn't panic and returns non-terminal
        let terminal = handle_sse_event(&event, &mut state);

        // THEN
        assert!(!terminal);
    }

    // json_mode: canceled IS terminal
    #[test]
    fn test_ac6_json_mode_canceled_is_terminal() {
        // GIVEN
        let event = make_event("canceled", serde_json::json!({}));
        let mut state = RunDisplayState::new(true, false);

        // WHEN
        let terminal = handle_sse_event(&event, &mut state);

        // THEN
        assert!(terminal);
    }

    // "started" is NOT terminal — the task is now running
    #[test]
    fn test_started_event_not_terminal() {
        // GIVEN
        let event = make_event(
            "started",
            serde_json::json!({"agent_id": "apollia-reviewer"}),
        );
        let mut state = RunDisplayState::new(false, false);

        // WHEN
        let terminal = handle_sse_event(&event, &mut state);

        // THEN — stream stays open
        assert!(!terminal);
    }

    // step_failed is NOT terminal (replanning may follow)
    #[test]
    fn test_step_failed_not_terminal() {
        // GIVEN
        let event = make_event(
            "step_failed",
            serde_json::json!({"duration_ms": 500, "error": "timeout", "retryable": true}),
        );
        let mut state = RunDisplayState::new(false, false);

        // WHEN
        let terminal = handle_sse_event(&event, &mut state);

        // THEN
        assert!(!terminal);
    }

    // plan_completed is NOT terminal (completed follows)
    #[test]
    fn test_plan_completed_not_terminal() {
        // GIVEN
        let event = make_event(
            "plan_completed",
            serde_json::json!({"step_count": 4, "duration_ms": 3200}),
        );
        let mut state = RunDisplayState::new(false, false);

        // WHEN
        let terminal = handle_sse_event(&event, &mut state);

        // THEN
        assert!(!terminal);
    }
}
