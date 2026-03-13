//! `apollia-os agent` subcommands — manage agents via the runtime API.
//!
//! Provides `list`, `start`, `stop`, and `info` operations on agents.

use std::path::PathBuf;

use clap::Subcommand;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

/// Agent subcommands: `apollia-os agent <verb>`.
#[derive(Debug, Subcommand)]
pub enum AgentCommand {
    /// List all registered agents.
    List,
    /// Start (register) a new agent from a Python module path.
    Start {
        /// Path to the agent Python module.
        path: String,
    },
    /// Stop (shutdown) a running agent.
    Stop {
        /// Agent identifier.
        agent_id: String,
    },
    /// Display detailed information about an agent.
    Info {
        /// Agent identifier.
        agent_id: String,
    },
}

/// Execute an `agent` subcommand.
///
/// Returns the process exit code.
pub async fn run(cmd: &AgentCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let client = RuntimeClient::new(socket_path);

    match cmd {
        AgentCommand::List => run_list(&client, json).await,
        AgentCommand::Start { path } => run_start(&client, path, json).await,
        AgentCommand::Stop { agent_id } => run_stop(&client, agent_id, json).await,
        AgentCommand::Info { agent_id } => run_info(&client, agent_id, json).await,
    }
}

/// `apollia-os agent list` — display all agents.
async fn run_list(client: &RuntimeClient, json: bool) -> i32 {
    match client.list_agents().await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                format_agent_list(&resp);
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os agent start <path>` — register a new agent.
async fn run_start(client: &RuntimeClient, path: &str, json: bool) -> i32 {
    match client.start_agent(path).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                let agent_id = resp.get("agent_id").and_then(|v| v.as_str()).unwrap_or("?");
                let state = resp.get("state").and_then(|v| v.as_str()).unwrap_or("?");
                println!("Agent {agent_id} started ({state})");
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// Return true if `arg` looks like a file path rather than an agent name or UUID.
///
/// Detects the common mistake of passing a Python module path (e.g. `agents/foo.py`)
/// to commands that expect a name or UUID (e.g. `apollia-reviewer`).
fn looks_like_file_path(arg: &str) -> bool {
    arg.contains('/') || arg.contains('\\') || arg.ends_with(".py")
}

/// `apollia-os agent stop <id>` — stop a running agent.
async fn run_stop(client: &RuntimeClient, agent_id: &str, json: bool) -> i32 {
    if looks_like_file_path(agent_id) {
        let msg = format!(
            "'{agent_id}' looks like a file path — use the agent name or UUID instead\n\
             Hint: apollia-os agent stop <name|uuid>  (e.g. apollia-os agent stop apollia-reviewer)"
        );
        if json {
            println!("{}", serde_json::json!({"error": msg}));
        } else {
            eprintln!("Error: {msg}");
        }
        return exit_codes::GENERAL_ERROR;
    }
    match client.stop_agent(agent_id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("Agent {agent_id} stopped");
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os agent info <id>` — display agent detail.
async fn run_info(client: &RuntimeClient, agent_id: &str, json: bool) -> i32 {
    if looks_like_file_path(agent_id) {
        let msg = format!(
            "'{agent_id}' looks like a file path — use the agent name or UUID instead\n\
             Hint: apollia-os agent info <name|uuid>  (e.g. apollia-os agent info apollia-reviewer)"
        );
        if json {
            println!("{}", serde_json::json!({"error": msg}));
        } else {
            eprintln!("Error: {msg}");
        }
        return exit_codes::GENERAL_ERROR;
    }
    match client.get_agent(agent_id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                format_agent_detail(&resp);
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// Format agent list as a human-readable table.
fn format_agent_list(resp: &serde_json::Value) {
    let agents = resp
        .get("agents")
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default();

    println!("  {:<32} {:<14} {}", "NOM", "STATE", "AGENT_ID");

    if agents.is_empty() {
        println!("  (no agents registered)");
    } else {
        for agent in &agents {
            let id = agent
                .get("agent_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let name = agent.get("name").and_then(|v| v.as_str()).unwrap_or(id);
            let state = agent.get("state").and_then(|v| v.as_str()).unwrap_or("?");
            println!("  {:<32} {:<14} {}", name, state, id);
        }
    }
}

/// Format agent detail as human-readable text.
fn format_agent_detail(resp: &serde_json::Value) {
    let agent_id = resp.get("agent_id").and_then(|v| v.as_str()).unwrap_or("?");
    let state = resp.get("state").and_then(|v| v.as_str()).unwrap_or("?");

    println!("  Agent     : {agent_id}");
    println!("  State     : {state}");

    if let Some(manifest) = resp.get("manifest") {
        let name = manifest.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let version = manifest
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let desc = manifest
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        println!("  Name      : {name}");
        println!("  Version   : {version}");
        if !desc.is_empty() {
            println!("  Desc      : {desc}");
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
