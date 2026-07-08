//! `apollia-os explain "<command or error>"`: explain in plain language.
//!
//! Runs a one-shot local completion. Read-only: never executes anything.

use std::path::PathBuf;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

const SYSTEM: &str = "You are Apollia's terminal assistant. Explain the given \
Apollia CLI command or error message in clear, plain language: what it does or \
what went wrong, and how to proceed. Be concise. Do not invent commands or flags.";

/// Explain `text` using the local model. Returns the process exit code.
pub async fn run(text: &str, socket: Option<PathBuf>, json: bool) -> i32 {
    if text.trim().is_empty() {
        eprintln!("nothing to explain: provide a command or error message");
        return exit_codes::GENERAL_ERROR;
    }
    let client = RuntimeClient::new(socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH)));
    match client.llm_complete(Some(SYSTEM), text, None).await {
        Ok(resp) => {
            if json {
                println!("{resp}");
            } else {
                eprintln!("(via backend: {})", resp["backend"].as_str().unwrap_or("?"));
                println!("{}", resp["content"].as_str().unwrap_or(""));
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ConnectionRefused) => {
            eprintln!("runtime not started");
            exit_codes::GENERAL_ERROR
        }
        Err(e) => {
            eprintln!("Error: {e}");
            exit_codes::GENERAL_ERROR
        }
    }
}
