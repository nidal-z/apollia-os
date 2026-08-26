//! Interactive chat REPL command.
//!
//! Provides an interactive terminal session for chatting with an LLM through
//! the Apollia runtime. Supports creating new sessions, resuming previous ones,
//! and listing recent sessions.
//!
//! Command history is persisted across sessions at `~/.apollia/repl_history`
//! (up to 10 000 entries, FIFO rotation). Emacs keybindings and Ctrl-R reverse
//! search are provided by `rustyline`.
//!
//! Slash commands:
//! - Built-in: `/fork`, `/fork N`, `/fork list`, `/list-commands`
//! - Custom: any `.md` file in `.apollia/commands/` or `~/.apollia/commands/`

use std::path::PathBuf;

use clap::Subcommand;
use rustyline::Config as RlConfig;

use crate::client::{default_socket_path, RuntimeClient};

mod session;
mod slash;

pub use hygiene::run_hygiene;
mod hygiene;

use session::{run_list, run_new_session, run_resume};

/// Subcommands of `apollia-os chat` for persisted session hygiene.
///
/// All variants operate directly on `~/.apollia/chat.db` via
/// [`ChatSessionRepository`], no runtime required. When the daemon is
/// running, SQLite WAL handles concurrent reads safely; a delete/rename of
/// an actively-served session is *not* atomic with a pending message round
/// trip, document this in the `--help`.
#[derive(Debug, Subcommand, Clone)]
pub enum ChatHygieneCommand {
    /// Delete a persisted chat session and all of its messages.
    Delete {
        /// Session id (8+ char ulid-like string returned by `chat --list`).
        session_id: String,
        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        confirm: bool,
        /// Override the chat database path (default: `~/.apollia/chat.db`).
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },

    /// Set the user-defined title of a persisted chat session.
    Rename {
        /// Session id.
        session_id: String,
        /// New title (max 100 chars, leading/trailing whitespace trimmed).
        title: String,
        /// Override the chat database path.
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },

    /// Export a persisted chat session to a file.
    Export {
        /// Session id.
        session_id: String,
        /// Output file path. Defaults to stdout when omitted.
        #[arg(long, value_name = "PATH")]
        output: Option<PathBuf>,
        /// Output format: `markdown` (default) or `json`.
        #[arg(long, value_name = "FORMAT", default_value = "markdown",
              value_parser = ["markdown", "json"])]
        format: String,
        /// Override the chat database path.
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },

    /// Manage the Chat Libre configuration (system prompt, allowed tools, backend).
    Config {
        /// Chat-config subcommand.
        #[command(subcommand)]
        command: super::chat_config::ChatConfigCommand,
    },
}

/// Maximum number of history entries retained across sessions.
const REPL_MAX_HISTORY: usize = 10_000;

/// Maximum number of scroll-back messages to show on resume.
const SCROLLBACK_COUNT: usize = 5;

/// Build a [`RuntimeClient`] from the optional socket path override.
fn make_client(socket: Option<PathBuf>) -> RuntimeClient {
    match socket {
        Some(p) => RuntimeClient::new(p),
        None => RuntimeClient::new(default_socket_path()),
    }
}

/// Resolve the path to the persistent REPL history file.
///
/// Returns `None` if the home directory cannot be determined or the
/// `~/.apollia/` directory cannot be created. In that case history
/// is kept in memory only for the current session.
fn history_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let dir = apollia_core::paths::data_dir_under(home);
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("repl_history"))
}

/// Build a rustyline config with FIFO rotation at [`REPL_MAX_HISTORY`] entries.
///
/// Falls back to a plain default config if the max-history-size API rejects
/// the value (which cannot happen for 10 000, but is handled defensively).
fn make_editor_config() -> RlConfig {
    let builder = RlConfig::builder().history_ignore_space(true);
    match builder.max_history_size(REPL_MAX_HISTORY) {
        Ok(b) => b.build(),
        Err(_) => RlConfig::default(),
    }
}

/// Run the `chat` command.
///
/// - `resume`: optional session ID to resume.
/// - `list`: if `true`, print the 10 most recent sessions and exit.
/// - `socket`: optional Unix socket path override.
/// - `json`: emit raw SSE frames instead of human rendering during streaming.
/// - `no_color`: disable ANSI styling in the streamed output.
pub async fn run(
    resume: Option<&str>,
    list: bool,
    socket: Option<PathBuf>,
    json: bool,
    no_color: bool,
) -> i32 {
    let client = make_client(socket);

    if list {
        return run_list(&client, json).await;
    }

    if let Some(session_id) = resume {
        return run_resume(&client, session_id, json, no_color).await;
    }

    run_new_session(&client, json, no_color).await
}
#[cfg(test)]
mod hygiene_tests {
    use super::hygiene::{resolve_chat_db, run_chat_delete, run_chat_rename};
    use super::*;
    use crate::exit_codes;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(subcommand)]
        cmd: ChatHygieneCommand,
    }

    #[test]
    fn parses_delete_requires_confirm_flag() {
        let cli = TestCli::parse_from(["x", "delete", "sess-abc"]);
        match cli.cmd {
            ChatHygieneCommand::Delete {
                session_id,
                confirm,
                ..
            } => {
                assert_eq!(session_id, "sess-abc");
                assert!(!confirm);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_delete_with_confirm() {
        let cli = TestCli::parse_from(["x", "delete", "sess-abc", "--confirm"]);
        match cli.cmd {
            ChatHygieneCommand::Delete { confirm, .. } => assert!(confirm),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_rename() {
        let cli = TestCli::parse_from(["x", "rename", "sess-abc", "Hello world"]);
        match cli.cmd {
            ChatHygieneCommand::Rename {
                session_id, title, ..
            } => {
                assert_eq!(session_id, "sess-abc");
                assert_eq!(title, "Hello world");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_export_default_format_is_markdown() {
        let cli = TestCli::parse_from(["x", "export", "sess-abc", "--output", "/tmp/out.md"]);
        match cli.cmd {
            ChatHygieneCommand::Export {
                session_id,
                output,
                format,
                ..
            } => {
                assert_eq!(session_id, "sess-abc");
                assert_eq!(output, Some(PathBuf::from("/tmp/out.md")));
                assert_eq!(format, "markdown");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_export_json_format() {
        let cli = TestCli::parse_from(["x", "export", "sess-abc", "--format", "json"]);
        match cli.cmd {
            ChatHygieneCommand::Export { format, .. } => assert_eq!(format, "json"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn export_rejects_invalid_format() {
        let result = TestCli::try_parse_from(["x", "export", "s", "--format", "xml"]);
        assert!(result.is_err(), "xml is not a valid format");
    }

    #[test]
    fn delete_without_confirm_returns_error() {
        let code = run_chat_delete("sess-abc", false, None, true);
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn rename_rejects_empty_title() {
        let code = run_chat_rename("sess-abc", "   ", None, true);
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn resolve_chat_db_honours_override() {
        let p = PathBuf::from("/tmp/custom-chat.db");
        assert_eq!(resolve_chat_db(Some(&p)), p);
    }

    #[test]
    fn resolve_chat_db_default_ends_with_canonical_filename() {
        let p = resolve_chat_db(None);
        assert!(
            p.to_string_lossy().ends_with(".apollia/chat.db"),
            "unexpected default path: {p:?}"
        );
    }
}
