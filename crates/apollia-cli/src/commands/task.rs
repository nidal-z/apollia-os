//! `apollia-os task` subcommands: manage tasks via the runtime API.
//!
//! Provides `list`, `status`, `cancel`, `inspect`, and `resume` operations on
//! tasks. The `inspect` subcommand reads directly from SQLite (`~/.apollia/plans.db`)
//! without requiring a running runtime (local-first).
//!
//! HITL additions:
//! - `task list --pending-approval`: filter tasks awaiting human approval.
//! - `task resume <id> --approve`: approve a suspended HITL task.
//! - `task resume <id> --reject [--reason "..."]`: reject a suspended HITL task.

use std::path::PathBuf;

use clap::Subcommand;

use crate::client::{default_socket_path, RuntimeClient};

mod display;
mod handlers;
mod util;

use handlers::{
    run_approvals, run_cancel, run_inspect, run_list, run_list_pending, run_resume, run_status,
    ResumeArgs,
};

/// Maximum output length before truncation in human-readable display.
const MAX_OUTPUT_LEN: usize = 120;

/// Maximum prompt length before truncation in the pending-approval table.
const MAX_PROMPT_LEN: usize = 60;

/// Task subcommands: `apollia-os task <verb>`.
#[derive(Debug, Subcommand)]
pub enum TaskCommand {
    /// List recent tasks.
    ///
    /// With `--pending-approval`, filters to tasks awaiting HITL approval.
    List {
        /// Show only tasks waiting for human approval (status = input_required).
        #[clap(long)]
        pending_approval: bool,
    },
    /// Display the status of a specific task.
    Status {
        /// Task identifier (UUID).
        task_id: String,
    },
    /// Cancel a running task.
    Cancel {
        /// Task identifier (UUID).
        task_id: String,

        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        confirm: bool,
    },
    /// Display the full execution plan of an orchestrated task.
    ///
    /// Reads directly from `~/.apollia/plans.db`, no runtime connection required.
    Inspect {
        /// Task identifier (UUID).
        id: String,
    },
    /// Approve or reject a task pending HITL approval.
    ///
    /// Exactly one of `--approve` or `--reject` must be supplied.
    Resume {
        /// Task identifier.
        task_id: String,

        /// Approve the pending task, resumes agent execution.
        #[clap(long, group = "decision")]
        approve: bool,

        /// Reject the pending task, terminates the task with REJECTED status.
        #[clap(long, group = "decision")]
        reject: bool,

        /// Human-readable reason for rejection (recommended with `--reject`).
        #[clap(long, requires = "reject")]
        reason: Option<String>,
    },
    /// List resolved HITL approvals (accepted or rejected).
    Approvals {
        /// Also include pending approvals.
        #[arg(long)]
        pending: bool,
    },
}

/// Execute a `task` subcommand.
///
/// Returns the process exit code.
pub async fn run(cmd: &TaskCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(default_socket_path);
    let client = RuntimeClient::new(socket_path);

    match cmd {
        TaskCommand::List { pending_approval } => {
            if *pending_approval {
                run_list_pending(&client, json).await
            } else {
                run_list(&client, json).await
            }
        }
        TaskCommand::Status { task_id } => run_status(&client, task_id, json).await,
        TaskCommand::Cancel { task_id, confirm } => {
            run_cancel(&client, task_id, *confirm, json).await
        }
        TaskCommand::Inspect { id } => run_inspect(id, json),
        TaskCommand::Resume {
            task_id,
            approve,
            reject,
            reason,
        } => {
            run_resume(
                &client,
                ResumeArgs {
                    task_id,
                    approve: *approve,
                    reject: *reject,
                    reason: reason.clone(),
                },
                json,
            )
            .await
        }
        TaskCommand::Approvals { pending } => run_approvals(&client, *pending, json).await,
    }
}
// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

/// Build the resume request body (exposed for testing).
///
/// Returns `{ "approved": <bool> }` optionally extended with `"reason"`.
pub fn build_resume_body(approved: bool, reason: Option<&str>) -> serde_json::Value {
    let mut body = serde_json::json!({ "approved": approved });
    if let Some(r) = reason {
        body["reason"] = serde_json::Value::String(r.to_string());
    }
    body
}

#[cfg(test)]
mod tests {
    use apollia_oria::plan_repository::{PlanWithSteps, StepRecord};
    use clap::Parser;

    use super::display::{build_pending_json, plan_to_json};
    use super::util::{
        approval_decision, format_duration_since, step_status_icon, truncate_output,
        truncate_prompt,
    };
    use super::*;
    use chrono::Utc;

    /// Minimal test app for parsing `task` subcommands without a full CLI.
    #[derive(Debug, Parser)]
    struct TestApp {
        #[command(subcommand)]
        cmd: TaskCommand,
    }

    /// Build a minimal `StepRecord` for use in tests.
    fn make_step(step_id: &str, status: &str, output: Option<&str>) -> StepRecord {
        StepRecord {
            step_id: step_id.into(),
            description: format!("Step {step_id}"),
            tool_hint: Some("file_io".into()),
            depends_on: vec![],
            status: status.into(),
            output: output.map(String::from),
            error: None,
            started_at: None,
            completed_at: None,
            input_rendered: None,
            input_truncated: false,
            output_text: None,
            output_truncated: false,
            tool_used: None,
            error_detail: None,
            duration_ms: None,
            rationale: None,
            provenance: apollia_core::plan::StepProvenance::default(),
            args: None,
        }
    }

    /// Build a minimal `PlanWithSteps` for use in tests.
    fn make_plan(task_id: &str, steps: Vec<StepRecord>) -> PlanWithSteps {
        PlanWithSteps {
            plan_id: "plan-001".into(),
            task_id: task_id.into(),
            agent_name: "test-agent".into(),
            status: "completed".into(),
            replan_count: 1,
            created_at: "2026-01-01T00:00:00".into(),
            steps,
        }
    }

    // GIVEN a 200-character output
    // WHEN truncate_output is called
    // THEN the output is truncated to 120 chars + "..."
    #[test]
    fn test_output_tronque() {
        // GIVEN
        let long_output = "x".repeat(200);

        // WHEN
        let truncated = truncate_output(&long_output);

        // THEN
        assert_eq!(truncated.len(), 123, "expected length: 120 + 3 = 123");
        assert!(truncated.ends_with("..."), "must end with '...'");
    }

    // GIVEN an output of exactly 120 characters
    // WHEN truncate_output is called
    // THEN the output is not truncated
    #[test]
    fn test_output_non_tronque_si_exact_120() {
        // GIVEN
        let exact = "y".repeat(120);

        // WHEN
        let result = truncate_output(&exact);

        // THEN
        assert_eq!(result.len(), 120);
        assert!(!result.ends_with("..."));
    }

    // GIVEN a completed plan with one step
    // WHEN plan_to_json is called
    // THEN the JSON contains the expected fields
    #[test]
    fn test_plan_to_json_structure() {
        // GIVEN
        let plan = make_plan(
            "task-001",
            vec![make_step("s1", "completed", Some("output A"))],
        );

        // WHEN
        let json = plan_to_json(&plan);

        // THEN
        assert_eq!(json["plan_id"], "plan-001");
        assert_eq!(json["task_id"], "task-001");
        assert_eq!(json["agent_name"], "test-agent");
        assert_eq!(json["status"], "completed");
        assert_eq!(json["replan_count"], 1);
        assert_eq!(json["steps"][0]["step_id"], "s1");
        assert_eq!(json["steps"][0]["output"], "output A");
        assert_eq!(json["steps"][0]["status"], "completed");
        assert_eq!(json["steps"][0]["tool_hint"], "file_io");
    }

    // GIVEN known and unknown step statuses
    // WHEN step_status_icon is called
    // THEN the correct icons are returned
    #[test]
    fn test_step_status_icons() {
        assert_eq!(step_status_icon("completed"), "✔");
        assert_eq!(step_status_icon("failed"), "✗");
        assert_eq!(step_status_icon("running"), "●");
        assert_eq!(step_status_icon("skipped"), "⏸");
        assert_eq!(step_status_icon("pending"), "○");
        assert_eq!(step_status_icon("unknown"), "○");
    }

    // resume --approve body
    // GIVEN approve=true, reason=None
    // WHEN build_resume_body is called
    // THEN body = { "approved": true } without "reason"
    #[test]
    fn test_resume_approve_body() {
        // GIVEN
        // WHEN
        let body = build_resume_body(true, None);

        // THEN
        assert_eq!(body["approved"], true);
        assert!(body.get("reason").is_none(), "reason should be absent");
    }

    // resume --reject --reason "Budget" body
    // GIVEN approve=false, reason=Some("Budget")
    // WHEN build_resume_body is called
    // THEN body = { "approved": false, "reason": "Budget" }
    #[test]
    fn test_resume_reject_with_reason_body() {
        // GIVEN
        // WHEN
        let body = build_resume_body(false, Some("Budget"));

        // THEN
        assert_eq!(body["approved"], false);
        assert_eq!(body["reason"], "Budget");
    }

    // JSON structure of the pending-approval list
    // GIVEN two pending tasks (mock)
    // WHEN build_pending_json is called
    // THEN JSON array with task_id, agent, waiting_since_secs, prompt, step_id
    #[test]
    fn test_pending_approval_json_output_structure() {
        // GIVEN a timestamp in the past so waiting_since_secs > 0
        let tasks = vec![
            serde_json::json!({
                "task_id": "t-0042",
                "agent_id": "devis-agent",
                "input_required_at": "2020-01-01T00:00:00Z",
                "input_required_prompt": "Quote 12,500 EUR incl. VAT - Dupont SA - confirm?",
                "step_id": "s1"
            }),
            serde_json::json!({
                "task_id": "t-0043",
                "agent_id": "contrats",
                "input_required_at": "2020-01-01T00:00:00Z",
                "input_required_prompt": "Send an email to dupont@acme.example - confirm?",
                "step_id": null
            }),
        ];

        // WHEN
        let output = build_pending_json(&tasks);

        // THEN array with expected fields
        assert_eq!(output.len(), 2);
        assert_eq!(output[0]["task_id"], "t-0042");
        assert_eq!(output[0]["agent"], "devis-agent");
        assert!(
            output[0]["waiting_since_secs"].as_u64().unwrap_or(0) > 0,
            "waiting_since_secs should be positive for a past timestamp"
        );
        assert_eq!(
            output[0]["prompt"],
            "Quote 12,500 EUR incl. VAT - Dupont SA - confirm?"
        );
        assert_eq!(output[0]["step_id"], "s1");
        assert_eq!(output[1]["task_id"], "t-0043");
        assert_eq!(output[1]["step_id"], serde_json::Value::Null);
    }

    // --approve and --reject are mutually exclusive
    // GIVEN "task resume t-0042 --approve --reject"
    // WHEN clap parses
    // THEN group conflict error (parse error)
    #[test]
    fn test_approve_and_reject_mutually_exclusive() {
        // GIVEN
        // WHEN
        let result = TestApp::try_parse_from(["app", "resume", "t-0042", "--approve", "--reject"]);

        // THEN clap returns an error for conflicting group members
        assert!(
            result.is_err(),
            "--approve and --reject must be mutually exclusive"
        );
    }

    // prompt truncation at 60 chars
    // GIVEN an 80-character prompt
    // WHEN truncate_prompt is called
    // THEN the prompt is truncated to 60 + "..."
    #[test]
    fn test_truncate_prompt_long() {
        // GIVEN
        let long_prompt = "a".repeat(80);

        // WHEN
        let result = truncate_prompt(&long_prompt);

        // THEN
        assert_eq!(result.len(), 63, "60 + '...' = 63");
        assert!(result.ends_with("..."));
    }

    // short prompt left untruncated
    // GIVEN a 30-character prompt
    // WHEN truncate_prompt is called
    // THEN the prompt is returned as-is
    #[test]
    fn test_truncate_prompt_short() {
        // GIVEN
        let short_prompt = "Hello world";

        // WHEN
        let result = truncate_prompt(short_prompt);

        // THEN
        assert_eq!(result, "Hello world");
        assert!(!result.ends_with("..."));
    }

    // format_duration_since with a past timestamp
    // GIVEN a timestamp 90 minutes ago
    // WHEN format_duration_since is called
    // THEN the duration is formatted as "1h"
    #[test]
    fn test_format_duration_since_hours() {
        use chrono::Duration;
        // GIVEN a timestamp 90 minutes ago
        let past = Utc::now() - Duration::minutes(90);
        let ts = past.to_rfc3339();

        // WHEN
        let result = format_duration_since(&ts);

        // THEN
        assert_eq!(result, "1h");
    }

    // format_duration_since with an invalid timestamp
    // GIVEN an invalid timestamp
    // WHEN format_duration_since is called
    // THEN "-" is returned
    #[test]
    fn test_format_duration_since_invalid() {
        // GIVEN / WHEN / THEN
        assert_eq!(format_duration_since("not-a-date"), "-");
    }

    // approvals without --pending
    // GIVEN "task approvals"
    // WHEN parse
    // THEN TaskCommand::Approvals { pending: false }
    #[test]
    fn test_task_approvals_parses() {
        // GIVEN / WHEN
        let cli = TestApp::parse_from(["app", "approvals"]);
        // THEN
        match &cli.cmd {
            TaskCommand::Approvals { pending } => assert!(!pending),
            other => panic!("expected Approvals, got {other:?}"),
        }
    }

    // approvals with --pending
    // GIVEN "task approvals --pending"
    // WHEN parse
    // THEN TaskCommand::Approvals { pending: true }
    #[test]
    fn test_task_approvals_pending_parses() {
        // GIVEN / WHEN
        let cli = TestApp::parse_from(["app", "approvals", "--pending"]);
        // THEN
        match &cli.cmd {
            TaskCommand::Approvals { pending } => assert!(pending),
            other => panic!("expected Approvals, got {other:?}"),
        }
    }

    // The approvals table renders exactly one decision, and it is the only
    // part of it a test can read: `format_approvals_list` prints and returns
    // nothing, so calling it asserted nothing about either branch.
    #[test]
    fn test_approval_decision_labels_every_state() {
        // GIVEN one approval row per state the API can return
        let approved = serde_json::json!({ "approved": true });
        let rejected = serde_json::json!({ "approved": false });
        let undecided = serde_json::json!({ "id": "appr-001" });

        // WHEN the decision column is rendered, resolved and pending
        // THEN each state has its own label, and a pending row ignores the flag
        assert_eq!(approval_decision(&approved, false), "approved");
        assert_eq!(approval_decision(&rejected, false), "rejected");
        assert_eq!(approval_decision(&undecided, false), "?");
        assert_eq!(approval_decision(&approved, true), "pending");
    }
}
