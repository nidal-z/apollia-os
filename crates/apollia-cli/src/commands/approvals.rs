//! `apollia-os approvals` subcommands: the decisions waiting for the user.
//!
//! `pending` lists, from the runtime, every decision a run waits on: the tool
//! consents of the conversations and of the runs, the paused tasks, the batch
//! items waiting for an access, the questions agents asked and the questions
//! running workflows stopped on. Each line names the answers it takes.
//!
//! `decide` gives one of those answers to one item, by the `id` `pending`
//! prints: the conversation, the task, the batch item or the workflow that
//! waited goes on as if it had been answered where it asked.

use std::path::PathBuf;

use clap::{Subcommand, ValueEnum};

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;
use crate::note;

/// Arguments for `apollia-os approvals`.
#[derive(Debug, clap::Args)]
pub struct ApprovalsArgs {
    /// Approvals subcommand.
    #[command(subcommand)]
    pub command: ApprovalsCommand,
}

/// Approvals subcommands: `apollia-os approvals <verb>`.
#[derive(Debug, Subcommand)]
pub enum ApprovalsCommand {
    /// List every decision waiting for you: consents, paused tasks, batch
    /// items waiting for an access, questions from agents and from workflows.
    Pending,
    /// Answer one waiting decision: the run that waits goes on with it.
    Decide {
        /// The `id` of the item, as `approvals pending` prints it.
        id: String,
        /// The answer, one of those the item takes.
        #[arg(value_enum)]
        answer: DecideAnswer,
        /// The answer to a question: one of its choices, or free text.
        #[arg(long, value_name = "TEXT")]
        value: Option<String>,
        /// Why, for a refusal; handed to the run that asked.
        #[arg(long, value_name = "TEXT")]
        reason: Option<String>,
    },
}

/// An answer `approvals decide` gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DecideAnswer {
    /// This call, or this item, only.
    Once,
    /// This one and every later one of the same agent in the same family.
    Always,
    /// Not this one.
    Refuse,
    /// The answer to a question, given with `--value`.
    Answer,
}

impl DecideAnswer {
    fn as_str(self) -> &'static str {
        match self {
            Self::Once => "once",
            Self::Always => "always",
            Self::Refuse => "refuse",
            Self::Answer => "answer",
        }
    }
}

/// The body of `POST /api/v1/pending-decisions/decide` for one answer.
fn decide_body(
    id: &str,
    answer: DecideAnswer,
    value: Option<&str>,
    reason: Option<&str>,
) -> serde_json::Value {
    let mut body = serde_json::json!({"id": id, "answer": answer.as_str()});
    if let Some(value) = value {
        body["value"] = serde_json::json!(value);
    }
    if let Some(reason) = reason {
        body["reason"] = serde_json::json!(reason);
    }
    body
}

/// Execute an `approvals` subcommand. Returns the process exit code.
pub async fn run(args: &ApprovalsArgs, socket: Option<PathBuf>, json: bool) -> i32 {
    let client = RuntimeClient::new(socket.unwrap_or_else(default_socket_path));
    match &args.command {
        ApprovalsCommand::Pending => run_pending(&client, json).await,
        ApprovalsCommand::Decide {
            id,
            answer,
            value,
            reason,
        } => {
            if *answer == DecideAnswer::Answer && value.is_none() {
                return crate::output::emit_error(
                    json,
                    exit_codes::GENERAL_ERROR,
                    "the answer to a question is given with --value",
                );
            }
            let body = decide_body(id, *answer, value.as_deref(), reason.as_deref());
            run_decide(&client, &body, json).await
        }
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

async fn run_decide(client: &RuntimeClient, body: &serde_json::Value, json: bool) -> i32 {
    match client.decide_pending(body).await {
        Ok(done) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&done).unwrap_or_default()
                );
            } else {
                println!("{}", field(&done, "message"));
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
    fn test_the_body_of_a_decision_carries_only_what_was_given() {
        // GIVEN an answer to a question, and a refusal with its reason
        // WHEN each is turned into the body the runtime reads
        let answered = decide_body(
            "workflow-question:r-1",
            DecideAnswer::Answer,
            Some("180.50"),
            None,
        );
        let refused = decide_body("task:t-1", DecideAnswer::Refuse, None, Some("not now"));

        // THEN each carries its id, its answer and the one field it was given
        assert_eq!(
            answered,
            serde_json::json!({"id": "workflow-question:r-1", "answer": "answer", "value": "180.50"})
        );
        assert_eq!(
            refused,
            serde_json::json!({"id": "task:t-1", "answer": "refuse", "reason": "not now"})
        );
    }

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
