//! `apollia-os stop`: send shutdown signal to a running runtime.
//!
//! Connects via Unix socket and sends `POST /api/v1/shutdown`.

use std::path::PathBuf;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

/// Execute the `stop` command.
///
/// Returns the process exit code.
pub async fn run(socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let client = RuntimeClient::new(socket_path);

    match client.shutdown().await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("Runtime stopping...");
                println!("Runtime stopped.");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ConnectionRefused) => {
            if json {
                let err = serde_json::json!({"error": "runtime not started (connection refused)"});
                println!("{}", serde_json::to_string_pretty(&err).unwrap_or_default());
            } else {
                eprintln!("Error: runtime not started (connection refused)");
            }
            exit_codes::RUNTIME_ERROR
        }
        Err(e) => {
            if json {
                let err = serde_json::json!({"error": e.to_string()});
                println!("{}", serde_json::to_string_pretty(&err).unwrap_or_default());
            } else {
                eprintln!("Error: {e}");
            }
            exit_codes::GENERAL_ERROR
        }
    }
}
