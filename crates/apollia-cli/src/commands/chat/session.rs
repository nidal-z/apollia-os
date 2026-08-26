//! Session listing, creation, resume, and the interactive REPL loop.

use std::path::PathBuf;

use rustyline::error::ReadlineError;
use rustyline::history::FileHistory;
use rustyline::Editor;

use apollia_runtime::commands::CommandRegistry;

use crate::client::{ClientError, RuntimeClient};
use crate::exit_codes;
use crate::note;

use super::slash::{resolve_repl_message, ResolvedMessage};
use super::{history_path, make_editor_config, SCROLLBACK_COUNT};

/// List the 10 most recent sessions in an ASCII table and return exit code.
pub(super) async fn run_list(client: &RuntimeClient, json: bool) -> i32 {
    let summaries = match client.list_recent_chat_sessions(10).await {
        Ok(v) => v,
        Err(ClientError::ConnectionRefused) => {
            return crate::output::emit_error(
                json,
                exit_codes::RUNTIME_ERROR,
                "runtime not started (connection refused)",
            );
        }
        Err(e) => {
            return crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string());
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
        let first_msg_short = crate::commands::truncate_for_display(first_msg, 50, 50, "");
        let date = item["created_at"].as_str().unwrap_or("-");
        println!(
            "{:<8}  {:<8}  {:<12}  {:<50}  {}",
            id_short, mode, status, first_msg_short, date
        );
    }

    exit_codes::SUCCESS
}

/// Start a new session and enter the REPL loop.
pub(super) async fn run_new_session(client: &RuntimeClient, json: bool, no_color: bool) -> i32 {
    let session_info = match client.create_chat_session("libre").await {
        Ok(v) => v,
        Err(ClientError::ConnectionRefused) => {
            return crate::output::emit_error(
                json,
                exit_codes::RUNTIME_ERROR,
                "runtime not started (connection refused)",
            );
        }
        Err(e) => {
            return crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string());
        }
    };

    let session_id = match session_info["id"].as_str() {
        Some(id) => id.to_string(),
        None => {
            return crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                "server did not return a session ID",
            );
        }
    };

    println!("Session: {session_id}");
    println!("Type your message (Ctrl+D to exit, Ctrl+R to search history):");

    repl_loop(client, &session_id, json, no_color).await
}

/// Resume an existing session and enter the REPL loop.
pub(super) async fn run_resume(
    client: &RuntimeClient,
    session_id: &str,
    json: bool,
    no_color: bool,
) -> i32 {
    let detail = match client.resume_chat_session(session_id).await {
        Ok(v) => v,
        Err(ClientError::ConnectionRefused) => {
            return crate::output::emit_error(
                json,
                exit_codes::RUNTIME_ERROR,
                "runtime not started (connection refused)",
            );
        }
        Err(ClientError::ServerError { status: 404, .. }) => {
            return crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!("session not found: {session_id}"),
            );
        }
        Err(e) => {
            return crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string());
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

    repl_loop(client, session_id, json, no_color).await
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
pub(super) async fn repl_loop(
    client: &RuntimeClient,
    session_id: &str,
    json: bool,
    no_color: bool,
) -> i32 {
    let mut current_session_id = session_id.to_string();
    let history_file = history_path();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // Load custom command registry once at session start.
    let mut registry = CommandRegistry::load(&cwd).await;

    let mut rl: Editor<(), FileHistory> = match Editor::with_config(make_editor_config()) {
        Ok(e) => e,
        Err(e) => {
            return crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!("initializing line editor: {e}"),
            );
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
                note!();
                break;
            }
            Err(e) => {
                return crate::output::emit_error(
                    json,
                    exit_codes::GENERAL_ERROR,
                    &format!("reading input: {e}"),
                );
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
        let message =
            match resolve_repl_message(client, &current_session_id, &trimmed, &cwd, &mut registry)
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

        // Envoyer le message et rendre la reponse en streaming token par token.
        if let Err(code) = crate::commands::chat_stream::stream_send(
            client,
            &current_session_id,
            &message,
            json,
            no_color,
        )
        .await
        {
            return code;
        }
    }

    exit_codes::SUCCESS
}
