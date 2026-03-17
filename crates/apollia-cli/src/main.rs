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
use commands::notify::NotifyCommand;
use commands::pipeline::PipelineCommand;
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

    /// Output machine-readable JSON instead of human-readable text.
    ///
    /// Accepted at any position: before or after the subcommand and its arguments.
    #[arg(long, global = true)]
    json: bool,

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
    Run {
        /// Agent identifier.
        agent_id: String,

        /// Task input text.
        input: String,

        /// Stream task progress in real-time via SSE.
        #[arg(long)]
        stream: bool,

        /// Submit the task and return immediately without waiting for the result.
        ///
        /// The task ID is printed so it can be tracked with `apollia-os task status <id>`.
        #[arg(long)]
        detach: bool,
    },

    /// Agent management (list, start, stop, info, install, uninstall, enable, disable, update).
    Agent {
        /// Agent subcommand.
        #[command(subcommand)]
        command: AgentCommand,
    },

    /// Task management (list, status, cancel).
    Task {
        /// Task subcommand.
        #[command(subcommand)]
        command: TaskCommand,
    },

    /// Tool registry queries (list, describe).
    Tools {
        /// Tools subcommand.
        #[command(subcommand)]
        command: ToolsCommand,
    },

    /// Audit trail (list, stats).
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

    /// LLM backend diagnostics (status, ping, chat).
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

    /// Trigger management (reload).
    Trigger {
        /// Trigger subcommand.
        #[command(subcommand)]
        command: TriggerCommand,
    },

    /// Notification channel management (test, list, logs).
    Notify {
        /// Notify subcommand.
        #[command(subcommand)]
        command: NotifyCommand,
    },

    /// Pipeline orchestration (list, run, runs, status).
    Pipeline {
        /// Pipeline subcommand.
        #[command(subcommand)]
        command: PipelineCommand,
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

    let json = cli.json;
    let exit_code = rt.block_on(async {
        match cli.command {
            Commands::Start { port } => match commands::start::run(cli.socket, port).await {
                Ok(()) => exit_codes::SUCCESS,
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
                stream,
                detach,
            } => commands::run::run(&agent_id, &input, cli.socket, json, stream, detach).await,
            Commands::Agent { command } => commands::agent::run(&command, cli.socket, json).await,
            Commands::Task { command } => commands::task::run(&command, cli.socket, json).await,
            Commands::Tools { command } => commands::tools::run(&command, cli.socket, json).await,
            Commands::Audit { command } => commands::audit::run(&command, cli.socket, json).await,
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
            Commands::Llm { command } => commands::llm::run(&command, cli.socket, json).await,
            Commands::Model { command } => commands::model::run(&command, json),
            Commands::Trigger { command } => {
                commands::trigger::run(&command, cli.socket, json).await
            }
            Commands::Notify { command } => commands::notify::run(&command, cli.socket, json).await,
            Commands::Pipeline { command } => {
                commands::pipeline::run(&command, cli.socket, json).await
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
    use commands::notify::NotifyCommand;
    use commands::pipeline::PipelineCommand;
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
    }

    // --- Level-2 command parsing tests (STORY-038) ---

    #[test]
    fn test_cli_parses_agent_list() {
        // GIVEN "apollia-os agent list"
        // WHEN parse
        let cli = parse(&["apollia-os", "agent", "list"]);
        // THEN Commands::Agent { command: AgentCommand::List }
        match &cli.command {
            Commands::Agent { command } => {
                assert!(matches!(command, AgentCommand::List));
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
    fn test_cli_parses_agent_install() {
        // GIVEN "apollia-os agent install ./agents/mon-agent.py"
        // WHEN parse
        let cli = parse(&["apollia-os", "agent", "install", "./agents/mon-agent.py"]);
        // THEN Commands::Agent { command: AgentCommand::Install { path } }
        match &cli.command {
            Commands::Agent { command } => match command {
                AgentCommand::Install { path } => {
                    assert_eq!(path, &PathBuf::from("./agents/mon-agent.py"));
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

    // ── STORY-073: trigger command parsing ───────────────────────────────────

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

    // ── STORY-104: notify command parsing ─────────────────────────────────────

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

    // ── STORY-121: pipeline command parsing ───────────────────────────────────

    #[test]
    fn test_cli_parses_pipeline_list() {
        // GIVEN "apollia-os pipeline list"
        // WHEN parse
        let cli = parse(&["apollia-os", "pipeline", "list"]);
        // THEN Commands::Pipeline { command: PipelineCommand::List }, json=false
        match &cli.command {
            Commands::Pipeline { command } => {
                assert!(matches!(command, PipelineCommand::List));
                assert!(!cli.json);
            }
            other => panic!("expected Commands::Pipeline, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_pipeline_run() {
        // GIVEN "apollia-os pipeline run traitement-facture acme.pdf"
        // WHEN parse
        let cli = parse(&[
            "apollia-os",
            "pipeline",
            "run",
            "traitement-facture",
            "acme.pdf",
        ]);
        // THEN PipelineCommand::Run with correct fields
        match &cli.command {
            Commands::Pipeline { command } => match command {
                PipelineCommand::Run {
                    pipeline_id,
                    input,
                    detach,
                } => {
                    assert_eq!(pipeline_id, "traitement-facture");
                    assert_eq!(input, "acme.pdf");
                    assert!(!detach);
                }
                other => panic!("expected PipelineCommand::Run, got {other:?}"),
            },
            other => panic!("expected Commands::Pipeline, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_pipeline_runs() {
        // GIVEN "apollia-os pipeline runs traitement-facture"
        // WHEN parse
        let cli = parse(&["apollia-os", "pipeline", "runs", "traitement-facture"]);
        // THEN PipelineCommand::Runs with default limit=20
        match &cli.command {
            Commands::Pipeline { command } => match command {
                PipelineCommand::Runs { pipeline_id, limit } => {
                    assert_eq!(pipeline_id, "traitement-facture");
                    assert_eq!(*limit, 20);
                }
                other => panic!("expected PipelineCommand::Runs, got {other:?}"),
            },
            other => panic!("expected Commands::Pipeline, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_pipeline_status() {
        // GIVEN "apollia-os pipeline status r-0017"
        // WHEN parse
        let cli = parse(&["apollia-os", "pipeline", "status", "r-0017"]);
        // THEN PipelineCommand::Status { run_id: "r-0017" }
        match &cli.command {
            Commands::Pipeline { command } => match command {
                PipelineCommand::Status { run_id } => assert_eq!(run_id, "r-0017"),
                other => panic!("expected PipelineCommand::Status, got {other:?}"),
            },
            other => panic!("expected Commands::Pipeline, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_pipeline_json_flag() {
        // GIVEN "apollia-os pipeline --json list"
        // WHEN parse
        let cli = parse(&["apollia-os", "pipeline", "--json", "list"]);
        // THEN global json = true
        assert!(matches!(cli.command, Commands::Pipeline { .. }));
        assert!(cli.json);
    }
}
