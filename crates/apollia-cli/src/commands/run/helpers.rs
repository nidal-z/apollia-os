//! Payload building, submission, polling, streaming and budget persistence.

use std::io::Write;
use std::time::Instant;

use futures::StreamExt;

use apollia_core::token_budget::TokenBudget;

use crate::client::{ClientError, RuntimeClient};
use crate::exit_codes;

use super::display::{RunDisplayState, SseEvent};
use super::events::handle_sse_event;
use super::plan::{handle_plan_approval, parse_plan_approval_line, PlanApprovalOutcome};

// ─── Private helpers ──────────────────────────────────────────────────────────

/// Build the AIPInput payload for the task submission.
///
/// - Default path: wrap the free-text input as a single AIPPart::Text so
///   Python agents can read `parts[0]["text"]` directly (see
///   AIPPart::Text serialisation in apollia-core).
/// - `--input-json` escape hatch: parse the raw JSON and forward it
///   verbatim, so the operator can target any AIPInput shape (data
///   parts, worker skill envelopes, etc.) without the CLI second-guessing
///   the structure.
///
/// Returns `Err(exit_code)` when `--input-json` is supplied but invalid.
pub(super) fn build_input_payload(
    input: &str,
    input_json: Option<&str>,
    json: bool,
) -> Result<serde_json::Value, i32> {
    match input_json {
        Some(raw) => serde_json::from_str::<serde_json::Value>(raw).map_err(|e| {
            output_error(
                &format!("--input-json is not valid JSON: {e}"),
                json,
                exit_codes::GENERAL_ERROR,
            )
        }),
        None => Ok(serde_json::json!({
            "parts": [{"type": "text", "text": input}]
        })),
    }
}

/// Print the `--detach` submission acknowledgement and return success.
pub(super) fn report_detached_submission(task_id: &str, agent_id: &str, json: bool) -> i32 {
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
    exit_codes::SUCCESS
}

/// Build the `session_config` fragment from the tool allow/deny lists.
///
/// Returns `Value::Null` when no restrictions are specified.
pub(super) fn build_session_filter(
    allowed_tools: &[String],
    disallowed_tools: &[String],
) -> serde_json::Value {
    if allowed_tools.is_empty() && disallowed_tools.is_empty() {
        return serde_json::Value::Null;
    }
    let allowed = if allowed_tools.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::Array(
            allowed_tools
                .iter()
                .map(|t| serde_json::Value::String(t.clone()))
                .collect(),
        )
    };
    let disallowed = serde_json::Value::Array(
        disallowed_tools
            .iter()
            .map(|t| serde_json::Value::String(t.clone()))
            .collect(),
    );
    serde_json::json!({
        "allowed_tools": allowed,
        "disallowed_tools": disallowed,
    })
}

/// Map a `submit_task` failure to a user-facing error and exit code.
pub(super) fn handle_submit_error(err: ClientError, agent_id: &str, json: bool) -> i32 {
    match err {
        ClientError::ConnectionRefused => output_error(
            "runtime not started (connection refused)",
            json,
            exit_codes::RUNTIME_ERROR,
        ),
        // 404 typically means the agent is installed-but-disabled (or never
        // loaded into the runtime registry). Give the operator a precise
        // recovery command instead of a bare "not found".
        ClientError::ServerError { status: 404, body } => {
            let hint = format!(
                "{body}\n\
                 Hint: install + enable + load the agent first:\n\
                 \t apollia-os agent install <path>   # if it's not yet installed\n\
                 \t apollia-os agent enable {agent_id} # if it's disabled\n\
                 \t apollia-os agent list             # to see current state"
            );
            output_error(&hint, json, exit_codes::GENERAL_ERROR)
        }
        e => output_error(&e.to_string(), json, exit_codes::GENERAL_ERROR),
    }
}

/// Poll task status until completion.
pub(super) async fn poll_task(
    client: &RuntimeClient,
    task_id: &str,
    json: bool,
    start: Instant,
) -> i32 {
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let task_json = match client.get_task(task_id).await {
            Ok(j) => j,
            Err(e) => return output_error(&e.to_string(), json, exit_codes::GENERAL_ERROR),
        };

        if let Some(code) = poll_terminal_outcome(&task_json, task_id, json, start) {
            return code;
        }
    }
}

/// Inspect a polled task body. Returns `Some(exit_code)` once the task has
/// reached a terminal state, or `None` to keep polling.
pub(super) fn poll_terminal_outcome(
    task_json: &serde_json::Value,
    task_id: &str,
    json: bool,
    start: Instant,
) -> Option<i32> {
    let status = task_json
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    match status {
        "completed" => {
            report_completed_task(task_json, task_id, json, start);
            Some(exit_codes::SUCCESS)
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
                    serde_json::to_string_pretty(task_json).unwrap_or_default()
                );
            } else {
                eprintln!("  x Failed in {:.1}s: {error_msg}", elapsed.as_secs_f64());
            }
            Some(exit_codes::TASK_FAILED)
        }
        "canceled" => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(task_json).unwrap_or_default()
                );
            } else {
                println!("  Task {task_id} was canceled");
            }
            Some(exit_codes::GENERAL_ERROR)
        }
        _ => None,
    }
}

/// Render a completed task: its result (or full JSON), elapsed time, and token
/// budget summary, persisting the budget for the non-`--json` path.
pub(super) fn report_completed_task(
    task_json: &serde_json::Value,
    task_id: &str,
    json: bool,
    start: Instant,
) {
    let elapsed = start.elapsed();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(task_json).unwrap_or_default()
        );
        return;
    }
    if let Some(text) = task_json.get("result").and_then(|v| v.as_str()) {
        println!("{text}");
    }
    println!("  * Completed in {:.1}s", elapsed.as_secs_f64());
    if let Some(budget) = extract_budget(task_json) {
        println!("  * {}", budget.format_summary());
        persist_budget(&budget, task_id);
    }
}

/// Stream task events via SSE and display them using [`handle_sse_event`].
///
/// Connects to `GET /api/v1/tasks/{id}/stream` using [`RuntimeClient::stream_sse_lines`],
/// which reads the HTTP body incrementally, one line at a time as the server flushes it.
/// Each SSE frame is parsed and dispatched to `handle_sse_event` immediately, producing
/// real-time output instead of waiting for the full response to buffer.
///
/// Returns when a terminal event is received or falls back to polling if the
/// stream closes without a terminal event (e.g. race on task already completed).
///
/// When `terminal_only` is `true`, intermediate events are suppressed and only
/// the final agent output is printed (used by the default non-`--stream` path).
///
/// When `alternatives` is `true`, the stream pauses on `plan_alternatives_generated`
/// and prompts the operator for a plan choice before continuing.
/// Parameters for [`stream_task`].
pub(super) struct StreamTaskArgs<'a> {
    /// Connected runtime client.
    pub(super) client: &'a RuntimeClient,
    /// Identifier of the task to stream.
    pub(super) task_id: &'a str,
    /// Emit machine-readable JSON.
    pub(super) json: bool,
    /// Wall-clock start used to report elapsed time on fallback polling.
    pub(super) start: Instant,
    /// Suppress intermediate events, surfacing only the final output.
    pub(super) terminal_only: bool,
    /// Pause on `plan_alternatives_generated` and prompt for a choice.
    pub(super) alternatives: bool,
    /// Pause on `plan_approval_required` and prompt for an approve/reject decision.
    pub(super) plan: bool,
}

pub(super) async fn stream_task(args: StreamTaskArgs<'_>) -> i32 {
    let StreamTaskArgs {
        client,
        task_id,
        json,
        start,
        terminal_only,
        alternatives,
        plan,
    } = args;

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

    let mut state = if alternatives {
        RunDisplayState::with_alternatives(json)
    } else if plan {
        RunDisplayState::with_plan(json)
    } else {
        RunDisplayState::new(json, terminal_only)
    };
    let mut terminal_event_type = String::new();

    // Each line arrives as soon as the server flushes it: true streaming.
    while let Some(line_result) = line_stream.next().await {
        let line = match line_result {
            Ok(l) => l,
            Err(e) => {
                eprintln!("  x Stream error: {e}");
                break;
            }
        };

        // In plan mode, intercept the approval gate to prompt and submit a
        // decision (async, needs the client) before the sync display handler.
        if state.plan_mode {
            if let Some(data) = parse_plan_approval_line(&line) {
                match handle_plan_approval(client, &data, &mut state).await {
                    PlanApprovalOutcome::Continue => continue,
                    PlanApprovalOutcome::Quit => return exit_codes::SUCCESS,
                }
            }
        }

        if let Some(event_type) = process_sse_line(&line, &mut state) {
            terminal_event_type = event_type;
            break;
        }
    }

    // Map terminal event type to exit code
    stream_terminal_exit_code(&terminal_event_type, client, task_id, json, start).await
}

/// Map the terminal SSE event type to a process exit code.
///
/// On `completed` (non-`--json`), fetches the final task state to surface and
/// persist the token budget, which is absent from the SSE payload. When no
/// terminal event was seen the stream closed early, so fall back to polling.
pub(super) async fn stream_terminal_exit_code(
    terminal_event_type: &str,
    client: &RuntimeClient,
    task_id: &str,
    json: bool,
    start: Instant,
) -> i32 {
    match terminal_event_type {
        "completed" => {
            persist_stream_budget(client, task_id, json).await;
            exit_codes::SUCCESS
        }
        "failed" => exit_codes::TASK_FAILED,
        "plan_failed" | "plan_abandoned" | "canceled" => exit_codes::GENERAL_ERROR,
        // No terminal event: stream closed early (task already done), fall back to polling
        _ => poll_task(client, task_id, json, start).await,
    }
}

/// Fetch the final task state and surface + persist its token budget.
///
/// No-op in `--json` mode (the budget is reported via the full JSON payload).
pub(super) async fn persist_stream_budget(client: &RuntimeClient, task_id: &str, json: bool) {
    if json {
        return;
    }
    if let Ok(task_json) = client.get_task(task_id).await {
        if let Some(budget) = extract_budget(&task_json) {
            println!("  * {}", budget.format_summary());
            persist_budget(&budget, task_id);
        }
    }
}

/// Parse a single SSE `data:` line and dispatch it to [`handle_sse_event`].
///
/// Returns `Some(event_type)` when the event is terminal (the caller should
/// stop reading), or `None` otherwise (including non-data / unparseable lines).
pub(super) fn process_sse_line(line: &str, state: &mut RunDisplayState) -> Option<String> {
    let data = line.strip_prefix("data: ")?;
    let parsed = serde_json::from_str::<serde_json::Value>(data).ok()?;
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

    if handle_sse_event(&sse_event, state) {
        Some(event_type)
    } else {
        None
    }
}

/// Extract a `TokenBudget` from a task JSON response.
///
/// Returns `None` if the `token_budget` field is absent or cannot be deserialized.
pub(super) fn extract_budget(task_json: &serde_json::Value) -> Option<TokenBudget> {
    task_json
        .get("token_budget")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

/// Append one JSON line to `~/.apollia/session_costs.jsonl`.
///
/// Each line is a JSON object with `task_id` + all `TokenBudget` fields.
/// Errors are silently ignored to avoid disrupting CLI output.
pub(super) fn persist_budget(budget: &TokenBudget, task_id: &str) {
    let dir = match apollia_core::paths::home_string() {
        Some(h) => apollia_core::paths::data_dir_under(h),
        None => return,
    };

    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }

    let path = dir.join("session_costs.jsonl");
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };

    let mut record = serde_json::to_value(budget).unwrap_or(serde_json::Value::Null);
    if let Some(obj) = record.as_object_mut() {
        obj.insert(
            "task_id".to_owned(),
            serde_json::Value::String(task_id.to_owned()),
        );
        let ts = chrono::Utc::now().to_rfc3339();
        obj.insert("recorded_at".to_owned(), serde_json::Value::String(ts));
    }

    let line = serde_json::to_string(&record).unwrap_or_default();
    let _ = writeln!(file, "{line}");
}

/// Output an error and return the given exit code.
pub(super) fn output_error(msg: &str, json: bool, code: i32) -> i32 {
    crate::output::emit_error(json, code, msg)
}
