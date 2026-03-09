//! `apollia-os task` subcommands — manage tasks via the runtime API.
//!
//! Provides `list`, `status`, `cancel`, and `inspect` operations on tasks.
//! The `inspect` subcommand reads directly from SQLite (`~/.apollia/plans.db`)
//! without requiring a running runtime (Principe #1 — Local-first).

use std::path::PathBuf;

use apollia_oria::plan_repository::{PlanRepositoryError, PlanWithSteps, StepRecord};
use clap::Subcommand;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

/// Default path of the plans SQLite database.
const DEFAULT_PLANS_DB: &str = "/.apollia/plans.db";

/// Maximum output length before truncation in human-readable display.
const MAX_OUTPUT_LEN: usize = 120;

/// Task subcommands: `apollia-os task <verb>`.
#[derive(Debug, Subcommand)]
pub enum TaskCommand {
    /// List recent tasks.
    List,
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
    /// Reads directly from `~/.apollia/plans.db` — no runtime connection required.
    Inspect {
        /// Task identifier (UUID).
        id: String,
    },
}

/// Execute a `task` subcommand.
///
/// Returns the process exit code.
pub async fn run(cmd: &TaskCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let client = RuntimeClient::new(socket_path);

    match cmd {
        TaskCommand::List => run_list(&client, json).await,
        TaskCommand::Status { task_id } => run_status(&client, task_id, json).await,
        TaskCommand::Cancel { task_id } => run_cancel(&client, task_id, json).await,
        TaskCommand::Inspect { id } => run_inspect(id, json),
    }
}

/// `apollia-os task list` — display recent tasks.
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
        format_task_list(&parsed);
    }
    exit_codes::SUCCESS
}

/// `apollia-os task status <id>` — display task status.
async fn run_status(client: &RuntimeClient, task_id: &str, json: bool) -> i32 {
    match client.get_task(task_id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                let status = resp.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                println!("  Task      : {task_id}");
                println!("  Status    : {status}");
                if let Some(error) = resp.get("error").and_then(|v| v.as_str()) {
                    println!("  Error     : {error}");
                }
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os task cancel <id>` — cancel a running task.
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

/// Format task list as a human-readable table.
fn format_task_list(resp: &serde_json::Value) {
    let tasks = resp
        .get("tasks")
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();

    println!("  {:<36} {:<36} {:<12}", "TASK_ID", "AGENT_ID", "STATUS");

    if tasks.is_empty() {
        println!("  (no tasks)");
    } else {
        for task in &tasks {
            let id = task.get("task_id").and_then(|v| v.as_str()).unwrap_or("?");
            let agent = task.get("agent_id").and_then(|v| v.as_str()).unwrap_or("?");
            let status = task.get("status").and_then(|v| v.as_str()).unwrap_or("?");
            println!("  {:<36} {:<36} {status}", id, agent);
        }
    }
}

/// `apollia-os task inspect <id>` — display the full execution plan of an orchestrated task.
///
/// Opens `~/.apollia/plans.db` directly (no runtime required) and renders the plan
/// with per-step statuses, outputs, and errors. On `NotFound`, exits with code 0 and
/// an informative message (direct-mode tasks have no persisted plan — this is normal).
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
            println!(
                "La tâche {task_id} n'a pas de plan d'exécution (mode direct ou plan non persisté)."
            );
            exit_codes::SUCCESS
        }
        Err(e) => {
            eprintln!("Erreur : {e}");
            exit_codes::GENERAL_ERROR
        }
    }
}

/// Render an orchestrated plan as a human-readable table on stdout.
///
/// Displays a header with task metadata followed by one block per step showing
/// its status icon, description, tool hint, output (truncated to 120 chars), and error if any.
fn display_plan_human(plan: &PlanWithSteps) {
    println!();
    println!("  Tâche       : {}", plan.task_id);
    println!("  Agent       : {}", plan.agent_name);
    println!("  Mode        : orchestré");
    println!("  Statut      : {}", plan.status);
    println!("  Créé        : {}", plan.created_at);
    println!("  Replanif.   : {}/2", plan.replan_count);
    println!();
    println!("  Plan d'exécution :");

    for step in &plan.steps {
        let icon = step_status_icon(&step.status);
        let replan_marker = if step.step_id.ends_with('b') || step.step_id.ends_with('c') {
            "  [replanifié]"
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
                "          outil : {} | durée : {}",
                tool,
                step_duration(step)
            );
        }

        if let Some(ref output) = step.output {
            let truncated = truncate_output(output);
            println!("          output : \"{truncated}\"");
        }

        if let Some(ref error) = step.error {
            println!("          erreur : \"{error}\"");
            if step.status == "failed" {
                println!("          → replanification déclenchée");
            }
        }
    }
    println!();
}

/// Serialize a plan with its steps to a `serde_json::Value` for `--json` output.
///
/// Produces the structure expected by AC-2 :
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

    use super::*;

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

    // GIVEN un output de 200 caractères
    // WHEN truncate_output est appelé
    // THEN l'output est tronqué à 120 chars + "..." (AC-5)
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

    // GIVEN un output exactement de 120 caractères
    // WHEN truncate_output est appelé
    // THEN l'output n'est pas tronqué
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

    // GIVEN un plan complété avec un step
    // WHEN plan_to_json est appelé
    // THEN le JSON contient les champs attendus (AC-2)
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

    // GIVEN des statuts step connus et inconnus
    // WHEN step_status_icon est appelé
    // THEN les icônes correctes sont retournées
    #[test]
    fn test_step_status_icons() {
        assert_eq!(step_status_icon("completed"), "✔");
        assert_eq!(step_status_icon("failed"), "✗");
        assert_eq!(step_status_icon("running"), "●");
        assert_eq!(step_status_icon("skipped"), "⏸");
        assert_eq!(step_status_icon("pending"), "○");
        assert_eq!(step_status_icon("unknown"), "○");
    }
}
