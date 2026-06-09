//! `apollia-os hooks` subcommands: inspect the lifecycle hook handlers.
//!
//! `hooks list` queries the running runtime (`GET /api/v1/hooks`). With
//! `--dry-run` it instead reads and validates the `[hooks]` section of
//! `apollia.toml` offline, without contacting the daemon.

use std::path::PathBuf;

use clap::Subcommand;

use apollia_runtime::{HookHandlerSummary, HookRegistry};

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

/// Hooks subcommands: `apollia-os hooks <verb>`.
#[derive(Debug, Subcommand)]
pub enum HooksCommand {
    /// List the registered lifecycle hook handlers.
    List {
        /// Read configuration from `apollia.toml` and validate it without
        /// connecting to the runtime.
        #[arg(long)]
        dry_run: bool,
    },
}

/// Execute a `hooks` subcommand. Returns the process exit code.
///
/// Machine-readable output is selected by the global `--json` flag.
pub async fn run(cmd: &HooksCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    match cmd {
        HooksCommand::List { dry_run } if *dry_run => dry_run_list(json),
        HooksCommand::List { .. } => {
            let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
            let client = RuntimeClient::new(socket_path);
            run_list(&client, json).await
        }
    }
}

/// `apollia-os hooks list`: fetch the active handlers from the runtime.
async fn run_list(client: &RuntimeClient, json: bool) -> i32 {
    let resp = match client.get("/api/v1/hooks").await {
        Ok(r) => r,
        Err(ClientError::ConnectionRefused) => {
            eprintln!("Error: runtime not started (connection refused)");
            return exit_codes::RUNTIME_ERROR;
        }
        Err(e) => {
            eprintln!("Error: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    if resp.status >= 400 {
        eprintln!("Error: HTTP {}: {}", resp.status, resp.body);
        return exit_codes::GENERAL_ERROR;
    }

    let summaries: Vec<HookHandlerSummary> = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: invalid JSON response: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };
    print_summaries(&summaries, json);
    exit_codes::SUCCESS
}

/// `apollia-os hooks list --dry-run`: validate and list from `apollia.toml`.
///
/// Exit 0 on success (including no config file), 1 when the config is invalid.
fn dry_run_list(json: bool) -> i32 {
    let summaries = match dry_run_summaries() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };
    print_summaries(&summaries, json);
    exit_codes::SUCCESS
}

/// Load, validate, and summarize the `[hooks]` section from `apollia.toml`.
///
/// Returns an empty list when no config file is found. Returns an error string
/// when the file cannot be parsed or the hooks section fails validation.
fn dry_run_summaries() -> Result<Vec<HookHandlerSummary>, String> {
    let Some(path) = crate::commands::start::find_config_file() else {
        return Ok(Vec::new());
    };
    let cfg = crate::config::parse_apollia_toml(&path).map_err(|e| e.to_string())?;
    let hooks = cfg.hooks.unwrap_or_default();
    hooks.validate().map_err(|e| e.to_string())?;
    Ok(HookRegistry::from_config(&hooks).list_all())
}

/// Render the handler summaries as JSON or a human table.
fn print_summaries(summaries: &[HookHandlerSummary], json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(summaries).unwrap_or_else(|_| "[]".to_string())
        );
        return;
    }
    if summaries.is_empty() {
        println!("(aucun hook configure)");
        return;
    }
    println!("#   Type      Evenements                     Timeout(ms)  Cible");
    for s in summaries {
        println!(
            "{:<3} {:<9} {:<30} {:<12} {}",
            s.id,
            s.r#type,
            s.events.join(","),
            s.timeout_ms,
            s.target,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{HookEventKind, HookHandlerConfig, HookHandlerKind, HooksConfig};

    #[test]
    fn test_dry_run_summaries_from_valid_config() {
        // GIVEN a valid hooks config with one command handler
        let cfg = HooksConfig {
            handlers: vec![HookHandlerConfig {
                events: vec![HookEventKind::PreToolUse],
                kind: HookHandlerKind::Command {
                    command: vec!["/usr/bin/hook".to_string()],
                },
                timeout_ms: 5_000,
            }],
        };

        // WHEN it is validated and summarized
        assert!(cfg.validate().is_ok());
        let summaries = HookRegistry::from_config(&cfg).list_all();

        // THEN one summary is produced
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].r#type, "command");
        assert_eq!(summaries[0].events, vec!["pre_tool_use"]);
    }

    #[test]
    fn test_dry_run_summaries_empty_config() {
        // GIVEN the default (empty) hooks config
        let cfg = HooksConfig::default();

        // WHEN summarized
        let summaries = HookRegistry::from_config(&cfg).list_all();

        // THEN the list is empty and validation passes
        assert!(summaries.is_empty());
        assert!(cfg.validate().is_ok());
    }
}
