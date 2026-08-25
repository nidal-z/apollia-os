//! `apollia-os stop`: send shutdown signal to a running runtime.
//!
//! Connects via Unix socket and sends `POST /api/v1/shutdown`.

use std::path::PathBuf;

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;

/// Execute the `stop` command.
///
/// Returns the process exit code.
pub async fn run(socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(default_socket_path);
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
        Err(ClientError::ConnectionRefused) => crate::output::emit_error(
            json,
            exit_codes::RUNTIME_ERROR,
            "runtime not started (connection refused)",
        ),
        Err(e) => crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string()),
    }
}
