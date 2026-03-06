//! Apollia OS — CLI binary entry point.
//!
//! Command structure follows noun-verb pattern (ADR-008):
//!   apollia-os <noun> <verb> [options]
//!
//! Global flags: --json (machine-readable output), TTY auto-detected.

mod commands;

use clap::Parser;

use commands::memory::MemoryCommand;

/// Apollia OS — Sovereign AI Agent Runtime.
#[derive(Debug, Parser)]
#[command(name = "apollia-os", version, about)]
struct Cli {
    /// Commande a executer.
    #[command(subcommand)]
    command: Commands,
}

/// Commandes de premier niveau.
#[derive(Debug, clap::Subcommand)]
enum Commands {
    /// Memory management.
    Memory {
        /// Sous-commande memoire.
        #[command(subcommand)]
        command: MemoryCommand,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        Commands::Memory { command } => commands::memory::run(command),
    };

    match result {
        Ok(output) => {
            println!("{output}");
        }
        Err(err) => {
            eprintln!("Error: {err}");
            std::process::exit(1);
        }
    }
}
