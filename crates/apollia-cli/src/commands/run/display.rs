//! The parsed SSE event, the display state it feeds, and the plan-alternative prompt.

use std::io::{self, BufRead, Write};

use apollia_core::plan_alternatives::{ChosenPlan, PlanAlternatives};
use apollia_core::ORIAConfig;

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
    /// When `true`, suppress all intermediate events: only terminal events produce output.
    ///
    /// Used by the default (non-`--stream`) `run` invocation to display only the final
    /// result while still using SSE internally to receive the agent output.
    pub terminal_only: bool,
    /// When `true`, the stream should pause on `plan_alternatives_generated` and prompt
    /// for a plan choice before continuing.
    pub alternatives_mode: bool,
    /// The chosen plan captured after an `plan_alternatives_generated` event, if any.
    pub chosen_plan: Option<ChosenPlan>,
    /// When `true`, the stream pauses on `plan_approval_required` to collect the
    /// operator's decision before execution proceeds.
    pub plan_mode: bool,
    /// Run id extracted from the last `plan_approval_required` event, used to
    /// submit the decision to the runtime API.
    pub pending_plan_run_id: Option<String>,
}

impl RunDisplayState {
    /// Create a new display state.
    pub fn new(json_mode: bool, terminal_only: bool) -> Self {
        Self {
            plan_id: None,
            step_count: 0,
            current_num: 0,
            json_mode,
            terminal_only,
            alternatives_mode: false,
            chosen_plan: None,
            plan_mode: false,
            pending_plan_run_id: None,
        }
    }

    /// Create a display state with alternatives mode enabled.
    pub fn with_alternatives(json_mode: bool) -> Self {
        Self {
            alternatives_mode: true,
            ..Self::new(json_mode, false)
        }
    }

    /// Create a display state with plan-approval mode enabled.
    pub fn with_plan(json_mode: bool) -> Self {
        Self {
            plan_mode: true,
            ..Self::new(json_mode, false)
        }
    }
}

// ─── Binary feedback ──────────────────────────────────────────────────────────

/// Displays two alternative plans and prompts the operator to choose one.
///
/// Reads the choice from stdin (expecting `"1"` for Plan A or `"2"` for Plan B).
/// Returns an error string if stdin is unavailable or the input is invalid.
///
/// This function is intentionally synchronous: it blocks until the operator
/// has made a choice, which is the correct behaviour for an interactive terminal.
pub fn handle_alternatives(
    alternatives: &PlanAlternatives,
    config: &ORIAConfig,
) -> Result<ChosenPlan, String> {
    println!(
        "\n--- Plan A (conservative, temperature {:.1}) ---",
        config.plan_alternatives_temp_a
    );
    for (i, step) in alternatives.plan_a.steps.iter().enumerate() {
        println!("  {}. {}", i + 1, step.description);
    }
    if alternatives.plan_a.steps.is_empty() {
        println!("  (no step)");
    }

    println!(
        "\n--- Plan B (exploratory, temperature {:.1}) ---",
        config.plan_alternatives_temp_b
    );
    for (i, step) in alternatives.plan_b.steps.iter().enumerate() {
        println!("  {}. {}", i + 1, step.description);
    }
    if alternatives.plan_b.steps.is_empty() {
        println!("  (no steps)");
    }

    print!("\nChoose a plan [1/2]: ");
    io::stdout()
        .flush()
        .map_err(|e| format!("stdout flush failed: {e}"))?;

    let stdin = io::stdin();
    let line = stdin
        .lock()
        .lines()
        .next()
        .ok_or_else(|| "stdin closed before input".to_string())?
        .map_err(|e| format!("stdin read error: {e}"))?;

    match line.trim() {
        "1" => Ok(ChosenPlan::PlanA),
        "2" => Ok(ChosenPlan::PlanB),
        other => Err(format!("invalid choice: '{other}' (expected 1 or 2)")),
    }
}
