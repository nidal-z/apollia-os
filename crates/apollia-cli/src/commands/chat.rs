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

use std::path::{Path, PathBuf};

use clap::Subcommand;
use rustyline::error::ReadlineError;
use rustyline::history::FileHistory;
use rustyline::{Config as RlConfig, Editor};

use apollia_runtime::chat::{ChatSessionRepository, MessageRow, SessionRow};
use apollia_runtime::commands::CommandRegistry;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

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
}

/// Maximum number of history entries retained across sessions.
const REPL_MAX_HISTORY: usize = 10_000;

/// Maximum number of scroll-back messages to show on resume.
const SCROLLBACK_COUNT: usize = 5;

/// Polling interval when waiting for an LLM response (milliseconds).
const POLL_INTERVAL_MS: u64 = 300;

/// Timeout for waiting for an LLM response (seconds).
const RESPONSE_TIMEOUT_SECS: u64 = 120;

/// Build a [`RuntimeClient`] from the optional socket path override.
fn make_client(socket: Option<PathBuf>) -> RuntimeClient {
    match socket {
        Some(p) => RuntimeClient::new(p),
        None => RuntimeClient::new(std::path::PathBuf::from(DEFAULT_SOCKET_PATH)),
    }
}

/// Resolve the path to the persistent REPL history file.
///
/// Returns `None` if the home directory cannot be determined or the
/// `~/.apollia/` directory cannot be created. In that case history
/// is kept in memory only for the current session.
fn history_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let dir = home.join(".apollia");
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
/// - `_json`: reserved for future machine-readable output mode.
pub async fn run(resume: Option<&str>, list: bool, socket: Option<PathBuf>, _json: bool) -> i32 {
    let client = make_client(socket);

    if list {
        return run_list(&client).await;
    }

    if let Some(session_id) = resume {
        return run_resume(&client, session_id).await;
    }

    run_new_session(&client).await
}

/// List the 10 most recent sessions in an ASCII table and return exit code.
async fn run_list(client: &RuntimeClient) -> i32 {
    let summaries = match client.list_recent_chat_sessions(10).await {
        Ok(v) => v,
        Err(ClientError::ConnectionRefused) => {
            eprintln!("runtime not started");
            return exit_codes::GENERAL_ERROR;
        }
        Err(e) => {
            eprintln!("Error: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    let arr = match summaries.as_array() {
        Some(a) => a,
        None => {
            eprintln!("unexpected response format");
            return exit_codes::GENERAL_ERROR;
        }
    };

    if arr.is_empty() {
        println!("No sessions found.");
        return exit_codes::SUCCESS;
    }

    // Header
    println!(
        "{:<8}  {:<8}  {:<12}  {:<50}  DATE",
        "ID", "MODE", "STATUS", "FIRST MESSAGE"
    );
    println!("{}", "-".repeat(90));

    for item in arr {
        let id = item["id"].as_str().unwrap_or("-");
        let id_short = if id.len() > 8 { &id[..8] } else { id };
        let mode = item["mode"].as_str().unwrap_or("-");
        let status = item["status"].as_str().unwrap_or("-");
        let first_msg = item["first_message"].as_str().unwrap_or("");
        let first_msg_short = if first_msg.len() > 50 {
            &first_msg[..50]
        } else {
            first_msg
        };
        let date = item["created_at"].as_str().unwrap_or("-");
        println!(
            "{:<8}  {:<8}  {:<12}  {:<50}  {}",
            id_short, mode, status, first_msg_short, date
        );
    }

    exit_codes::SUCCESS
}

/// Start a new session and enter the REPL loop.
async fn run_new_session(client: &RuntimeClient) -> i32 {
    let session_info = match client.create_chat_session("libre").await {
        Ok(v) => v,
        Err(ClientError::ConnectionRefused) => {
            eprintln!("runtime not started");
            return exit_codes::GENERAL_ERROR;
        }
        Err(e) => {
            eprintln!("Error: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    let session_id = match session_info["id"].as_str() {
        Some(id) => id.to_string(),
        None => {
            eprintln!("Error: server did not return a session ID");
            return exit_codes::GENERAL_ERROR;
        }
    };

    println!("Session: {session_id}");
    println!("Type your message (Ctrl+D to exit, Ctrl+R to search history):");

    repl_loop(client, &session_id).await
}

/// Resume an existing session and enter the REPL loop.
async fn run_resume(client: &RuntimeClient, session_id: &str) -> i32 {
    let detail = match client.resume_chat_session(session_id).await {
        Ok(v) => v,
        Err(ClientError::ConnectionRefused) => {
            eprintln!("runtime not started");
            return exit_codes::GENERAL_ERROR;
        }
        Err(ClientError::ServerError { status: 404, .. }) => {
            eprintln!("session not found: {session_id}");
            return exit_codes::GENERAL_ERROR;
        }
        Err(e) => {
            eprintln!("Error: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    // Print scroll-back: last N messages from history
    if let Some(session) = detail.get("session") {
        if let Some(history) = session["history"].as_array() {
            let skip = if history.len() > SCROLLBACK_COUNT {
                history.len() - SCROLLBACK_COUNT
            } else {
                0
            };
            println!("--- Resuming session: {session_id} ---");
            for msg in &history[skip..] {
                let role = msg["role"].as_str().unwrap_or("?");
                let content = msg["content"].as_str().unwrap_or("");
                match role {
                    "User" => println!("> {content}"),
                    "Assistant" => println!("{content}"),
                    _ => {}
                }
            }
            println!("--- Continue ---");
        }
    }

    repl_loop(client, session_id).await
}

/// Core REPL loop using `rustyline` for line editing and persistent history.
///
/// History is loaded from and saved to `~/.apollia/repl_history` after each
/// accepted line.  Up to [`REPL_MAX_HISTORY`] entries are retained (FIFO rotation).
///
/// Ctrl+D (EOF) exits cleanly.  Ctrl+C cancels the current line (Interrupted)
/// and prints a "session saved" notice before returning.
///
/// Slash commands:
/// - `/fork`            : fork current session (copies full history)
/// - `/fork N`          : fork keeping the first N messages
/// - `/fork list`       : list child sessions of the current session
/// - `/list-commands`   : list all available commands (built-in + custom)
/// - `/<name> [arg]`    : execute a custom command defined in `.apollia/commands/`
async fn repl_loop(client: &RuntimeClient, session_id: &str) -> i32 {
    let mut current_session_id = session_id.to_string();
    let history_file = history_path();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // Load custom command registry once at session start.
    let mut registry = CommandRegistry::load(&cwd).await;

    let mut rl: Editor<(), FileHistory> = match Editor::with_config(make_editor_config()) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Error initializing line editor: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    // Load persistent history (silently ignored if the file does not exist yet).
    if let Some(ref path) = history_file {
        let _ = rl.load_history(path);
    }

    loop {
        // `readline` blocks the thread, run it in a blocking context so the
        // async executor is not starved.
        let readline_result = tokio::task::block_in_place(|| rl.readline("> "));

        let line = match readline_result {
            Ok(line) => line,
            Err(ReadlineError::Interrupted) => {
                // Ctrl+C: exit cleanly, session is already persisted server-side.
                println!("\nSession saved: {current_session_id}");
                return exit_codes::SUCCESS;
            }
            Err(ReadlineError::Eof) => {
                // Ctrl+D: clean exit.
                println!();
                break;
            }
            Err(e) => {
                eprintln!("Error reading input: {e}");
                return exit_codes::GENERAL_ERROR;
            }
        };

        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }

        // Persist the entry immediately so it survives even if the
        // process is killed mid-session.
        let _ = rl.add_history_entry(line.as_str());
        if let Some(ref path) = history_file {
            let _ = rl.save_history(path);
        }

        // Determine the message to send to the LLM.
        let message = match resolve_repl_message(
            client,
            &current_session_id,
            &trimmed,
            &cwd,
            &mut registry,
        )
        .await
        {
            ResolvedMessage::Send(msg) => msg,
            ResolvedMessage::Continue => continue,
            ResolvedMessage::SwitchSession(new_id) => {
                current_session_id = new_id;
                continue;
            }
            ResolvedMessage::Exit(code) => return code,
        };

        // Send the resolved message and wait for a reply.
        if let Err(code) = send_message_and_poll(client, &current_session_id, &message).await {
            return code;
        }
    }

    exit_codes::SUCCESS
}

/// Result of resolving an accepted REPL line into an action for the loop.
enum ResolvedMessage {
    /// Send this expanded message to the LLM.
    Send(String),
    /// Skip sending and read the next line.
    Continue,
    /// Switch to this session id and read the next line.
    SwitchSession(String),
    /// Exit the REPL with the given code.
    Exit(i32),
}

/// Resolves an accepted, non-empty REPL line into a [`ResolvedMessage`].
///
/// Plain text is sent verbatim; slash commands are dispatched (with hot-reload
/// of the custom command registry) via [`handle_slash_command`].
async fn resolve_repl_message(
    client: &RuntimeClient,
    session_id: &str,
    trimmed: &str,
    cwd: &Path,
    registry: &mut CommandRegistry,
) -> ResolvedMessage {
    if !trimmed.starts_with('/') {
        return ResolvedMessage::Send(trimmed.to_string());
    }
    // Hot reload: check if command files changed since last load.
    if registry.needs_reload(cwd).await {
        *registry = CommandRegistry::load(cwd).await;
    }
    match handle_slash_command(client, session_id, trimmed, registry).await {
        SlashOutcome::Continue => ResolvedMessage::Continue,
        SlashOutcome::SwitchSession(new_id) => ResolvedMessage::SwitchSession(new_id),
        SlashOutcome::Exit(code) => ResolvedMessage::Exit(code),
        SlashOutcome::SendToLlm(msg) => ResolvedMessage::Send(msg),
    }
}

/// Outcome of a slash command handler.
enum SlashOutcome {
    /// Command handled: continue the REPL loop without sending a message.
    Continue,
    /// Fork created: switch to the new session and continue.
    SwitchSession(String),
    /// Exit the REPL with the given exit code.
    Exit(i32),
    /// Expand the command to this message and send it to the LLM.
    SendToLlm(String),
}

/// Dispatch a slash command line and return the appropriate [`SlashOutcome`].
///
/// `input` must start with `/`.
async fn handle_slash_command(
    client: &RuntimeClient,
    session_id: &str,
    input: &str,
    registry: &CommandRegistry,
) -> SlashOutcome {
    let mut parts = input.splitn(2, ' ');
    let cmd = parts.next().unwrap_or("");
    let arg = parts.next().map(str::trim).unwrap_or("").trim();

    match cmd {
        "/fork" if arg.eq_ignore_ascii_case("list") => handle_fork_list(client, session_id).await,
        "/fork" => {
            let up_to: Option<usize> = if arg.is_empty() {
                None
            } else {
                match arg.parse::<usize>() {
                    Ok(n) => Some(n),
                    Err(_) => {
                        eprintln!("Usage: /fork [N|list]");
                        return SlashOutcome::Continue;
                    }
                }
            };
            handle_fork(client, session_id, up_to).await
        }
        "/list-commands" => handle_list_commands(registry),
        _ => {
            // Check custom command registry.
            let name = cmd.trim_start_matches('/');
            if let Some(custom) = registry.get(name) {
                SlashOutcome::SendToLlm(custom.render(arg))
            } else {
                let builtin = "/fork, /fork N, /fork list, /list-commands";
                let customs: Vec<String> = registry
                    .list()
                    .iter()
                    .map(|c| format!("/{}", c.name))
                    .collect();
                if customs.is_empty() {
                    eprintln!("Unknown command: {cmd}. Available: {builtin}");
                } else {
                    eprintln!(
                        "Unknown command: {cmd}. Available: {builtin}, {}",
                        customs.join(", ")
                    );
                }
                SlashOutcome::Continue
            }
        }
    }
}

/// Execute `/fork [N]`: create a child session and switch to it.
async fn handle_fork(
    client: &RuntimeClient,
    session_id: &str,
    up_to_index: Option<usize>,
) -> SlashOutcome {
    match client.fork_chat_session(session_id, up_to_index).await {
        Ok(info) => {
            let child_id = info["id"].as_str().unwrap_or("").to_string();
            if child_id.is_empty() {
                eprintln!("fork error: server did not return a session id");
                return SlashOutcome::Continue;
            }
            let msg_count = match up_to_index {
                Some(n) => format!("first {n} messages"),
                None => "full history".to_string(),
            };
            println!("Forked → {child_id} ({msg_count} copied). Switching to child session.");
            SlashOutcome::SwitchSession(child_id)
        }
        Err(ClientError::ConnectionRefused) => {
            eprintln!("runtime not started");
            SlashOutcome::Exit(exit_codes::GENERAL_ERROR)
        }
        Err(e) => {
            eprintln!("fork error: {e}");
            SlashOutcome::Continue
        }
    }
}

/// Execute `/fork list`: print child sessions of the current session.
async fn handle_fork_list(client: &RuntimeClient, session_id: &str) -> SlashOutcome {
    match client.list_session_children(session_id).await {
        Ok(arr) => {
            let children = arr.as_array().map(|v| v.as_slice()).unwrap_or(&[]);
            if children.is_empty() {
                println!("No forks for this session.");
            } else {
                println!("{:<8}  {:<12}  DATE", "ID", "STATUS");
                println!("{}", "-".repeat(40));
                for child in children {
                    let id = child["id"].as_str().unwrap_or("-");
                    let id_short = if id.len() > 8 { &id[..8] } else { id };
                    let status = child["status"].as_str().unwrap_or("-");
                    let date = child["created_at"].as_str().unwrap_or("-");
                    println!("{id_short:<8}  {status:<12}  {date}");
                }
            }
            SlashOutcome::Continue
        }
        Err(ClientError::ConnectionRefused) => {
            eprintln!("runtime not started");
            SlashOutcome::Exit(exit_codes::GENERAL_ERROR)
        }
        Err(e) => {
            eprintln!("fork list error: {e}");
            SlashOutcome::Continue
        }
    }
}

/// Execute `/list-commands`: print all available built-in and custom commands.
fn handle_list_commands(registry: &CommandRegistry) -> SlashOutcome {
    println!("Built-in commands:");
    println!("  /fork              Fork current session (copies full history)");
    println!("  /fork N            Fork keeping the first N messages");
    println!("  /fork list         List child sessions");
    println!("  /list-commands     List all available commands");

    let customs = registry.list();
    if customs.is_empty() {
        println!("\nNo custom commands found.");
        println!(
            "Add .md files to .apollia/commands/ or ~/.apollia/commands/ to define custom commands."
        );
    } else {
        println!("\nCustom commands:");
        for cmd in customs {
            let args_hint = if cmd.args.is_empty() {
                String::new()
            } else {
                format!(" <{}>", cmd.args.join("> <"))
            };
            println!("  /{}{:<20}  {}", cmd.name, args_hint, cmd.description);
        }
    }

    SlashOutcome::Continue
}

/// Sends `message` to the LLM and polls for the assistant reply.
///
/// Returns `Ok(())` on success (reply printed to stdout) or after a timeout.
/// Returns `Err(exit_code)` on fatal connection or server errors.
async fn send_message_and_poll(
    client: &RuntimeClient,
    session_id: &str,
    message: &str,
) -> Result<(), i32> {
    let count_before = match get_message_count(client, session_id).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            return Err(exit_codes::GENERAL_ERROR);
        }
    };

    match client.send_chat_message(session_id, message).await {
        Ok(_) => {}
        Err(ClientError::ConnectionRefused) => {
            eprintln!("runtime not started");
            return Err(exit_codes::GENERAL_ERROR);
        }
        Err(e) => {
            eprintln!("Error: {e}");
            return Err(exit_codes::GENERAL_ERROR);
        }
    }

    match poll_for_response(client, session_id, count_before).await {
        Ok(Some(reply)) => println!("{reply}"),
        Ok(None) => eprintln!("[no response received within timeout]"),
        Err(e) => eprintln!("Error while waiting for response: {e}"),
    }

    Ok(())
}

/// Return the current number of messages in the session.
async fn get_message_count(client: &RuntimeClient, session_id: &str) -> Result<usize, ClientError> {
    let detail = client.get_chat_session(session_id).await?;
    let count = detail["message_count"].as_u64().unwrap_or(0) as usize;
    Ok(count)
}

/// Poll `GET /api/v1/sessions/:id` until the message count has grown and status
/// is `"active"`, then return the last assistant message content.
///
/// Returns `Ok(None)` on timeout after [`RESPONSE_TIMEOUT_SECS`] seconds.
async fn poll_for_response(
    client: &RuntimeClient,
    session_id: &str,
    count_before: usize,
) -> Result<Option<String>, ClientError> {
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(RESPONSE_TIMEOUT_SECS);

    loop {
        if std::time::Instant::now() >= deadline {
            return Ok(None);
        }

        tokio::time::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS)).await;

        let detail = match client.get_chat_session(session_id).await {
            Ok(d) => d,
            Err(_) => continue,
        };

        let status = detail
            .get("session")
            .and_then(|s| s["status"].as_str())
            .unwrap_or("");
        let count = detail["message_count"].as_u64().unwrap_or(0) as usize;

        if count > count_before && status.eq_ignore_ascii_case("active") {
            // Find the last assistant message in the history.
            let last_reply = detail
                .get("session")
                .and_then(|s| s["history"].as_array())
                .and_then(|history| {
                    history.iter().rev().find_map(|m| {
                        if m["role"].as_str() == Some("Assistant") {
                            m["content"].as_str().map(str::to_string)
                        } else {
                            None
                        }
                    })
                });
            return Ok(last_reply);
        }
    }
}

// ─── hygiene subcommands ──────────────────────────────────────────────────────

/// Resolve the chat database path: `--db <PATH>` override, then
/// `~/.apollia/chat.db`. Tests inject explicit paths via the `--db` flag.
fn resolve_chat_db(db: Option<&Path>) -> PathBuf {
    if let Some(p) = db {
        return p.to_path_buf();
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".apollia")
        .join("chat.db")
}

fn open_chat_repo(db: Option<&Path>, json: bool) -> Option<ChatSessionRepository> {
    let path = resolve_chat_db(db);
    if !path.exists() {
        emit_chat_error(
            format!(
                "chat database not found at {} (no chat session has ever been persisted)",
                path.display()
            ),
            json,
        );
        return None;
    }
    match ChatSessionRepository::open(&path) {
        Ok(r) => Some(r),
        Err(e) => {
            emit_chat_error(format!("open {} failed: {e}", path.display()), json);
            None
        }
    }
}

fn emit_chat_error(msg: String, json: bool) {
    if json {
        let out = serde_json::json!({ "error": msg });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    } else {
        eprintln!("Error: {msg}");
    }
}

/// Synchronous entry point for `chat <subcommand>`.
pub fn run_hygiene(cmd: &ChatHygieneCommand, json: bool) -> i32 {
    match cmd {
        ChatHygieneCommand::Delete {
            session_id,
            confirm,
            db,
        } => run_chat_delete(session_id, *confirm, db.as_deref(), json),
        ChatHygieneCommand::Rename {
            session_id,
            title,
            db,
        } => run_chat_rename(session_id, title, db.as_deref(), json),
        ChatHygieneCommand::Export {
            session_id,
            output,
            format,
            db,
        } => run_chat_export(session_id, output.as_deref(), format, db.as_deref(), json),
    }
}

fn run_chat_delete(session_id: &str, confirm: bool, db: Option<&Path>, json: bool) -> i32 {
    if session_id.trim().is_empty() {
        emit_chat_error("session_id must not be empty".into(), json);
        return exit_codes::GENERAL_ERROR;
    }
    if !confirm {
        emit_chat_error(
            format!("use --confirm to delete chat session '{session_id}' (irreversible)"),
            json,
        );
        return exit_codes::GENERAL_ERROR;
    }
    let Some(repo) = open_chat_repo(db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    match repo.delete_session(session_id) {
        Ok(()) => {
            if json {
                let out = serde_json::json!({ "session_id": session_id, "deleted": true });
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!("  * chat session '{session_id}' deleted");
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_chat_error(format!("delete failed: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

fn run_chat_rename(session_id: &str, title: &str, db: Option<&Path>, json: bool) -> i32 {
    if session_id.trim().is_empty() {
        emit_chat_error("session_id must not be empty".into(), json);
        return exit_codes::GENERAL_ERROR;
    }
    let trimmed = title.trim();
    if trimmed.is_empty() {
        emit_chat_error("title must not be empty".into(), json);
        return exit_codes::GENERAL_ERROR;
    }
    let cleaned: String = trimmed.chars().take(100).collect();
    let Some(repo) = open_chat_repo(db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    match repo.rename_session(session_id, &cleaned) {
        Ok(()) => {
            if json {
                let out = serde_json::json!({
                    "session_id": session_id,
                    "title": cleaned,
                    "renamed": true,
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!("  * chat session '{session_id}' renamed to: {cleaned}");
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_chat_error(format!("rename failed: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

fn run_chat_export(
    session_id: &str,
    output: Option<&Path>,
    format: &str,
    db: Option<&Path>,
    json: bool,
) -> i32 {
    if session_id.trim().is_empty() {
        emit_chat_error("session_id must not be empty".into(), json);
        return exit_codes::GENERAL_ERROR;
    }
    let Some(repo) = open_chat_repo(db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let session = match repo.get_session(session_id) {
        Ok(Some(s)) => s,
        Ok(None) => {
            emit_chat_error(format!("chat session '{session_id}' not found"), json);
            return exit_codes::GENERAL_ERROR;
        }
        Err(e) => {
            emit_chat_error(format!("read session failed: {e}"), json);
            return exit_codes::GENERAL_ERROR;
        }
    };
    let messages = match repo.get_messages(session_id, None) {
        Ok(m) => m,
        Err(e) => {
            emit_chat_error(format!("read messages failed: {e}"), json);
            return exit_codes::GENERAL_ERROR;
        }
    };

    let body = match format {
        "json" => format_export_json(&session, &messages),
        "markdown" | _ => format_export_markdown(&session, &messages),
    };

    match output {
        Some(path) => {
            if let Err(e) = std::fs::write(path, body.as_bytes()) {
                emit_chat_error(
                    format!("write to {} failed: {e}", path.display()),
                    json,
                );
                return exit_codes::GENERAL_ERROR;
            }
            if json {
                let out = serde_json::json!({
                    "session_id": session_id,
                    "format": format,
                    "output": path.display().to_string(),
                    "bytes": body.len(),
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!(
                    "  * chat session '{session_id}' exported to {} ({} bytes)",
                    path.display(),
                    body.len()
                );
            }
        }
        None => {
            print!("{body}");
        }
    }
    exit_codes::SUCCESS
}

fn format_export_markdown(session: &SessionRow, messages: &[MessageRow]) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let title = session
        .title
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("Untitled session");
    let _ = writeln!(out, "# {title}");
    let _ = writeln!(out, "\n- id: {}", session.id);
    let _ = writeln!(out, "- created_at: {}", session.created_at);
    let _ = writeln!(out, "- mode: {}", session.mode);
    let _ = writeln!(out, "- status: {}", session.status);
    let _ = writeln!(out, "- messages: {}", messages.len());
    let _ = writeln!(out);
    for m in messages {
        let _ = writeln!(out, "## {} ({})", m.role, m.created_at);
        let _ = writeln!(out, "\n{}\n", m.content);
    }
    out
}

fn format_export_json(session: &SessionRow, messages: &[MessageRow]) -> String {
    let payload = serde_json::json!({
        "session": {
            "id": session.id,
            "title": session.title,
            "mode": session.mode,
            "status": session.status,
            "created_at": session.created_at,
            "closed_at": session.closed_at,
            "agent_name": session.agent_name,
        },
        "messages": messages.iter().map(|m| serde_json::json!({
            "id": m.id,
            "role": m.role,
            "content": m.content,
            "created_at": m.created_at,
            "seq": m.seq,
        })).collect::<Vec<_>>(),
    });
    serde_json::to_string_pretty(&payload).unwrap_or_default()
}

#[cfg(test)]
mod hygiene_tests {
    use super::*;
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
                session_id, confirm, ..
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
        let cli =
            TestCli::parse_from(["x", "export", "sess-abc", "--output", "/tmp/out.md"]);
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
