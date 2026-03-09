//! Apollia OS — CLI binary entry point.
//!
//! Command structure follows noun-verb pattern (ADR-008):
//!   apollia-os <command> [options]
//!
//! Level 1 commands: start, stop, status, run (STORY-037)
//! Level 2 commands: memory (STORY-023), agent/task/tools/audit (STORY-038)
//!
//! Global flags: --json (machine-readable output), --socket (custom path).

pub mod client;
pub mod commands;
pub mod config;
pub mod exit_codes;

use std::path::PathBuf;

use clap::Parser;

use commands::agent::AgentCommand;
use commands::audit::AuditCommand;
use commands::llm::LlmCommand;
use commands::memory::MemoryCommand;
use commands::model::ModelCommand;
use commands::task::TaskCommand;
use commands::tools::ToolsCommand;
use commands::trigger::TriggerCommand;

/// Apollia OS — Sovereign AI Agent Runtime.
#[derive(Debug, Parser)]
#[command(name = "apollia-os", version, about)]
struct Cli {
    /// Unix socket path (default: /tmp/apollia.sock).
    #[arg(long, global = true, value_name = "PATH")]
    socket: Option<PathBuf>,

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
    Stop {
        /// Output JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },

    /// Display runtime and agent status.
    Status {
        /// Output JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },

    /// Submit a task to an agent and wait for the result.
    Run {
        /// Agent identifier.
        agent_id: String,

        /// Task input text.
        input: String,

        /// Stream task progress in real-time via SSE.
        #[arg(long)]
        stream: bool,

        /// Output JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },

    /// Agent management (list, start, stop, info).
    Agent {
        /// Agent subcommand.
        #[command(subcommand)]
        command: AgentCommand,

        /// Output JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },

    /// Task management (list, status, cancel).
    Task {
        /// Task subcommand.
        #[command(subcommand)]
        command: TaskCommand,

        /// Output JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },

    /// Tool registry queries (list, describe).
    Tools {
        /// Tools subcommand.
        #[command(subcommand)]
        command: ToolsCommand,

        /// Output JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },

    /// Audit trail (list, stats).
    Audit {
        /// Audit subcommand.
        #[command(subcommand)]
        command: AuditCommand,

        /// Output JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },

    /// Memory management.
    Memory {
        /// Memory subcommand.
        #[command(subcommand)]
        command: MemoryCommand,
    },

    /// LLM backend diagnostics (status, ping, chat).
    Llm {
        /// LLM subcommand.
        #[command(subcommand)]
        command: LlmCommand,

        /// Output JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },

    /// Local model file management.
    Model {
        /// Model subcommand.
        #[command(subcommand)]
        command: ModelCommand,

        /// Output JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },

    /// Trigger management (reload).
    Trigger {
        /// Trigger subcommand.
        #[command(subcommand)]
        command: TriggerCommand,

        /// Output JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    // Initialize tracing for start command (other commands use minimal logging)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("apollia=info".parse().expect("valid directive")),
        )
        .with_target(false)
        .init();

    let rt = tokio::runtime::Runtime::new().expect("failed to create Tokio runtime");

    let exit_code = rt.block_on(async {
        match cli.command {
            Commands::Start { port } => match commands::start::run(cli.socket, port).await {
                Ok(()) => exit_codes::SUCCESS,
                Err(e) => {
                    eprintln!("Error: {e}");
                    exit_codes::GENERAL_ERROR
                }
            },
            Commands::Stop { json } => commands::stop::run(cli.socket, json).await,
            Commands::Status { json } => commands::status::run(cli.socket, json).await,
            Commands::Run {
                agent_id,
                input,
                stream,
                json,
            } => commands::run::run(&agent_id, &input, cli.socket, json, stream).await,
            Commands::Agent { command, json } => {
                commands::agent::run(&command, cli.socket, json).await
            }
            Commands::Task { command, json } => {
                commands::task::run(&command, cli.socket, json).await
            }
            Commands::Tools { command, json } => {
                commands::tools::run(&command, cli.socket, json).await
            }
            Commands::Audit { command, json } => {
                commands::audit::run(&command, cli.socket, json).await
            }
            Commands::Memory { command } => match commands::memory::run(&command) {
                Ok(output) => {
                    println!("{output}");
                    exit_codes::SUCCESS
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    exit_codes::GENERAL_ERROR
                }
            },
            Commands::Llm { command, json } => commands::llm::run(&command, cli.socket, json).await,
            Commands::Model { command, json } => commands::model::run(&command, json),
            Commands::Trigger { command, json } => {
                commands::trigger::run(&command, cli.socket, json).await
            }
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
    use commands::task::TaskCommand;
    use commands::tools::ToolsCommand;
    use commands::trigger::TriggerCommand;

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
        // THEN Commands::Stop
        assert!(matches!(cli.command, Commands::Stop { json: false }));
    }

    #[test]
    fn test_cli_parses_stop_json() {
        // GIVEN "apollia-os stop --json"
        let cli = parse(&["apollia-os", "stop", "--json"]);
        // THEN Commands::Stop with json=true
        assert!(matches!(cli.command, Commands::Stop { json: true }));
    }

    #[test]
    fn test_cli_parses_status_command() {
        // GIVEN "apollia-os status"
        let cli = parse(&["apollia-os", "status"]);
        // THEN Commands::Status
        assert!(matches!(cli.command, Commands::Status { json: false }));
    }

    #[test]
    fn test_cli_parses_status_json_flag() {
        // GIVEN "apollia-os status --json"
        let cli = parse(&["apollia-os", "status", "--json"]);
        // THEN Commands::Status with json=true
        assert!(matches!(cli.command, Commands::Status { json: true }));
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
                json,
            } => {
                assert_eq!(agent_id, "hello-agent");
                assert_eq!(input, "Bonjour");
                assert!(!stream);
                assert!(!json);
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
    fn test_cli_parses_run_json_flag() {
        // GIVEN "apollia-os run hello-agent Bonjour --json"
        let cli = parse(&["apollia-os", "run", "hello-agent", "Bonjour", "--json"]);
        // THEN Commands::Run with json=true
        match &cli.command {
            Commands::Run { json, .. } => assert!(json),
            other => panic!("expected Commands::Run, got {other:?}"),
        }
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
    }

    // --- Level-2 command parsing tests (STORY-038) ---

    #[test]
    fn test_cli_parses_agent_list() {
        // GIVEN "apollia-os agent list"
        // WHEN parse
        let cli = parse(&["apollia-os", "agent", "list"]);
        // THEN Commands::Agent { command: AgentCommand::List }
        match &cli.command {
            Commands::Agent { command, json } => {
                assert!(matches!(command, AgentCommand::List));
                assert!(!json);
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
            Commands::Agent { command, .. } => match command {
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
            Commands::Agent { command, .. } => match command {
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
            Commands::Agent { command, .. } => match command {
                AgentCommand::Info { agent_id } => {
                    assert_eq!(agent_id, "hello-agent");
                }
                other => panic!("expected AgentCommand::Info, got {other:?}"),
            },
            other => panic!("expected Commands::Agent, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_agent_json_flag() {
        // GIVEN "apollia-os agent --json list"
        // WHEN parse
        let cli = parse(&["apollia-os", "agent", "--json", "list"]);
        // THEN json=true
        match &cli.command {
            Commands::Agent { json, .. } => assert!(json),
            other => panic!("expected Commands::Agent, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_task_list() {
        // GIVEN "apollia-os task list"
        // WHEN parse
        let cli = parse(&["apollia-os", "task", "list"]);
        // THEN Commands::Task { command: TaskCommand::List }
        match &cli.command {
            Commands::Task { command, json } => {
                assert!(
                    matches!(
                        command,
                        TaskCommand::List {
                            pending_approval: false
                        }
                    ),
                    "expected TaskCommand::List {{ pending_approval: false }}, got {command:?}"
                );
                assert!(!json);
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
            Commands::Task { command, .. } => match command {
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
            Commands::Task { command, .. } => match command {
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
            Commands::Task { command, json } => {
                match command {
                    TaskCommand::Inspect { id } => assert_eq!(id, "t-0042"),
                    other => panic!("expected TaskCommand::Inspect, got {other:?}"),
                }
                assert!(!json);
            }
            other => panic!("expected Commands::Task, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_task_inspect_json_flag() {
        // GIVEN "apollia-os task --json inspect t-0042"
        // WHEN parse
        let cli = parse(&["apollia-os", "task", "--json", "inspect", "t-0042"]);
        // THEN json = true
        match &cli.command {
            Commands::Task { json, .. } => assert!(json),
            other => panic!("expected Commands::Task, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_tools_list() {
        // GIVEN "apollia-os tools list"
        // WHEN parse
        let cli = parse(&["apollia-os", "tools", "list"]);
        // THEN Commands::Tools { command: ToolsCommand::List }
        match &cli.command {
            Commands::Tools { command, json } => {
                assert!(matches!(command, ToolsCommand::List));
                assert!(!json);
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
            Commands::Tools { command, .. } => match command {
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
            Commands::Audit { command, json } => {
                match command {
                    AuditCommand::List { limit } => assert_eq!(*limit, 20),
                    other => panic!("expected AuditCommand::List, got {other:?}"),
                }
                assert!(!json);
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
            Commands::Audit { command, .. } => match command {
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
            Commands::Audit { command, .. } => {
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
        // THEN Commands::Memory preserved from STORY-023
        match &cli.command {
            Commands::Memory { command } => match command {
                MemoryCommand::Inspect { namespace, .. } => {
                    assert_eq!(namespace, "test-ns");
                }
            },
            other => panic!("expected Commands::Memory, got {other:?}"),
        }
    }

    // ── STORY-063: llm / model command parsing ───────────────────────────────

    #[test]
    fn test_cli_parses_llm_status() {
        // GIVEN "apollia-os llm status"
        // WHEN parse
        let cli = parse(&["apollia-os", "llm", "status"]);
        // THEN Commands::Llm { command: LlmCommand::Status }
        match &cli.command {
            Commands::Llm { command, json } => {
                assert!(matches!(command, LlmCommand::Status));
                assert!(!json);
            }
            other => panic!("expected Commands::Llm, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_llm_status_json_flag() {
        // GIVEN "apollia-os llm --json status"
        // WHEN parse
        let cli = parse(&["apollia-os", "llm", "--json", "status"]);
        // THEN json = true
        match &cli.command {
            Commands::Llm { json, .. } => assert!(json),
            other => panic!("expected Commands::Llm, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_llm_ping_no_backend() {
        // GIVEN "apollia-os llm ping"
        // WHEN parse
        let cli = parse(&["apollia-os", "llm", "ping"]);
        // THEN LlmCommand::Ping { backend: None }
        match &cli.command {
            Commands::Llm { command, .. } => match command {
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
            Commands::Llm { command, .. } => match command {
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
            Commands::Llm { command, .. } => match command {
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
            Commands::Llm { command, .. } => match command {
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
            Commands::Model { command, json } => {
                assert!(matches!(command, ModelCommand::List));
                assert!(!json);
            }
            other => panic!("expected Commands::Model, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_model_list_json_flag() {
        // GIVEN "apollia-os model --json list"
        // WHEN parse
        let cli = parse(&["apollia-os", "model", "--json", "list"]);
        // THEN json = true
        match &cli.command {
            Commands::Model { json, .. } => assert!(json),
            other => panic!("expected Commands::Model, got {other:?}"),
        }
    }

    // ── STORY-073: trigger command parsing ───────────────────────────────────

    #[test]
    fn test_cli_parses_trigger_reload() {
        // GIVEN "apollia-os trigger reload"
        // WHEN parse
        let cli = parse(&["apollia-os", "trigger", "reload"]);
        // THEN Commands::Trigger { command: TriggerCommand::Reload, json: false }
        match &cli.command {
            Commands::Trigger { command, json } => {
                assert!(matches!(command, TriggerCommand::Reload));
                assert!(!json);
            }
            other => panic!("expected Commands::Trigger, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_trigger_reload_json_flag() {
        // GIVEN "apollia-os trigger --json reload"
        // WHEN parse
        let cli = parse(&["apollia-os", "trigger", "--json", "reload"]);
        // THEN json = true
        match &cli.command {
            Commands::Trigger { json, .. } => assert!(json),
            other => panic!("expected Commands::Trigger, got {other:?}"),
        }
    }
}
