//! `apollia-os a2a <verb>`: Agent-to-Agent skill discovery and direct invocation.
//!
//! Worker agents do not consume free-text prompts; they expose typed skills
//! (e.g. `pdf.read_text`, `email.summarize`) routed through the A2A invoker.
//! These sub-commands give the operator a way to list available skills and
//! invoke one with a structured payload, without having to know which worker
//! owns the skill.
//!
//! Routes wrapped:
//! - `GET  /api/v1/a2a/skills`
//! - `POST /api/v1/a2a/invoke`

use std::fs;
use std::path::PathBuf;

use clap::Subcommand;
use serde_json::Value;

use crate::client::{ClientError, RuntimeClient};
use crate::exit_codes;

/// A2A sub-commands.
#[derive(Debug, Clone, Subcommand)]
pub enum A2aCommand {
    /// List every skill exposed by active Worker Agents.
    ///
    /// Output columns: SKILL_ID · AGENT · NAME · DESCRIPTION.
    /// Use `--json` for the full schemas and examples.
    Skills,

    /// Invoke a Worker skill by `skill_id` with a structured payload.
    ///
    /// The payload comes from `--args '<JSON>'`, from `--args-file <PATH>`
    /// (use `-` for stdin), or defaults to `{}` if neither is supplied.
    /// The runtime routes to the worker owning the skill; the caller does
    /// not need to know which agent it is.
    Invoke {
        /// Skill identifier (e.g. `pdf.read_text`).
        skill_id: String,

        /// Structured JSON payload (overrides `--args-file`).
        #[arg(long, value_name = "JSON")]
        args: Option<String>,

        /// Read the JSON payload from a file (use `-` for stdin).
        #[arg(long, value_name = "PATH", conflicts_with = "args")]
        args_file: Option<PathBuf>,

        /// Optional invocation timeout in seconds (default: 120).
        #[arg(long, value_name = "SECS")]
        timeout: Option<u64>,

        /// Caller label surfaced in observability (default: `cli`).
        #[arg(long, value_name = "NAME")]
        caller: Option<String>,
    },
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

/// Dispatch the `a2a` sub-command.
pub async fn run(cmd: &A2aCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(|| PathBuf::from("/tmp/apollia.sock"));
    let client = RuntimeClient::new(socket_path);

    match cmd {
        A2aCommand::Skills => run_skills(&client, json).await,
        A2aCommand::Invoke {
            skill_id,
            args,
            args_file,
            timeout,
            caller,
        } => {
            run_invoke(
                &client,
                InvokeArgs {
                    skill_id,
                    args_inline: args.as_deref(),
                    args_file: args_file.as_deref(),
                    timeout: *timeout,
                    caller: caller.as_deref(),
                },
                json,
            )
            .await
        }
    }
}

async fn run_skills(client: &RuntimeClient, json: bool) -> i32 {
    match client.list_a2a_skills().await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
                return exit_codes::SUCCESS;
            }
            let skills = resp.get("skills").and_then(|v| v.as_array());
            let Some(items) = skills else {
                println!("No skills returned.");
                return exit_codes::SUCCESS;
            };
            if items.is_empty() {
                println!("No A2A skills exposed by any active worker.");
                println!();
                println!("Tip: install a worker agent and start the runtime,");
                println!("     then re-run `apollia-os a2a skills`.");
                return exit_codes::SUCCESS;
            }
            println!(
                "  {:<28} {:<22} {:<22} DESCRIPTION",
                "SKILL_ID", "AGENT", "NAME"
            );
            for s in items {
                let skill_id = s.get("skill_id").and_then(|v| v.as_str()).unwrap_or("?");
                let agent = s.get("agent_name").and_then(|v| v.as_str()).unwrap_or("?");
                let name = s.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let description = s.get("description").and_then(|v| v.as_str()).unwrap_or("");
                println!(
                    "  {:<28} {:<22} {:<22} {}",
                    truncate(skill_id, 28),
                    truncate(agent, 22),
                    truncate(name, 22),
                    truncate(description, 60),
                );
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ConnectionRefused) => {
            eprintln!("Error: runtime not started (connection refused)");
            eprintln!("Hint: run `apollia-os start` first.");
            exit_codes::RUNTIME_ERROR
        }
        Err(e) => {
            eprintln!("Error: {e}");
            exit_codes::GENERAL_ERROR
        }
    }
}

/// Parameters for `apollia-os a2a invoke`.
struct InvokeArgs<'a> {
    skill_id: &'a str,
    args_inline: Option<&'a str>,
    args_file: Option<&'a std::path::Path>,
    timeout: Option<u64>,
    caller: Option<&'a str>,
}

/// Resolves the raw payload text from `--args`, `--args-file` (or `-` for
/// stdin), defaulting to `{}`. Returns the error exit code on read failure.
fn resolve_payload_text(
    args_inline: Option<&str>,
    args_file: Option<&std::path::Path>,
) -> Result<String, i32> {
    match (args_inline, args_file) {
        (Some(s), _) => Ok(s.to_string()),
        (None, Some(path)) if path.as_os_str() == "-" => {
            use std::io::Read;
            let mut buf = String::new();
            if let Err(e) = std::io::stdin().read_to_string(&mut buf) {
                eprintln!("Error: failed to read --args-file from stdin: {e}");
                return Err(exit_codes::GENERAL_ERROR);
            }
            Ok(buf)
        }
        (None, Some(path)) => fs::read_to_string(path).map_err(|e| {
            eprintln!("Error: failed to read {}: {e}", path.display());
            exit_codes::GENERAL_ERROR
        }),
        (None, None) => Ok("{}".to_string()),
    }
}

/// Renders the human-readable result of a successful invocation.
fn render_invoke_success(resp: &Value, skill_id: &str) -> i32 {
    let agent = resp
        .get("agent_name")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let duration = resp
        .get("duration_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let result = resp.get("result");
    let status = result
        .and_then(|r| r.get("status"))
        .and_then(|s| s.as_str())
        .unwrap_or("?");
    println!("  Skill     : {skill_id}");
    println!("  Worker    : {agent}");
    println!("  Status    : {status}");
    println!("  Duration  : {duration} ms");
    if status.eq_ignore_ascii_case("failed") {
        render_invoke_error(result);
        return exit_codes::GENERAL_ERROR;
    }
    if let Some(output) = result.and_then(|r| r.get("output")) {
        println!("  Output    :");
        let rendered = serde_json::to_string_pretty(output).unwrap_or_default();
        for line in rendered.lines() {
            println!("    {line}");
        }
    }
    exit_codes::SUCCESS
}

/// Prints the error block for a failed invocation result.
fn render_invoke_error(result: Option<&Value>) {
    let Some(err) = result.and_then(|r| r.get("error")) else {
        return;
    };
    let code = err.get("code").and_then(|v| v.as_str()).unwrap_or("?");
    let message = err.get("message").and_then(|v| v.as_str()).unwrap_or("");
    println!("  Error     : [{code}] {message}");
    if let Some(details) = err.get("details") {
        println!("  Details   : {details}");
    }
}

async fn run_invoke(client: &RuntimeClient, args: InvokeArgs<'_>, json: bool) -> i32 {
    let InvokeArgs {
        skill_id,
        args_inline,
        args_file,
        timeout,
        caller,
    } = args;
    // Resolve the payload from --args, --args-file (or "-" for stdin),
    // defaulting to {} when neither is supplied. We parse it as JSON so a
    // malformed payload fails fast at the CLI boundary, not on the worker.
    let payload_text = match resolve_payload_text(args_inline, args_file) {
        Ok(t) => t,
        Err(code) => return code,
    };

    let input: Value = match serde_json::from_str(&payload_text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: --args/--args-file is not valid JSON: {e}");
            eprintln!("Hint: wrap inline JSON in single quotes, e.g.");
            eprintln!(
                "      apollia-os a2a invoke {skill_id} --args '{{\"path\":\"/tmp/foo.pdf\"}}'"
            );
            return exit_codes::GENERAL_ERROR;
        }
    };

    match client
        .invoke_a2a_skill(skill_id, input, timeout, caller.or(Some("cli")))
        .await
    {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
                return exit_codes::SUCCESS;
            }
            render_invoke_success(&resp, skill_id)
        }
        Err(ClientError::ConnectionRefused) => {
            eprintln!("Error: runtime not started (connection refused)");
            eprintln!("Hint: run `apollia-os start` first.");
            exit_codes::RUNTIME_ERROR
        }
        Err(ClientError::ServerError { status: 404, body }) => {
            eprintln!("Error: skill not found ({body})");
            eprintln!("Hint: run `apollia-os a2a skills` to list available skill IDs.");
            exit_codes::GENERAL_ERROR
        }
        Err(ClientError::ServerError { status: 503, body }) => {
            eprintln!("Error: A2A invoker unavailable ({body})");
            eprintln!("Hint: ensure the runtime started cleanly and a worker is active.");
            exit_codes::RUNTIME_ERROR
        }
        Err(e) => {
            eprintln!("Error: {e}");
            exit_codes::GENERAL_ERROR
        }
    }
}
