//! `apollia-os approvals` subcommands: the decisions waiting for the user.
//!
//! `pending` lists, from the runtime, every decision a run waits on: the tool
//! consents of the conversations and of the runs, the paused tasks, the batch
//! items waiting for an access, and the questions agents asked. Each line
//! names the answers it takes.

use std::path::PathBuf;

use clap::Subcommand;

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;
use crate::note;

/// Approvals subcommands: `apollia-os approvals <verb>`.
#[derive(Debug, Subcommand)]
pub enum ApprovalsCommand {
    /// List every decision waiting for you: consents, paused tasks, batch
    /// items waiting for an access, questions from agents.
    Pending,
}

/// Execute an `approvals` subcommand. Returns the process exit code.
pub async fn run(cmd: &ApprovalsCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let client = RuntimeClient::new(socket.unwrap_or_else(default_socket_path));
    match cmd {
        ApprovalsCommand::Pending => run_pending(&client, json).await,
    }
}

fn field<'a>(value: &'a serde_json::Value, key: &str) -> &'a str {
    value.get(key).and_then(|v| v.as_str()).unwrap_or("-")
}

fn answers(item: &serde_json::Value) -> String {
    item.get("answers")
        .and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join("/")
        })
        .unwrap_or_default()
}

async fn run_pending(client: &RuntimeClient, json: bool) -> i32 {
    match client.pending_decisions().await {
        Ok(list) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&list).unwrap_or_default()
                );
                return exit_codes::SUCCESS;
            }
            let items = list["items"].as_array().map(Vec::as_slice).unwrap_or(&[]);
            if items.is_empty() {
                note!("Nothing waits for your decision.");
                return exit_codes::SUCCESS;
            }
            note!(
                "  {:<11} {:<20} {:<24} {:<20} WHAT",
                "KIND",
                "AGENT",
                "ACCESS",
                "ANSWERS"
            );
            for item in items {
                println!(
                    "  {:<11} {:<20} {:<24} {:<20} {}",
                    field(item, "kind"),
                    field(item, "agentId"),
                    field(item, "access"),
                    answers(item),
                    field(item, "summary")
                );
                note!("    id: {}", field(item, "id"));
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 503, body }) => {
            crate::output::emit_error(json, exit_codes::RUNTIME_ERROR, &body)
        }
        Err(ClientError::ServerError { body, .. }) => {
            crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &body)
        }
        Err(other) => crate::output::emit_client_error(json, &other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_answers_of_an_item_read_as_one_word() {
        // GIVEN an item that takes three answers
        let item = serde_json::json!({"answers": ["once", "always", "refuse"]});

        // WHEN they are rendered for a line
        let rendered = answers(&item);

        // THEN they read as one word, in their order
        assert_eq!(rendered, "once/always/refuse");
    }
}
