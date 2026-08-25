//! `apollia-os agent` subcommands: manage agents via the runtime API and local persistence.
//!
//! Provides `list`, `start`, `stop`, `info` (runtime-dependent) and
//! `install`, `uninstall`, `enable`, `disable`, `update` (local).

use std::path::{Path, PathBuf};

use apollia_aip::package_loader::{load_manifest_only, PackageLoaderError};
use apollia_runtime::agents::registry_remote::{self, parse_install_source, AgentInstallSource};
use apollia_runtime::api::routes_agents::AgentLoader;
use apollia_tools::{AgentRepository, InstalledAgent, InstalledPackage, PackageRepository};
use apollia_triggers::{
    definition_repository::TriggerDefinitionRepository, parse_triggers_from_toml_str, OnBusy,
    OnBusyPolicy, TriggerDefinitionRow, TriggerSourceConfig,
};
use clap::Subcommand;

use crate::community::{validate_community_agent, AgentValidationError};

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

mod display;
mod install;
mod lifecycle;
mod package;
mod scaffold;
mod validate;

#[cfg(test)]
mod tests;

pub(in crate::commands::agent) use display::{
    build_list_json, format_a2a_agent_list, format_agent_detail, format_enriched_agent_list,
    format_status_snapshot, handle_error, local_agent_detail, print_audit_event_row,
    print_trust_banner, run_info_local_fallback,
};
pub(in crate::commands::agent) use install::run_install;
pub(in crate::commands::agent) use lifecycle::{
    run_info, run_list, run_logs, run_messages, run_start, run_status, run_stop,
};
pub(in crate::commands::agent) use package::{
    run_disable, run_enable, run_package_info, run_package_list, run_package_uninstall, run_repair,
    run_uninstall, run_update,
};
pub(in crate::commands::agent) use scaffold::run_new;
pub(in crate::commands::agent) use validate::run_validate;

/// Agent subcommands: `apollia-os agent <verb>`.
#[derive(Debug, Subcommand)]
pub enum AgentCommand {
    /// List all agents (installed and/or runtime).
    List {
        /// Show only A2A-capable agents with their skill descriptors.
        #[arg(long)]
        supports_a2a: bool,
    },
    /// Start (register) a new agent from a Python module path.
    Start {
        /// Path to the agent Python module.
        path: String,
    },
    /// Stop (shutdown) a running agent.
    Stop {
        /// Agent identifier.
        agent_id: String,
    },
    /// Display detailed information about an agent.
    Show {
        /// Agent identifier.
        agent_id: String,
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
    /// Install an agent permanently from a local path or a Git URL.
    ///
    /// Accepts a local filesystem path (e.g. `./agents/my-agent.py`) or a Git
    /// remote URL (e.g. `https://github.com/user/my-agent.git`).  An optional
    /// `#<tag>` suffix pins the clone to a specific tag or branch
    /// (e.g. `https://github.com/user/my-agent.git#v1.2.0`).
    Install {
        /// Local path to a Python module or a Git URL (with optional #tag).
        source: String,

        /// Skip the agent test suite (not recommended, reduces validation coverage).
        #[arg(long)]
        skip_tests: bool,
    },
    /// Uninstall a permanently installed agent.
    Uninstall {
        /// Agent name (as declared in manifest).
        name: String,
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
    /// Update an installed agent with a new Python module.
    Update {
        /// Agent name.
        name: String,
        /// Path to the new Python module.
        path: PathBuf,
    },
    /// Create a new agent from an SDK template.
    Create {
        /// Agent name in kebab-case (e.g. my-agent).
        name: String,

        /// Template type: react, conversational, or orchestrated.
        #[arg(long, default_value = "react")]
        r#type: String,
    },
    /// Manage agent packages (multi-agent bundles described by agent.toml).
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
    /// Validate an agent manifest without installing or starting the agent.
    Validate {
        /// Path to the Python agent module.
        path: PathBuf,
    },
    /// Re-provision an installed agent's per-agent Python venv from its manifest.
    ///
    /// Reads `~/.apollia/agents/packages/<name>/agent.toml` (or the single-file
    /// agent's manifest), then re-runs `setup_venv` with the declared `packages`
    /// list. Useful when an agent was installed before per-agent venv
    /// provisioning landed, or when a venv was deleted by hand.
    Repair {
        /// Installed agent name (as declared in the manifest).
        name: String,
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
    /// Uninstall a package and all its agents and triggers.
    Uninstall {
        /// Package name.
        name: String,
    },
}

/// Execute an `agent` subcommand.
///
/// Returns the process exit code.
pub async fn run(cmd: &AgentCommand, socket: Option<PathBuf>, json: bool, quiet: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let client = RuntimeClient::new(socket_path);

    match cmd {
        AgentCommand::List { supports_a2a } => run_list(&client, *supports_a2a, json, quiet).await,
        AgentCommand::Start { path } => run_start(&client, path, json).await,
        AgentCommand::Stop { agent_id } => run_stop(&client, agent_id, json).await,
        AgentCommand::Show { agent_id } => run_info(&client, agent_id, json).await,
        AgentCommand::Status { agent_id } => run_status(&client, agent_id, json).await,
        AgentCommand::Messages { agent_id, limit } => {
            run_messages(&client, agent_id, *limit, json).await
        }
        AgentCommand::Install { source, skip_tests } => {
            run_install(source, &client, json, *skip_tests).await
        }
        AgentCommand::Uninstall { name } => run_uninstall(name, json),
        AgentCommand::Enable { name } => run_enable(&client, name, json).await,
        AgentCommand::Disable { name } => run_disable(&client, name, json).await,
        AgentCommand::Update { name, path } => run_update(name, path, json),
        AgentCommand::Create { name, r#type } => run_new(name, r#type, json),
        AgentCommand::Package { cmd } => match cmd {
            PackageCommand::List => run_package_list(json),
            PackageCommand::Show { name } => run_package_info(name, json),
            PackageCommand::Uninstall { name } => run_package_uninstall(name, json).await,
        },
        AgentCommand::Logs {
            agent_id,
            last,
            follow,
        } => run_logs(&client, agent_id, *last, *follow, json).await,
        AgentCommand::Validate { path } => run_validate(path, json),
        AgentCommand::Repair { name } => run_repair(name, json).await,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Existing commands (list/start/stop/info)
// ─────────────────────────────────────────────────────────────────────────────

/// Return true if `arg` looks like a file path rather than an agent name or UUID.
///
/// Detects the common mistake of passing a Python module path (e.g. `agents/foo.py`)
/// to commands that expect a name or UUID (e.g. `apollia-guide`).
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
fn open_repository() -> Option<AgentRepository> {
    let data_dir = apollia_data_dir();
    let db_path = apollia_core::paths::DataFile::Agents.path(&data_dir);
    if !db_path.exists() {
        return None;
    }
    AgentRepository::open(&db_path).ok()
}

/// Real agent loader using PyO3 AIPLoader + validate_agent.
///
/// Loads a Python module, validates AIP duck typing, and returns the manifest.
struct CliAgentLoader;

impl AgentLoader for CliAgentLoader {
    fn load_and_validate(&self, path: &Path) -> Result<apollia_core::AgentManifest, String> {
        // Share the community validation helper so `apollia-os agent
        // validate` benefits from the same sys.path injection as
        // `agent install`: per-agent venv + enclosing package root +
        // workspace SDK fallback.
        let extras = crate::community::validation_sys_paths(path);
        let module = apollia_aip::loader::load_agent_module_with_sys_paths(path, &extras)
            .map_err(|e| e.to_string())?;
        let validated =
            apollia_aip::validator::validate_agent(&module).map_err(|e| e.to_string())?;
        Ok(validated.manifest)
    }
}

/// Current timestamp in RFC 3339 format.
fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Print an error message (human or JSON) and return `GENERAL_ERROR`.
fn print_error_and_exit(msg: &str, json: bool) -> i32 {
    if json {
        let output = serde_json::json!({"error": msg});
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
    } else {
        eprintln!("Error: {msg}");
    }
    exit_codes::GENERAL_ERROR
}

/// Emit an `{"error": msg}` payload as compact JSON (or a plain `Error:`
/// line) and return `GENERAL_ERROR`. Distinct from `print_error_and_exit`,
/// which pretty-prints the JSON form.
fn print_compact_error_and_exit(msg: &str, json: bool) -> i32 {
    if json {
        println!("{}", serde_json::json!({ "error": msg }));
    } else {
        eprintln!("Error: {msg}");
    }
    exit_codes::GENERAL_ERROR
}
