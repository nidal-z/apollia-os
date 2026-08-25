//! `apollia-os resilience` subcommands: circuit breaker inspection and management.
//!
//! Connects to the runtime via Unix socket and queries the shared
//! `ResilienceLayer` snapshot. The layer is hydrated by a `ToolCallCompleted`
//! / `ToolCallDenied` event subscriber (see
//! [`apollia_runtime::observability::spawn_resilience_subscriber`]) so the
//! snapshot reflects actual production traffic without the engines having to
//! synchronously update a global mutex.

use std::path::PathBuf;

use clap::Subcommand;

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;

/// Resilience subcommands: `apollia-os resilience <verb>`.
#[derive(Debug, Subcommand)]
pub enum ResilienceCommand {
    /// List all registered circuit breakers with their state and counters.
    List,
    /// Show the state of a single circuit breaker.
    Show {
        /// Tool name as registered in the Tool Registry.
        tool_name: String,
    },
    /// Reset a circuit breaker to CLOSED state immediately.
    Reset {
        /// Tool name to reset.
        tool_name: String,
    },
}

/// Execute a `resilience` subcommand. Returns the process exit code.
pub async fn run(cmd: &ResilienceCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(default_socket_path);
    let client = RuntimeClient::new(socket_path);

    match cmd {
        ResilienceCommand::List => run_list(&client, json).await,
        ResilienceCommand::Show { tool_name } => run_show(&client, tool_name, json).await,
        ResilienceCommand::Reset { tool_name } => run_reset(&client, tool_name, json).await,
    }
}

/// `apollia-os resilience list`: display all circuit breakers.
async fn run_list(client: &RuntimeClient, json: bool) -> i32 {
    let resp = match client.resilience_list().await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&resp).unwrap_or_default()
        );
        return exit_codes::SUCCESS;
    }

    let items = resp.get("circuit_breakers").and_then(|v| v.as_array());
    match items.map(Vec::as_slice) {
        None | Some([]) => println!("No circuit breakers registered."),
        Some(list) => {
            println!(
                "  {:<30} {:<10} {:<8} COOLDOWN",
                "TOOL", "STATE", "FAILURES"
            );
            for item in list {
                let name = item
                    .get("tool_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let state = item.get("state").and_then(|v| v.as_str()).unwrap_or("?");
                let failures = item
                    .get("failure_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let cooldown = item
                    .get("cooldown_remaining_secs")
                    .and_then(|v| v.as_u64())
                    .map(|s| format!("{s}s"))
                    .unwrap_or_else(|| "-".to_string());
                println!("  {:<30} {:<10} {:<8} {}", name, state, failures, cooldown);
            }
        }
    }
    exit_codes::SUCCESS
}

/// `apollia-os resilience show <tool>`: display a single circuit breaker.
async fn run_show(client: &RuntimeClient, tool_name: &str, json: bool) -> i32 {
    match client.resilience_show(tool_name).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                let state = resp.get("state").and_then(|v| v.as_str()).unwrap_or("?");
                let failures = resp
                    .get("failure_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let cooldown = resp
                    .get("cooldown_remaining_secs")
                    .and_then(|v| v.as_u64())
                    .map(|s| format!("{s}s"))
                    .unwrap_or_else(|| "0s".to_string());
                println!("Tool          : {tool_name}");
                println!("State         : {state}");
                println!("Failures      : {failures}");
                println!("Cooldown left : {cooldown}");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, .. }) => print_error_and_exit(
            &format!("no circuit breaker recorded for tool '{tool_name}' (it has not run, or failed no calls yet)"),
            json,
        ),
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os resilience reset <tool>`: reset a circuit breaker to CLOSED.
async fn run_reset(client: &RuntimeClient, tool_name: &str, json: bool) -> i32 {
    match client.resilience_reset(tool_name).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("Circuit breaker for '{tool_name}' reset to CLOSED");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, .. }) => print_error_and_exit(
            &format!("no circuit breaker recorded for tool '{tool_name}' (it has not run, or failed no calls yet)"),
            json,
        ),
        Err(e) => handle_error(e, json),
    }
}

fn print_error_and_exit(msg: &str, json: bool) -> i32 {
    crate::output::emit_error(json, exit_codes::GENERAL_ERROR, msg)
}

fn handle_error(err: ClientError, json: bool) -> i32 {
    match err {
        ClientError::ConnectionRefused => crate::output::emit_error(
            json,
            exit_codes::RUNTIME_ERROR,
            "runtime not started (connection refused)",
        ),
        // A 404 here means the tool name was never seen by the shared layer
        // (no call attempted yet), distinguish it from the runtime being off.
        ClientError::ServerError { status: 404, body } => {
            let msg = if body.is_empty() {
                "tool not registered with a circuit breaker yet".to_string()
            } else {
                body
            };
            crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &msg)
        }
        other => crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: ResilienceCommand,
    }

    #[test]
    fn test_resilience_list_parses() {
        // GIVEN "list"
        // WHEN parsed
        let cli = TestCli::parse_from(["test", "list"]);
        // THEN ResilienceCommand::List
        assert!(matches!(cli.cmd, ResilienceCommand::List));
    }

    #[test]
    fn test_resilience_show_parses() {
        // GIVEN "show mcp_erp"
        // WHEN parsed
        let cli = TestCli::parse_from(["test", "show", "mcp_erp"]);
        // THEN ResilienceCommand::Show with correct tool_name
        match cli.cmd {
            ResilienceCommand::Show { tool_name } => assert_eq!(tool_name, "mcp_erp"),
            other => panic!("expected Show, got {other:?}"),
        }
    }

    #[test]
    fn test_resilience_reset_parses() {
        // GIVEN "reset mcp_erp"
        // WHEN parsed
        let cli = TestCli::parse_from(["test", "reset", "mcp_erp"]);
        // THEN ResilienceCommand::Reset with correct tool_name
        match cli.cmd {
            ResilienceCommand::Reset { tool_name } => assert_eq!(tool_name, "mcp_erp"),
            other => panic!("expected Reset, got {other:?}"),
        }
    }

    #[test]
    fn test_list_json_output_contains_expected_fields() {
        // GIVEN a circuit_breakers response payload with one OPEN circuit
        let resp = serde_json::json!({
            "circuit_breakers": [{
                "tool_name": "mcp_erp",
                "state": "OPEN",
                "failure_count": 3,
                "cooldown_remaining_secs": 15
            }]
        });
        // WHEN serializing to JSON
        let output = serde_json::to_string_pretty(&resp).unwrap();
        // THEN output contains expected fields
        assert!(output.contains("mcp_erp"));
        assert!(output.contains("OPEN"));
        assert!(output.contains("failure_count"));
        assert!(output.contains("cooldown_remaining_secs"));
    }

    #[test]
    fn test_reset_unknown_tool_returns_general_error() {
        // GIVEN an unknown tool name
        // WHEN print_error_and_exit is called
        let code = print_error_and_exit("Tool 'unknown_tool' not found in Tool Registry", false);
        // THEN exit code is GENERAL_ERROR
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn test_reset_unknown_tool_json_output() {
        // GIVEN an unknown tool name and json=true
        // WHEN print_error_and_exit is called
        let code = print_error_and_exit("Tool 'unknown_tool' not found in Tool Registry", true);
        // THEN exit code is GENERAL_ERROR
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[tokio::test]
    async fn test_resilience_list_connection_refused() {
        // GIVEN a client pointing to a nonexistent socket
        let client = RuntimeClient::new(PathBuf::from("/tmp/apollia-test-nonexistent-9999.sock"));
        // WHEN run_list is called
        let code = run_list(&client, false).await;
        // THEN returns RUNTIME_ERROR
        assert_eq!(code, exit_codes::RUNTIME_ERROR);
    }

    #[tokio::test]
    async fn test_resilience_reset_connection_refused() {
        // GIVEN a client pointing to a nonexistent socket
        let client = RuntimeClient::new(PathBuf::from("/tmp/apollia-test-nonexistent-9999.sock"));
        // WHEN run_reset is called with an unknown tool
        let code = run_reset(&client, "unknown_tool", false).await;
        // THEN returns RUNTIME_ERROR (runtime not running)
        assert_eq!(code, exit_codes::RUNTIME_ERROR);
    }
}
