//! `apollia-os do "<natural language>"`: map intent to a command.
//!
//! The model answers under a JSON Schema built from the clap tree, so the
//! command it names is one of the valid `<noun> <verb>` prefixes or the
//! `unknown` sentinel, and its arguments are a separate string. A schema rather
//! than a raw grammar: a grammar binds the first token, which a model that
//! reasons before answering cannot satisfy, and only the local engine reads one.
//! The mapped command is shown as a dry-run and executed only after
//! confirmation, through a fresh process (normal dispatch + governance, no
//! bypass).

use std::io::Write;
use std::path::PathBuf;
use std::process::Command as ProcCommand;

use clap::{CommandFactory, Parser};

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;

/// What the model answers when no command fits the request.
const UNKNOWN: &str = "unknown";

/// Map `request` to a command and (after confirmation) run it.
pub async fn run(request: &str, yes: bool, socket: Option<PathBuf>, json: bool) -> i32 {
    if request.trim().is_empty() {
        eprintln!("describe what you want to do");
        return exit_codes::GENERAL_ERROR;
    }
    // The schema constrains the answer, and the catalogue is also written into
    // the prompt so the model reads the vocabulary it has to choose from.
    let cli_cmd = crate::Cli::command();
    let catalog = command_list(&cli_cmd);
    let schema = build_schema_from(&catalog);
    let system = format!(
        "You translate a user's natural-language request into a single Apollia \
CLI command. Answer with a JSON object: `command` is exactly one of the forms \
below, copied verbatim, or `unknown` if nothing fits; `arguments` holds the \
arguments that follow it, or an empty string. Do not explain.\n{}",
        catalog.join("\n")
    );
    let client = RuntimeClient::new(socket.unwrap_or_else(default_socket_path));
    let resp = match client
        .llm_complete(Some(&system), request, Some(&schema))
        .await
    {
        Ok(r) => r,
        Err(ClientError::ConnectionRefused) => {
            return crate::output::emit_error(
                json,
                exit_codes::RUNTIME_ERROR,
                "runtime not started (connection refused)",
            );
        }
        Err(e) => {
            return crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string());
        }
    };
    let mapped = mapped_command(resp["content"].as_str().unwrap_or(""));

    if mapped.is_empty() || mapped == UNKNOWN {
        eprintln!("no command matched, try rephrasing");
        return exit_codes::GENERAL_ERROR;
    }

    let args: Vec<String> = mapped.split_whitespace().map(String::from).collect();

    // No recursion on `do`.
    if args.first().map(|s| s == "do").unwrap_or(false) {
        eprintln!("refusing to map to `do` itself, try rephrasing");
        return exit_codes::GENERAL_ERROR;
    }

    // A schema is honoured by the backends that support one and ignored by the
    // others, so the mapped command is ALWAYS validated against the real parser
    // before anything is proposed: that is the backend-independent safety net.
    let argv = std::iter::once("apollia-os".to_string()).chain(args.iter().cloned());
    if crate::Cli::try_parse_from(argv).is_err() {
        eprintln!("mapped to an invalid command: `apollia-os {mapped}` (try rephrasing)");
        return exit_codes::GENERAL_ERROR;
    }

    if json {
        println!("{}", serde_json::json!({ "command": mapped }));
    }
    // Transparency: which backend produced the mapping (never a silent cloud).
    eprintln!("(via backend: {})", resp["backend"].as_str().unwrap_or("?"));
    // Dry run: show the deduced command before any execution.
    println!("would run: apollia-os {mapped}");

    // Confirmation required unless `-y`. In `--json` mode (non interactive),
    // nothing is asked: it only executes with `-y`.
    let do_exec = if yes {
        true
    } else if json {
        false
    } else {
        confirm()
    };
    if !do_exec {
        if !json {
            println!("cancelled");
        }
        return exit_codes::SUCCESS;
    }

    // Re-executed through a fresh process: it goes through the normal dispatch,
    // so governance, permissions and audit apply as for a manual invocation.
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            return crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string());
        }
    };
    match ProcCommand::new(exe).args(&args).status() {
        Ok(s) => s.code().unwrap_or(exit_codes::GENERAL_ERROR),
        Err(e) => crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string()),
    }
}

/// Interactive `[y/N]` confirmation. Blocks the thread (one-shot command).
fn confirm() -> bool {
    print!("execute? [o/N] ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    matches!(
        line.trim().to_ascii_lowercase().as_str(),
        "o" | "oui" | "y" | "yes"
    )
}

/// Valid `<noun> <verb>` (or bare command) prefixes from the clap tree, as plain
/// strings: the vocabulary of the schema, also written into the prompt.
fn command_list(cmd: &clap::Command) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for sub in cmd.get_subcommands() {
        let name = sub.get_name();
        if name == "do" {
            continue;
        }
        let verbs: Vec<&str> = sub.get_subcommands().map(clap::Command::get_name).collect();
        if verbs.is_empty() {
            out.push(name.to_string());
        } else {
            for v in verbs {
                out.push(format!("{name} {v}"));
            }
        }
    }
    out
}

/// The JSON Schema of the answer: one of the valid prefixes, or the `unknown`
/// sentinel, and the arguments as a separate string.
fn build_schema_from(catalog: &[String]) -> serde_json::Value {
    let mut commands: Vec<&str> = catalog.iter().map(String::as_str).collect();
    commands.push(UNKNOWN);
    serde_json::json!({
        "type": "object",
        "properties": {
            "command": { "type": "string", "enum": commands },
            "arguments": { "type": "string" },
        },
        "required": ["command", "arguments"],
    })
}

/// The command line the answer names, empty when it names none. An answer that
/// is not the expected object is read as plain text: a backend that ignores the
/// schema still gets its line through the parser check that follows.
fn mapped_command(content: &str) -> String {
    let Ok(answer) = serde_json::from_str::<serde_json::Value>(content.trim()) else {
        return content.trim().to_string();
    };
    let command = answer["command"].as_str().unwrap_or("").trim();
    let arguments = answer["arguments"].as_str().unwrap_or("").trim();
    if arguments.is_empty() || command == UNKNOWN {
        command.to_string()
    } else {
        format!("{command} {arguments}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_lists_valid_prefixes_and_the_sentinel() {
        // GIVEN the real clap tree
        let catalog = command_list(&crate::Cli::command());
        // WHEN the schema of the answer is built
        let schema = build_schema_from(&catalog);
        // THEN `command` is one of the noun-verb prefixes, a bare command or
        // the sentinel, and never `do` itself
        let allowed: Vec<&str> = schema["properties"]["command"]["enum"]
            .as_array()
            .expect("an enum")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        assert!(allowed.contains(&"unknown"));
        assert!(allowed.contains(&"agent list"));
        assert!(allowed.contains(&"status"));
        assert!(!allowed.iter().any(|c| *c == "do" || c.starts_with("do ")));
    }

    #[test]
    fn test_an_answer_becomes_one_command_line() {
        // GIVEN the three shapes an answer takes
        let with_arguments = r#"{"command": "agent show", "arguments": "seed-classifier"}"#;
        let bare = r#"{"command": "agent list", "arguments": ""}"#;
        let plain = "agent list";
        // WHEN each is read
        // THEN the arguments follow the command, an empty string adds nothing,
        // and text that is not the object is kept for the parser to judge
        assert_eq!(mapped_command(with_arguments), "agent show seed-classifier");
        assert_eq!(mapped_command(bare), "agent list");
        assert_eq!(mapped_command(plain), "agent list");
    }
}
