//! Prompt d'approbation d'outil en ligne, humain et JSON.

use std::io::{self, BufRead, Write};

use crate::client::{AuthorizeToolArgs, RuntimeClient};

use super::classify::{parse_scope_choice, parse_tool_decision, ToolDecisionInput};

// ─── Approbation en ligne ────────────────────────────────────────────────────

/// Lit une ligne de stdin, `None` si le flux est ferme. Bloque le thread: a
/// appeler sous `block_in_place` depuis un contexte async.
pub(super) fn read_stdin_line() -> Option<String> {
    io::stdin().lock().lines().next().and_then(|r| r.ok())
}

/// Prompt d'approbation TTY: affiche la demande, lit la decision, la soumet.
///
/// Le stream est en pause pendant la saisie (le pump SSE est une tache separee
/// bufferisee, une pause breve est sans risque).
pub(super) async fn handle_chat_approval(
    client: &RuntimeClient,
    session_id: &str,
    message_id: &str,
    tool_name: &str,
    prompt: &str,
) {
    if !prompt.is_empty() {
        println!("{prompt}");
    }
    loop {
        print!(
            "the agent wants to use {tool_name}: [A]llow once / [T]always / [R]efuse [reason] > "
        );
        let _ = io::stdout().flush();
        let Some(line) = tokio::task::block_in_place(read_stdin_line) else {
            // stdin ferme: refuser par securite.
            submit_authorize(
                client,
                &refuse_args(session_id, message_id, tool_name, Some("stdin closed")),
            )
            .await;
            return;
        };
        match parse_tool_decision(&line) {
            ToolDecisionInput::Accept => {
                submit_authorize(
                    client,
                    &AuthorizeToolArgs {
                        session_id,
                        message_id,
                        tool_name,
                        decision: "accept",
                        reason: None,
                        scope: None,
                    },
                )
                .await;
                return;
            }
            ToolDecisionInput::Refuse(reason) => {
                submit_authorize(
                    client,
                    &refuse_args(session_id, message_id, tool_name, reason.as_deref()),
                )
                .await;
                return;
            }
            ToolDecisionInput::Always => {
                let scope = prompt_scope();
                submit_authorize(
                    client,
                    &AuthorizeToolArgs {
                        session_id,
                        message_id,
                        tool_name,
                        decision: "always_accept",
                        reason: None,
                        scope: Some(scope),
                    },
                )
                .await;
                return;
            }
            ToolDecisionInput::Invalid => {
                println!("Invalid input. [A]llow / [T]always / [R]efuse");
            }
        }
    }
}

/// Construit les arguments d'un refus.
pub(super) fn refuse_args<'a>(
    session_id: &'a str,
    message_id: &'a str,
    tool_name: &'a str,
    reason: Option<&'a str>,
) -> AuthorizeToolArgs<'a> {
    AuthorizeToolArgs {
        session_id,
        message_id,
        tool_name,
        decision: "refuse",
        reason,
        scope: None,
    }
}

/// Sous-prompt de portee pour "toujours autoriser". Renvoie la valeur wire.
pub(super) fn prompt_scope() -> &'static str {
    loop {
        print!("  Always scope: [1] this session (default) / [2] this tool / [3] this project > ");
        let _ = io::stdout().flush();
        let Some(line) = tokio::task::block_in_place(read_stdin_line) else {
            return "this_session";
        };
        match parse_scope_choice(&line) {
            Some(s) => return s,
            None => println!("  Invalid choice."),
        }
    }
}

/// Prompt d'approbation en mode machine: lit une decision JSON sur stdin.
pub(super) async fn handle_chat_approval_json(
    client: &RuntimeClient,
    session_id: &str,
    message_id: &str,
    tool_name: &str,
) {
    let Some(line) = tokio::task::block_in_place(read_stdin_line) else {
        submit_authorize(
            client,
            &refuse_args(session_id, message_id, tool_name, Some("stdin closed")),
        )
        .await;
        return;
    };
    let parsed =
        serde_json::from_str::<serde_json::Value>(&line).unwrap_or(serde_json::Value::Null);
    submit_authorize(
        client,
        &AuthorizeToolArgs {
            session_id,
            message_id,
            tool_name,
            decision: parsed["decision"].as_str().unwrap_or("refuse"),
            reason: parsed["reason"].as_str(),
            scope: parsed["scope"].as_str(),
        },
    )
    .await;
}

/// Soumet la decision d'approbation a l'API. Une erreur de transport est
/// signalee mais ne tue pas le REPL.
pub(super) async fn submit_authorize(client: &RuntimeClient, args: &AuthorizeToolArgs<'_>) {
    if let Err(e) = client.authorize_tool(args).await {
        eprintln!("  x Failed to submit the decision: {e}");
    }
}
