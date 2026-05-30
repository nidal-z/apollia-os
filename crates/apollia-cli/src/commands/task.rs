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

use apollia_oria::plan_repository::{PlanRepositoryError, PlanWithSteps, StepRecord};
use chrono::{DateTime, Utc};
use clap::Subcommand;

use crate::client::{ClientError, RawResponse, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

/// Default path of the plans SQLite database.
const DEFAULT_PLANS_DB: &str = "/.apollia/plans.db";

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
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
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
        TaskCommand::Cancel { task_id } => run_cancel(&client, task_id, json).await,
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
// Handlers
// ────────────────────────────────────────────────────────────────────────────

/// `apollia-os task list`: display recent tasks.
///
/// Resolves `agent_id` (UUID) to the human-readable agent name by fetching
/// the agents list once and joining locally. The runtime does not embed the
/// agent name in `GET /api/v1/tasks` responses, and a UUID-only table is
/// hostile to operators.
async fn run_list(client: &RuntimeClient, json: bool) -> i32 {
    let resp = match client.get("/api/v1/tasks").await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
    };

    if resp.status >= 400 {
        return handle_server_error(resp.status, &resp.body, json);
    }

    let parsed: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: invalid JSON response: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&parsed).unwrap_or_default()
        );
    } else {
        // Best-effort agent_id → name lookup. If the agents endpoint is
        // unreachable, we just fall back to displaying the raw UUIDs.
        let agent_names = client
            .list_agents()
            .await
            .ok()
            .and_then(|v| {
                v.get("agents")
                    .or_else(|| Some(&v))
                    .and_then(|x| x.as_array())
                    .cloned()
            })
            .map(|agents| {
                agents
                    .iter()
                    .filter_map(|a| {
                        let id = a.get("agent_id").or_else(|| a.get("id"))?.as_str()?;
                        let name = a.get("name")?.as_str()?;
                        Some((id.to_string(), name.to_string()))
                    })
                    .collect::<std::collections::HashMap<String, String>>()
            })
            .unwrap_or_default();
        format_task_list(&parsed, &agent_names);
    }
    exit_codes::SUCCESS
}

/// `apollia-os task list --pending-approval`: display tasks awaiting HITL approval.
///
/// Calls `GET /api/v1/tasks?status=input_required` and renders a table with
/// `TASK_ID | AGENT | DEPUIS | PROMPT` columns, or a JSON array.
async fn run_list_pending(client: &RuntimeClient, json: bool) -> i32 {
    let resp = match client.get("/api/v1/tasks?status=input_required").await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
    };

    if resp.status >= 400 {
        return handle_server_error(resp.status, &resp.body, json);
    }

    let parsed: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: invalid JSON response: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    let tasks = extract_tasks_array(&parsed);

    if json {
        let output = build_pending_json(&tasks);
        match serde_json::to_string_pretty(&output) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("Error: JSON serialization failed: {e}");
                return exit_codes::GENERAL_ERROR;
            }
        }
    } else {
        format_pending_table(&tasks);
    }
    exit_codes::SUCCESS
}

/// Prints an indented, labelled multi-line block when the field is present.
fn print_status_block(resp: &serde_json::Value, key: &str, label: &str) {
    let Some(text) = resp.get(key).and_then(|v| v.as_str()) else {
        return;
    };
    println!("  {label}:");
    for line in text.lines() {
        println!("    {line}");
    }
}

/// Prints the token budget line when usage is recorded.
fn print_token_budget(resp: &serde_json::Value) {
    let Some(budget) = resp.get("token_budget") else {
        return;
    };
    let input = budget
        .get("input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output = budget
        .get("output_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if input + output > 0 {
        println!("  Tokens    : {input} in / {output} out");
    }
}

/// Renders the human-readable status summary for a task response.
fn print_status_human(resp: &serde_json::Value, task_id: &str) {
    let status = resp.get("status").and_then(|v| v.as_str()).unwrap_or("?");
    println!("  Task      : {task_id}");
    println!("  Status    : {status}");
    print_status_block(resp, "error", "Error     ");
    print_status_block(resp, "result", "Result    ");
    print_token_budget(resp);
}

/// `apollia-os task status <id>`: display task status.
async fn run_status(client: &RuntimeClient, task_id: &str, json: bool) -> i32 {
    let resp = match client.get_task(task_id).await {
        Ok(resp) => resp,
        Err(e) => return handle_error(e, json),
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&resp).unwrap_or_default()
        );
    } else {
        print_status_human(&resp, task_id);
    }
    exit_codes::SUCCESS
}

/// `apollia-os task cancel <id>`: cancel a running task.
async fn run_cancel(client: &RuntimeClient, task_id: &str, json: bool) -> i32 {
    match client.cancel_task(task_id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("Task {task_id} canceled");
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// Decision fields supplied to `apollia-os task resume`.
struct ResumeArgs<'a> {
    task_id: &'a str,
    approve: bool,
    reject: bool,
    reason: Option<String>,
}

/// `apollia-os task resume <id> --approve|--reject [--reason "..."]`
///
/// Posts `{ approved: bool, reason?: String }` to
/// `POST /api/v1/tasks/{id}/resume` and prints the result.
async fn run_resume(client: &RuntimeClient, args: ResumeArgs<'_>, json: bool) -> i32 {
    let ResumeArgs {
        task_id,
        approve,
        reject,
        reason,
    } = args;
    // Manual guard: clap groups make --approve/--reject mutually exclusive but
    // not required; validate the "neither" case here.
    if !approve && !reject {
        eprintln!("Error: one of --approve or --reject must be specified");
        return exit_codes::GENERAL_ERROR;
    }

    let approved = approve;
    let resp = match client.resume_task(task_id, approved, reason).await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
    };

    // HTTP 409 means the task is not in input_required state.
    if resp.status == 409 {
        let msg = extract_error_message(&resp, "task is not awaiting approval");
        if json {
            let output = serde_json::json!({ "error": msg });
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_default()
            );
        } else {
            eprintln!("Error: task {task_id} is not awaiting approval");
        }
        return exit_codes::GENERAL_ERROR;
    }

    if resp.status >= 400 {
        return handle_server_error(resp.status, &resp.body, json);
    }

    let parsed: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: invalid JSON response: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&parsed).unwrap_or_default()
        );
    } else {
        let agent = parsed
            .get("agent_id")
            .or_else(|| parsed.get("agent"))
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let status = parsed.get("status").and_then(|v| v.as_str()).unwrap_or("?");

        if approved {
            println!("✔ Task {task_id} approved — {agent} › {status}...");
        } else {
            println!("✔ Task {task_id} rejected — {agent} › done ({status})");
        }
    }
    exit_codes::SUCCESS
}

/// `apollia-os task inspect <id>`: display the full execution plan of an orchestrated task.
///
/// Opens `~/.apollia/plans.db` directly (no runtime required) and renders the plan
/// with per-step statuses, outputs, and errors. On `NotFound`, exits with code 0 and
/// an informative message (direct-mode tasks have no persisted plan, this is normal).
fn run_inspect(task_id: &str, json: bool) -> i32 {
    let db_path = {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}{DEFAULT_PLANS_DB}")
    };

    let repo = match apollia_oria::plan_repository::PlanRepository::new(&db_path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: impossible d'ouvrir la base de plans : {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    match repo.get_plan_with_steps(task_id) {
        Ok(plan) => {
            if json {
                let json_val = plan_to_json(&plan);
                match serde_json::to_string_pretty(&json_val) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("Error: JSON serialization failed: {e}");
                        return exit_codes::GENERAL_ERROR;
                    }
                }
            } else {
                display_plan_human(&plan);
            }
            exit_codes::SUCCESS
        }
        Err(PlanRepositoryError::NotFound(_)) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "task_id": task_id,
                        "plan": null,
                        "reason": "no_plan_persisted",
                    })
                );
            } else {
                println!("Task {task_id} has no execution plan.");
                println!();
                println!("Plans are only generated for agents with `execution_mode =");
                println!("\"orchestrated\"` in their manifest. Agents marked `direct` drive");
                println!("their own logic (state machines, A2A delegation, ReAct loops) and");
                println!("never go through the ORIA Reasoner, so plans.db has nothing to");
                println!("show for them.");
                println!();
                println!("To see a real plan, run an orchestrated agent (e.g. email-triage)");
                println!("then re-run `apollia-os task inspect <id>`.");
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: {e}");
            exit_codes::GENERAL_ERROR
        }
    }
}

/// `apollia-os task approvals [--pending]`: list resolved (or pending) HITL approvals.
///
/// Without `--pending`: calls `GET /api/v1/approvals/resolved`.
/// With `--pending`: calls `GET /api/v1/approvals/pending`.
async fn run_approvals(client: &RuntimeClient, pending: bool, json: bool) -> i32 {
    let uri = if pending {
        "/api/v1/approvals/pending"
    } else {
        "/api/v1/approvals/resolved"
    };

    let resp = match client.get(uri).await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
    };

    if resp.status >= 400 {
        return handle_server_error(resp.status, &resp.body, json);
    }

    let parsed: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: invalid JSON response: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&parsed).unwrap_or_default()
        );
    } else {
        format_approvals_list(&parsed, pending);
    }
    exit_codes::SUCCESS
}

// ────────────────────────────────────────────────────────────────────────────
// Display helpers
// ────────────────────────────────────────────────────────────────────────────

/// Format task list as a human-readable table.
///
/// The `agent_names` map (agent_id → name) is populated by the caller from
/// `GET /api/v1/agents`. When the lookup misses (deleted agent, stale UUID)
/// we display the truncated UUID instead so the column still aligns and
/// the operator can still copy-paste it.
fn format_task_list(
    resp: &serde_json::Value,
    agent_names: &std::collections::HashMap<String, String>,
) {
    let tasks = extract_tasks_array(resp);

    println!("  {:<36} {:<26} {:<12}", "TASK_ID", "AGENT", "STATUS");

    if tasks.is_empty() {
        println!("  (no tasks)");
    } else {
        for task in &tasks {
            let id = task.get("task_id").and_then(|v| v.as_str()).unwrap_or("?");
            let agent_id = task.get("agent_id").and_then(|v| v.as_str()).unwrap_or("?");
            let agent_label = agent_names
                .get(agent_id)
                .cloned()
                .unwrap_or_else(|| short_uuid(agent_id));
            let status = task.get("status").and_then(|v| v.as_str()).unwrap_or("?");
            println!("  {:<36} {:<26} {status}", id, agent_label);
        }
    }
}

/// Shorten a UUID to its first segment for display when the agent name is
/// unknown. Keeps the column aligned and the value copy-paste-traceable.
fn short_uuid(uuid: &str) -> String {
    uuid.split('-').next().unwrap_or(uuid).to_string()
}

/// Render pending-approval tasks as a human-readable table.
///
/// Columns: `TASK_ID | AGENT | DEPUIS | PROMPT` (prompt truncated to 60 chars).
fn format_pending_table(tasks: &[serde_json::Value]) {
    println!("  {:<36} {:<20} {:<8} PROMPT", "TASK_ID", "AGENT", "DEPUIS");

    if tasks.is_empty() {
        println!("  (no pending approvals)");
        return;
    }

    for task in tasks {
        let id = task.get("task_id").and_then(|v| v.as_str()).unwrap_or("?");
        let agent = task
            .get("agent_id")
            .or_else(|| task.get("agent"))
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let since = task
            .get("input_required_at")
            .and_then(|v| v.as_str())
            .map(format_duration_since)
            .unwrap_or_else(|| "-".to_string());
        let prompt = task
            .get("input_required_prompt")
            .or_else(|| task.get("prompt"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        println!(
            "  {:<36} {:<20} {:<8} \"{}\"",
            id,
            agent,
            since,
            truncate_prompt(prompt)
        );
    }
}

/// Build the JSON array for `--pending-approval --json` output.
///
/// Each element has: `task_id`, `agent`, `waiting_since_secs`, `prompt`, `step_id`.
pub fn build_pending_json(tasks: &[serde_json::Value]) -> Vec<serde_json::Value> {
    tasks
        .iter()
        .map(|task| {
            let task_id = task.get("task_id").and_then(|v| v.as_str()).unwrap_or("?");
            let agent = task
                .get("agent_id")
                .or_else(|| task.get("agent"))
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let prompt = task
                .get("input_required_prompt")
                .or_else(|| task.get("prompt"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let step_id = task
                .get("step_id")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let waiting_since_secs = task
                .get("input_required_at")
                .and_then(|v| v.as_str())
                .map(elapsed_seconds)
                .unwrap_or(0);

            serde_json::json!({
                "task_id": task_id,
                "agent": agent,
                "waiting_since_secs": waiting_since_secs,
                "prompt": prompt,
                "step_id": step_id,
            })
        })
        .collect()
}

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

/// Render an orchestrated plan as a human-readable table on stdout.
///
/// Displays a header with task metadata followed by one block per step showing
/// its status icon, description, tool hint, output (truncated to 120 chars), and error if any.
fn display_plan_human(plan: &PlanWithSteps) {
    println!();
    println!("  Task        : {}", plan.task_id);
    println!("  Agent       : {}", plan.agent_name);
    println!("  Mode        : orchestrated");
    println!("  Status      : {}", plan.status);
    println!("  Created     : {}", format_rfc3339_compact(&plan.created_at));
    println!("  Replans     : {}/2", plan.replan_count);
    println!();
    println!("  Execution plan:");

    for step in &plan.steps {
        let icon = step_status_icon(&step.status);
        let replan_marker = if step.step_id.ends_with('b') || step.step_id.ends_with('c') {
            "  [replanned]"
        } else {
            ""
        };

        println!();
        println!(
            "  {} [{}]  {}{}",
            icon, step.step_id, step.description, replan_marker
        );

        if let Some(ref tool) = step.tool_hint {
            println!(
                "          tool: {} | duration: {}",
                tool,
                step_duration(step)
            );
        }

        if let Some(ref output) = step.output {
            let truncated = truncate_output(output);
            println!("          output: \"{truncated}\"");
        }

        if let Some(ref error) = step.error {
            println!("          error: \"{error}\"");
            if step.status == "failed" {
                println!("          → replanning triggered");
            }
        }
    }
    println!();
}

/// Render an RFC3339 timestamp as `YYYY-MM-DD HH:MM:SS` for compact display.
///
/// Falls back to the raw value when parsing fails; the formatter must
/// never lose information just because the runtime grew a new format.
fn format_rfc3339_compact(raw: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|dt| {
            dt.with_timezone(&chrono::Utc)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|_| raw.to_string())
}

/// Serialize a plan with its steps to a `serde_json::Value` for `--json` output.
///
/// Produces the JSON structure :
/// `{ plan_id, task_id, agent_name, status, replan_count, created_at, steps: [...] }`.
fn plan_to_json(plan: &PlanWithSteps) -> serde_json::Value {
    serde_json::json!({
        "plan_id":      plan.plan_id,
        "task_id":      plan.task_id,
        "agent_name":   plan.agent_name,
        "status":       plan.status,
        "replan_count": plan.replan_count,
        "created_at":   plan.created_at,
        "steps": plan.steps.iter().map(|s| serde_json::json!({
            "step_id":      s.step_id,
            "description":  s.description,
            "tool_hint":    s.tool_hint,
            "depends_on":   s.depends_on,
            "status":       s.status,
            "output":       s.output,
            "error":        s.error,
            "started_at":   s.started_at,
            "completed_at": s.completed_at,
        })).collect::<Vec<_>>()
    })
}

// ────────────────────────────────────────────────────────────────────────────
// Pure utilities
// ────────────────────────────────────────────────────────────────────────────

/// Format the approvals list as a human-readable table.
///
/// Columns: `ID | TASK_ID | AGENT | DECISION | DATE`
fn format_approvals_list(resp: &serde_json::Value, pending: bool) {
    let key = if pending { "pending" } else { "approvals" };
    let approvals = resp
        .get(key)
        .or_else(|| resp.get("approvals"))
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default();

    let header = if pending {
        "APPROBATIONS EN ATTENTE"
    } else {
        "APPROBATIONS RÉSOLUES"
    };
    println!("  {header}");
    println!(
        "  {:<36} {:<36} {:<20} {:<10} DATE",
        "ID", "TASK_ID", "AGENT", "DÉCISION"
    );

    if approvals.is_empty() {
        println!("  (aucune)");
        return;
    }

    for a in &approvals {
        print_approval_row(a, pending);
    }
}

/// Maps the approval decision to its display label.
fn approval_decision(a: &serde_json::Value, pending: bool) -> &'static str {
    if pending {
        return "en attente";
    }
    match a.get("approved").and_then(|v| v.as_bool()) {
        Some(true) => "approved",
        Some(false) => "rejected",
        None => "?",
    }
}

/// Renders a single approvals-table row.
fn print_approval_row(a: &serde_json::Value, pending: bool) {
    let id = a.get("id").and_then(|v| v.as_str()).unwrap_or("?");
    let task_id = a.get("task_id").and_then(|v| v.as_str()).unwrap_or("?");
    let agent = a.get("agent_id").and_then(|v| v.as_str()).unwrap_or("?");
    let decision = approval_decision(a, pending);
    let date = a
        .get("resolved_at")
        .or_else(|| a.get("requested_at"))
        .and_then(|v| v.as_str())
        .map(|s| if s.len() >= 19 { &s[..19] } else { s })
        .unwrap_or("?");
    println!(
        "  {:<36} {:<36} {:<20} {:<10} {}",
        id, task_id, agent, decision, date
    );
}

/// Extract the `tasks` array from a server response, defaulting to an empty vec.
fn extract_tasks_array(resp: &serde_json::Value) -> Vec<serde_json::Value> {
    resp.get("tasks")
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default()
}

/// Extract an error message from a raw response body, with a fallback.
fn extract_error_message(resp: &RawResponse, fallback: &str) -> String {
    serde_json::from_str::<serde_json::Value>(&resp.body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
        .unwrap_or_else(|| fallback.to_string())
}

/// Format a duration since an RFC3339 timestamp as a human-readable string.
///
/// Returns `"Xmin"` for durations under one hour, `"Xh"` otherwise.
/// Returns `"-"` if the timestamp cannot be parsed.
pub fn format_duration_since(input_required_at: &str) -> String {
    let Ok(dt) = input_required_at.parse::<DateTime<Utc>>() else {
        return "-".to_string();
    };
    let elapsed = Utc::now().signed_duration_since(dt);
    let mins = elapsed.num_minutes().max(0);
    if mins < 1 {
        "< 1min".to_string()
    } else if mins < 60 {
        format!("{mins}min")
    } else {
        format!("{}h", mins / 60)
    }
}

/// Compute elapsed seconds since an RFC3339 timestamp (for JSON `waiting_since_secs`).
///
/// Returns `0` if the timestamp cannot be parsed or is in the future.
pub fn elapsed_seconds(input_required_at: &str) -> u64 {
    let Ok(dt) = input_required_at.parse::<DateTime<Utc>>() else {
        return 0;
    };
    Utc::now().signed_duration_since(dt).num_seconds().max(0) as u64
}

/// Truncate a prompt string to [`MAX_PROMPT_LEN`] characters, appending `"..."`.
pub fn truncate_prompt(prompt: &str) -> String {
    if prompt.len() > MAX_PROMPT_LEN {
        format!("{}...", &prompt[..MAX_PROMPT_LEN])
    } else {
        prompt.to_string()
    }
}

/// Return the Unicode status icon for a step status string.
fn step_status_icon(status: &str) -> &'static str {
    match status {
        "completed" => "✔",
        "failed" => "✗",
        "running" => "●",
        "skipped" => "⏸",
        _ => "○",
    }
}

/// Compute a human-readable duration for a step, or `"-"` if unavailable.
fn step_duration(step: &StepRecord) -> &'static str {
    if step.started_at.is_some() && step.completed_at.is_some() {
        "?"
    } else {
        "-"
    }
}

/// Truncate `output` to [`MAX_OUTPUT_LEN`] chars, appending `"..."` if truncated.
fn truncate_output(output: &str) -> String {
    if output.len() > MAX_OUTPUT_LEN {
        format!("{}...", &output[..MAX_OUTPUT_LEN])
    } else {
        output.to_string()
    }
}

/// Handle client errors uniformly.
fn handle_error(err: ClientError, json: bool) -> i32 {
    match err {
        ClientError::ConnectionRefused => {
            if json {
                let output =
                    serde_json::json!({"error": "runtime not started (connection refused)"});
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                );
            } else {
                eprintln!("Error: runtime not started (connection refused)");
            }
            exit_codes::RUNTIME_ERROR
        }
        other => {
            if json {
                let output = serde_json::json!({"error": other.to_string()});
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                );
            } else {
                eprintln!("Error: {other}");
            }
            exit_codes::GENERAL_ERROR
        }
    }
}

/// Handle HTTP server errors.
fn handle_server_error(status: u16, body: &str, json: bool) -> i32 {
    let error_msg = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
        .unwrap_or_else(|| format!("server error ({status})"));

    if json {
        let output = serde_json::json!({"error": error_msg});
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
    } else {
        eprintln!("Error: {error_msg}");
    }
    exit_codes::GENERAL_ERROR
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use apollia_oria::plan_repository::{PlanWithSteps, StepRecord};
    use clap::Parser;

    use super::*;

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
    fn test_ac5_output_tronque() {
        // GIVEN
        let long_output = "x".repeat(200);

        // WHEN
        let truncated = truncate_output(&long_output);

        // THEN
        assert_eq!(truncated.len(), 123, "longueur attendue : 120 + 3 = 123");
        assert!(truncated.ends_with("..."), "doit se terminer par '...'");
    }

    // GIVEN an output of exactly 120 characters
    // WHEN truncate_output is called
    // THEN the output is not truncated
    #[test]
    fn test_ac5_output_non_tronque_si_exact_120() {
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
    fn test_ac2_resume_approve_body() {
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
    fn test_ac3_resume_reject_with_reason_body() {
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
    fn test_ac5_pending_approval_json_output_structure() {
        // GIVEN a timestamp in the past so waiting_since_secs > 0
        let tasks = vec![
            serde_json::json!({
                "task_id": "t-0042",
                "agent_id": "devis-agent",
                "input_required_at": "2020-01-01T00:00:00Z",
                "input_required_prompt": "Devis 12 500€ TTC — Dupont SA — confirmer ?",
                "step_id": "s1"
            }),
            serde_json::json!({
                "task_id": "t-0043",
                "agent_id": "contrats",
                "input_required_at": "2020-01-01T00:00:00Z",
                "input_required_prompt": "Envoyer email à dupont@acme.fr — confirmer ?",
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
            "Devis 12 500€ TTC — Dupont SA — confirmer ?"
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
    fn test_ac6_approve_and_reject_mutually_exclusive() {
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

    // format_approvals_list with no approvals
    // GIVEN an empty response
    // WHEN format_approvals_list is called
    // THEN no panic
    #[test]
    fn test_format_approvals_list_empty() {
        // GIVEN
        let resp = serde_json::json!({ "approvals": [] });
        // WHEN / THEN no panic
        format_approvals_list(&resp, false);
    }

    // format_approvals_list with data
    // GIVEN a resolved approval
    // WHEN format_approvals_list is called
    // THEN no panic
    #[test]
    fn test_format_approvals_list_with_data() {
        // GIVEN
        let resp = serde_json::json!({
            "approvals": [
                {
                    "id": "appr-001",
                    "task_id": "t-0042",
                    "agent_id": "devis-agent",
                    "approved": true,
                    "resolved_at": "2026-01-01T10:00:00Z"
                }
            ]
        });
        // WHEN / THEN no panic
        format_approvals_list(&resp, false);
    }
}
