//! `apollia-os run <agent> <input>`: submit a task and wait for the result.
//!
//! Supports `--stream` for real-time SSE streaming of task progress.
//! In orchestrated mode, the stream displays the execution plan, step-by-step
//! progression, and replanning events. In direct mode, the legacy
//! behaviour is preserved.
//!
//! With `--alternatives`, displays two plan alternatives (conservative vs. exploratory)
//! received via the `plan_alternatives_generated` SSE event and prompts for a choice
//! before the task proceeds.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::time::Instant;

use futures::StreamExt;

use std::str::FromStr;

use apollia_core::plan_alternatives::{ChosenPlan, PlanAlternatives};
use apollia_core::token_budget::TokenBudget;
use apollia_core::{AutonomyLevel, ORIAConfig};

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
    /// When `true`, suppress all intermediate events: only terminal events produce output.
    ///
    /// Used by the default (non-`--stream`) `run` invocation to display only the final
    /// result while still using SSE internally to receive the agent output.
    pub quiet: bool,
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
    pub fn new(json_mode: bool, quiet: bool) -> Self {
        Self {
            plan_id: None,
            step_count: 0,
            current_num: 0,
            json_mode,
            quiet,
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
        other => Err(format!("choix invalide : '{other}' (attendu 1 ou 2)")),
    }
}

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
fn parse_plan_approval_line(line: &str) -> Option<serde_json::Value> {
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
fn print_plan_for_review(data: &serde_json::Value) {
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
fn read_stdin_line() -> Option<String> {
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
async fn submit_plan_decision_request(
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

    // In quiet mode only terminal events produce output; intermediate plan/step
    // events are silently consumed.  This is used by the default (non-`--stream`)
    // invocation so that the final agent output is still surfaced cleanly.
    if state.quiet {
        return handle_quiet_event(event);
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
            println!();
            println!("  ✔ Plan completed - {step_count} steps in {secs:.1}s");
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
            println!("  ✔ Plan approved, executing.");
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
                println!();
                println!("{result}");
            }
            println!();
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

        // ── Plan alternatives: display plans, read choice ─────────────────
        "plan_alternatives_generated" => {
            handle_plan_alternatives(event, state);
            false
        }

        _ => false,
    }
}

/// Quiet-mode dispatch: only terminal events produce output.
fn handle_quiet_event(event: &SseEvent) -> bool {
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
fn handle_plan_generated(event: &SseEvent, state: &mut RunDisplayState) -> bool {
    let step_count = event.data["step_count"].as_u64().unwrap_or(0) as usize;
    state.step_count = step_count;
    state.plan_id = event.data["plan_id"].as_str().map(String::from);
    eprintln!();
    println!("  Plan generated ({step_count} steps):");
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
fn handle_plan_alternatives(event: &SseEvent, state: &mut RunDisplayState) {
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

// ─── Public entry point ───────────────────────────────────────────────────────

/// Arguments forwarded from the `run` CLI sub-command to [`run`].
pub struct RunCommandArgs<'a> {
    /// Target agent identifier.
    pub agent_id: &'a str,
    /// Free-text task input (used when `input_json` is `None`).
    pub input: &'a str,
    /// Raw JSON payload that bypasses the `parts:[text]` wrapper.
    ///
    /// Mutually exclusive with `input` at the clap layer. When set, this
    /// value is parsed and forwarded to `submit_task` as-is, so the caller
    /// can target any AIPInput shape (data parts, custom envelopes, etc.).
    pub input_json: Option<&'a str>,
    /// Optional Unix socket path override.
    pub socket: Option<PathBuf>,
    /// Output machine-readable JSON.
    pub json: bool,
    /// Stream task progress in real-time via SSE.
    pub stream: bool,
    /// Submit and return immediately without waiting.
    pub detach: bool,
    /// Show two plan alternatives and prompt for a choice before executing.
    pub alternatives: bool,
    /// Pause after plan generation to review and approve the plan before
    /// execution. Forces the plan gate active for this run and streams the
    /// `plan_approval_required` event to collect the operator's decision.
    pub plan: bool,
    /// Session-level tool allow-list (empty = all tools permitted).
    pub allowed_tools: Vec<String>,
    /// Session-level tool deny-list (takes priority over `allowed_tools`).
    pub disallowed_tools: Vec<String>,
    /// Autonomy tier for this run. `None` means assisted (the runtime default).
    ///
    /// When set to a valid tier it is forwarded to the runtime as the
    /// `autonomy_level` field of the submission payload, which selects the
    /// effective execution budget, memory injection, and verification.
    pub autonomy: Option<&'a str>,
}

/// Execute the `run` command.
///
/// Submits a task to the specified agent and waits for the result.
/// With `--detach`, returns immediately after submission and prints the task ID.
/// With `--alternatives`, activates the binary feedback mode: the SSE stream will
/// pause on `plan_alternatives_generated` and prompt the operator to choose a plan.
/// With `--allowed-tools` / `--disallowed-tools`, enforces a session-level tool
/// filter on the runtime side (forwarded in the task submission payload).
/// Returns the process exit code.
pub async fn run(args: RunCommandArgs<'_>) -> i32 {
    let RunCommandArgs {
        agent_id,
        input,
        input_json,
        socket,
        json,
        stream,
        detach,
        alternatives,
        plan,
        allowed_tools,
        disallowed_tools,
        autonomy,
    } = args;

    // Validate the autonomy tier before any runtime connection (fail fast).
    let autonomy_level: Option<AutonomyLevel> = match autonomy {
        None => None,
        Some(s) => match AutonomyLevel::from_str(s) {
            Ok(level) => Some(level),
            Err(_) => {
                return output_error(
                    &format!(
                        "niveau d'autonomie invalide '{s}'; valeurs acceptees : \
                         assisted, supervised, bounded_autonomous, long_autonomous"
                    ),
                    json,
                    exit_codes::GENERAL_ERROR,
                );
            }
        },
    };

    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let client = RuntimeClient::new(socket_path);
    let start = Instant::now();

    // Build the session tool filter fragment if any restrictions are specified.
    // The runtime applies them when constructing the ToolDispatcher for this task.
    let session_filter = build_session_filter(&allowed_tools, &disallowed_tools);

    // Build the AIPInput payload (see [`build_input_payload`]), merging in the
    // session tool filter when present.
    let mut input_value = match build_input_payload(input, input_json, json) {
        Ok(v) => v,
        Err(code) => return code,
    };

    if !session_filter.is_null() {
        if let Some(obj) = input_value.as_object_mut() {
            obj.insert("session_config".to_string(), session_filter);
        }
    }

    // Build per-run control options forwarded under `run_options` (the input
    // payload only carries `parts`, so control fields must travel separately).
    // On the CLI run path --plan drives the gate explicitly: present forces it
    // on, absent forces it off (autonomous), overriding the agent tier.
    let mut run_options = serde_json::json!({ "plan_gate": plan });
    if let Some(level) = autonomy_level {
        run_options["autonomy_level"] = serde_json::Value::String(level.as_str().to_string());
    }

    let submit_result = client
        .submit_task_with_options(agent_id, input_value, run_options)
        .await;

    let task_json = match submit_result {
        Ok(j) => j,
        Err(e) => return handle_submit_error(e, agent_id, json),
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

    // --detach: fire-and-forget, print task_id and return immediately.
    if detach {
        return report_detached_submission(&task_id, agent_id, json);
    }

    if !json {
        println!("  -> Task {task_id} submitted to {agent_id}");
    }

    // Default path uses polling: GET /api/v1/tasks/:id until completion.
    // The router now stores task output alongside status (see router.rs), so polling
    // correctly surfaces the agent result without SSE race conditions.
    //
    // With `--stream` or `--alternatives`: SSE streaming shows plan/step events in real time.
    let code = if stream || alternatives || plan {
        stream_task(StreamTaskArgs {
            client: &client,
            task_id: &task_id,
            json,
            start,
            quiet: false,
            alternatives,
            plan,
        })
        .await
    } else {
        poll_task(&client, &task_id, json, start).await
    };

    // Surface the shared cost/ceiling state once the run has terminated. The
    // run/task engine does not enforce the hybrid ceiling (that hard-stop lives
    // in the chat loop), so this reports the figures without altering the exit
    // code, which stays governed by task success.
    surface_cost_ceiling(&client, json).await;
    code
}

/// Cost-ceiling fields appended to `apollia-os run --json` output after a run ends.
///
/// `ceiling_usd` and `ceiling_reached` come from the shared hybrid router via
/// `GET /api/v1/llm/status`. `ceiling_usd` is omitted when hybrid routing is not
/// configured; `ceiling_reached` is always `false` in that case.
#[derive(Debug, serde::Serialize)]
struct RunCostSummary {
    /// Accumulated session cost in USD, when the backend reports it.
    #[serde(skip_serializing_if = "Option::is_none")]
    cost_usd: Option<f64>,
    /// Hybrid routing cost ceiling in USD, when configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    ceiling_usd: Option<f64>,
    /// Whether the cost ceiling was reached. Always `false` without hybrid routing.
    ceiling_reached: bool,
}

/// Fetch the shared LLM cost/ceiling state and surface it after a run.
///
/// In `--json` mode this prints one trailing JSON line; on a TTY it prints a
/// human cost line and a stderr warning when the ceiling was reached. The fetch
/// is best-effort: any status error is swallowed so it never masks the run
/// result, which is the primary output of `run`.
async fn surface_cost_ceiling(client: &RuntimeClient, json: bool) {
    let Ok(resp) = client.get("/api/v1/llm/status").await else {
        return;
    };
    if resp.status >= 400 {
        return;
    }
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&resp.body) else {
        return;
    };

    let cost_usd = parsed.get("cost_usd").and_then(serde_json::Value::as_f64);
    let ceiling_usd = parsed
        .get("ceiling_usd")
        .and_then(serde_json::Value::as_f64);
    let ceiling_reached = parsed
        .get("ceiling_reached")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    // Nothing meaningful to surface when the backend reports neither figure.
    if cost_usd.is_none() && ceiling_usd.is_none() {
        return;
    }

    if json {
        // Emit the cost summary on stderr so stdout stays a single JSON document
        // (the task result). A second top-level object on stdout breaks any
        // strict single-document `--json` parser.
        let summary = RunCostSummary {
            cost_usd,
            ceiling_usd,
            ceiling_reached,
        };
        if let Ok(line) = serde_json::to_string(&summary) {
            eprintln!("{line}");
        }
    } else {
        match (cost_usd, ceiling_usd) {
            (Some(cost), Some(ceiling)) => {
                println!("  session cost: {cost:.2} USD / {ceiling:.2} USD");
                if ceiling_reached {
                    eprintln!(
                        "  Plafond de cout atteint : le run s'arrete proprement quand ceiling_action = hard_stop"
                    );
                }
            }
            (Some(cost), None) => {
                println!("  session cost: {cost:.2} USD (no hybrid cost ceiling configured)");
            }
            _ => {}
        }
    }
}

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
fn build_input_payload(
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
fn report_detached_submission(task_id: &str, agent_id: &str, json: bool) -> i32 {
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
fn build_session_filter(
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
fn handle_submit_error(err: ClientError, agent_id: &str, json: bool) -> i32 {
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
async fn poll_task(client: &RuntimeClient, task_id: &str, json: bool, start: Instant) -> i32 {
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
fn poll_terminal_outcome(
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
fn report_completed_task(task_json: &serde_json::Value, task_id: &str, json: bool, start: Instant) {
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
/// When `quiet` is `true`, intermediate events are suppressed and only the final
/// agent output is printed (used by the default non-`--stream` path).
///
/// When `alternatives` is `true`, the stream pauses on `plan_alternatives_generated`
/// and prompts the operator for a plan choice before continuing.
/// Parameters for [`stream_task`].
struct StreamTaskArgs<'a> {
    /// Connected runtime client.
    client: &'a RuntimeClient,
    /// Identifier of the task to stream.
    task_id: &'a str,
    /// Emit machine-readable JSON.
    json: bool,
    /// Wall-clock start used to report elapsed time on fallback polling.
    start: Instant,
    /// Suppress intermediate events, surfacing only the final output.
    quiet: bool,
    /// Pause on `plan_alternatives_generated` and prompt for a choice.
    alternatives: bool,
    /// Pause on `plan_approval_required` and prompt for an approve/reject decision.
    plan: bool,
}

async fn stream_task(args: StreamTaskArgs<'_>) -> i32 {
    let StreamTaskArgs {
        client,
        task_id,
        json,
        start,
        quiet,
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
        RunDisplayState::new(json, quiet)
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
async fn stream_terminal_exit_code(
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
async fn persist_stream_budget(client: &RuntimeClient, task_id: &str, json: bool) {
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
fn process_sse_line(line: &str, state: &mut RunDisplayState) -> Option<String> {
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
fn extract_budget(task_json: &serde_json::Value) -> Option<TokenBudget> {
    task_json
        .get("token_budget")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

/// Append one JSON line to `~/.apollia/session_costs.jsonl`.
///
/// Each line is a JSON object with `task_id` + all `TokenBudget` fields.
/// Errors are silently ignored to avoid disrupting CLI output.
fn persist_budget(budget: &TokenBudget, task_id: &str) {
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
fn output_error(msg: &str, json: bool, code: i32) -> i32 {
    crate::output::emit_error(json, code, msg)
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

    // parse_plan_decision recognizes approve variants.
    #[test]
    fn test_parse_plan_decision_approve_variants() {
        // GIVEN approval inputs in several forms
        // WHEN parsed
        // THEN all map to Approve
        for s in ["a", "A", "approve", "approuver", "  a  "] {
            assert_eq!(parse_plan_decision(s), PlanDecisionInput::Approve, "{s:?}");
        }
    }

    // parse_plan_decision extracts reject feedback.
    #[test]
    fn test_parse_plan_decision_reject_variants() {
        // GIVEN a bare reject
        assert_eq!(parse_plan_decision("r"), PlanDecisionInput::Reject(None));
        // GIVEN a reject with feedback
        assert_eq!(
            parse_plan_decision("r add a validation step"),
            PlanDecisionInput::Reject(Some("add a validation step".to_string()))
        );
        assert_eq!(
            parse_plan_decision("rejeter trop long"),
            PlanDecisionInput::Reject(Some("trop long".to_string()))
        );
    }

    // parse_plan_decision recognizes quit and rejects unknown input.
    #[test]
    fn test_parse_plan_decision_quit_and_invalid() {
        // GIVEN quit inputs
        assert_eq!(parse_plan_decision("q"), PlanDecisionInput::Quit);
        assert_eq!(parse_plan_decision("Quitter"), PlanDecisionInput::Quit);
        // GIVEN unrecognized input
        assert_eq!(parse_plan_decision("xyz"), PlanDecisionInput::Invalid);
        assert_eq!(parse_plan_decision(""), PlanDecisionInput::Invalid);
    }

    // parse_plan_approval_line only matches the gate event.
    #[test]
    fn test_parse_plan_approval_line() {
        // GIVEN a plan_approval_required SSE data line
        let line = r#"data: {"event":"plan_approval_required","run_id":"r1","step_count":2}"#;
        // WHEN parsed
        let parsed = parse_plan_approval_line(line);
        // THEN it returns the data
        assert!(parsed.is_some());
        assert_eq!(parsed.unwrap()["run_id"], "r1");

        // GIVEN a different event
        let other = r#"data: {"event":"plan_generated","plan_id":"p1"}"#;
        // THEN it does not match
        assert!(parse_plan_approval_line(other).is_none());
    }

    // plan_generated updates state and is NOT terminal
    #[test]
    fn test_plan_generated_handler() {
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
    fn test_plan_generated_with_steps() {
        // GIVEN 2 steps, second depends on first
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
    fn test_step_started_not_terminal() {
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
    fn test_replanning_not_terminal() {
        // GIVEN
        let event = make_event(
            "replanning",
            serde_json::json!({"attempt": 1, "failed_step": "s3", "reason": "timeout"}),
        );
        let mut state = RunDisplayState::new(false, false);

        // WHEN
        let terminal = handle_sse_event(&event, &mut state);

        // THEN replanning is informational, not terminal
        assert!(!terminal);
    }

    // plan_failed is terminal
    #[test]
    fn test_plan_failed_est_terminal() {
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
    fn test_completed_est_terminal() {
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
    fn test_direct_mode_step_not_terminal() {
        // GIVEN legacy direct-mode step event
        let event = make_event("step", serde_json::json!({"step": 1, "tool": "file_io"}));
        let mut state = RunDisplayState::new(false, false);

        // WHEN
        let terminal = handle_sse_event(&event, &mut state);

        // THEN not terminal, state untouched (step_count stays 0)
        assert!(!terminal);
        assert_eq!(state.step_count, 0);
    }

    // json_mode prints raw_json; step_started is NOT terminal in json mode
    #[test]
    fn test_json_mode_passe_en_brut() {
        // GIVEN
        let event = SseEvent {
            event_type: "step_started".into(),
            data: serde_json::json!({}),
            raw_json: r#"{"event":"step_started"}"#.into(),
        };
        let mut state = RunDisplayState::new(true, false); // json_mode = true

        // WHEN just verify it doesn't panic and returns non-terminal
        let terminal = handle_sse_event(&event, &mut state);

        // THEN
        assert!(!terminal);
    }

    // json_mode: canceled IS terminal
    #[test]
    fn test_json_mode_canceled_is_terminal() {
        // GIVEN
        let event = make_event("canceled", serde_json::json!({}));
        let mut state = RunDisplayState::new(true, false);

        // WHEN
        let terminal = handle_sse_event(&event, &mut state);

        // THEN
        assert!(terminal);
    }

    // "started" is NOT terminal, the task is now running
    #[test]
    fn test_started_event_not_terminal() {
        // GIVEN
        let event = make_event("started", serde_json::json!({"agent_id": "apollia-guide"}));
        let mut state = RunDisplayState::new(false, false);

        // WHEN
        let terminal = handle_sse_event(&event, &mut state);

        // THEN stream stays open
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

    // ─── --autonomy flag (story 551) ───────────────────────────────────────

    // An invalid tier string is rejected before any runtime connection.
    #[test]
    fn test_invalid_autonomy_level_rejected() {
        // GIVEN an unknown tier string
        let result = AutonomyLevel::from_str("turbo");

        // WHEN / THEN it fails with a message naming the value and the accepted set
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("turbo"));
        assert!(msg.contains("assisted"));
    }

    // Every accepted tier round-trips between the string and the enum.
    #[test]
    fn test_valid_autonomy_level_round_trips() {
        // GIVEN the four canonical tier strings
        let inputs = [
            ("assisted", AutonomyLevel::Assisted),
            ("supervised", AutonomyLevel::Supervised),
            ("bounded_autonomous", AutonomyLevel::BoundedAutonomous),
            ("long_autonomous", AutonomyLevel::LongAutonomous),
        ];

        // WHEN / THEN each parses and serializes back to the same string
        for (s, expected) in inputs {
            let parsed = AutonomyLevel::from_str(s).expect("doit parser");
            assert_eq!(parsed, expected);
            assert_eq!(parsed.as_str(), s);
        }
    }

    // Without a tier, no field is added to the submission payload.
    #[test]
    fn test_none_autonomy_does_not_add_field() {
        // GIVEN a base payload and no tier
        let mut payload = serde_json::json!({
            "parts": [{"type": "text", "text": "tache"}]
        });
        let autonomy: Option<AutonomyLevel> = None;

        // WHEN the insertion guard runs
        if let Some(level) = autonomy {
            payload
                .as_object_mut()
                .expect("object")
                .insert("autonomy_level".into(), level.as_str().into());
        }

        // THEN the field is absent
        assert!(payload.get("autonomy_level").is_none());
    }

    // A valid tier is added to the payload under `autonomy_level`.
    #[test]
    fn test_some_autonomy_adds_field_to_payload() {
        // GIVEN a base payload and a long-autonomous tier
        let mut payload = serde_json::json!({
            "parts": [{"type": "text", "text": "tache"}]
        });

        // WHEN the field is inserted
        let level = AutonomyLevel::LongAutonomous;
        payload
            .as_object_mut()
            .expect("object")
            .insert("autonomy_level".into(), level.as_str().into());

        // THEN it carries the canonical snake_case value
        assert_eq!(payload["autonomy_level"].as_str(), Some("long_autonomous"));
    }

    // Mixed-case input is not accepted: only canonical snake_case parses.
    #[test]
    fn test_mixed_case_autonomy_rejected() {
        // GIVEN a capitalized tier string
        let result = AutonomyLevel::from_str("Assisted");

        // THEN it is rejected
        assert!(result.is_err());
    }

    // The run cost summary carries cost and ceiling when hybrid routing reports them.
    #[test]
    fn test_run_cost_summary_includes_cost_and_ceiling() {
        // GIVEN a summary built from a run with hybrid routing configured
        let summary = RunCostSummary {
            cost_usd: Some(0.45),
            ceiling_usd: Some(2.0),
            ceiling_reached: false,
        };

        // WHEN it is serialised
        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&summary).expect("serialize"))
                .expect("parse");

        // THEN every cost field is present with the expected values
        assert_eq!(value["cost_usd"].as_f64(), Some(0.45));
        assert_eq!(value["ceiling_usd"].as_f64(), Some(2.0));
        assert_eq!(value["ceiling_reached"].as_bool(), Some(false));
    }

    // When the ceiling is reached, the flag is true and the figures are retained.
    #[test]
    fn test_run_cost_summary_ceiling_reached_flag() {
        // GIVEN a summary built from a run that crossed the ceiling
        let summary = RunCostSummary {
            cost_usd: Some(2.05),
            ceiling_usd: Some(2.0),
            ceiling_reached: true,
        };

        // WHEN it is serialised
        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&summary).expect("serialize"))
                .expect("parse");

        // THEN ceiling_reached is true and the cost figures are carried through
        assert_eq!(value["ceiling_reached"].as_bool(), Some(true));
        assert_eq!(value["cost_usd"].as_f64(), Some(2.05));
        assert_eq!(value["ceiling_usd"].as_f64(), Some(2.0));
    }

    // Without hybrid routing, ceiling_usd is omitted and ceiling_reached stays false.
    #[test]
    fn test_run_cost_summary_no_hybrid_omits_ceiling() {
        // GIVEN a summary built from a run without hybrid routing
        let summary = RunCostSummary {
            cost_usd: Some(0.12),
            ceiling_usd: None,
            ceiling_reached: false,
        };

        // WHEN it is serialised
        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&summary).expect("serialize"))
                .expect("parse");

        // THEN there is no phantom ceiling field and the flag is present and false
        assert!(value.get("ceiling_usd").is_none());
        assert_eq!(value["ceiling_reached"].as_bool(), Some(false));
        assert_eq!(value["cost_usd"].as_f64(), Some(0.12));
    }
}
