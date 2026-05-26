//! Apollia OS — CLI binary entry point.
//!
//! Command structure follows noun-verb pattern (ADR-008):
//!   apollia-os <command> [options]
//!
//! Level 1 commands: start, stop, status, run.
//! Level 2 commands: memory, agent/task/tools/audit.
//!
//! Global flags: --json (machine-readable output), --socket (custom path).

pub mod client;
pub mod commands;
pub mod community;
pub mod config;
pub mod exit_codes;

use std::path::PathBuf;

use clap::Parser;

use commands::a2a::A2aCommand;
use commands::agent::AgentCommand;
use commands::audit::AuditCommand;
use commands::auth::AuthCommand;
use commands::chat;
use commands::config::ConfigCommand;
use commands::connector::ConnectorCommand;
use commands::llm::LlmCommand;
use commands::mcp::McpCommand;
use commands::mcp_server::McpServerArgs;
use commands::memory::MemoryCommand;
use commands::model::ModelCommand;
use commands::notify::NotifyCommand;
use commands::permissions::PermissionsCommand;
use commands::plan_cache::PlanCacheCommand;
use commands::project::ProjectCommand;
use commands::resilience::ResilienceCommand;
use commands::stt::SttCommand;
use commands::task::TaskCommand;
use commands::tools::ToolsCommand;
use commands::trigger::TriggerCommand;
use commands::user_memory::UserMemoryCommand;
use commands::workspace::WorkspaceCommand;

/// Apollia OS — Sovereign AI Agent Runtime.
#[derive(Debug, Parser)]
#[command(name = "apollia-os", version, about)]
struct Cli {
    /// Unix socket path (default: /tmp/apollia.sock).
    #[arg(long, global = true, value_name = "PATH")]
    socket: Option<PathBuf>,

    /// Output machine-readable JSON instead of human-readable text.
    ///
    /// Accepted at any position: before or after the subcommand and its arguments.
    #[arg(long, global = true)]
    json: bool,

    /// Suppress all non-essential output (only success/error shown).
    ///
    /// When combined with `--json`, JSON output takes priority.
    #[arg(short = 'q', long, global = true)]
    quiet: bool,

    /// Show additional details such as durations and step counts.
    #[arg(short = 'v', long, global = true)]
    verbose: bool,

    /// Enable internal debug logs and ORIA traces on stderr.
    ///
    /// Equivalent to setting `RUST_LOG=debug`.
    #[arg(long, global = true)]
    debug: bool,

    /// Disable ANSI color codes even when stdout is a TTY.
    #[arg(long, global = true)]
    no_color: bool,

    /// Command to execute.
    #[command(subcommand)]
    command: Commands,
}

/// Top-level commands.
#[derive(Debug, clap::Subcommand)]
enum Commands {
    /// Start the runtime in foreground.
    Start {
        /// TCP port to listen on (default: 7771).
        #[arg(long, value_name = "PORT")]
        port: Option<u16>,
    },

    /// Stop a running runtime.
    Stop,

    /// Display runtime and agent status.
    Status,

    /// Submit a task to an agent and wait for the result.
    ///
    /// Use the positional `<INPUT>` for free-text input (react/conversational
    /// agents). For worker agents that expect a typed skill payload, use
    /// `apollia-os a2a invoke <skill_id> --args '<JSON>'` instead — or pass
    /// `--input-json '<JSON>'` here to override the default text wrapping.
    Run {
        /// Agent identifier.
        agent_id: String,

        /// Task input text (ignored when `--input-json` is provided).
        #[arg(default_value = "")]
        input: String,

        /// Raw JSON payload that bypasses the `parts:[text]` wrapper.
        ///
        /// Use this when the target agent expects a structured input shape
        /// (e.g. an AIPInput with `data` parts, a worker skill envelope, or
        /// any custom contract).
        #[arg(long, value_name = "JSON", conflicts_with = "input")]
        input_json: Option<String>,

        /// Stream task progress in real-time via SSE.
        #[arg(long)]
        stream: bool,

        /// Submit the task and return immediately without waiting for the result.
        ///
        /// The task ID is printed so it can be tracked with `apollia-os task status <id>`.
        #[arg(long)]
        detach: bool,

        /// Display two alternative plans (conservative vs. exploratory) and choose
        /// which one to execute before submitting the task.
        ///
        /// Requires the runtime to support plan alternatives (ORIA engine with LLM).
        #[arg(long)]
        alternatives: bool,

        /// Restrict this session to the listed tools only (comma-separated).
        ///
        /// When specified, only the named tools can be invoked. All other tools
        /// are blocked regardless of the global configuration.
        #[arg(long, value_delimiter = ',', value_name = "TOOL")]
        allowed_tools: Vec<String>,

        /// Explicitly block the listed tools for this session (comma-separated).
        ///
        /// Takes priority over `--allowed-tools` when the same tool appears in both.
        #[arg(long, value_delimiter = ',', value_name = "TOOL")]
        disallowed_tools: Vec<String>,
    },

    /// OAuth2 PKCE authentication management (login, status, logout).
    Auth {
        /// Auth subcommand.
        #[command(subcommand)]
        command: AuthCommand,
    },

    /// Agent management (list, start, stop, info, install, uninstall, enable, disable, update, new, package, logs, validate, repair).
    Agent {
        /// Agent subcommand.
        #[command(subcommand)]
        command: AgentCommand,
    },

    /// Agent-to-Agent skill discovery and direct invocation.
    ///
    /// Worker agents expose typed skills (not free-text prompts). Use these
    /// sub-commands to discover skills (`a2a skills`) and invoke one with a
    /// structured payload (`a2a invoke <skill_id> --args '<JSON>'`).
    A2a {
        /// A2A subcommand.
        #[command(subcommand)]
        command: A2aCommand,
    },

    /// Task management (list, status, cancel, inspect, resume, approvals).
    Task {
        /// Task subcommand.
        #[command(subcommand)]
        command: TaskCommand,
    },

    /// Native tool governance (list, enable, disable, config, reload, credentials, describe, approvals).
    Tools {
        /// Tools subcommand.
        #[command(subcommand)]
        command: ToolsCommand,
    },

    /// Audit trail (list, stats, export).
    Audit {
        /// Audit subcommand.
        #[command(subcommand)]
        command: AuditCommand,
    },

    /// Memory management.
    Memory {
        /// Memory subcommand.
        #[command(subcommand)]
        command: MemoryCommand,
    },

    /// LLM backend diagnostics (status, ping, chat, costs, backends, reload).
    Llm {
        /// LLM subcommand.
        #[command(subcommand)]
        command: LlmCommand,
    },

    /// Local model file management.
    Model {
        /// Model subcommand.
        #[command(subcommand)]
        command: ModelCommand,
    },

    /// Trigger management (list, status, fire, enable, disable, logs, reload, create, update, delete).
    Trigger {
        /// Trigger subcommand.
        #[command(subcommand)]
        command: TriggerCommand,
    },

    /// Notification channel management (test, list, logs, create, update, delete, events).
    Notify {
        /// Notify subcommand.
        #[command(subcommand)]
        command: NotifyCommand,
    },

    /// Speech-to-Text management (status, transcribe, transcriptions, model, config).
    Stt {
        /// STT subcommand.
        #[command(subcommand)]
        command: SttCommand,
    },

    /// Launch onboarding or re-onboarding on a specific topic.
    Onboard {
        /// Focus on a specific topic (identity, preferences, tools, domain, agents).
        #[arg(long)]
        topic: Option<String>,
    },

    /// Permission rule management (list, revoke, audit).
    Permissions {
        /// Permissions subcommand.
        #[command(subcommand)]
        command: PermissionsCommand,
    },

    /// Interactive chat REPL + persisted session hygiene (delete, rename, export).
    ///
    /// Without a subcommand: launches the REPL (resume with `--resume <id>`,
    /// or list recent sessions with `--list`). With a subcommand: operates on
    /// `~/.apollia/chat.db` directly — no runtime required.
    Chat {
        /// Resume an existing session from its last message.
        #[arg(long, value_name = "SESSION_ID")]
        resume: Option<String>,
        /// List the 10 most recent sessions.
        #[arg(long)]
        list: bool,
        /// Optional hygiene subcommand (delete | rename | export). When
        /// omitted, the REPL launches.
        #[command(subcommand)]
        command: Option<commands::chat::ChatHygieneCommand>,
    },

    /// MCP server management (list, add, remove, get, test, restart, update, raw-config, set-approval, list-pending, revoke-approval).
    Mcp {
        /// MCP subcommand.
        #[command(subcommand)]
        command: McpCommand,
    },

    /// Launch Apollia as an MCP stdio server for external clients.
    ///
    /// Exposes 9 native tools (bash_executor, file_*, mcp_client, agent_install)
    /// to MCP clients such as Claude Desktop, VS Code Copilot Chat, or Cursor.
    /// Use `--with-runtime` to additionally expose `submit_task`.
    McpServer(McpServerArgs),

    /// Check for and install updates from GitHub Releases.
    Update(commands::update::UpdateArgs),

    /// Workspace inspection and initialization (status, init).
    Workspace {
        /// Workspace subcommand.
        #[command(subcommand)]
        command: WorkspaceCommand,
    },

    /// Automated code or plan review via the apollia-review agent.
    Review(commands::review::ReviewArgs),

    /// Revert filesystem mutations recorded by the agent during a chat session.
    ///
    /// Reads the reversible journal at `~/.apollia/journal/<session-id>/` and
    /// replays the inverse of every native mutation in reverse order.
    /// Use `--dry-run` to preview, `--list` to enumerate available sessions.
    Rollback(commands::rollback::RollbackArgs),

    /// Circuit breaker inspection and reset (list, show, reset).
    Resilience {
        /// Resilience subcommand.
        #[command(subcommand)]
        command: ResilienceCommand,
    },

    /// Plan cache management (stats, clear, evict).
    PlanCache {
        /// Plan-cache subcommand.
        #[command(subcommand)]
        command: PlanCacheCommand,
    },

    /// Diagnose the local Apollia environment (no runtime required).
    ///
    /// Verifies the Apollia home directory, the config file, local SQLite
    /// databases, the Python bridge, and the runtime socket. Prints a status
    /// table and exits 0 when healthy, 1 when at least one check errors.
    Doctor,

    /// Tail or follow the runtime log file.
    ///
    /// Defaults to `~/.apollia/logs/runtime.log`. When this file is absent
    /// the command prints a hint explaining how to redirect daemon stderr.
    Logs(commands::logs::LogsArgs),

    /// Print the binary version (use `--json` for machine-readable output).
    Version,

    /// Native SaaS connector management (list, accounts, test, revoke).
    ///
    /// Operates on the multi-account keyring without requiring the runtime
    /// to be started. Use `apollia-os auth login <provider>` first to
    /// connect an account.
    Connector {
        /// Connector subcommand.
        #[command(subcommand)]
        command: ConnectorCommand,
    },

    /// Global apollia.toml management (get, set, validate, edit, show).
    ///
    /// Edits the on-disk config without touching the runtime. The section
    /// helpers `tools config` and `stt config` remain available for their
    /// respective slices.
    Config {
        /// Config subcommand.
        #[command(subcommand)]
        command: ConfigCommand,
    },

    /// User memory / profile management (show, set, forget, reset, export, import).
    ///
    /// Operates on `~/.apollia/user_memory.db` directly; no runtime required.
    #[command(name = "user-memory")]
    UserMemory {
        /// User-memory subcommand.
        #[command(subcommand)]
        command: UserMemoryCommand,
    },

    /// Shortcut for `task list --pending-approval` — list pending HITL tasks.
    Hitl,

    /// Project management (list, create, show, update, delete, agents, templates).
    ///
    /// Operates locally on `~/.apollia/projects.db`; the runtime does not need
    /// to be running.
    Project {
        /// Project subcommand.
        #[command(subcommand)]
        command: ProjectCommand,
    },

    /// Print the event-sourced trace of a task (ADR-088).
    Trace {
        /// Task identifier.
        task_id: String,
        /// Force JSON output even without global `--json`.
        #[arg(long, value_name = "FORMAT", value_parser = ["human", "json"], default_value = "human")]
        format: String,
    },

    /// Aggregated activity overview (tasks + LLM costs + audit stats).
    Digest {
        /// Time window: 24h, 7d, or 30d.
        #[arg(long, default_value = "24h")]
        since: commands::digest::DigestWindow,
    },

    /// Manage the Chat Libre configuration (system prompt, allowed tools, backend).
    ///
    /// Operates on `governance.db` directly; the runtime does not need to be
    /// running. The session permission rules are managed via the standard
    /// `apollia-os permissions` commands.
    #[command(name = "chat-config")]
    ChatConfig {
        /// Chat-config subcommand.
        #[command(subcommand)]
        command: commands::chat_config::ChatConfigCommand,
    },
}

fn main() {
    let cli = Cli::parse();

    // Apply --no-color before tracing init so the subscriber respects it.
    if cli.no_color {
        std::env::set_var("NO_COLOR", "1");
    }

    // Choose tracing level based on global verbosity flags.
    // RUST_LOG (if set by the user) takes priority via `try_from_default_env`.
    let filter_str = if cli.debug {
        "debug"
    } else if cli.verbose {
        "apollia=debug,info"
    } else if cli.quiet {
        "warn"
    } else {
        "apollia=info"
    };
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter_str));

    if cli.no_color {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_target(false)
            .with_ansi(false)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_target(false)
            .init();
    }

    let rt = tokio::runtime::Runtime::new().expect("failed to create Tokio runtime");

    let json = cli.json;
    let quiet = cli.quiet;
    let exit_code = rt.block_on(async {
        match cli.command {
            Commands::Start { port } => match commands::start::run(cli.socket, port).await {
                Ok(interrupted) => {
                    if interrupted {
                        exit_codes::INTERRUPTED
                    } else {
                        exit_codes::SUCCESS
                    }
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    exit_codes::GENERAL_ERROR
                }
            },
            Commands::Stop => commands::stop::run(cli.socket, json).await,
            Commands::Status => commands::status::run(cli.socket, json).await,
            Commands::Run {
                agent_id,
                input,
                input_json,
                stream,
                detach,
                alternatives,
                allowed_tools,
                disallowed_tools,
            } => {
                if input.is_empty() && input_json.is_none() {
                    eprintln!("Error: missing task input.");
                    eprintln!("Usage:");
                    eprintln!("  apollia-os run <AGENT_ID> \"<free-text input>\"");
                    eprintln!("  apollia-os run <AGENT_ID> --input-json '<JSON>'");
                    eprintln!("  apollia-os a2a invoke <SKILL_ID> --args '<JSON>'   # for workers");
                    return exit_codes::GENERAL_ERROR;
                }
                commands::run::run(commands::run::RunCommandArgs {
                    agent_id: &agent_id,
                    input: &input,
                    input_json: input_json.as_deref(),
                    socket: cli.socket,
                    json,
                    stream,
                    detach,
                    alternatives,
                    allowed_tools,
                    disallowed_tools,
                })
                .await
            }
            Commands::Auth { command } => commands::auth::run(&command, json).await,
            Commands::Agent { command } => {
                commands::agent::run(&command, cli.socket, json, quiet).await
            }
            Commands::A2a { command } => commands::a2a::run(&command, cli.socket, json).await,
            Commands::Task { command } => commands::task::run(&command, cli.socket, json).await,
            Commands::Tools { command } => commands::tools::run(&command, cli.socket, json).await,
            Commands::Audit { command } => commands::audit::run(&command, cli.socket, json).await,
            Commands::Memory { command } => match commands::memory::run(&command, json) {
                Ok(output) => {
                    println!("{output}");
                    exit_codes::SUCCESS
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    exit_codes::GENERAL_ERROR
                }
            },
            Commands::Llm { command } => commands::llm::run(&command, cli.socket, json).await,
            Commands::Model { command } => commands::model::run(&command, cli.socket, json).await,
            Commands::Trigger { command } => {
                commands::trigger::run(&command, cli.socket, json).await
            }
            Commands::Notify { command } => commands::notify::run(&command, cli.socket, json).await,
            Commands::Stt { command } => commands::stt::run(&command, cli.socket, json).await,
            Commands::Onboard { topic } => {
                commands::onboard::run(topic.as_deref(), cli.socket, json).await
            }
            Commands::Permissions { command } => {
                commands::permissions::run(&command, cli.socket, json).await
            }
            Commands::Chat {
                resume,
                list,
                command,
            } => match command {
                Some(sub) => chat::run_hygiene(&sub, json),
                None => chat::run(resume.as_deref(), list, cli.socket, json).await,
            },
            Commands::Mcp { command } => commands::mcp::run(&command, cli.socket, json).await,
            Commands::McpServer(args) => match commands::mcp_server::run(&args).await {
                Ok(()) => exit_codes::SUCCESS,
                Err(e) => {
                    eprintln!("Error: {e}");
                    exit_codes::GENERAL_ERROR
                }
            },
            Commands::Update(args) => commands::update::run(&args, "apollia-os").await,
            Commands::Workspace { command } => commands::workspace::run(&command, json).await,
            Commands::Review(args) => commands::review::run(&args, cli.socket, json).await,
            Commands::Rollback(args) => commands::rollback::run(&args, json).await,
            Commands::Resilience { command } => {
                commands::resilience::run(&command, cli.socket, json).await
            }
            Commands::PlanCache { command } => commands::plan_cache::run(&command, json),
            Commands::Doctor => commands::doctor::run(cli.socket, json).await,
            Commands::Logs(args) => commands::logs::run(&args, json).await,
            Commands::Version => commands::version::run(json),
            Commands::Connector { command } => commands::connector::run(&command, json).await,
            Commands::Config { command } => commands::config::run(&command, json),
            Commands::UserMemory { command } => commands::user_memory::run(&command, json),
            Commands::Hitl => {
                let cmd = commands::task::TaskCommand::List {
                    pending_approval: true,
                };
                commands::task::run(&cmd, cli.socket, json).await
            }
            Commands::Project { command } => commands::project::run(&command, json),
            Commands::Trace { task_id, format } => {
                let format_json = format == "json";
                commands::trace::run(&task_id, format_json, cli.socket, json).await
            }
            Commands::Digest { since } => commands::digest::run(since, cli.socket, json).await,
            Commands::ChatConfig { command } => commands::chat_config::run(&command, json),
        }
    });

    std::process::exit(exit_code);
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use commands::agent::AgentCommand;
    use commands::audit::AuditCommand;
    use commands::llm::LlmCommand;
    use commands::memory::MemoryCommand;
    use commands::model::ModelCommand;
    use commands::notify::NotifyCommand;
    use commands::stt::{SttCommand, SttModelCommand, TranscriptionsCommand};
    use commands::task::TaskCommand;
    use commands::tools::ToolsCommand;
    use commands::trigger::TriggerCommand;
    use commands::workspace::WorkspaceCommand;

    fn parse(args: &[&str]) -> Cli {
        Cli::parse_from(args)
    }

    #[test]
    fn test_cli_parses_start_command() {
        // GIVEN "apollia-os start"
        // WHEN parse
        let cli = parse(&["apollia-os", "start"]);
        // THEN Commands::Start
        assert!(matches!(cli.command, Commands::Start { port: None }));
    }

    #[test]
    fn test_cli_parses_start_with_port() {
        // GIVEN "apollia-os start --port 8080"
        let cli = parse(&["apollia-os", "start", "--port", "8080"]);
        // THEN Commands::Start with port
        assert!(matches!(cli.command, Commands::Start { port: Some(8080) }));
    }

    #[test]
    fn test_cli_parses_stop_command() {
        // GIVEN "apollia-os stop"
        let cli = parse(&["apollia-os", "stop"]);
        // THEN Commands::Stop, json=false (global flag not set)
        assert!(matches!(cli.command, Commands::Stop));
        assert!(!cli.json);
    }

    #[test]
    fn test_cli_parses_stop_json() {
        // GIVEN "apollia-os stop --json"
        let cli = parse(&["apollia-os", "stop", "--json"]);
        // THEN Commands::Stop, global json=true
        assert!(matches!(cli.command, Commands::Stop));
        assert!(cli.json);
    }

    #[test]
    fn test_cli_parses_status_command() {
        // GIVEN "apollia-os status"
        let cli = parse(&["apollia-os", "status"]);
        // THEN Commands::Status, json=false
        assert!(matches!(cli.command, Commands::Status));
        assert!(!cli.json);
    }

    #[test]
    fn test_cli_parses_status_json_flag() {
        // GIVEN "apollia-os status --json"
        let cli = parse(&["apollia-os", "status", "--json"]);
        // THEN Commands::Status, global json=true
        assert!(matches!(cli.command, Commands::Status));
        assert!(cli.json);
    }

    #[test]
    fn test_cli_parses_run_command() {
        // GIVEN "apollia-os run hello-agent Bonjour"
        let cli = parse(&["apollia-os", "run", "hello-agent", "Bonjour"]);
        // THEN Commands::Run with agent_id and input
        match &cli.command {
            Commands::Run {
                agent_id,
                input,
                stream,
                detach,
                ..
            } => {
                assert_eq!(agent_id, "hello-agent");
                assert_eq!(input, "Bonjour");
                assert!(!stream);
                assert!(!detach);
                assert!(!cli.json);
            }
            other => panic!("expected Commands::Run, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_run_stream_flag() {
        // GIVEN "apollia-os run hello-agent Bonjour --stream"
        let cli = parse(&["apollia-os", "run", "hello-agent", "Bonjour", "--stream"]);
        // THEN Commands::Run with stream=true
        match &cli.command {
            Commands::Run { stream, .. } => assert!(stream),
            other => panic!("expected Commands::Run, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_run_detach_flag() {
        // GIVEN "apollia-os run hello-agent Bonjour --detach"
        let cli = parse(&["apollia-os", "run", "hello-agent", "Bonjour", "--detach"]);
        // THEN Commands::Run with detach=true, stream=false
        match &cli.command {
            Commands::Run { detach, stream, .. } => {
                assert!(detach);
                assert!(!stream);
            }
            other => panic!("expected Commands::Run, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_run_detach_flag_after_input() {
        // GIVEN — detach appears after the positional arguments (the pattern from ONBOARDING)
        let cli = parse(&[
            "apollia-os",
            "run",
            "apollia-reviewer",
            "/some/path",
            "--detach",
        ]);
        // THEN detach=true, agent_id and input are correctly captured
        match &cli.command {
            Commands::Run {
                agent_id,
                input,
                detach,
                ..
            } => {
                assert_eq!(agent_id, "apollia-reviewer");
                assert_eq!(input, "/some/path");
                assert!(detach);
            }
            other => panic!("expected Commands::Run, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_run_json_flag() {
        // GIVEN "apollia-os run hello-agent Bonjour --json"
        let cli = parse(&["apollia-os", "run", "hello-agent", "Bonjour", "--json"]);
        // THEN global json=true
        assert!(matches!(cli.command, Commands::Run { .. }));
        assert!(cli.json);
    }

    #[test]
    fn test_cli_global_socket_flag() {
        // GIVEN "apollia-os --socket /custom/path.sock status"
        let cli = parse(&["apollia-os", "--socket", "/custom/path.sock", "status"]);
        // THEN socket is set
        assert_eq!(cli.socket, Some(PathBuf::from("/custom/path.sock")));
    }

    #[test]
    fn test_exit_codes_constants() {
        // GIVEN the exit codes
        // THEN they follow POSIX convention
        assert_eq!(exit_codes::SUCCESS, 0);
        assert_eq!(exit_codes::GENERAL_ERROR, 1);
        assert_eq!(exit_codes::RUNTIME_ERROR, 2);
        assert_eq!(exit_codes::TASK_FAILED, 3);
        assert_eq!(exit_codes::TIMEOUT, 4);
        // FC09: Ctrl+C / SIGINT must map to exit code 5
        assert_eq!(exit_codes::INTERRUPTED, 5);
    }

    // FC09: SIGINT → exit code 5 mapping logic
    #[test]
    fn test_interrupted_exit_code_is_distinct_from_success() {
        // GIVEN a SIGINT-triggered shutdown (simulated as Ok(true))
        // WHEN mapping the result to an exit code
        // THEN INTERRUPTED (5) is returned, not SUCCESS (0)
        let was_interrupted = true;
        let code = if was_interrupted {
            exit_codes::INTERRUPTED
        } else {
            exit_codes::SUCCESS
        };
        assert_eq!(code, 5);
        assert_ne!(code, exit_codes::SUCCESS);
    }

    // FC10: global flag parsing
    #[test]
    fn test_cli_parses_quiet_flag() {
        // GIVEN "apollia-os --quiet agent list"
        // WHEN parse
        let cli = parse(&["apollia-os", "--quiet", "agent", "list"]);
        // THEN quiet=true, json=false
        assert!(cli.quiet);
        assert!(!cli.json);
    }

    #[test]
    fn test_cli_parses_verbose_flag() {
        // GIVEN "apollia-os --verbose status"
        // WHEN parse
        let cli = parse(&["apollia-os", "--verbose", "status"]);
        // THEN verbose=true
        assert!(cli.verbose);
        assert!(!cli.debug);
    }

    #[test]
    fn test_cli_parses_debug_flag() {
        // GIVEN "apollia-os --debug run mon-agent '...'"
        // WHEN parse
        let cli = parse(&["apollia-os", "--debug", "run", "mon-agent", "task"]);
        // THEN debug=true
        assert!(cli.debug);
    }

    #[test]
    fn test_cli_parses_no_color_flag() {
        // GIVEN "apollia-os --no-color status"
        // WHEN parse
        let cli = parse(&["apollia-os", "--no-color", "status"]);
        // THEN no_color=true → no ANSI codes in output
        assert!(cli.no_color);
        assert!(!cli.json);
    }

    #[test]
    fn test_cli_quiet_and_json_together() {
        // GIVEN "apollia-os --quiet --json agent list"
        // WHEN parse
        let cli = parse(&["apollia-os", "--quiet", "--json", "agent", "list"]);
        // THEN both flags are set — handlers must give --json priority
        assert!(cli.quiet);
        assert!(cli.json);
    }

    #[test]
    fn test_cli_quiet_flag_short_form() {
        // GIVEN "apollia-os -q status"
        // WHEN parse
        let cli = parse(&["apollia-os", "-q", "status"]);
        // THEN quiet=true (short form -q accepted)
        assert!(cli.quiet);
    }

    #[test]
    fn test_cli_verbose_flag_short_form() {
        // GIVEN "apollia-os -v status"
        // WHEN parse
        let cli = parse(&["apollia-os", "-v", "status"]);
        // THEN verbose=true (short form -v accepted)
        assert!(cli.verbose);
    }

    // --- Level-2 command parsing tests ---

    #[test]
    fn test_cli_parses_agent_list() {
        // GIVEN "apollia-os agent list"
        // WHEN parse
        let cli = parse(&["apollia-os", "agent", "list"]);
        // THEN Commands::Agent { command: AgentCommand::List }
        match &cli.command {
            Commands::Agent { command } => {
                assert!(matches!(
                    command,
                    AgentCommand::List {
                        supports_a2a: false
                    }
                ));
                assert!(!cli.json);
            }
            other => panic!("expected Commands::Agent, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_agent_start() {
        // GIVEN "apollia-os agent start /path/to/agent.py"
        // WHEN parse
        let cli = parse(&["apollia-os", "agent", "start", "/path/to/agent.py"]);
        // THEN Commands::Agent { command: AgentCommand::Start { path } }
        match &cli.command {
            Commands::Agent { command } => match command {
                AgentCommand::Start { path } => {
                    assert_eq!(path, "/path/to/agent.py");
                }
                other => panic!("expected AgentCommand::Start, got {other:?}"),
            },
            other => panic!("expected Commands::Agent, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_agent_stop() {
        // GIVEN "apollia-os agent stop hello-agent"
        // WHEN parse
        let cli = parse(&["apollia-os", "agent", "stop", "hello-agent"]);
        // THEN Commands::Agent { command: AgentCommand::Stop { agent_id } }
        match &cli.command {
            Commands::Agent { command } => match command {
                AgentCommand::Stop { agent_id } => {
                    assert_eq!(agent_id, "hello-agent");
                }
                other => panic!("expected AgentCommand::Stop, got {other:?}"),
            },
            other => panic!("expected Commands::Agent, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_agent_info() {
        // GIVEN "apollia-os agent info hello-agent"
        // WHEN parse
        let cli = parse(&["apollia-os", "agent", "info", "hello-agent"]);
        // THEN Commands::Agent { command: AgentCommand::Info { agent_id } }
        match &cli.command {
            Commands::Agent { command } => match command {
                AgentCommand::Info { agent_id } => {
                    assert_eq!(agent_id, "hello-agent");
                }
                other => panic!("expected AgentCommand::Info, got {other:?}"),
            },
            other => panic!("expected Commands::Agent, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_agent_status() {
        let cli = parse(&["apollia-os", "agent", "status", "hello"]);
        match &cli.command {
            Commands::Agent { command } => match command {
                AgentCommand::Status { agent_id } => assert_eq!(agent_id, "hello"),
                other => panic!("expected AgentCommand::Status, got {other:?}"),
            },
            other => panic!("expected Commands::Agent, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_agent_messages_default_limit() {
        let cli = parse(&["apollia-os", "agent", "messages", "hello"]);
        match &cli.command {
            Commands::Agent { command } => match command {
                AgentCommand::Messages { agent_id, limit } => {
                    assert_eq!(agent_id, "hello");
                    assert_eq!(*limit, 20);
                }
                other => panic!("expected AgentCommand::Messages, got {other:?}"),
            },
            other => panic!("expected Commands::Agent, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_agent_messages_with_limit() {
        let cli = parse(&["apollia-os", "agent", "messages", "hello", "--limit", "5"]);
        match &cli.command {
            Commands::Agent { command } => match command {
                AgentCommand::Messages { agent_id, limit } => {
                    assert_eq!(agent_id, "hello");
                    assert_eq!(*limit, 5);
                }
                other => panic!("expected AgentCommand::Messages, got {other:?}"),
            },
            other => panic!("expected Commands::Agent, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_agent_install() {
        // GIVEN "apollia-os agent install ./agents/mon-agent.py"
        // WHEN parse
        let cli = parse(&["apollia-os", "agent", "install", "./agents/mon-agent.py"]);
        // THEN Commands::Agent { command: AgentCommand::Install { path } }
        match &cli.command {
            Commands::Agent { command } => match command {
                AgentCommand::Install { source, .. } => {
                    assert_eq!(source, "./agents/mon-agent.py");
                }
                other => panic!("expected AgentCommand::Install, got {other:?}"),
            },
            other => panic!("expected Commands::Agent, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_agent_uninstall() {
        // GIVEN "apollia-os agent uninstall mon-agent"
        // WHEN parse
        let cli = parse(&["apollia-os", "agent", "uninstall", "mon-agent"]);
        // THEN Commands::Agent { command: AgentCommand::Uninstall { name } }
        match &cli.command {
            Commands::Agent { command } => match command {
                AgentCommand::Uninstall { name } => {
                    assert_eq!(name, "mon-agent");
                }
                other => panic!("expected AgentCommand::Uninstall, got {other:?}"),
            },
            other => panic!("expected Commands::Agent, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_agent_enable() {
        // GIVEN "apollia-os agent enable mon-agent"
        let cli = parse(&["apollia-os", "agent", "enable", "mon-agent"]);
        match &cli.command {
            Commands::Agent { command } => match command {
                AgentCommand::Enable { name } => assert_eq!(name, "mon-agent"),
                other => panic!("expected AgentCommand::Enable, got {other:?}"),
            },
            other => panic!("expected Commands::Agent, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_agent_disable() {
        // GIVEN "apollia-os agent disable mon-agent"
        let cli = parse(&["apollia-os", "agent", "disable", "mon-agent"]);
        match &cli.command {
            Commands::Agent { command } => match command {
                AgentCommand::Disable { name } => assert_eq!(name, "mon-agent"),
                other => panic!("expected AgentCommand::Disable, got {other:?}"),
            },
            other => panic!("expected Commands::Agent, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_agent_update() {
        // GIVEN "apollia-os agent update mon-agent ./agents/v2.py"
        let cli = parse(&[
            "apollia-os",
            "agent",
            "update",
            "mon-agent",
            "./agents/v2.py",
        ]);
        match &cli.command {
            Commands::Agent { command } => match command {
                AgentCommand::Update { name, path } => {
                    assert_eq!(name, "mon-agent");
                    assert_eq!(path, &PathBuf::from("./agents/v2.py"));
                }
                other => panic!("expected AgentCommand::Update, got {other:?}"),
            },
            other => panic!("expected Commands::Agent, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_agent_json_flag_before_verb() {
        // GIVEN "apollia-os agent --json list" (--json avant le verbe, syntaxe historique)
        // WHEN parse
        let cli = parse(&["apollia-os", "agent", "--json", "list"]);
        // THEN global json=true
        assert!(matches!(cli.command, Commands::Agent { .. }));
        assert!(cli.json);
    }

    #[test]
    fn test_cli_parses_agent_json_flag_after_arg() {
        // GIVEN "apollia-os agent info hello-agent --json" (--json après l'arg positionnel)
        // WHEN parse
        let cli = parse(&["apollia-os", "agent", "info", "hello-agent", "--json"]);
        // THEN global json=true — la forme attendue dans l'ONBOARDING
        assert!(matches!(cli.command, Commands::Agent { .. }));
        assert!(cli.json);
    }

    #[test]
    fn test_cli_parses_task_list() {
        // GIVEN "apollia-os task list"
        // WHEN parse
        let cli = parse(&["apollia-os", "task", "list"]);
        // THEN Commands::Task { command: TaskCommand::List }
        match &cli.command {
            Commands::Task { command } => {
                assert!(
                    matches!(
                        command,
                        TaskCommand::List {
                            pending_approval: false
                        }
                    ),
                    "expected TaskCommand::List {{ pending_approval: false }}, got {command:?}"
                );
                assert!(!cli.json);
            }
            other => panic!("expected Commands::Task, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_task_cancel() {
        // GIVEN "apollia-os task cancel t-001"
        // WHEN parse
        let cli = parse(&["apollia-os", "task", "cancel", "t-001"]);
        // THEN Commands::Task { command: TaskCommand::Cancel { task_id: "t-001" } }
        match &cli.command {
            Commands::Task { command } => match command {
                TaskCommand::Cancel { task_id } => {
                    assert_eq!(task_id, "t-001");
                }
                other => panic!("expected TaskCommand::Cancel, got {other:?}"),
            },
            other => panic!("expected Commands::Task, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_task_status() {
        // GIVEN "apollia-os task status t-002"
        // WHEN parse
        let cli = parse(&["apollia-os", "task", "status", "t-002"]);
        // THEN Commands::Task { command: TaskCommand::Status { task_id: "t-002" } }
        match &cli.command {
            Commands::Task { command } => match command {
                TaskCommand::Status { task_id } => {
                    assert_eq!(task_id, "t-002");
                }
                other => panic!("expected TaskCommand::Status, got {other:?}"),
            },
            other => panic!("expected Commands::Task, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_task_inspect() {
        // GIVEN "apollia-os task inspect t-0042"
        // WHEN parse
        let cli = parse(&["apollia-os", "task", "inspect", "t-0042"]);
        // THEN Commands::Task { command: TaskCommand::Inspect { id: "t-0042" } }
        match &cli.command {
            Commands::Task { command } => {
                match command {
                    TaskCommand::Inspect { id } => assert_eq!(id, "t-0042"),
                    other => panic!("expected TaskCommand::Inspect, got {other:?}"),
                }
                assert!(!cli.json);
            }
            other => panic!("expected Commands::Task, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_task_inspect_json_flag() {
        // GIVEN "apollia-os task --json inspect t-0042"
        // WHEN parse
        let cli = parse(&["apollia-os", "task", "--json", "inspect", "t-0042"]);
        // THEN global json = true
        assert!(matches!(cli.command, Commands::Task { .. }));
        assert!(cli.json);
    }

    #[test]
    fn test_cli_parses_tools_list() {
        // GIVEN "apollia-os tools list"
        // WHEN parse
        let cli = parse(&["apollia-os", "tools", "list"]);
        // THEN Commands::Tools { command: ToolsCommand::List }
        match &cli.command {
            Commands::Tools { command } => {
                assert!(matches!(command, ToolsCommand::List));
                assert!(!cli.json);
            }
            other => panic!("expected Commands::Tools, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_tools_describe() {
        // GIVEN "apollia-os tools describe file_io"
        // WHEN parse
        let cli = parse(&["apollia-os", "tools", "describe", "file_io"]);
        // THEN Commands::Tools { command: ToolsCommand::Describe { tool_name: "file_io" } }
        match &cli.command {
            Commands::Tools { command } => match command {
                ToolsCommand::Describe { tool_name } => {
                    assert_eq!(tool_name, "file_io");
                }
                other => panic!("expected ToolsCommand::Describe, got {other:?}"),
            },
            other => panic!("expected Commands::Tools, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_audit_list_default() {
        // GIVEN "apollia-os audit list"
        // WHEN parse
        let cli = parse(&["apollia-os", "audit", "list"]);
        // THEN Commands::Audit { command: AuditCommand::List { limit: 20 } }
        match &cli.command {
            Commands::Audit { command } => {
                match command {
                    AuditCommand::List { limit } => assert_eq!(*limit, 20),
                    other => panic!("expected AuditCommand::List, got {other:?}"),
                }
                assert!(!cli.json);
            }
            other => panic!("expected Commands::Audit, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_audit_list_custom_limit() {
        // GIVEN "apollia-os audit list --limit 50"
        // WHEN parse
        let cli = parse(&["apollia-os", "audit", "list", "--limit", "50"]);
        // THEN Commands::Audit { command: AuditCommand::List { limit: 50 } }
        match &cli.command {
            Commands::Audit { command } => match command {
                AuditCommand::List { limit } => assert_eq!(*limit, 50),
                other => panic!("expected AuditCommand::List, got {other:?}"),
            },
            other => panic!("expected Commands::Audit, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_audit_stats() {
        // GIVEN "apollia-os audit stats"
        // WHEN parse
        let cli = parse(&["apollia-os", "audit", "stats"]);
        // THEN Commands::Audit { command: AuditCommand::Stats }
        match &cli.command {
            Commands::Audit { command } => {
                assert!(matches!(command, AuditCommand::Stats));
            }
            other => panic!("expected Commands::Audit, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_memory_still_works() {
        // GIVEN "apollia-os memory inspect test-ns"
        // WHEN parse
        let cli = parse(&["apollia-os", "memory", "inspect", "test-ns"]);
        // THEN Commands::Memory preserved
        match &cli.command {
            Commands::Memory { command } => match command {
                MemoryCommand::Inspect { namespace, .. } => {
                    assert_eq!(namespace, "test-ns");
                }
                other => panic!("expected MemoryCommand::Inspect, got {other:?}"),
            },
            other => panic!("expected Commands::Memory, got {other:?}"),
        }
    }

    // ── llm / model command parsing ───────────────────────────────

    #[test]
    fn test_cli_parses_llm_status() {
        // GIVEN "apollia-os llm status"
        // WHEN parse
        let cli = parse(&["apollia-os", "llm", "status"]);
        // THEN Commands::Llm { command: LlmCommand::Status }
        match &cli.command {
            Commands::Llm { command } => {
                assert!(matches!(command, LlmCommand::Status));
                assert!(!cli.json);
            }
            other => panic!("expected Commands::Llm, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_llm_status_json_flag() {
        // GIVEN "apollia-os llm --json status"
        // WHEN parse
        let cli = parse(&["apollia-os", "llm", "--json", "status"]);
        // THEN global json = true
        assert!(matches!(cli.command, Commands::Llm { .. }));
        assert!(cli.json);
    }

    #[test]
    fn test_cli_parses_llm_ping_no_backend() {
        // GIVEN "apollia-os llm ping"
        // WHEN parse
        let cli = parse(&["apollia-os", "llm", "ping"]);
        // THEN LlmCommand::Ping { backend: None }
        match &cli.command {
            Commands::Llm { command } => match command {
                LlmCommand::Ping { backend } => assert!(backend.is_none()),
                other => panic!("expected LlmCommand::Ping, got {other:?}"),
            },
            other => panic!("expected Commands::Llm, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_llm_ping_with_backend() {
        // GIVEN "apollia-os llm ping anthropic"
        // WHEN parse
        let cli = parse(&["apollia-os", "llm", "ping", "anthropic"]);
        // THEN LlmCommand::Ping { backend: Some("anthropic") }
        match &cli.command {
            Commands::Llm { command } => match command {
                LlmCommand::Ping { backend } => {
                    assert_eq!(backend.as_deref(), Some("anthropic"));
                }
                other => panic!("expected LlmCommand::Ping, got {other:?}"),
            },
            other => panic!("expected Commands::Llm, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_llm_chat() {
        // GIVEN "apollia-os llm chat 'Hello'"
        // WHEN parse
        let cli = parse(&["apollia-os", "llm", "chat", "Hello"]);
        // THEN LlmCommand::Chat { prompt: "Hello", backend: None }
        match &cli.command {
            Commands::Llm { command } => match command {
                LlmCommand::Chat { prompt, backend } => {
                    assert_eq!(prompt, "Hello");
                    assert!(backend.is_none());
                }
                other => panic!("expected LlmCommand::Chat, got {other:?}"),
            },
            other => panic!("expected Commands::Llm, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_llm_chat_with_backend() {
        // GIVEN "apollia-os llm chat 'Hello' --backend anthropic"
        // WHEN parse
        let cli = parse(&[
            "apollia-os",
            "llm",
            "chat",
            "Hello",
            "--backend",
            "anthropic",
        ]);
        // THEN LlmCommand::Chat { backend: Some("anthropic") }
        match &cli.command {
            Commands::Llm { command } => match command {
                LlmCommand::Chat { backend, .. } => {
                    assert_eq!(backend.as_deref(), Some("anthropic"));
                }
                other => panic!("expected LlmCommand::Chat, got {other:?}"),
            },
            other => panic!("expected Commands::Llm, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_model_list() {
        // GIVEN "apollia-os model list"
        // WHEN parse
        let cli = parse(&["apollia-os", "model", "list"]);
        // THEN Commands::Model { command: ModelCommand::List }
        match &cli.command {
            Commands::Model { command } => {
                assert!(matches!(command, ModelCommand::List));
                assert!(!cli.json);
            }
            other => panic!("expected Commands::Model, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_model_list_json_flag() {
        // GIVEN "apollia-os model --json list"
        // WHEN parse
        let cli = parse(&["apollia-os", "model", "--json", "list"]);
        // THEN global json = true
        assert!(matches!(cli.command, Commands::Model { .. }));
        assert!(cli.json);
    }

    // ── trigger command parsing ───────────────────────────────────

    #[test]
    fn test_cli_parses_trigger_reload() {
        // GIVEN "apollia-os trigger reload"
        // WHEN parse
        let cli = parse(&["apollia-os", "trigger", "reload"]);
        // THEN Commands::Trigger { command: TriggerCommand::Reload }, json=false
        match &cli.command {
            Commands::Trigger { command } => {
                assert!(matches!(command, TriggerCommand::Reload));
                assert!(!cli.json);
            }
            other => panic!("expected Commands::Trigger, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_trigger_reload_json_flag() {
        // GIVEN "apollia-os trigger --json reload"
        // WHEN parse
        let cli = parse(&["apollia-os", "trigger", "--json", "reload"]);
        // THEN global json = true
        assert!(matches!(cli.command, Commands::Trigger { .. }));
        assert!(cli.json);
    }

    // ── notify command parsing ─────────────────────────────────────

    #[test]
    fn test_cli_parses_notify_test() {
        // GIVEN "apollia-os notify test"
        // WHEN parse
        let cli = parse(&["apollia-os", "notify", "test"]);
        // THEN Commands::Notify { command: NotifyCommand::Test }
        match &cli.command {
            Commands::Notify { command } => {
                assert!(matches!(command, NotifyCommand::Test));
                assert!(!cli.json);
            }
            other => panic!("expected Commands::Notify, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_notify_list() {
        // GIVEN "apollia-os notify list"
        // WHEN parse
        let cli = parse(&["apollia-os", "notify", "list"]);
        // THEN Commands::Notify { command: NotifyCommand::List }
        match &cli.command {
            Commands::Notify { command } => {
                assert!(matches!(command, NotifyCommand::List));
            }
            other => panic!("expected Commands::Notify, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_notify_logs_default() {
        // GIVEN "apollia-os notify logs"
        // WHEN parse
        let cli = parse(&["apollia-os", "notify", "logs"]);
        // THEN NotifyCommand::Logs { last: 20 }
        match &cli.command {
            Commands::Notify { command } => match command {
                NotifyCommand::Logs { last } => assert_eq!(*last, 20),
                other => panic!("expected NotifyCommand::Logs, got {other:?}"),
            },
            other => panic!("expected Commands::Notify, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_notify_test_json_flag() {
        // GIVEN "apollia-os notify --json test"
        // WHEN parse
        let cli = parse(&["apollia-os", "notify", "--json", "test"]);
        // THEN global json = true
        assert!(matches!(cli.command, Commands::Notify { .. }));
        assert!(cli.json);
    }

    // ── agent new command parsing ─────────────────────────────────

    #[test]
    fn test_cli_parses_agent_new_default_type() {
        // GIVEN "apollia-os agent new my-agent"
        let cli = parse(&["apollia-os", "agent", "new", "my-agent"]);
        // THEN AgentCommand::New with default type "react"
        match &cli.command {
            Commands::Agent { command } => match command {
                AgentCommand::New { name, r#type } => {
                    assert_eq!(name, "my-agent");
                    assert_eq!(r#type, "react");
                }
                other => panic!("expected AgentCommand::New, got {other:?}"),
            },
            other => panic!("expected Commands::Agent, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_agent_new_with_type() {
        // GIVEN "apollia-os agent new my-bot --type conversational"
        let cli = parse(&[
            "apollia-os",
            "agent",
            "new",
            "my-bot",
            "--type",
            "conversational",
        ]);
        // THEN AgentCommand::New with specified type
        match &cli.command {
            Commands::Agent { command } => match command {
                AgentCommand::New { name, r#type } => {
                    assert_eq!(name, "my-bot");
                    assert_eq!(r#type, "conversational");
                }
                other => panic!("expected AgentCommand::New, got {other:?}"),
            },
            other => panic!("expected Commands::Agent, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_agent_new_json_flag() {
        // GIVEN "apollia-os agent new my-agent --json"
        let cli = parse(&["apollia-os", "agent", "new", "my-agent", "--json"]);
        // THEN global json = true
        assert!(matches!(cli.command, Commands::Agent { .. }));
        assert!(cli.json);
    }

    // ── onboard command parsing ───────────────────────────────────

    #[test]
    fn test_cli_parses_onboard_no_topic() {
        // GIVEN "apollia-os onboard"
        let cli = parse(&["apollia-os", "onboard"]);
        // THEN Commands::Onboard { topic: None }
        match &cli.command {
            Commands::Onboard { topic } => {
                assert!(topic.is_none());
                assert!(!cli.json);
            }
            other => panic!("expected Commands::Onboard, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_onboard_with_topic() {
        // GIVEN "apollia-os onboard --topic preferences"
        let cli = parse(&["apollia-os", "onboard", "--topic", "preferences"]);
        // THEN Commands::Onboard { topic: Some("preferences") }
        match &cli.command {
            Commands::Onboard { topic } => {
                assert_eq!(topic.as_deref(), Some("preferences"));
            }
            other => panic!("expected Commands::Onboard, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_onboard_json_flag() {
        // GIVEN "apollia-os onboard --json --topic tools"
        let cli = parse(&["apollia-os", "onboard", "--json", "--topic", "tools"]);
        // THEN global json = true, topic = "tools"
        match &cli.command {
            Commands::Onboard { topic } => {
                assert_eq!(topic.as_deref(), Some("tools"));
                assert!(cli.json);
            }
            other => panic!("expected Commands::Onboard, got {other:?}"),
        }
    }

    // ── stt command parsing ───────────────────────────────────────

    #[test]
    fn test_cli_parses_stt_status() {
        let cli = parse(&["apollia-os", "stt", "status"]);
        match &cli.command {
            Commands::Stt { command } => {
                assert!(matches!(command, SttCommand::Status));
                assert!(!cli.json);
            }
            other => panic!("expected Commands::Stt, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_stt_status_json() {
        let cli = parse(&["apollia-os", "stt", "--json", "status"]);
        assert!(matches!(cli.command, Commands::Stt { .. }));
        assert!(cli.json);
    }

    #[test]
    fn test_cli_parses_stt_transcribe() {
        let cli = parse(&["apollia-os", "stt", "transcribe", "audio.wav"]);
        match &cli.command {
            Commands::Stt { command } => match command {
                SttCommand::Transcribe { file, output } => {
                    assert_eq!(file, &PathBuf::from("audio.wav"));
                    assert!(output.is_none());
                }
                other => panic!("expected SttCommand::Transcribe, got {other:?}"),
            },
            other => panic!("expected Commands::Stt, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_stt_transcribe_with_output() {
        let cli = parse(&[
            "apollia-os",
            "stt",
            "transcribe",
            "audio.wav",
            "--output",
            "out.json",
        ]);
        match &cli.command {
            Commands::Stt { command } => match command {
                SttCommand::Transcribe { file, output } => {
                    assert_eq!(file, &PathBuf::from("audio.wav"));
                    assert_eq!(output.as_deref(), Some(std::path::Path::new("out.json")));
                }
                other => panic!("expected SttCommand::Transcribe, got {other:?}"),
            },
            other => panic!("expected Commands::Stt, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_stt_transcriptions_list() {
        let cli = parse(&["apollia-os", "stt", "transcriptions", "list"]);
        match &cli.command {
            Commands::Stt { command } => match command {
                SttCommand::Transcriptions { command } => {
                    assert!(matches!(command, TranscriptionsCommand::List { limit: 20 }));
                }
                other => panic!("expected SttCommand::Transcriptions, got {other:?}"),
            },
            other => panic!("expected Commands::Stt, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_stt_transcriptions_list_custom_limit() {
        let cli = parse(&[
            "apollia-os",
            "stt",
            "transcriptions",
            "list",
            "--limit",
            "50",
        ]);
        match &cli.command {
            Commands::Stt { command } => match command {
                SttCommand::Transcriptions { command } => match command {
                    TranscriptionsCommand::List { limit } => assert_eq!(*limit, 50),
                    TranscriptionsCommand::Delete { .. } => {
                        panic!("expected List variant, got Delete")
                    }
                },
                other => panic!("expected SttCommand::Transcriptions, got {other:?}"),
            },
            other => panic!("expected Commands::Stt, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_stt_model_list() {
        let cli = parse(&["apollia-os", "stt", "model", "list"]);
        match &cli.command {
            Commands::Stt { command } => match command {
                SttCommand::Model { command } => {
                    assert!(matches!(command, SttModelCommand::List));
                }
                other => panic!("expected SttCommand::Model, got {other:?}"),
            },
            other => panic!("expected Commands::Stt, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_stt_model_download() {
        let cli = parse(&[
            "apollia-os",
            "stt",
            "model",
            "download",
            "whisper-large-v3-fr-q5_0",
        ]);
        match &cli.command {
            Commands::Stt { command } => match command {
                SttCommand::Model { command } => match command {
                    SttModelCommand::Download { name } => {
                        assert_eq!(name, "whisper-large-v3-fr-q5_0");
                    }
                    other => panic!("expected SttModelCommand::Download, got {other:?}"),
                },
                other => panic!("expected SttCommand::Model, got {other:?}"),
            },
            other => panic!("expected Commands::Stt, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_stt_json_flag_after_subcommand() {
        let cli = parse(&["apollia-os", "stt", "model", "list", "--json"]);
        assert!(matches!(cli.command, Commands::Stt { .. }));
        assert!(cli.json);
    }

    #[test]
    fn test_cli_parses_workspace_status() {
        // GIVEN "apollia-os workspace status"
        let cli = parse(&["apollia-os", "workspace", "status"]);
        // THEN Commands::Workspace avec WorkspaceCommand::Status
        assert!(
            matches!(
                cli.command,
                Commands::Workspace {
                    command: WorkspaceCommand::Status
                }
            ),
            "doit parser workspace status"
        );
        assert!(!cli.json);
    }

    #[test]
    fn test_cli_parses_workspace_init() {
        // GIVEN "apollia-os workspace init"
        let cli = parse(&["apollia-os", "workspace", "init"]);
        // THEN Commands::Workspace avec WorkspaceCommand::Init { force: false }
        assert!(
            matches!(
                cli.command,
                Commands::Workspace {
                    command: WorkspaceCommand::Init { force: false }
                }
            ),
            "doit parser workspace init sans --force"
        );
    }

    #[test]
    fn test_cli_parses_workspace_init_force() {
        // GIVEN "apollia-os workspace init --force"
        let cli = parse(&["apollia-os", "workspace", "init", "--force"]);
        // THEN Commands::Workspace avec WorkspaceCommand::Init { force: true }
        assert!(
            matches!(
                cli.command,
                Commands::Workspace {
                    command: WorkspaceCommand::Init { force: true }
                }
            ),
            "doit parser workspace init --force"
        );
    }

    #[test]
    fn test_cli_parses_workspace_status_json() {
        // GIVEN "apollia-os --json workspace status"
        let cli = parse(&["apollia-os", "--json", "workspace", "status"]);
        // THEN json=true
        assert!(cli.json, "flag global --json doit être activé");
        assert!(matches!(
            cli.command,
            Commands::Workspace {
                command: WorkspaceCommand::Status
            }
        ));
    }
}
