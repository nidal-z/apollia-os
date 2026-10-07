//! `apollia-os agent` subcommands: manage agents via the runtime API and local persistence.
//!
//! Provides `list`, `start`, `stop`, `show`, `set`, `reset`, `uninstall`
//! (runtime-dependent) and `enable`, `disable` (local). An agent arrives with
//! a package: see `apollia-os package`.

use std::path::{Path, PathBuf};

use apollia_tools::{AgentRepository, PackageRepository, StoredAgent};
use clap::Subcommand;

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;

mod definition;
mod display;
mod export;
mod lifecycle;
mod package;
mod uninstall;

#[cfg(test)]
mod tests;

pub(in crate::commands::agent) use definition::{
    print_definition_section, run_list_definitions, run_reset, run_set,
};
pub(in crate::commands::agent) use display::{
    build_list_json, format_a2a_agent_list, format_agent_detail, format_enriched_agent_list,
    format_status_snapshot, handle_error, local_agent_detail, print_audit_event_row,
    run_info_local_fallback,
};
pub(in crate::commands::agent) use export::{run_export, ExportChoices};
pub(in crate::commands::agent) use lifecycle::{
    run_info, run_list, run_logs, run_messages, run_start, run_status, run_stop,
};
pub(in crate::commands::agent) use package::{
    run_disable, run_enable, run_package_info, run_package_list,
};
pub(in crate::commands::agent) use uninstall::{run_package_uninstall, run_uninstall, Erase};

/// Agent subcommands: `apollia-os agent <verb>`.
#[derive(Debug, Subcommand)]
pub enum AgentCommand {
    /// List all agents (installed and/or runtime).
    List {
        /// Show only A2A-capable agents with their skill descriptors.
        #[arg(long, conflicts_with = "definitions")]
        supports_a2a: bool,
        /// List the stored agents as resolved (package definition, then your
        /// overrides) instead of the runtime inventory.
        #[arg(long)]
        definitions: bool,
    },
    /// Make a stored agent available again, after `agent stop`.
    ///
    /// An agent is available as soon as it is installed and enabled; this is
    /// only needed for one that was stopped.
    Start {
        /// Agent identifier.
        agent_id: String,
    },
    /// Stop (shutdown) a running agent.
    Stop {
        /// Agent identifier.
        agent_id: String,
    },
    /// Display detailed information about an agent, with its definition as
    /// resolved (package defaults, then your overrides).
    Show {
        /// Agent identifier.
        agent_id: String,
    },
    /// Override one field of an agent's definition; a package update keeps it.
    ///
    /// Fields: `name`, `mission`, `autonomy`, `planning` (never, auto or
    /// always) and `profile` (a profile identifier, see `profile list`) take
    /// one value; `examples`, `access` and `sources` take the whole list, one
    /// value per item (none for an empty list). A source is an identifier of
    /// `source list`, `profile` or `memory`; the runtime refuses any other.
    /// The source `profile` lets the agent read your profile through
    /// `ctx.profile`: `agent set <id> sources profile` checks it, a list
    /// without it unchecks it, and `agent reset --field sources` goes back to
    /// the package default. Recorded in the audit journal.
    Set {
        /// Agent identifier.
        agent_id: String,
        /// Field to override: name, mission, examples, access, sources,
        /// autonomy, planning or profile.
        field: String,
        /// The value, or the items of a list.
        #[arg(num_args = 0..)]
        values: Vec<String>,
    },
    /// Go back to the package defaults: remove every override of an agent,
    /// or the override of one field. Recorded in the audit journal.
    Reset {
        /// Agent identifier.
        agent_id: String,
        /// Remove only the override of this field.
        #[arg(long)]
        field: Option<String>,
        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        confirm: bool,
    },
    /// Export an agent created in the interface as a `.apo` package.
    ///
    /// Its current values become the defaults of the package. No secret, no
    /// memory and no history is exported; the account of a connection stays
    /// here and becomes a required connection; an access to another package,
    /// agent or workflow is left out and listed. The publisher is the
    /// `author.name` setting, or `--publisher`. Needs the runtime.
    Export {
        /// Agent identifier.
        agent_id: String,
        /// Sign the package with the author key, generated in the secret
        /// store on the first signature.
        #[arg(long)]
        sign: bool,
        /// Publisher name for this export only.
        #[arg(long, value_name = "NAME")]
        publisher: Option<String>,
        /// File to write. Default: `<id>-<version>.apo` in the current folder.
        #[arg(long, short = 'o', value_name = "FILE")]
        output: Option<PathBuf>,
    },
    /// Show a compact runtime-status snapshot for `<agent_id>`.
    ///
    /// Distilled view of `agent show` focused on online / idle / error state.
    /// Useful in poll loops where the full info payload is overkill.
    Status {
        /// Agent identifier.
        agent_id: String,
    },
    /// List in-memory A2A messages for `<agent_id>` (oldest-first within window).
    Messages {
        /// Agent identifier (recipient).
        agent_id: String,
        /// Maximum number of messages to display (server-clamped to 100).
        #[arg(long, value_name = "N", default_value = "20")]
        limit: u32,
    },
    /// Uninstall an agent installed on its own, through the runtime.
    ///
    /// Removes its definition, its overrides, its model, its triggers, its
    /// environment and its files. Its memory and its history are kept unless
    /// asked otherwise. An agent of an installed package goes with its
    /// package.
    Uninstall {
        /// Agent name (as declared in manifest).
        name: String,

        /// Erase the agent's memory instead of keeping it.
        #[arg(long)]
        clear_memory: bool,

        /// Erase the agent's history (tasks, batches) instead of keeping it.
        #[arg(long)]
        clear_history: bool,

        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        confirm: bool,
    },
    /// Enable an installed agent (will auto-start on boot).
    Enable {
        /// Agent name.
        name: String,
    },
    /// Disable an installed agent (will not auto-start on boot).
    Disable {
        /// Agent name.
        name: String,
    },
    /// List, show and uninstall the installed packages.
    Package {
        #[command(subcommand)]
        cmd: PackageCommand,
    },
    /// Display recent log lines from a running agent.
    Logs {
        /// Agent identifier (name or UUID).
        agent_id: String,
        /// Number of recent log lines to display.
        #[arg(long, default_value = "50", value_name = "N")]
        last: u32,
        // The published reference is generated from the doc comment below, so a
        // line describing this flag as working made the reference say so too.
        /// Not implemented: refuses with an error naming `--last` instead.
        #[arg(long)]
        follow: bool,
    },
}

/// Package sub-subcommands: `apollia-os agent package <verb>`.
#[derive(Debug, Subcommand)]
pub enum PackageCommand {
    /// List all installed agent packages.
    List,
    /// Show details for an installed package.
    Show {
        /// Package name.
        name: String,
    },
    /// Uninstall a package and everything it installed, through the runtime.
    ///
    /// Removes its agents with their overrides, models and triggers, its
    /// workflows and tools, its environments, its files and the MCP servers it
    /// brought; the connections it required stay. The memory and the history
    /// of its agents are kept unless asked otherwise.
    Uninstall {
        /// Package name.
        name: String,

        /// Erase the memory of its agents instead of keeping it.
        #[arg(long)]
        clear_memory: bool,

        /// Erase the history of its agents and workflows (tasks, batches,
        /// workflow runs) instead of keeping it.
        #[arg(long)]
        clear_history: bool,

        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        confirm: bool,
    },
}

/// Execute an `agent` subcommand.
///
/// Returns the process exit code.
pub async fn run(cmd: &AgentCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(default_socket_path);
    let client = RuntimeClient::new(socket_path);

    match cmd {
        AgentCommand::List {
            definitions: true, ..
        } => run_list_definitions(&client, json).await,
        AgentCommand::List { supports_a2a, .. } => run_list(&client, *supports_a2a, json).await,
        AgentCommand::Start { agent_id } => run_start(&client, agent_id, json).await,
        AgentCommand::Stop { agent_id } => run_stop(&client, agent_id, json).await,
        AgentCommand::Show { agent_id } => run_info(&client, agent_id, json).await,
        AgentCommand::Set {
            agent_id,
            field,
            values,
        } => run_set(&client, agent_id, field, values, json).await,
        AgentCommand::Reset {
            agent_id,
            field,
            confirm,
        } => run_reset(&client, agent_id, field.as_deref(), *confirm, json).await,
        AgentCommand::Export {
            agent_id,
            sign,
            publisher,
            output,
        } => {
            let choices = ExportChoices {
                sign: *sign,
                publisher: publisher.as_deref(),
                output: output.as_deref(),
            };
            run_export(&client, agent_id, choices, json).await
        }
        AgentCommand::Status { agent_id } => run_status(&client, agent_id, json).await,
        AgentCommand::Messages { agent_id, limit } => {
            run_messages(&client, agent_id, *limit, json).await
        }
        AgentCommand::Uninstall {
            name,
            clear_memory,
            clear_history,
            confirm,
        } => {
            let erase = Erase {
                memory: *clear_memory,
                history: *clear_history,
            };
            run_uninstall(&client, name, erase, *confirm, json).await
        }
        AgentCommand::Enable { name } => run_enable(&client, name, json).await,
        AgentCommand::Disable { name } => run_disable(&client, name, json).await,
        AgentCommand::Package { cmd } => match cmd {
            PackageCommand::List => run_package_list(json),
            PackageCommand::Show { name } => run_package_info(name, json),
            PackageCommand::Uninstall {
                name,
                clear_memory,
                clear_history,
                confirm,
            } => {
                let erase = Erase {
                    memory: *clear_memory,
                    history: *clear_history,
                };
                run_package_uninstall(&client, name, erase, *confirm, json).await
            }
        },
        AgentCommand::Logs {
            agent_id,
            last,
            follow,
        } => run_logs(&client, agent_id, *last, *follow, json).await,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Existing commands (list/start/stop/info)
// ─────────────────────────────────────────────────────────────────────────────

/// Return true if `arg` looks like a file path rather than an agent name or UUID.
///
/// Detects the common mistake of passing a Python module path (e.g. `agents/foo.py`)
/// to commands that expect a name or UUID (e.g. `onboarding-agent`).
fn looks_like_file_path(arg: &str) -> bool {
    arg.contains('/') || arg.contains('\\') || arg.ends_with(".py")
}

/// Resolve `~/.apollia/` data directory.
fn apollia_data_dir() -> PathBuf {
    apollia_core::paths::data_dir_under(apollia_core::paths::home_dir_or_temp())
}

/// Open the package repository at `<data_dir>/agents.db`, creating it if needed.
fn open_package_repository_or_create(data_dir: &Path) -> Result<PackageRepository, String> {
    if let Err(e) = std::fs::create_dir_all(data_dir) {
        return Err(format!(
            "cannot create data directory {}: {e}",
            data_dir.display()
        ));
    }
    let db_path = apollia_core::paths::DataFile::Agents.path(data_dir);
    PackageRepository::open(&db_path).map_err(|e| format!("cannot open agents.db: {e}"))
}

/// Open the agent repository at `<data_dir>/agents.db`, creating it if needed.
fn open_repository_or_create(data_dir: &Path) -> Result<AgentRepository, String> {
    if let Err(e) = std::fs::create_dir_all(data_dir) {
        return Err(format!(
            "cannot create data directory {}: {e}",
            data_dir.display()
        ));
    }
    let db_path = apollia_core::paths::DataFile::Agents.path(data_dir);
    AgentRepository::open(&db_path).map_err(|e| format!("cannot open agents.db: {e}"))
}

/// Try to open the agent repository (returns `None` if file or DB unavailable).
/// The stored agents of the local store, the Apollia agent left out: it is
/// the built-in chat, never started, stopped nor uninstalled.
fn stored_agents() -> Vec<StoredAgent> {
    open_repository()
        .and_then(|repo| repo.list_stored().ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|agent| !apollia_tools::is_built_in_agent(&agent.name))
        .collect()
}

fn open_repository() -> Option<AgentRepository> {
    let data_dir = apollia_data_dir();
    let db_path = apollia_core::paths::DataFile::Agents.path(&data_dir);
    if !db_path.exists() {
        return None;
    }
    AgentRepository::open(&db_path).ok()
}

/// Print an error message (human or JSON) and return `GENERAL_ERROR`.
fn print_error_and_exit(msg: &str, json: bool) -> i32 {
    crate::output::emit_error(json, exit_codes::GENERAL_ERROR, msg)
}

/// Emit an `{"error": msg}` payload as compact JSON (or a plain `Error:`
/// line) and return `GENERAL_ERROR`. Distinct from `print_error_and_exit`,
/// which pretty-prints the JSON form.
fn print_compact_error_and_exit(msg: &str, json: bool) -> i32 {
    crate::output::emit_error(json, exit_codes::GENERAL_ERROR, msg)
}
