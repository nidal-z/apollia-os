//! `apollia-os model embeddings <verb>`: the embeddings model search by meaning
//! uses.
//!
//! Both leaves go through the runtime (`GET` and `PUT
//! /api/v1/llm/embeddings/default`), which owns `system.db` and checks the
//! identifier against the registry. Daemon off, each refuses with exit 2.

use std::path::PathBuf;

use clap::Subcommand;

use crate::exit_codes;

/// Route of the default embeddings model.
const DEFAULT_ROUTE: &str = "/api/v1/llm/embeddings/default";

/// `apollia-os model embeddings <verb>`.
#[derive(Debug, Subcommand)]
pub enum EmbeddingsCommand {
    /// Show the default embeddings model, and whether its file is installed.
    Show,
    /// Set the default embeddings model, by its registry identifier.
    SetDefault {
        /// Identifier of an embeddings entry of the registry, as
        /// `GET /api/v1/llm/embeddings` lists them (for example
        /// `multilingual-e5-small`).
        model: String,
    },
}

/// Run one leaf; returns the process exit code.
pub async fn run(cmd: &EmbeddingsCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let socket = socket.unwrap_or_else(crate::client::default_socket_path);
    let client = crate::client::RuntimeClient::new(socket);
    let response = match cmd {
        EmbeddingsCommand::Show => client.get(DEFAULT_ROUTE).await,
        EmbeddingsCommand::SetDefault { model } => {
            let body = serde_json::json!({ "model": model });
            client.put(DEFAULT_ROUTE, Some(&body)).await
        }
    };
    match response {
        Ok(resp) if resp.status < 400 => {
            if json {
                println!("{}", resp.body);
            } else {
                for line in default_lines(&resp.body) {
                    println!("{line}");
                }
            }
            exit_codes::SUCCESS
        }
        Ok(resp) => {
            let message = serde_json::from_str::<serde_json::Value>(&resp.body)
                .ok()
                .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_owned))
                .unwrap_or(resp.body);
            let code = if resp.status < 500 {
                exit_codes::GENERAL_ERROR
            } else {
                exit_codes::RUNTIME_ERROR
            };
            crate::output::emit_error(json, code, &message)
        }
        Err(e) => crate::output::emit_client_error(json, &e),
    }
}

/// The human lines of a `GET /api/v1/llm/embeddings/default` answer.
fn default_lines(body: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
        return vec![body.to_owned()];
    };
    match v.get("model").and_then(serde_json::Value::as_str) {
        None => vec!["  model       none (search uses keywords alone)".to_owned()],
        Some(model) => {
            let installed = match v.get("path").and_then(serde_json::Value::as_str) {
                Some(path) => format!("yes, {path}"),
                None => "no".to_owned(),
            };
            vec![
                format!("  model       {model}"),
                format!("  installed   {installed}"),
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    use crate::commands::model::ModelCommand;
    use crate::{Cli, Commands};

    /// The embeddings leaf the real top-level parser selects for `args`.
    fn parse(args: &[&str]) -> EmbeddingsCommand {
        match Cli::parse_from(args).command {
            Commands::Model {
                command: ModelCommand::Embeddings { command },
            } => command,
            other => panic!("expected model embeddings, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_model_embeddings_show() {
        // GIVEN "apollia-os model embeddings show"
        // WHEN the top-level parser reads it
        let leaf = parse(&["apollia-os", "model", "embeddings", "show"]);
        // THEN the show leaf is selected
        assert!(matches!(leaf, EmbeddingsCommand::Show));
    }

    #[test]
    fn test_cli_parses_model_embeddings_set_default() {
        // GIVEN "apollia-os model embeddings set-default multilingual-e5-small"
        // WHEN the top-level parser reads it
        let leaf = parse(&[
            "apollia-os",
            "model",
            "embeddings",
            "set-default",
            "multilingual-e5-small",
        ]);
        // THEN the identifier is captured
        assert!(
            matches!(&leaf, EmbeddingsCommand::SetDefault { model } if model == "multilingual-e5-small"),
            "{leaf:?}"
        );
    }

    #[test]
    fn test_the_lines_say_when_no_model_is_set_and_where_one_is_installed() {
        // GIVEN the two shapes the route answers
        let none = r#"{"model":null,"installed":false,"path":null}"#;
        let set = r#"{"model":"multilingual-e5-small","installed":true,"path":"/m/e5.gguf"}"#;

        // WHEN they are rendered
        let none = default_lines(none);
        let set = default_lines(set);

        // THEN the first says keywords alone, the second names model and file
        assert_eq!(
            none,
            vec!["  model       none (search uses keywords alone)"]
        );
        assert_eq!(
            set,
            vec![
                "  model       multilingual-e5-small",
                "  installed   yes, /m/e5.gguf"
            ]
        );
    }
}
