//! `apollia-os task` subcommands — manage tasks via the runtime API.
//!
//! Provides `list`, `status`, and `cancel` operations on tasks.

use std::path::PathBuf;

use clap::Subcommand;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

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
