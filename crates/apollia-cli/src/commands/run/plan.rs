//! The plan approval gate: reading the operator's decision and submitting it.

use std::io::{self, BufRead, Write};

use crate::client::RuntimeClient;

use super::display::RunDisplayState;

// ─── Plan approval ────────────────────────────────────────────────────────────

/// Parsed operator decision at the plan gate.
#[derive(Debug, PartialEq, Eq)]
pub enum PlanDecisionInput {
    /// Approve the plan as generated.
    Approve,
    /// Reject the plan, with optional feedback for replanning.
    Reject(Option<String>),
    /// Abandon the run.
    Quit,
    /// Unrecognized input: the caller re-prompts.
    Invalid,
}

/// Parse a free-text plan decision entered at the interactive prompt.
///
/// Accepts `a`/`approve`/`approuver`, `r`/`reject`/`rejeter` (with optional
/// trailing feedback), and `q`/`quit`/`quitter`. Anything else is `Invalid`.
pub fn parse_plan_decision(input: &str) -> PlanDecisionInput {
    let trimmed = input.trim();
    let (head, rest) = match trimmed.split_once(char::is_whitespace) {
        Some((h, r)) => (h, r.trim()),
        None => (trimmed, ""),
    };
    match head.to_ascii_lowercase().as_str() {
        "a" | "approve" | "approuver" => PlanDecisionInput::Approve,
        "r" | "reject" | "rejeter" => {
            let feedback = if rest.is_empty() {
                None
            } else {
                Some(rest.to_string())
            };
            PlanDecisionInput::Reject(feedback)
        }
        "q" | "quit" | "quitter" => PlanDecisionInput::Quit,
        _ => PlanDecisionInput::Invalid,
    }
}

/// Extract the event data of a `plan_approval_required` SSE line, if any.
pub(super) fn parse_plan_approval_line(line: &str) -> Option<serde_json::Value> {
    let data = line.strip_prefix("data: ")?;
    let parsed = serde_json::from_str::<serde_json::Value>(data).ok()?;
    if parsed.get("event").and_then(|v| v.as_str()) == Some("plan_approval_required") {
        Some(parsed)
    } else {
        None
    }
}

/// Outcome of handling a plan-approval gate in the stream loop.
pub enum PlanApprovalOutcome {
    /// Decision submitted; keep streaming.
    Continue,
    /// Operator quit; the caller exits with success.
    Quit,
}

/// Render the plan steps carried by a `plan_approval_required` event.
pub(super) fn print_plan_for_review(data: &serde_json::Value) {
    println!("\n--- Proposed plan ---");
    match data["steps"].as_array() {
        Some(steps) if !steps.is_empty() => {
            for (i, step) in steps.iter().enumerate() {
                let desc = step["description"].as_str().unwrap_or("");
                match step["tool_hint"].as_str() {
                    Some(tool) => println!("  {}. {desc}  [tool: {tool}]", i + 1),
                    None => println!("  {}. {desc}", i + 1),
                }
            }
        }
        _ => {
            let count = data["step_count"].as_u64().unwrap_or(0);
            println!("  ({count} step(s), details unavailable)");
        }
    }
}

/// Read a single line from stdin, returning `None` when the stream is closed.
pub(super) fn read_stdin_line() -> Option<String> {
    io::stdin().lock().lines().next().and_then(|r| r.ok())
}

/// Handle a `plan_approval_required` event: display the plan, collect a decision,
/// and submit it to the runtime API.
///
/// In `--json` mode the plan is emitted as JSON and the decision is read as a
/// JSON object (`{"decision":"approved"}` or `{"decision":"rejected","feedback":"..."}`)
/// from stdin. On a TTY the operator is prompted interactively until the input is
/// valid. Reads from stdin; writes to stdout.
pub async fn handle_plan_approval(
    client: &RuntimeClient,
    data: &serde_json::Value,
    state: &mut RunDisplayState,
) -> PlanApprovalOutcome {
    let run_id = data["run_id"].as_str().unwrap_or("").to_string();
    state.pending_plan_run_id = Some(run_id.clone());

    if state.json_mode {
        println!("{data}");
        let Some(line) = read_stdin_line() else {
            eprintln!("stdin closed before a plan decision");
            return PlanApprovalOutcome::Quit;
        };
        let parsed =
            serde_json::from_str::<serde_json::Value>(&line).unwrap_or(serde_json::Value::Null);
        let decision = parsed["decision"].as_str().unwrap_or("rejected");
        let feedback = parsed["feedback"].as_str().map(String::from);
        submit_plan_decision_request(client, &run_id, decision, feedback).await;
        return PlanApprovalOutcome::Continue;
    }

    print_plan_for_review(data);
    loop {
        print!("\n[A]pprove  [R]eject [optional feedback]  [Q]uit: ");
        let _ = io::stdout().flush();
        let Some(line) = read_stdin_line() else {
            return PlanApprovalOutcome::Quit;
        };
        match parse_plan_decision(&line) {
            PlanDecisionInput::Approve => {
                submit_plan_decision_request(client, &run_id, "approved", None).await;
                return PlanApprovalOutcome::Continue;
            }
            PlanDecisionInput::Reject(feedback) => {
                submit_plan_decision_request(client, &run_id, "rejected", feedback).await;
                return PlanApprovalOutcome::Continue;
            }
            PlanDecisionInput::Quit => {
                submit_plan_decision_request(client, &run_id, "rejected", None).await;
                println!("Run cancelled.");
                return PlanApprovalOutcome::Quit;
            }
            PlanDecisionInput::Invalid => {
                println!("Invalid input. [A]pprove / [R]eject / [Q]uit");
            }
        }
    }
}

/// POST a plan decision to `/api/v1/tasks/{run_id}/plan-decision`.
pub(super) async fn submit_plan_decision_request(
    client: &RuntimeClient,
    run_id: &str,
    decision: &str,
    feedback: Option<String>,
) {
    let mut body = serde_json::json!({ "decision": decision });
    if let Some(fb) = feedback {
        body["feedback"] = serde_json::Value::String(fb);
    }
    let uri = format!("/api/v1/tasks/{run_id}/plan-decision");
    if let Err(e) = client.post(&uri, Some(&body)).await {
        eprintln!("  x Failed to submit plan decision: {e}");
    }
}
