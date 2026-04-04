//! Interactive chat REPL command.
//!
//! Provides an interactive terminal session for chatting with an LLM through
//! the Apollia runtime. Supports creating new sessions, resuming previous ones,
//! and listing recent sessions.

use std::io::Write;
use std::path::PathBuf;

use tokio::io::AsyncBufReadExt;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

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
    println!("Type your message (Ctrl+D to exit):");

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

/// Core REPL loop: read user input, send message, poll for response, print reply.
///
/// Handles Ctrl+D (EOF) for clean exit and Ctrl+C (SIGINT) for saving the session.
async fn repl_loop(client: &RuntimeClient, session_id: &str) -> i32 {
    let stdin = tokio::io::BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();
    let sid = session_id.to_string();

    loop {
        // Print the prompt.
        print!("> ");
        if std::io::stdout().flush().is_err() {
            break;
        }

        // Read one line, with Ctrl+C catching.
        let line_result = tokio::select! {
            line = lines.next_line() => line,
            _ = tokio::signal::ctrl_c() => {
                println!("\nSession saved: {sid}");
                return exit_codes::SUCCESS;
            }
        };

        let line = match line_result {
            Ok(Some(l)) => l,
            Ok(None) => {
                // EOF (Ctrl+D)
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

        // Count current messages before sending so we can detect new ones.
        let count_before = match get_message_count(client, session_id).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error: {e}");
                return exit_codes::GENERAL_ERROR;
            }
        };

        // Send the message.
        match client.send_chat_message(session_id, &trimmed).await {
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
        match poll_for_response(client, session_id, count_before).await {
            Ok(Some(reply)) => println!("{reply}"),
            Ok(None) => eprintln!("[no response received within timeout]"),
            Err(e) => eprintln!("Error while waiting for response: {e}"),
        }
    }

    exit_codes::SUCCESS
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
