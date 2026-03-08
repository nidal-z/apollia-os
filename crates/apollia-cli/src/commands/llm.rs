//! `apollia-os llm` subcommands — diagnose and test LLM backends via the runtime API.
//!
//! Provides `status`, `ping`, and `chat` operations for LLM backend management.

use std::path::PathBuf;

use clap::Subcommand;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

/// LLM subcommands: `apollia-os llm <verb>`.
#[derive(Debug, Subcommand)]
pub enum LlmCommand {
    /// Display the status of all configured LLM backends.
    Status,
    /// Measure the latency of a specific LLM backend.
    Ping {
        /// Backend name (default: the router's configured default backend).
        backend: Option<String>,
    },
    /// Send a direct prompt to an LLM backend and print the response.
    Chat {
        /// The prompt text to send to the LLM.
        prompt: String,
        /// Backend to use (optional — uses the configured default if omitted).
        #[arg(long)]
        backend: Option<String>,
    },
}

/// Execute a `llm` subcommand.
///
/// Returns the process exit code: `0` = success, non-zero = error.
pub async fn run(cmd: &LlmCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let client = RuntimeClient::new(socket_path);

    match cmd {
        LlmCommand::Status => run_status(&client, json).await,
        LlmCommand::Ping { backend } => run_ping(&client, backend.as_deref(), json).await,
        LlmCommand::Chat { prompt, backend } => {
            run_chat(&client, prompt, backend.as_deref(), json).await
        }
    }
}

/// `apollia-os llm status` — display all LLM backends with their current state.
async fn run_status(client: &RuntimeClient, json: bool) -> i32 {
    let resp = match client.get("/api/v1/llm/status").await {
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
        format_llm_status(&parsed);
    }
    exit_codes::SUCCESS
}

/// `apollia-os llm ping [backend]` — measure the latency of a backend.
///
/// Returns exit code `0` if the backend is available, `1` otherwise (AC-4).
async fn run_ping(client: &RuntimeClient, backend: Option<&str>, json: bool) -> i32 {
    let body = serde_json::json!({ "backend": backend });
    let resp = match client.post("/api/v1/llm/ping", Some(&body)).await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
    };

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
        format_ping_result(&parsed);
    }

    let available = parsed
        .get("available")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if available {
        exit_codes::SUCCESS
    } else {
        exit_codes::GENERAL_ERROR
    }
}

/// `apollia-os llm chat "prompt"` — send a prompt to an LLM backend.
async fn run_chat(client: &RuntimeClient, prompt: &str, backend: Option<&str>, json: bool) -> i32 {
    let body = serde_json::json!({ "prompt": prompt, "backend": backend });
    let resp = match client.post("/api/v1/llm/chat", Some(&body)).await {
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
        let content = parsed.get("content").and_then(|v| v.as_str()).unwrap_or("");
        println!("{content}");
    }
    exit_codes::SUCCESS
}

// ─────────────────────────────────────────────
// Formatting helpers
// ─────────────────────────────────────────────

/// Render `GET /api/v1/llm/status` response as a human-readable table.
fn format_llm_status(resp: &serde_json::Value) {
    let backends = resp
        .get("backends")
        .and_then(|b| b.as_array())
        .cloned()
        .unwrap_or_default();

    println!("  {:<24} {:<32} STATUS", "BACKEND", "MODEL");
    if backends.is_empty() {
        println!("  (no LLM backends configured)");
    } else {
        for b in &backends {
            let name = b.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let model = b.get("model_id").and_then(|v| v.as_str()).unwrap_or("?");
            let available = b
                .get("available")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let status = if available { "ready" } else { "unavailable" };
            println!("  {name:<24} {model:<32} {status}");
        }
    }
}

/// Render `POST /api/v1/llm/ping` response as a human-readable line.
fn format_ping_result(resp: &serde_json::Value) {
    let backend = resp.get("backend").and_then(|v| v.as_str()).unwrap_or("?");
    let available = resp
        .get("available")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if available {
        let latency = resp.get("latency_ms").and_then(|v| v.as_u64()).unwrap_or(0);
        println!("{backend}: OK ({latency}ms)");
    } else {
        let error = resp
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        println!("{backend}: UNAVAILABLE ({error})");
    }
}

// ─────────────────────────────────────────────
// Error helpers
// ─────────────────────────────────────────────

/// Handle client-level errors uniformly.
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

/// Handle HTTP server errors uniformly.
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
