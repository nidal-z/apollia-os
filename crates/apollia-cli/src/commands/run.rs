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

use std::path::PathBuf;
use std::time::Instant;

use std::str::FromStr;

use apollia_core::AutonomyLevel;

use crate::client::{default_socket_path, RuntimeClient};
use crate::exit_codes;

mod display;
mod events;
mod helpers;
mod plan;

use helpers::{
    build_input_payload, build_session_filter, handle_submit_error, output_error, poll_task,
    report_detached_submission, stream_task, StreamTaskArgs,
};

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

    let socket_path = socket.unwrap_or_else(default_socket_path);
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
            terminal_only: false,
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
                        "  Cost ceiling reached: the run stops cleanly when ceiling_action = hard_stop"
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
// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::display::{RunDisplayState, SseEvent};
    use super::events::handle_sse_event;
    use super::plan::{parse_plan_approval_line, parse_plan_decision, PlanDecisionInput};
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
        // WHEN it is parsed
        // THEN it is a rejection carrying no feedback
        assert_eq!(parse_plan_decision("r"), PlanDecisionInput::Reject(None));
        // GIVEN a reject with feedback
        // WHEN each is parsed
        // THEN the feedback is kept, in either language
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
        // WHEN each is parsed
        // THEN both mean quit, whatever the casing
        assert_eq!(parse_plan_decision("q"), PlanDecisionInput::Quit);
        assert_eq!(parse_plan_decision("Quitter"), PlanDecisionInput::Quit);
        // GIVEN unrecognized input
        // WHEN each is parsed
        // THEN neither is read as an approval
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
        let event = make_event("completed", serde_json::json!({"result": "Final result"}));
        let mut state = RunDisplayState::new(false, false);

        // WHEN
        let terminal = handle_sse_event(&event, &mut state);

        // THEN
        assert!(terminal);
    }

    // an event type the stream never carries is ignored, not terminal
    #[test]
    fn test_unknown_event_type_not_terminal() {
        // GIVEN an event type no route emits
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
            let parsed = AutonomyLevel::from_str(s).expect("must parse");
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

        // WHEN it is read as a tier
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
