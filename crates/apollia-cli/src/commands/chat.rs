//! Interactive chat REPL command.
//!
//! Provides an interactive terminal session for chatting with an LLM through
//! the Apollia runtime. Supports creating new sessions, resuming previous ones,
//! and listing recent sessions.
//!
//! Command history is persisted across sessions at `~/.apollia/repl_history`
//! (up to 10 000 entries, FIFO rotation). Emacs keybindings and Ctrl-R reverse
//! search are provided by `rustyline`.

use std::path::PathBuf;

use rustyline::error::ReadlineError;
use rustyline::history::FileHistory;
use rustyline::{Config as RlConfig, Editor};

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

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
/// accepted line. Up to [`REPL_MAX_HISTORY`] entries are retained (FIFO rotation).
///
/// Ctrl+D (EOF) exits cleanly. Ctrl+C cancels the current line (Interrupted)
/// and prints a "session saved" notice before returning.
///
/// Slash commands:
/// - `/fork`      — fork current session (copies full history)
/// - `/fork N`    — fork keeping the first N messages
/// - `/fork list` — list child sessions of the current session
async fn repl_loop(client: &RuntimeClient, session_id: &str) -> i32 {
    // Active session — may change after a /fork.
    let mut current_session_id = session_id.to_string();
    let history_file = history_path();

    let mut rl: Editor<(), FileHistory> = match Editor::with_config(make_editor_config()) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Error initializing line editor: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    // Load persistent history — silently ignored if the file does not exist yet.
    if let Some(ref path) = history_file {
        let _ = rl.load_history(path);
    }

    loop {
        // `readline` blocks the thread — run it in a blocking context so the
        // async executor is not starved.
        let readline_result = tokio::task::block_in_place(|| rl.readline("> "));

        match readline_result {
            Ok(line) => {
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

                // Handle slash commands before sending to the LLM.
                if let Some(slash_result) =
                    handle_slash_command(client, &current_session_id, &trimmed).await
                {
                    match slash_result {
                        SlashOutcome::Continue => continue,
                        SlashOutcome::SwitchSession(new_id) => {
                            current_session_id = new_id;
                            continue;
                        }
                        SlashOutcome::Exit(code) => return code,
                    }
                }

                // Count current messages before sending so we can detect new ones.
                let count_before = match get_message_count(client, &current_session_id).await {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Error: {e}");
                        return exit_codes::GENERAL_ERROR;
                    }
                };

                // Send the message.
                match client
                    .send_chat_message(&current_session_id, &trimmed)
                    .await
                {
                    Ok(_) => {}
                    Err(ClientError::ConnectionRefused) => {
                        eprintln!("runtime not started");
                        return exit_codes::GENERAL_ERROR;
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        return exit_codes::GENERAL_ERROR;
                    }
                }

                // Poll until a new assistant message appears or timeout.
                match poll_for_response(client, &current_session_id, count_before).await {
                    Ok(Some(reply)) => println!("{reply}"),
                    Ok(None) => eprintln!("[no response received within timeout]"),
                    Err(e) => eprintln!("Error while waiting for response: {e}"),
                }
            }

            Err(ReadlineError::Interrupted) => {
                // Ctrl+C — exit cleanly, session is already persisted server-side.
                println!("\nSession saved: {current_session_id}");
                return exit_codes::SUCCESS;
            }

            Err(ReadlineError::Eof) => {
                // Ctrl+D — clean exit.
                println!();
                break;
            }

            Err(e) => {
                eprintln!("Error reading input: {e}");
                return exit_codes::GENERAL_ERROR;
            }
        }
    }

    exit_codes::SUCCESS
}

/// Outcome of a slash command handler.
enum SlashOutcome {
    /// Command handled — continue the REPL loop.
    Continue,
    /// Fork created — switch to the new session and continue.
    SwitchSession(String),
    /// Exit the REPL with the given code.
    Exit(i32),
}

/// Dispatch slash commands (lines starting with `/`).
///
/// Returns `None` when `input` is not a slash command (regular message).
/// Returns `Some(SlashOutcome)` when the command was handled.
async fn handle_slash_command(
    client: &RuntimeClient,
    session_id: &str,
    input: &str,
) -> Option<SlashOutcome> {
    if !input.starts_with('/') {
        return None;
    }

    let mut parts = input.splitn(2, ' ');
    let cmd = parts.next().unwrap_or("");
    let arg = parts.next().map(str::trim).unwrap_or("").trim();

    match cmd {
        "/fork" if arg.eq_ignore_ascii_case("list") => {
            Some(handle_fork_list(client, session_id).await)
        }
        "/fork" => {
            let up_to: Option<usize> = if arg.is_empty() {
                None
            } else {
                match arg.parse::<usize>() {
                    Ok(n) => Some(n),
                    Err(_) => {
                        eprintln!("Usage: /fork [N|list]");
                        return Some(SlashOutcome::Continue);
                    }
                }
            };
            Some(handle_fork(client, session_id, up_to).await)
        }
        _ => {
            eprintln!("Unknown command: {cmd}. Available: /fork, /fork N, /fork list");
            Some(SlashOutcome::Continue)
        }
    }
}

/// Execute `/fork [N]` — create a child session and switch to it.
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

/// Execute `/fork list` — print child sessions of the current session.
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
