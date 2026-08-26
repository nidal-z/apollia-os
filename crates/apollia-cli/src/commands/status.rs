//! `apollia-os status`: display runtime and agent status.
//!
//! Connects via Unix socket and queries health + agent list endpoints.

use std::path::PathBuf;

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;
use crate::note;

/// Execute the `status` command.
///
/// Returns the process exit code.
pub async fn run(socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(default_socket_path);
    let client = RuntimeClient::new(socket_path);

    // Check health first
    if let Err(e) = client.health().await {
        return handle_connection_error(e, json);
    }

    // Fetch agents
    let agents_result = client.list_agents().await;

    match agents_result {
        Ok(agents_json) => {
            if json {
                let output = serde_json::json!({
                    "status": "running",
                    "agents": agents_json.get("agents").cloned().unwrap_or(serde_json::json!([])),
                    "security": serde_json::to_value(apollia_core::SecurityPosture::detect())
                        .unwrap_or(serde_json::Value::Null),
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                );
            } else {
                format_text_status(&agents_json);
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_connection_error(e, json),
    }
}

/// Format status output as human-readable text.
///
/// Health was already verified by the caller; reaching this function
/// means the runtime is active and reachable.
fn format_text_status(agents_json: &serde_json::Value) {
    let agents = agents_json
        .get("agents")
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default();

    let active_count = agents
        .iter()
        .filter(|a| a.get("state").and_then(|s| s.as_str()) == Some("active"))
        .count();

    println!("  Runtime  ACTIVE");
    note!();
    println!("  AGENTS ({active_count} active)");
    println!("  {:<30} {:<12}", "NAME", "STATE");

    if agents.is_empty() {
        println!("  (no agents registered)");
    } else {
        for agent in &agents {
            // Prefer the manifest name; fall back to UUID only when the
            // registry response is missing it (older runtimes or unwired agents).
            let label = agent
                .get("name")
                .and_then(|v| v.as_str())
                .or_else(|| agent.get("agent_id").and_then(|v| v.as_str()))
                .unwrap_or("?");
            let state = agent.get("state").and_then(|v| v.as_str()).unwrap_or("?");
            let marker = match state {
                "active" => "*",
                "degraded" => "!",
                _ => " ",
            };
            println!("  {:<30} {marker} {state}", label);
        }
    }
}

/// Handle connection errors uniformly.
fn handle_connection_error(err: ClientError, json: bool) -> i32 {
    match err {
        ClientError::ConnectionRefused => crate::output::emit_error(
            json,
            exit_codes::RUNTIME_ERROR,
            "runtime not started (connection refused)",
        ),
        other => crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &other.to_string()),
    }
}
