//! `apollia-os tools` subcommands — query registered tools via the runtime API.
//!
//! Provides `list` and `describe` operations on the tool registry.

use std::path::PathBuf;

use clap::Subcommand;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

/// Tools subcommands: `apollia-os tools <verb>`.
#[derive(Debug, Subcommand)]
pub enum ToolsCommand {
    /// List all registered tools.
    List,
    /// Describe a specific tool.
    Describe {
        /// Tool name.
        tool_name: String,
    },
}

/// Execute a `tools` subcommand.
///
/// Returns the process exit code.
pub async fn run(cmd: &ToolsCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let client = RuntimeClient::new(socket_path);

    match cmd {
        ToolsCommand::List => run_list(&client, json).await,
        ToolsCommand::Describe { tool_name } => run_describe(&client, tool_name, json).await,
    }
}

/// `apollia-os tools list` — display all registered tools.
async fn run_list(client: &RuntimeClient, json: bool) -> i32 {
    let resp = match client.get("/api/v1/tools").await {
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
        format_tools_list(&parsed);
    }
    exit_codes::SUCCESS
}

/// `apollia-os tools describe <name>` — display tool detail.
async fn run_describe(client: &RuntimeClient, tool_name: &str, json: bool) -> i32 {
    let resp = match client.get(&format!("/api/v1/tools/{tool_name}")).await {
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
        format_tool_detail(&parsed);
    }
    exit_codes::SUCCESS
}

/// Format tools list as a human-readable table.
fn format_tools_list(resp: &serde_json::Value) {
    let tools = resp
        .get("tools")
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();

    println!("  {:<24} {:<10}", "NAME", "KIND");

    if tools.is_empty() {
        println!("  (no tools registered)");
    } else {
        for tool in &tools {
            let name = tool.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let kind = tool_kind_label(tool.get("kind"));
            println!("  {:<24} {kind}", name);
        }
    }
}

/// Extract a human-readable label from a [`ToolKind`] JSON value.
///
/// `ToolKind` is serialized as an adjacently-tagged object `{"type": "native"}`,
/// not a plain string. This function reads the `"type"` field of that object.
fn tool_kind_label(kind: Option<&serde_json::Value>) -> &str {
    kind.and_then(|v| v.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("?")
}

/// Format tool detail as human-readable text.
fn format_tool_detail(resp: &serde_json::Value) {
    let name = resp.get("name").and_then(|v| v.as_str()).unwrap_or("?");
    let kind = tool_kind_label(resp.get("kind"));
    let desc = resp
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    println!("  Name      : {name}");
    println!("  Kind      : {kind}");
    if !desc.is_empty() {
        println!("  Desc      : {desc}");
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
