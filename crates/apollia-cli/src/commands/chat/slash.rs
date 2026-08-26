//! Slash commands accepted inside the REPL, and their outcomes.

use std::path::Path;

use apollia_runtime::commands::CommandRegistry;

use crate::client::{ClientError, RuntimeClient};
use crate::exit_codes;
use crate::note;

/// Result of resolving an accepted REPL line into an action for the loop.
pub(super) enum ResolvedMessage {
    /// Send this expanded message to the LLM.
    Send(String),
    /// Skip sending and read the next line.
    Continue,
    /// Switch to this session id and read the next line.
    SwitchSession(String),
    /// Exit the REPL with the given code.
    Exit(i32),
}

/// Resolves an accepted, non-empty REPL line into a [`ResolvedMessage`].
///
/// Plain text is sent verbatim; slash commands are dispatched (with hot-reload
/// of the custom command registry) via [`handle_slash_command`].
pub(super) async fn resolve_repl_message(
    client: &RuntimeClient,
    session_id: &str,
    trimmed: &str,
    cwd: &Path,
    registry: &mut CommandRegistry,
) -> ResolvedMessage {
    if !trimmed.starts_with('/') {
        return ResolvedMessage::Send(trimmed.to_string());
    }
    // Hot reload: check if command files changed since last load.
    if registry.needs_reload(cwd).await {
        *registry = CommandRegistry::load(cwd).await;
    }
    match handle_slash_command(client, session_id, trimmed, registry).await {
        SlashOutcome::Continue => ResolvedMessage::Continue,
        SlashOutcome::SwitchSession(new_id) => ResolvedMessage::SwitchSession(new_id),
        SlashOutcome::Exit(code) => ResolvedMessage::Exit(code),
        SlashOutcome::SendToLlm(msg) => ResolvedMessage::Send(msg),
    }
}

/// Outcome of a slash command handler.
pub(super) enum SlashOutcome {
    /// Command handled: continue the REPL loop without sending a message.
    Continue,
    /// Fork created: switch to the new session and continue.
    SwitchSession(String),
    /// Exit the REPL with the given exit code.
    Exit(i32),
    /// Expand the command to this message and send it to the LLM.
    SendToLlm(String),
}

/// Dispatch a slash command line and return the appropriate [`SlashOutcome`].
///
/// `input` must start with `/`.
pub(super) async fn handle_slash_command(
    client: &RuntimeClient,
    session_id: &str,
    input: &str,
    registry: &CommandRegistry,
) -> SlashOutcome {
    let mut parts = input.splitn(2, ' ');
    let cmd = parts.next().unwrap_or("");
    let arg = parts.next().map(str::trim).unwrap_or("").trim();

    match cmd {
        "/fork" if arg.eq_ignore_ascii_case("list") => handle_fork_list(client, session_id).await,
        "/fork" => {
            let up_to: Option<usize> = if arg.is_empty() {
                None
            } else {
                match arg.parse::<usize>() {
                    Ok(n) => Some(n),
                    Err(_) => {
                        eprintln!("Usage: /fork [N|list]");
                        return SlashOutcome::Continue;
                    }
                }
            };
            handle_fork(client, session_id, up_to).await
        }
        "/list-commands" => handle_list_commands(registry),
        "/reprompt" => handle_reprompt(client, arg).await,
        "/find" => handle_find(arg, registry),
        _ => {
            // Check custom command registry.
            let name = cmd.trim_start_matches('/');
            if let Some(custom) = registry.get(name) {
                SlashOutcome::SendToLlm(custom.render(arg))
            } else {
                let builtin = "/fork, /fork N, /fork list, /list-commands";
                let customs: Vec<String> = registry
                    .list()
                    .iter()
                    .map(|c| format!("/{}", c.name))
                    .collect();
                if customs.is_empty() {
                    eprintln!("Unknown command: {cmd}. Available: {builtin}");
                } else {
                    eprintln!(
                        "Unknown command: {cmd}. Available: {builtin}, {}",
                        customs.join(", ")
                    );
                }
                SlashOutcome::Continue
            }
        }
    }
}

/// Execute `/fork [N]`: create a child session and switch to it.
pub(super) async fn handle_fork(
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

/// Execute `/fork list`: print child sessions of the current session.
pub(super) async fn handle_fork_list(client: &RuntimeClient, session_id: &str) -> SlashOutcome {
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

/// Execute `/list-commands`: print all available built-in and custom commands.
/// `/reprompt <text>`: rewrite the pending prompt via the local model.
///
/// Prints the improved prompt for the user to copy or edit; never auto-sends.
pub(super) async fn handle_reprompt(client: &RuntimeClient, arg: &str) -> SlashOutcome {
    if arg.is_empty() {
        eprintln!("Usage: /reprompt <prompt to improve>");
        return SlashOutcome::Continue;
    }
    const SYS: &str = "Rewrite the user's prompt to be clearer and more specific \
for an AI agent, preserving the original intent and language. Output ONLY the \
improved prompt, with no preamble.";
    match client.llm_complete(Some(SYS), arg, None).await {
        Ok(resp) => {
            let improved = resp["content"].as_str().unwrap_or("").trim();
            eprintln!("(via backend: {})", resp["backend"].as_str().unwrap_or("?"));
            println!("Improved prompt (copy / edit then send):\n{improved}");
        }
        Err(e) => eprintln!("reprompt unavailable: {e}"),
    }
    SlashOutcome::Continue
}

/// `/find <query>`: fuzzy-search the command surface + slash commands.
pub(super) fn handle_find(arg: &str, registry: &CommandRegistry) -> SlashOutcome {
    let catalog = command_catalog(registry);
    let refs: Vec<&str> = catalog.iter().map(String::as_str).collect();
    let ranked = crate::commands::fuzzy::rank(arg, &refs);
    if ranked.is_empty() {
        println!("no command matches '{arg}'");
    } else {
        note!("Commands:");
        for cmd in ranked.iter().take(12) {
            println!("  {cmd}");
        }
    }
    SlashOutcome::Continue
}

/// Build the searchable catalog: `<noun> <verb>` from the clap tree + the custom
/// slash commands from the registry.
pub(super) fn command_catalog(registry: &CommandRegistry) -> Vec<String> {
    use clap::CommandFactory;
    let cmd = crate::Cli::command();
    let mut out: Vec<String> = Vec::new();
    for sub in cmd.get_subcommands() {
        let name = sub.get_name();
        let verbs: Vec<&str> = sub.get_subcommands().map(clap::Command::get_name).collect();
        if verbs.is_empty() {
            out.push(name.to_string());
        } else {
            for v in verbs {
                out.push(format!("{name} {v}"));
            }
        }
    }
    for c in registry.list() {
        out.push(format!("/{}", c.name));
    }
    out
}

pub(super) fn handle_list_commands(registry: &CommandRegistry) -> SlashOutcome {
    note!("Built-in commands:");
    println!("  /fork              Fork current session (copies full history)");
    println!("  /fork N            Fork keeping the first N messages");
    println!("  /fork list         List child sessions");
    println!("  /list-commands     List all available commands");
    println!("  /reprompt <text>   Improve a prompt with the local model");
    println!("  /find <query>      Fuzzy-search the command surface");

    let customs = registry.list();
    if customs.is_empty() {
        println!("\nNo custom commands found.");
        println!(
            "Add .md files to {dir}/commands/ or ~/{dir}/commands/ to define custom commands.",
            dir = apollia_core::paths::DATA_DIR_NAME
        );
    } else {
        println!("\nCustom commands:");
        for cmd in customs {
            let args_hint = if cmd.args.is_empty() {
                String::new()
            } else {
                format!(" <{}>", cmd.args.join("> <"))
            };
            println!("  /{}{:<20}  {}", cmd.name, args_hint, cmd.description);
        }
    }

    SlashOutcome::Continue
}
