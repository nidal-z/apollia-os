//! `apollia-os do "<natural language>"`: map intent to a command.
//!
//! The local model is constrained by a GBNF grammar built from the clap tree, so
//! it can only emit a valid `<noun> <verb>` prefix (plus free tokens) or the
//! `unknown` sentinel. The mapped command is shown as a dry-run and executed only
//! after confirmation, through a fresh process (normal dispatch + governance, no
//! bypass).

use std::io::Write;
use std::path::PathBuf;
use std::process::Command as ProcCommand;

use clap::{CommandFactory, Parser};

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;

/// Map `request` to a command and (after confirmation) run it.
pub async fn run(request: &str, yes: bool, socket: Option<PathBuf>, json: bool) -> i32 {
    if request.trim().is_empty() {
        eprintln!("describe what you want to do");
        return exit_codes::GENERAL_ERROR;
    }
    // GBNF grammar (applied by the llama-cpp backend) AND catalogue injected
    // into the prompt (for the backends that ignore the grammar, OpenAI style).
    let cli_cmd = crate::Cli::command();
    let grammar = build_grammar_from(&cli_cmd);
    let catalog = command_list(&cli_cmd);
    let system = format!(
        "You translate a user's natural-language request into a single Apollia \
CLI command. Output ONLY the command without the `apollia-os` prefix, or exactly \
`unknown` if nothing fits. Do not explain. The command prefix MUST be exactly one \
of these forms (followed by any needed arguments), copied verbatim:\n{}",
        catalog.join("\n")
    );
    let client = RuntimeClient::new(socket.unwrap_or_else(default_socket_path));
    let resp = match client
        .llm_complete(Some(&system), request, Some(&grammar))
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
    let mapped = resp["content"].as_str().unwrap_or("").trim().to_string();

    if mapped.is_empty() || mapped == "unknown" {
        eprintln!("no command matched, try rephrasing");
        return exit_codes::GENERAL_ERROR;
    }

    let args: Vec<String> = mapped.split_whitespace().map(String::from).collect();

    // No recursion on `do`.
    if args.first().map(|s| s == "do").unwrap_or(false) {
        eprintln!("refusing to map to `do` itself, try rephrasing");
        return exit_codes::GENERAL_ERROR;
    }

    // The GBNF grammar is only applied by the local llama-cpp backend; an
    // OpenAI-compatible backend ignores it and can return an invalid command.
    // So the mapped command is ALWAYS validated against the real parser before
    // anything is proposed: that is the backend-independent safety net.
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
/// strings. Injected into the prompt so backends that ignore the GBNF grammar
/// still see the exact command vocabulary.
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

/// Pure grammar builder over a clap [`clap::Command`] (testable).
fn build_grammar_from(cmd: &clap::Command) -> String {
    let mut prefixes: Vec<String> = Vec::new();
    for sub in cmd.get_subcommands() {
        let name = sub.get_name();
        if name == "do" {
            continue; // no recursion on `do`
        }
        let verbs: Vec<&str> = sub.get_subcommands().map(clap::Command::get_name).collect();
        if verbs.is_empty() {
            prefixes.push(format!("\"{name}\""));
        } else {
            for v in verbs {
                prefixes.push(format!("\"{name} {v}\""));
            }
        }
    }
    let alt = if prefixes.is_empty() {
        "\"unknown\"".to_string()
    } else {
        prefixes.join(" | ")
    };
    format!(
        "root ::= command | \"unknown\"\n\
         command ::= prefix rest\n\
         prefix ::= {alt}\n\
         rest ::= (\" \" token)*\n\
         token ::= [a-zA-Z0-9_./@:=-]+\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN the real clap tree WHEN the grammar is generated THEN it holds
    // valid noun-verb prefixes and the unknown sentinel, and excludes `do`.
    #[test]
    fn test_grammar_contains_valid_prefixes() {
        let g = build_grammar_from(&crate::Cli::command());
        assert!(g.contains("\"unknown\""));
        assert!(g.contains("\"agent list\""));
        assert!(g.contains("\"status\"")); // commande nue
        assert!(!g.contains("\"do "));
        assert!(g.contains("prefix ::="));
    }
}
