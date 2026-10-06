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
mod capabilities;
mod hygiene;
mod settings;

pub use settings::{ChatNotesCommand, ChatSettingsCommand};

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
    /// List the persisted chat sessions, each with the folder it is in.
    ///
    /// Reads the chat database, no runtime required. A session the desktop
    /// application sent to its trash is not listed.
    List {
        /// Override the chat database path (default: `~/.apollia/chat.db`).
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },

    /// Delete a persisted chat session and all of its messages, for good.
    ///
    /// Nothing goes to the trash of the desktop application: a session
    /// deleted here cannot be restored. A session already in that trash is
    /// erased the same way.
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

    /// Show what the model of a chat session can do: the reasoning setting it
    /// offers (none, on or off, or a level) and whether it reads images.
    ///
    /// Reaches the runtime; exits 2 when it is not running.
    Capabilities {
        /// Session id.
        session_id: String,
    },

    /// Read or replace what one conversation changes from its agent: its
    /// request settings, its instructions, the accesses it switches off.
    Settings {
        /// Settings subcommand.
        #[command(subcommand)]
        command: ChatSettingsCommand,
    },

    /// Read or replace the notes kept beside one conversation.
    Notes {
        /// Notes subcommand.
        #[command(subcommand)]
        command: ChatNotesCommand,
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
    let home = apollia_core::paths::home_dir()?;
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
    use super::hygiene::{
        format_session_list, listed_sessions, resolve_chat_db, run_chat_delete, run_chat_list,
        run_chat_rename,
    };
    use super::*;
    use crate::exit_codes;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(subcommand)]
        cmd: ChatHygieneCommand,
    }

    #[test]
    fn parses_list_with_and_without_a_database_path() {
        // GIVEN "chat list", alone and with --db
        // WHEN clap parses each argument line
        let bare = TestCli::parse_from(["x", "list"]);
        let with_db = TestCli::parse_from(["x", "list", "--db", "/tmp/chat.db"]);
        // THEN both are the list, the second carrying the path given
        assert!(matches!(bare.cmd, ChatHygieneCommand::List { db: None }));
        match with_db.cmd {
            ChatHygieneCommand::List { db } => {
                assert_eq!(db, Some(PathBuf::from("/tmp/chat.db")));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn list_shows_each_session_with_its_folder_and_leaves_out_the_trash() {
        use apollia_runtime::chat::{ChatMode, ChatSessionRepository};

        // GIVEN a chat database of three sessions: one in the folder
        // "Clients", one in no folder, one in the trash
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("chat.db");
        let repo = ChatSessionRepository::open(&path).expect("open");
        for (id, day) in [("s-filed", 1), ("s-loose", 2), ("s-trashed", 3)] {
            let created_at = format!("2026-10-0{day}T00:00:00Z");
            repo.create_session(id, &ChatMode::Libre, None, "", &[], &created_at, None)
                .expect("session");
        }
        repo.rename_session("s-filed", "Quarterly invoice")
            .expect("rename");
        let folder = repo.create_folder("Clients").expect("folder");
        repo.move_session_to_folder("s-filed", Some(&folder.id))
            .expect("move");
        repo.trash_session("s-trashed", "2026-10-04T00:00:00Z")
            .expect("trash");

        // WHEN the list is read and printed
        let sessions = listed_sessions(&repo).expect("listed");
        let table = format_session_list(&sessions);
        let json = serde_json::to_value(&sessions).expect("json");

        // THEN the two listed sessions come most recent first, the filed one
        // names its folder on its line, and the JSON carries the same
        assert_eq!(json.as_array().map(Vec::len), Some(2));
        assert_eq!(json[0]["id"], "s-loose");
        assert_eq!(json[0]["folder"], serde_json::Value::Null);
        assert_eq!(json[1]["id"], "s-filed");
        assert_eq!(json[1]["folder"], "Clients");
        assert_eq!(json[1]["title"], "Quarterly invoice");
        let lines: Vec<&str> = table.lines().collect();
        assert!(
            lines[0].starts_with("ID") && lines[0].contains("FOLDER"),
            "{table}"
        );
        let filed = lines
            .iter()
            .find(|line| line.starts_with("s-filed"))
            .expect("a line for the filed session");
        assert!(
            filed.contains("Clients") && filed.contains("Quarterly invoice"),
            "{table}"
        );
        assert!(!table.contains("s-trashed"), "{table}");
        drop(repo);
        assert_eq!(run_chat_list(Some(&path), true), exit_codes::SUCCESS);
    }

    #[test]
    fn list_of_a_missing_database_is_an_error_and_an_empty_one_says_so() {
        // GIVEN no chat database at the path given, then an empty one
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("chat.db");

        // WHEN the list runs before the file exists
        let missing = run_chat_list(Some(&path), true);

        // THEN it stops on an error, and nothing was created
        assert_eq!(missing, exit_codes::GENERAL_ERROR);
        assert!(!path.exists());

        // WHEN the file exists and holds no session
        drop(apollia_runtime::chat::ChatSessionRepository::open(&path).expect("open"));

        // THEN the list says there is none
        assert_eq!(format_session_list(&[]), "No sessions found.\n");
        assert_eq!(run_chat_list(Some(&path), false), exit_codes::SUCCESS);
    }

    #[test]
    fn parses_delete_requires_confirm_flag() {
        // GIVEN "chat delete sess-abc", with no --confirm
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "delete", "sess-abc"]);
        // THEN the session is captured and the confirmation stays down
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
        // GIVEN the same command line with --confirm
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "delete", "sess-abc", "--confirm"]);
        // THEN the confirmation is up
        match cli.cmd {
            ChatHygieneCommand::Delete { confirm, .. } => assert!(confirm),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_rename() {
        // GIVEN a rename carrying a two-word title
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "rename", "sess-abc", "Hello world"]);
        // THEN the session and the title land in the right order, spaces included
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
        // GIVEN an export carrying --output and no --format
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "export", "sess-abc", "--output", "/tmp/out.md"]);
        // THEN the output path is captured and the format defaults to markdown
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
        // GIVEN an export carrying --format json
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "export", "sess-abc", "--format", "json"]);
        // THEN the format given wins over the default
        match cli.cmd {
            ChatHygieneCommand::Export { format, .. } => assert_eq!(format, "json"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn export_rejects_invalid_format() {
        // GIVEN an export whose --format names a format the command does not write
        // WHEN clap parses the argument line
        let result = TestCli::try_parse_from(["x", "export", "s", "--format", "xml"]);
        // THEN parsing fails rather than the export producing an empty file
        assert!(result.is_err(), "xml is not a valid format");
    }

    #[test]
    fn delete_without_confirm_returns_error() {
        // GIVEN a session deletion asked for without confirmation
        // WHEN it runs
        let code = run_chat_delete("sess-abc", false, None, true);
        // THEN it stops on an error rather than dropping the conversation
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn rename_rejects_empty_title() {
        // GIVEN a rename whose title is made of spaces
        // WHEN it runs
        let code = run_chat_rename("sess-abc", "   ", None, true);
        // THEN it stops on an error rather than leaving the session untitled
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn resolve_chat_db_honours_override() {
        // GIVEN an explicit database path
        let p = PathBuf::from("/tmp/custom-chat.db");
        // WHEN the chat database is resolved
        // THEN the path given wins over the default one
        assert_eq!(resolve_chat_db(Some(&p)), p);
    }

    #[test]
    fn resolve_chat_db_default_ends_with_canonical_filename() {
        // GIVEN no explicit database path
        // WHEN the chat database is resolved
        let p = resolve_chat_db(None);
        // THEN it is the canonical file under the Apollia home
        assert!(
            p.to_string_lossy().ends_with(".apollia/chat.db"),
            "unexpected default path: {p:?}"
        );
    }
}
