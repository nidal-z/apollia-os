//! `apollia-os chat settings` and `apollia-os chat notes`: what one
//! conversation changes from its agent, and the notes kept beside it.
//!
//! Both read and write through the runtime (`/api/v1/sessions/{id}/settings`
//! and `/notes`), so they exit 2 when it is not running. The settings hold
//! for the conversation alone: nothing is written to its agent. `set`
//! replaces the whole document; a field left out is inherited from the agent,
//! and a field the settings do not declare is refused before anything is
//! sent.

use std::path::PathBuf;

use clap::Subcommand;

use apollia_runtime::chat::SessionSettings;

use crate::client::{default_socket_path, RuntimeClient};
use crate::exit_codes;

/// Subcommands of `apollia-os chat settings`.
#[derive(Debug, Clone, Subcommand)]
pub enum ChatSettingsCommand {
    /// Print what a conversation changes from its agent.
    ///
    /// Reaches the runtime; exits 2 when it is not running.
    Get {
        /// Session id.
        session_id: String,
    },
    /// Replace the settings of a conversation.
    ///
    /// Reaches the runtime; exits 2 when it is not running. The fields are
    /// `preset`, `thinking`, `reasoning_effort` (`low`, `medium`, `high`),
    /// `preserve_thinking` (the reasoning of the earlier answers goes back to
    /// the model), `temperature`, `max_tokens`, `context_overflow` (`summarize_middle`,
    /// `sliding_window`, `stop_at_limit`), `instructions`,
    /// `disabled_accesses`, `proposed_accesses`, `only_accesses` and
    /// `profile_access`. `{}`
    /// returns the conversation to the settings of its agent.
    Set {
        /// Session id.
        session_id: String,
        /// The settings, a JSON object. A field left out is inherited.
        settings: String,
    },
}

/// Subcommands of `apollia-os chat notes`.
#[derive(Debug, Clone, Subcommand)]
pub enum ChatNotesCommand {
    /// Print the notes kept beside a conversation.
    ///
    /// Reaches the runtime; exits 2 when it is not running.
    Get {
        /// Session id.
        session_id: String,
    },
    /// Replace the notes kept beside a conversation. They are never sent to
    /// a model.
    ///
    /// Reaches the runtime; exits 2 when it is not running.
    Set {
        /// Session id.
        session_id: String,
        /// The notes. An empty string erases them.
        notes: String,
    },
}

/// Run `chat settings`.
pub(super) async fn run_settings(
    command: &ChatSettingsCommand,
    socket: Option<PathBuf>,
    json: bool,
) -> i32 {
    let client = RuntimeClient::new(socket.unwrap_or_else(default_socket_path));
    let answer = match command {
        ChatSettingsCommand::Get { session_id } => client.get_session_settings(session_id).await,
        ChatSettingsCommand::Set {
            session_id,
            settings,
        } => {
            let document = match parse_settings(settings) {
                Ok(document) => document,
                Err(message) => {
                    return crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &message)
                }
            };
            client.put_session_settings(session_id, &document).await
        }
    };
    match answer {
        Ok(body) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&body).unwrap_or_default()
                );
            } else {
                for line in settings_lines(&body) {
                    println!("{line}");
                }
            }
            exit_codes::SUCCESS
        }
        Err(e) => crate::output::emit_client_error(json, &e),
    }
}

/// Run `chat notes`.
pub(super) async fn run_notes(
    command: &ChatNotesCommand,
    socket: Option<PathBuf>,
    json: bool,
) -> i32 {
    let client = RuntimeClient::new(socket.unwrap_or_else(default_socket_path));
    let answer = match command {
        ChatNotesCommand::Get { session_id } => client.get_session_notes(session_id).await,
        ChatNotesCommand::Set { session_id, notes } => {
            client.put_session_notes(session_id, notes).await
        }
    };
    match answer {
        Ok(body) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&body).unwrap_or_default()
                );
            } else {
                match body["notes"].as_str() {
                    Some(notes) if !notes.is_empty() => println!("{notes}"),
                    _ => println!("No notes."),
                }
            }
            exit_codes::SUCCESS
        }
        Err(e) => crate::output::emit_client_error(json, &e),
    }
}

/// Reads `raw` as the settings of a conversation and returns the document to
/// send. A text that is not a JSON object, or that names a field the
/// settings do not declare, is refused with the reason.
fn parse_settings(raw: &str) -> Result<serde_json::Value, String> {
    let settings: SessionSettings =
        serde_json::from_str(raw).map_err(|e| format!("invalid settings: {e}"))?;
    serde_json::to_value(settings).map_err(|e| format!("invalid settings: {e}"))
}

/// One line per field the conversation sets, or one line saying it sets none.
fn settings_lines(body: &serde_json::Value) -> Vec<String> {
    let set: Vec<String> = body
        .as_object()
        .map(|fields| {
            fields
                .iter()
                .map(|(name, value)| match value.as_str() {
                    Some(text) => format!("{name}: {text}"),
                    None => format!("{name}: {value}"),
                })
                .collect()
        })
        .unwrap_or_default();
    if set.is_empty() {
        vec!["No setting: the conversation follows its agent.".to_owned()]
    } else {
        set
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_settings, settings_lines};

    #[test]
    fn settings_are_checked_before_they_are_sent() {
        // GIVEN a document of known fields, one with an unknown field, and
        // a text that is not JSON
        let known = r#"{"max_tokens": 256, "context_overflow": "sliding_window"}"#;
        let unknown = r#"{"system_prompt": "You are someone else."}"#;

        // WHEN each is read
        let sent = parse_settings(known);
        let refused = parse_settings(unknown);
        let broken = parse_settings("max_tokens=256");

        // THEN the first is sent as written, the others refused with a reason
        assert_eq!(
            sent,
            Ok(serde_json::json!({
                "max_tokens": 256,
                "context_overflow": "sliding_window"
            }))
        );
        assert!(refused.is_err_and(|message| message.contains("system_prompt")));
        assert!(broken.is_err_and(|message| message.starts_with("invalid settings")));
    }

    #[test]
    fn keeping_the_earlier_reasoning_is_written_and_read_like_any_setting() {
        // GIVEN a document that keeps the reasoning of the earlier answers,
        // and the answer of the runtime for that conversation
        let written = r#"{"thinking": true, "preserve_thinking": true}"#;
        let stored = serde_json::json!({ "preserve_thinking": true, "thinking": true });

        // WHEN the first is read to be sent and the second rendered for a person
        let sent = parse_settings(written);
        let lines = settings_lines(&stored);

        // THEN the field is sent as written and named on a line of its own
        assert_eq!(sent, Ok(stored.clone()));
        assert_eq!(lines, ["preserve_thinking: true", "thinking: true"]);
    }

    #[test]
    fn a_conversation_without_settings_reads_in_one_line() {
        // GIVEN the answer for a conversation that sets nothing, and for one
        // that sets two fields
        let none = serde_json::json!({});
        let two = serde_json::json!({ "instructions": "Be brief.", "max_tokens": 64 });

        // WHEN each is rendered for a person
        // THEN the first says so, the second names each field
        assert_eq!(
            settings_lines(&none),
            ["No setting: the conversation follows its agent."]
        );
        assert_eq!(
            settings_lines(&two),
            ["instructions: Be brief.", "max_tokens: 64"]
        );
    }
}
