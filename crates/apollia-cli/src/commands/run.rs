//! `apollia-os run <agent> <input>` — submit a task and wait for the result.
//!
//! Supports `--stream` for real-time SSE streaming of task progress.

use std::path::PathBuf;
use std::time::Instant;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

/// Execute the `run` command.
///
/// Submits a task to the specified agent and waits for the result.
/// Returns the process exit code.
pub async fn run(
    agent_id: &str,
    input: &str,
    socket: Option<PathBuf>,
    json: bool,
    stream: bool,
) -> i32 {
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let client = RuntimeClient::new(socket_path);
    let start = Instant::now();

    // Submit the task
    let input_value = serde_json::json!({ "prompt": input });
    let submit_result = client.submit_task(agent_id, input_value).await;

    let task_json = match submit_result {
        Ok(j) => j,
        Err(ClientError::ConnectionRefused) => {
            return output_error(
                "runtime not started (connection refused)",
                json,
                exit_codes::RUNTIME_ERROR,
            );
        }
        Err(e) => {
            return output_error(&e.to_string(), json, exit_codes::GENERAL_ERROR);
        }
    };

    let task_id = match task_json.get("task_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => {
            return output_error(
                "missing task_id in response",
                json,
                exit_codes::GENERAL_ERROR,
            );
        }
    };

    if !json {
        println!("  -> Task {task_id} submitted to {agent_id}");
    }

    if stream {
        return stream_task(&client, &task_id, json, start).await;
    }

    // Poll for completion
    poll_task(&client, &task_id, json, start).await
}

/// Poll task status until completion.
async fn poll_task(client: &RuntimeClient, task_id: &str, json: bool, start: Instant) -> i32 {
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let result = client.get_task(task_id).await;
        match result {
            Ok(task_json) => {
                let status = task_json
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                match status {
                    "completed" | "\"Completed\"" => {
                        let elapsed = start.elapsed();
                        if json {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&task_json).unwrap_or_default()
                            );
                        } else {
                            println!("  * Completed in {:.1}s", elapsed.as_secs_f64());
                            if let Some(result) = task_json.get("result") {
                                println!("  RESULT: {result}");
                            }
                        }
                        return exit_codes::SUCCESS;
                    }
                    "failed" | "\"Failed\"" => {
                        let elapsed = start.elapsed();
                        let error_msg = task_json
                            .get("error")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown error");
                        if json {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&task_json).unwrap_or_default()
                            );
                        } else {
                            eprintln!("  x Failed in {:.1}s: {error_msg}", elapsed.as_secs_f64());
                        }
                        return exit_codes::TASK_FAILED;
                    }
                    "canceled" | "\"Canceled\"" => {
                        if json {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&task_json).unwrap_or_default()
                            );
                        } else {
                            println!("  Task {task_id} was canceled");
                        }
                        return exit_codes::GENERAL_ERROR;
                    }
                    _ => continue,
                }
            }
            Err(e) => {
                return output_error(&e.to_string(), json, exit_codes::GENERAL_ERROR);
            }
        }
    }
}

/// Stream task events via SSE.
///
/// Connects to `GET /api/v1/tasks/{id}/stream` and reads SSE frames.
async fn stream_task(client: &RuntimeClient, task_id: &str, json: bool, start: Instant) -> i32 {
    let uri = format!("/api/v1/tasks/{task_id}/stream");
    let resp = match client.get(&uri).await {
        Ok(r) => r,
        Err(ClientError::ConnectionRefused) => {
            return output_error(
                "runtime not started (connection refused)",
                json,
                exit_codes::RUNTIME_ERROR,
            );
        }
        Err(e) => {
            return output_error(&e.to_string(), json, exit_codes::GENERAL_ERROR);
        }
    };

    // SSE streams are returned as a single body for now (hyper closes the connection).
    // Parse the SSE events from the response body.
    if json {
        println!("{}", resp.body);
    } else {
        for line in resp.body.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(event) = serde_json::from_str::<serde_json::Value>(data) {
                    let event_type = event.get("event").and_then(|v| v.as_str()).unwrap_or("?");
                    match event_type {
                        "step" => {
                            let step = event.get("step").and_then(|v| v.as_u64()).unwrap_or(0);
                            let tool = event
                                .get("tool")
                                .and_then(|v| v.as_str())
                                .map(|t| format!(" tool_call {t}"))
                                .unwrap_or_default();
                            println!("  ~ Step {step}:{tool}");
                        }
                        "completed" => {
                            let elapsed = start.elapsed();
                            println!("  * Completed in {:.1}s", elapsed.as_secs_f64());
                            return exit_codes::SUCCESS;
                        }
                        "failed" => {
                            let elapsed = start.elapsed();
                            let error_msg = event
                                .get("error")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown error");
                            eprintln!("  x Failed in {:.1}s: {error_msg}", elapsed.as_secs_f64());
                            return exit_codes::TASK_FAILED;
                        }
                        "canceled" => {
                            println!("  Task {task_id} was canceled");
                            return exit_codes::GENERAL_ERROR;
                        }
                        _ => {
                            println!("  -> {event_type}");
                        }
                    }
                }
            }
        }
    }

    // Fallback to polling if SSE didn't yield a terminal event
    poll_task(client, task_id, json, start).await
}

/// Output an error and return the given exit code.
fn output_error(msg: &str, json: bool, code: i32) -> i32 {
    if json {
        let err = serde_json::json!({"error": msg});
        println!("{}", serde_json::to_string_pretty(&err).unwrap_or_default());
    } else {
        eprintln!("Error: {msg}");
    }
    code
}
