//! `apollia-os trigger` subcommands — trigger management.
//!
//! Fournit la commande `reload` pour recharger la section `[[triggers]]`
//! depuis `apollia.toml` sans arrêter le runtime (STORY-073).

use std::path::PathBuf;

use clap::Subcommand;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

/// Trigger subcommands : `apollia-os trigger <verb>`.
#[derive(Debug, Subcommand)]
pub enum TriggerCommand {
    /// Reload triggers from apollia.toml without stopping the runtime.
    ///
    /// Rereads `[[triggers]]` from `apollia.toml`, validates the new definitions,
    /// and restarts modified sources. Invalid TOML or invalid trigger configuration
    /// returns an error without interrupting the currently-running triggers.
    Reload,
}

/// Execute a `trigger` subcommand.
///
/// Returns the process exit code.
pub async fn run(cmd: &TriggerCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let client = RuntimeClient::new(socket_path);

    match cmd {
        TriggerCommand::Reload => run_reload(&client, json).await,
    }
}

/// `apollia-os trigger reload` — hot reload des triggers depuis `apollia.toml`.
async fn run_reload(client: &RuntimeClient, json: bool) -> i32 {
    match client.reload_triggers().await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                let count = resp.get("reloaded").and_then(|v| v.as_u64()).unwrap_or(0);
                println!("✔ Triggers rechargés — {count} actif(s)");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status, body }) => {
            if json {
                let output = serde_json::json!({ "error": body, "status": status });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                );
            } else {
                eprintln!("Erreur reload triggers ({status}): {body}");
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// Gestion uniforme des erreurs client.
fn handle_client_error(err: ClientError, json: bool) -> i32 {
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

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use clap::Parser;

    /// Helper CLI minimal pour tester le parsing.
    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(subcommand)]
        command: super::TriggerCommand,
    }

    #[test]
    fn test_trigger_reload_parses() {
        // GIVEN "reload"
        let cli = TestCli::parse_from(["apollia-os", "reload"]);
        // THEN TriggerCommand::Reload
        assert!(matches!(cli.command, super::TriggerCommand::Reload));
    }
}
