//! `apollia-os trigger` subcommands: trigger management.
//!
//! Provides the `list`, `status`, `fire`, `enable`, `disable`, `logs`, and
//! `reload` subcommands to manage, debug, and audit automatic agent triggers
//! from the terminal without editing `apollia.toml`.
//!
//! Noun-verb pattern consistent with `agent` and `task`.

use std::path::PathBuf;

use clap::Subcommand;

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;

mod display;
mod mutate;
mod query;

use mutate::{run_create, run_delete, run_update, CreateArgs, UpdateArgs};
use query::{run_disable, run_enable, run_fire, run_list, run_logs, run_reload, run_status};

// ─── Subcommands ──────────────────────────────────────────────────────────

/// Trigger subcommands: `apollia-os trigger <verb>`.
#[derive(Debug, Subcommand)]
pub enum TriggerCommand {
    /// List all triggers with their status.
    List,
    /// Show the detailed status of a trigger.
    Status {
        /// Trigger identifier.
        id: String,
    },
    /// Fire a trigger immediately (debug/test).
    Fire {
        /// Trigger identifier.
        id: String,
    },
    /// Enable a disabled trigger.
    Enable {
        /// Trigger identifier.
        id: String,
    },
    /// Disable a trigger without editing apollia.toml.
    Disable {
        /// Trigger identifier.
        id: String,
    },
    /// Show the firing history from SQLite.
    Logs {
        /// Trigger identifier.
        id: String,
        /// Maximum number of entries to display.
        #[arg(long, default_value = "20")]
        last: usize,
    },
    /// Reload trigger config from apollia.toml (hot reload).
    ///
    /// Rereads `[[triggers]]` from `apollia.toml`, validates the new definitions,
    /// and restarts modified sources. Invalid TOML or invalid trigger configuration
    /// returns an error without interrupting the currently-running triggers.
    Reload,
    /// Create a new trigger (CRUD, complements hot-reload via apollia.toml).
    Create {
        /// Unique trigger identifier.
        id: String,
        /// Target agent.
        #[arg(long)]
        agent: String,
        /// Source type: cron, interval, oneshot, filewatch, webhook.
        #[arg(long, value_name = "TYPE")]
        kind: String,
        /// Source-specific detail:
        ///   cron      → cron expression (e.g. `"0 9 * * 1"`)
        ///   interval  → duration string (`30m`, `1h`, `6h`, `1d`)
        ///   oneshot   → RFC 3339 timestamp
        ///   filewatch → path to a file or directory
        ///   webhook   → shared HMAC-SHA256 secret of at least 32 chars
        #[arg(long)]
        detail: Option<String>,
        /// Policy when the agent is busy when a fire arrives.
        /// `queue` enqueues the fire (default), `drop` discards it.
        #[arg(long, value_parser = ["queue", "drop"], default_value = "queue")]
        on_busy: String,
        /// Input template sent to the agent when fired.
        #[arg(long)]
        input: Option<String>,
    },
    /// Update an existing trigger.
    Update {
        /// Trigger identifier.
        id: String,
        /// New source detail (kind is read from the existing definition).
        #[arg(long)]
        detail: Option<String>,
        /// New on-busy policy (`queue` or `drop`).
        #[arg(long, value_parser = ["queue", "drop"])]
        on_busy: Option<String>,
        /// New input template.
        #[arg(long)]
        input: Option<String>,
    },
    /// Delete a trigger.
    Delete {
        /// Trigger identifier.
        id: String,
        /// Confirm deletion without an interactive prompt.
        #[arg(long)]
        confirm: bool,
    },
}

// ─── Entry point ──────────────────────────────────────────────────────────

/// Execute a `trigger` subcommand.
///
/// Returns the process exit code (0 = success, 1 = error, 2 = runtime offline).
pub async fn run(cmd: &TriggerCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(default_socket_path);
    let client = RuntimeClient::new(socket_path);

    match cmd {
        TriggerCommand::List => run_list(&client, json).await,
        TriggerCommand::Status { id } => run_status(&client, id, json).await,
        TriggerCommand::Fire { id } => run_fire(&client, id, json).await,
        TriggerCommand::Enable { id } => run_enable(&client, id, json).await,
        TriggerCommand::Disable { id } => run_disable(&client, id, json).await,
        TriggerCommand::Logs { id, last } => run_logs(&client, id, *last, json).await,
        TriggerCommand::Reload => run_reload(&client, json).await,
        TriggerCommand::Create {
            id,
            agent,
            kind,
            detail,
            on_busy,
            input,
        } => {
            run_create(
                &client,
                CreateArgs {
                    id,
                    agent,
                    kind,
                    detail: detail.as_deref(),
                    on_busy,
                    input: input.as_deref(),
                },
                json,
            )
            .await
        }
        TriggerCommand::Update {
            id,
            detail,
            on_busy,
            input,
        } => {
            run_update(
                &client,
                UpdateArgs {
                    id,
                    detail: detail.as_deref(),
                    on_busy: on_busy.as_deref(),
                    input: input.as_deref(),
                },
                json,
            )
            .await
        }
        TriggerCommand::Delete { id, confirm } => run_delete(&client, id, *confirm, json).await,
    }
}

// ─── Error handling ───────────────────────────────────────────────────────

/// Uniform handling of client errors.
fn handle_client_error(err: ClientError, json: bool) -> i32 {
    match err {
        ClientError::ConnectionRefused => crate::output::emit_error(
            json,
            exit_codes::RUNTIME_ERROR,
            "runtime not started (connection refused)",
        ),
        other => crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &other.to_string()),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use clap::Parser;

    /// Minimal CLI to test parsing of the trigger subcommands.
    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(subcommand)]
        command: super::TriggerCommand,
    }

    // ── Parsing tests ──────────────────────────────────────────────────────

    #[test]
    fn test_trigger_list_parses() {
        // GIVEN "list"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "list"]);
        // THEN TriggerCommand::List
        assert!(matches!(cli.command, super::TriggerCommand::List));
    }

    #[test]
    fn test_trigger_status_parses() {
        // GIVEN "status rapport-hebdo"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "status", "rapport-hebdo"]);
        // THEN TriggerCommand::Status { id: "rapport-hebdo" }
        match &cli.command {
            super::TriggerCommand::Status { id } => assert_eq!(id, "rapport-hebdo"),
            other => panic!("expected Status, got {other:?}"),
        }
    }

    #[test]
    fn test_trigger_fire_parses() {
        // GIVEN "fire rapport-hebdo"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "fire", "rapport-hebdo"]);
        // THEN TriggerCommand::Fire { id: "rapport-hebdo" }
        match &cli.command {
            super::TriggerCommand::Fire { id } => assert_eq!(id, "rapport-hebdo"),
            other => panic!("expected Fire, got {other:?}"),
        }
    }

    #[test]
    fn test_trigger_enable_parses() {
        // GIVEN "enable rapport-hebdo"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "enable", "rapport-hebdo"]);
        // THEN TriggerCommand::Enable { id: "rapport-hebdo" }
        match &cli.command {
            super::TriggerCommand::Enable { id } => assert_eq!(id, "rapport-hebdo"),
            other => panic!("expected Enable, got {other:?}"),
        }
    }

    #[test]
    fn test_trigger_disable_parses() {
        // GIVEN "disable rapport-hebdo"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "disable", "rapport-hebdo"]);
        // THEN TriggerCommand::Disable { id: "rapport-hebdo" }
        match &cli.command {
            super::TriggerCommand::Disable { id } => assert_eq!(id, "rapport-hebdo"),
            other => panic!("expected Disable, got {other:?}"),
        }
    }

    #[test]
    fn test_trigger_logs_default_last_20() {
        // GIVEN "logs rapport-hebdo" (no --last flag)
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "logs", "rapport-hebdo"]);
        // THEN default last = 20
        match &cli.command {
            super::TriggerCommand::Logs { id, last } => {
                assert_eq!(id, "rapport-hebdo");
                assert_eq!(*last, 20);
            }
            other => panic!("expected Logs, got {other:?}"),
        }
    }

    #[test]
    fn test_trigger_logs_custom_last() {
        // GIVEN "logs rapport-hebdo --last 5"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "logs", "rapport-hebdo", "--last", "5"]);
        // THEN last = 5
        match &cli.command {
            super::TriggerCommand::Logs { last, .. } => assert_eq!(*last, 5),
            other => panic!("expected Logs, got {other:?}"),
        }
    }

    #[test]
    fn test_trigger_reload_parses() {
        // GIVEN "reload"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "reload"]);
        // THEN TriggerCommand::Reload
        assert!(matches!(cli.command, super::TriggerCommand::Reload));
    }

    #[test]
    fn test_trigger_create_parses() {
        // GIVEN "create rapport-hebdo --agent mon-agent --kind cron --detail '0 9 * * 1'"
        // WHEN
        let cli = TestCli::parse_from([
            "apollia-os",
            "create",
            "rapport-hebdo",
            "--agent",
            "mon-agent",
            "--kind",
            "cron",
            "--detail",
            "0 9 * * 1",
        ]);
        // THEN TriggerCommand::Create with the right fields
        match &cli.command {
            super::TriggerCommand::Create {
                id,
                agent,
                kind,
                detail,
                on_busy,
                input,
            } => {
                assert_eq!(id, "rapport-hebdo");
                assert_eq!(agent, "mon-agent");
                assert_eq!(kind, "cron");
                assert_eq!(detail.as_deref(), Some("0 9 * * 1"));
                // Default on_busy was changed from "skip" (CLI-only fiction)
                // to "queue" (runtime canonical value) in the v0.1.0 trigger
                // payload fix; see run_create.
                assert_eq!(on_busy, "queue");
                assert!(input.is_none());
            }
            other => panic!("expected Create, got {other:?}"),
        }
    }

    #[test]
    fn test_trigger_create_with_on_busy_parses() {
        // GIVEN "create t1 --agent a1 --kind interval --on-busy queue"
        // WHEN
        let cli = TestCli::parse_from([
            "apollia-os",
            "create",
            "t1",
            "--agent",
            "a1",
            "--kind",
            "interval",
            "--on-busy",
            "queue",
        ]);
        // THEN on_busy = "queue"
        match &cli.command {
            super::TriggerCommand::Create { on_busy, .. } => assert_eq!(on_busy, "queue"),
            other => panic!("expected Create, got {other:?}"),
        }
    }

    #[test]
    fn test_trigger_update_parses() {
        // GIVEN "update rapport-hebdo --detail '0 10 * * 1'"
        // WHEN
        let cli = TestCli::parse_from([
            "apollia-os",
            "update",
            "rapport-hebdo",
            "--detail",
            "0 10 * * 1",
        ]);
        // THEN Update { id: "rapport-hebdo", detail: Some("0 10 * * 1"), on_busy: None, input: None }
        match &cli.command {
            super::TriggerCommand::Update {
                id,
                detail,
                on_busy,
                input,
            } => {
                assert_eq!(id, "rapport-hebdo");
                assert_eq!(detail.as_deref(), Some("0 10 * * 1"));
                assert!(on_busy.is_none());
                assert!(input.is_none());
            }
            other => panic!("expected Update, got {other:?}"),
        }
    }

    #[test]
    fn test_trigger_delete_parses() {
        // GIVEN "delete rapport-hebdo --confirm"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "delete", "rapport-hebdo", "--confirm"]);
        // THEN Delete { id: "rapport-hebdo", confirm: true }
        match &cli.command {
            super::TriggerCommand::Delete { id, confirm } => {
                assert_eq!(id, "rapport-hebdo");
                assert!(confirm);
            }
            other => panic!("expected Delete, got {other:?}"),
        }
    }

    #[test]
    fn test_trigger_delete_without_confirm() {
        // GIVEN "delete rapport-hebdo" without --confirm
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "delete", "rapport-hebdo"]);
        // THEN confirm = false
        match &cli.command {
            super::TriggerCommand::Delete { confirm, .. } => assert!(!confirm),
            other => panic!("expected Delete, got {other:?}"),
        }
    }
}
