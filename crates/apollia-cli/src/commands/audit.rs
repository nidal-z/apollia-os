//! `apollia-os audit` subcommands — query audit trail via the runtime API.
//!
//! Provides `list` and `stats` operations on the audit trail.

use std::path::PathBuf;

use clap::Subcommand;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

/// Audit subcommands: `apollia-os audit <verb>`.
#[derive(Debug, Subcommand)]
pub enum AuditCommand {
    /// List recent audit events (default).
    #[command(name = "list")]
    List {
        /// Maximum number of events to display.
        #[arg(long, default_value = "20")]
        limit: u32,
    },
    /// Display audit statistics.
    Stats,
}

/// Execute an `audit` subcommand.
///
/// Returns the process exit code.
pub async fn run(cmd: &AuditCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let client = RuntimeClient::new(socket_path);

    match cmd {
        AuditCommand::List { limit } => run_list(&client, *limit, json).await,
        AuditCommand::Stats => run_stats(&client, json).await,
    }
}

/// `apollia-os audit list` — display recent audit events.
async fn run_list(client: &RuntimeClient, limit: u32, json: bool) -> i32 {
    let uri = format!("/api/v1/audit?limit={limit}");
    let resp = match client.get(&uri).await {
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
        format_audit_list(&parsed);
    }
    exit_codes::SUCCESS
}

/// `apollia-os audit stats` — display audit statistics.
async fn run_stats(client: &RuntimeClient, json: bool) -> i32 {
    let resp = match client.get("/api/v1/audit/stats").await {
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
        format_audit_stats(&parsed);
    }
    exit_codes::SUCCESS
}

/// Format audit events as a human-readable table.
fn format_audit_list(resp: &serde_json::Value) {
    let events = resp
        .get("events")
        .and_then(|e| e.as_array())
        .cloned()
        .unwrap_or_default();

    println!(
        "  {:<24} {:<24} {:<20} {:<8} {:<8}",
        "TIMESTAMP", "AGENT_ID", "TOOL", "STATUS", "MS"
    );

    if events.is_empty() {
        println!("  (no audit events)");
    } else {
        for event in &events {
            // API field names: started_at (RFC3339), success (bool), duration_ms (u64)
            let ts = event
                .get("started_at")
                .and_then(|v| v.as_str())
                // Trim to 23 chars (drop sub-second precision) for compact display
                .map(|s| s.get(..19).unwrap_or(s))
                .unwrap_or("?");
            let agent = event
                .get("agent_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let tool = event
                .get("tool_name")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let status = match event.get("success").and_then(|v| v.as_bool()) {
                Some(true) => "ok",
                Some(false) => "failed",
                None => "?",
            };
            let ms = event
                .get("duration_ms")
                .and_then(|v| v.as_u64())
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string());
            println!(
                "  {:<24} {:<24} {:<20} {:<8} {:<8}",
                ts, agent, tool, status, ms
            );
        }
    }
}

/// Format audit stats as human-readable text.
fn format_audit_stats(resp: &serde_json::Value) {
    let total = resp
        .get("total_events")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let tools_used = resp
        .get("unique_tools")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let agents = resp
        .get("unique_agents")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    println!("  Total events  : {total}");
    println!("  Unique tools  : {tools_used}");
    println!("  Unique agents : {agents}");
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
