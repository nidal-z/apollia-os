//! Dispatch of one SSE event onto the display state.

use apollia_core::plan_alternatives::{ChosenPlan, PlanAlternatives};
use apollia_core::ORIAConfig;

use crate::note;

use super::display::{handle_alternatives, RunDisplayState, SseEvent};

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
            "completed" | "canceled" | "failed" | "plan_failed" | "plan_abandoned"
        );
    }

    // In terminal-only mode intermediate plan/step events produce no output;
    // events are silently consumed.  This is used by the default (non-`--stream`)
    // invocation so that the final agent output is still surfaced cleanly.
    if state.terminal_only {
        return handle_terminal_only_event(event);
    }

    match event.event_type.as_str() {
        // ── Orchestrated: plan generated ──────────────────────────────────
        "plan_generated" => handle_plan_generated(event, state),

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
                "\r  ✔ [{}/{}] (completed)  {:.1}s",
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
                eprintln!("  Unrecoverable error.");
            }
            false
        }

        // ── Orchestrated: replanning notice ───────────────────────────────
        "replanning" => {
            let attempt = event.data["attempt"].as_u64().unwrap_or(1);
            let failed_step = event.data["failed_step"].as_str().unwrap_or("?");
            let reason = event.data["reason"].as_str().unwrap_or("?");
            println!("  ↻ Replanning ({attempt}/2) - step {failed_step} failed: {reason}");
            false
        }

        // ── Orchestrated: plan completed (all steps done) ─────────────────
        "plan_completed" => {
            let step_count = event.data["step_count"].as_u64().unwrap_or(0);
            let duration_ms = event.data["duration_ms"].as_u64().unwrap_or(0);
            let secs = duration_ms as f64 / 1000.0;
            note!();
            note!("  ✔ Plan completed - {step_count} steps in {secs:.1}s");
            false
        }

        // ── Orchestrated: plan failed (unrecoverable), terminal ──────────
        "plan_failed" => {
            let reason = event.data["reason"].as_str().unwrap_or("unknown error");
            eprintln!();
            eprintln!("  ✗ Plan failed: {reason}");
            true
        }

        // ── Plan gate: approved, execution resumes ───────────────────────
        "plan_approved" => {
            note!("  ✔ Plan approved, executing.");
            false
        }

        // ── Plan gate: rejected, replanning ──────────────────────────────
        "plan_rejected" => {
            println!("  ↻ Plan rejected, replanning.");
            false
        }

        // ── Plan gate: abandoned after the replan limit, terminal ────────
        "plan_abandoned" => {
            let reason = event.data["reason"].as_str().unwrap_or("unknown");
            eprintln!("  ✗ Run abandoned ({reason}).");
            true
        }

        // ── Common: task completed (direct or orchestrated), terminal ────
        "completed" => {
            if let Some(result) = event.data["result"].as_str() {
                note!();
                println!("{result}");
            }
            note!();
            true
        }

        // ── Common: task failed (direct mode), terminal ──────────────────
        "failed" => {
            let error = event.data["error"].as_str().unwrap_or("unknown error");
            eprintln!("  x Failed: {error}");
            true
        }

        // ── Common: task canceled, terminal ──────────────────────────────
        "canceled" => {
            eprintln!("  Task cancelled.");
            true
        }

        // ── Common: task picked up by executor, shows the stream is live ──
        "started" => {
            let agent = event.data["agent_id"].as_str().unwrap_or("?");
            println!("  ~ Running on {agent}...");
            false
        }

        // ── Plan alternatives: display plans, read choice ─────────────────
        "plan_alternatives_generated" => {
            handle_plan_alternatives(event, state);
            false
        }

        _ => false,
    }
}

/// Quiet-mode dispatch: only terminal events produce output.
pub(super) fn handle_terminal_only_event(event: &SseEvent) -> bool {
    match event.event_type.as_str() {
        "completed" => {
            if let Some(result) = event.data["result"].as_str() {
                println!("{result}");
            }
            true
        }
        "failed" => {
            let error = event.data["error"].as_str().unwrap_or("unknown error");
            eprintln!("  x Failed: {error}");
            true
        }
        "plan_failed" => {
            let reason = event.data["reason"].as_str().unwrap_or("Unknown error");
            eprintln!("  ✗ Plan failed: {reason}");
            true
        }
        "canceled" => {
            eprintln!("  Task cancelled.");
            true
        }
        _ => false,
    }
}

/// Render the generated plan tree and record its id / step count.
pub(super) fn handle_plan_generated(event: &SseEvent, state: &mut RunDisplayState) -> bool {
    let step_count = event.data["step_count"].as_u64().unwrap_or(0) as usize;
    state.step_count = step_count;
    state.plan_id = event.data["plan_id"].as_str().map(String::from);
    eprintln!();
    note!("  Plan generated ({step_count} steps):");
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

/// Display plan alternatives and read the operator's choice into `state`.
pub(super) fn handle_plan_alternatives(event: &SseEvent, state: &mut RunDisplayState) {
    if !state.alternatives_mode {
        return;
    }
    let Ok(alternatives) = serde_json::from_value::<PlanAlternatives>(event.data.clone()) else {
        return;
    };
    let config = ORIAConfig::default();
    match handle_alternatives(&alternatives, &config) {
        Ok(chosen) => {
            let label = match &chosen {
                ChosenPlan::PlanA => "Plan A (conservative)",
                ChosenPlan::PlanB => "Plan B (exploratory)",
            };
            println!("  -> {label} selected.");
            state.chosen_plan = Some(chosen);
        }
        Err(e) => {
            eprintln!("  x Error during plan choice: {e}");
        }
    }
}
