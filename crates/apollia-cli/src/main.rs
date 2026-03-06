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
pub mod exit_codes;

use std::path::PathBuf;

use clap::Parser;

use commands::memory::MemoryCommand;

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

    /// Memory management.
    Memory {
        /// Memory subcommand.
        #[command(subcommand)]
        command: MemoryCommand,
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
        }
    });

    std::process::exit(exit_code);
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

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
}
