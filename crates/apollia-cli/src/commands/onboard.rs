//! `apollia-os onboard [--topic <topic>]`: launch onboarding or re-onboarding on a specific topic.
//!
//! Submits a task to the `onboarding-agent` via the runtime API. When `--topic`
//! is specified, the agent focuses the conversation on that particular domain
//! instead of running the full onboarding flow.

use std::path::PathBuf;
use std::time::Instant;

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;

/// Agent name used for onboarding tasks.
const ONBOARDING_AGENT: &str = "onboarding-agent";

/// Accepted topic values for partial re-onboarding.
const VALID_TOPICS: &[&str] = &["identity", "preferences", "tools", "domain", "agents"];

/// Errors specific to the `onboard` command.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OnboardError {
    /// The user specified a topic that does not exist.
    #[error("invalid topic '{topic}', valid topics: {}", valid.join(", "))]
    InvalidTopic {
        /// The topic value provided by the user.
        topic: String,
        /// List of accepted topic values.
        valid: Vec<String>,
    },
}

/// Validate a topic string against the allowed values.
///
/// Returns `Ok(())` when the topic is valid, or `Err(OnboardError::InvalidTopic)` otherwise.
fn validate_topic(topic: &str) -> Result<(), OnboardError> {
    if VALID_TOPICS.contains(&topic) {
        Ok(())
    } else {
        Err(OnboardError::InvalidTopic {
            topic: topic.to_string(),
            valid: VALID_TOPICS.iter().map(|s| (*s).to_string()).collect(),
        })
    }
}

/// Build the task input payload for the onboarding agent.
///
/// When a topic is provided, the input includes a `focus_topic` field so the
/// agent narrows its conversation to that specific domain.
fn build_onboarding_input(topic: Option<&str>) -> serde_json::Value {
    match topic {
        Some(t) => serde_json::json!({
            "parts": [{"type": "text", "text": format!("Re-onboarding focused on topic: {t}")}],
            "focus_topic": t,
        }),
        None => serde_json::json!({
            "parts": [{"type": "text", "text": "Start onboarding"}],
        }),
    }
}

/// Execute the `onboard` command.
///
/// Validates the optional `--topic` flag, submits an onboarding task to the
/// runtime, and polls for the result. Returns a POSIX exit code.
pub async fn run(topic: Option<&str>, socket: Option<PathBuf>, json: bool) -> i32 {
    if let Some(t) = topic {
        if let Err(e) = validate_topic(t) {
            return crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string());
        }
    }

    let socket_path = socket.unwrap_or_else(default_socket_path);
    let client = RuntimeClient::new(socket_path);
    let start = Instant::now();

    let input = build_onboarding_input(topic);
    let submit_result = client.submit_task(ONBOARDING_AGENT, input).await;

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

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "task_id": task_id,
                "agent": ONBOARDING_AGENT,
                "topic": topic,
                "status": "submitted"
            }))
            .unwrap_or_default()
        );
    } else {
        let topic_display = topic.map(|t| format!(" (topic: {t})")).unwrap_or_default();
        println!("  -> Onboarding task {task_id} submitted{topic_display}");
    }

    poll_task(&client, &task_id, json, start).await
}

/// Poll task status until completion.
async fn poll_task(client: &RuntimeClient, task_id: &str, json: bool, start: Instant) -> i32 {
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let task_json = match client.get_task(task_id).await {
            Ok(task_json) => task_json,
            Err(e) => return output_error(&e.to_string(), json, exit_codes::GENERAL_ERROR),
        };

        let status = task_json
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        if let Some(code) = render_task_status(&task_json, status, task_id, json, start) {
            return code;
        }
    }
}

/// Renders a terminal task status and returns its exit code, or `None` when the
/// task is not yet terminal (the caller keeps polling).
fn render_task_status(
    task_json: &serde_json::Value,
    status: &str,
    task_id: &str,
    json: bool,
    start: Instant,
) -> Option<i32> {
    match status {
        "completed" => {
            if json {
                print_task_json(task_json);
            } else {
                if let Some(text) = task_json.get("result").and_then(|v| v.as_str()) {
                    println!("{text}");
                }
                println!(
                    "  * Onboarding completed in {:.1}s",
                    start.elapsed().as_secs_f64()
                );
            }
            Some(exit_codes::SUCCESS)
        }
        "failed" => {
            if json {
                print_task_json(task_json);
            } else {
                let error_msg = task_json
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error");
                eprintln!(
                    "  x Onboarding failed in {:.1}s: {error_msg}",
                    start.elapsed().as_secs_f64()
                );
            }
            Some(exit_codes::TASK_FAILED)
        }
        "canceled" => {
            if json {
                print_task_json(task_json);
            } else {
                println!("  Onboarding task {task_id} was canceled");
            }
            Some(exit_codes::GENERAL_ERROR)
        }
        _ => None,
    }
}

/// Pretty-prints the task JSON payload to stdout.
fn print_task_json(task_json: &serde_json::Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(task_json).unwrap_or_default()
    );
}

/// Output an error and return the given exit code.
fn output_error(msg: &str, json: bool, code: i32) -> i32 {
    crate::output::emit_error(json, code, msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_onboard_command_parses_topic_flag() {
        // GIVEN a valid topic
        // WHEN we validate it
        let result = validate_topic("preferences");

        // THEN validation succeeds
        assert!(result.is_ok());
    }

    #[test]
    fn test_onboard_invalid_topic_rejected() {
        // GIVEN an invalid topic
        // WHEN we validate it
        let result = validate_topic("invalid");

        // THEN validation fails with InvalidTopic
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            OnboardError::InvalidTopic { topic, valid } => {
                assert_eq!(topic, "invalid");
                assert_eq!(valid.len(), VALID_TOPICS.len());
                assert!(valid.contains(&"identity".to_string()));
                assert!(valid.contains(&"preferences".to_string()));
                assert!(valid.contains(&"tools".to_string()));
                assert!(valid.contains(&"domain".to_string()));
                assert!(valid.contains(&"agents".to_string()));
            }
        }
    }

    #[test]
    fn test_all_valid_topics_accepted() {
        // GIVEN every valid topic
        // WHEN we validate each
        // THEN all pass
        for topic in VALID_TOPICS {
            assert!(
                validate_topic(topic).is_ok(),
                "topic '{topic}' should be valid"
            );
        }
    }

    #[test]
    fn test_build_onboarding_input_without_topic() {
        // GIVEN no topic
        // WHEN building the input
        let input = build_onboarding_input(None);

        // THEN input has parts but no focus_topic
        assert!(input.get("parts").is_some());
        assert!(input.get("focus_topic").is_none());
    }

    #[test]
    fn test_build_onboarding_input_with_topic() {
        // GIVEN a specific topic
        // WHEN building the input
        let input = build_onboarding_input(Some("tools"));

        // THEN input has parts and focus_topic
        assert!(input.get("parts").is_some());
        assert_eq!(input["focus_topic"].as_str(), Some("tools"));
        let text = input["parts"][0]["text"].as_str().unwrap_or("");
        assert!(text.contains("tools"));
    }
}
