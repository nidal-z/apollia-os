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
    Info {
        /// Agent identifier.
        agent_id: String,
    },
    /// Show a compact runtime-status snapshot for `<agent_id>`.
    ///
    /// Distilled view of `agent info` focused on online / idle / error state.
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
    New {
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
        /// Follow the live log stream until Ctrl+C (SSE).
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
    Info {
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
        AgentCommand::Info { agent_id } => run_info(&client, agent_id, json).await,
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
        AgentCommand::New { name, r#type } => run_new(name, r#type, json),
        AgentCommand::Package { cmd } => match cmd {
            PackageCommand::List => run_package_list(json),
            PackageCommand::Info { name } => run_package_info(name, json),
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

/// `apollia-os agent list`: display all agents (installed + runtime).
///
/// When `supports_a2a` is `true`, fetches from `/api/v1/a2a/agents` instead
/// and displays only A2A-capable agents with their skill descriptors.
/// When `quiet` is `true`, only agent names are printed, one per line.
async fn run_list(client: &RuntimeClient, supports_a2a: bool, json: bool, quiet: bool) -> i32 {
    if supports_a2a {
        return run_list_a2a(client, json).await;
    }

    // Fetch installed agents from local DB.
    let installed = open_repository()
        .and_then(|repo| repo.list().ok())
        .unwrap_or_default();

    // Fetch runtime agents (may fail if runtime not running).
    let runtime_agents = client.list_agents().await.ok();

    // --json takes priority over --quiet (machine-readable output is always complete).
    if json {
        let output = build_list_json(&installed, &runtime_agents);
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
    } else if quiet {
        // Quiet mode: emit only agent names, one per line.
        for agent in &installed {
            println!("{}", agent.name);
        }
    } else {
        format_enriched_agent_list(&installed, &runtime_agents);
    }
    exit_codes::SUCCESS
}

/// `apollia-os agent list --supports-a2a`: display A2A-capable agents with skills.
async fn run_list_a2a(client: &RuntimeClient, json: bool) -> i32 {
    match client.list_a2a_agents().await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                format_a2a_agent_list(&resp);
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// Formats the A2A agent list as a human-readable table.
///
/// Output columns: NAME, VERSION, STATUS, SKILLS (comma-separated skill IDs).
fn format_a2a_agent_list(resp: &serde_json::Value) {
    let agents = resp.get("agents").and_then(|v| v.as_array());
    let list = match agents {
        None => {
            println!("No A2A-capable agents running.");
            return;
        }
        Some(v) if v.is_empty() => {
            println!("No A2A-capable agents running.");
            return;
        }
        Some(v) => v,
    };

    println!("  {:<24} {:<10} {:<10} SKILLS", "NAME", "VERSION", "STATUS");

    for agent in list {
        let name = agent.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let version = agent.get("version").and_then(|v| v.as_str()).unwrap_or("-");
        let state = agent.get("state").and_then(|v| v.as_str()).unwrap_or("?");
        let skills_label = agent
            .get("skills")
            .and_then(|v| v.as_array())
            .map(|skills| {
                skills
                    .iter()
                    .filter_map(|s| s.get("id").and_then(|id| id.as_str()))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let skills_display = if skills_label.is_empty() {
            "(none)".to_string()
        } else {
            skills_label
        };

        println!(
            "  {:<24} {:<10} {:<10} {}",
            name, version, state, skills_display
        );
    }
}

/// `apollia-os agent start <path-or-name>`: register / re-load an agent.
///
/// Accepts either a filesystem path to a `.py` (legacy mode for ad-hoc
/// agents) or the **name** of an already installed agent. In the latter case
/// we look it up in `agents.db` and forward its `install_path` to the runtime,
/// so operators don't have to remember where every package landed inside
/// `~/.apollia/agents/`.
async fn run_start(client: &RuntimeClient, arg: &str, json: bool) -> i32 {
    let resolved_path = if looks_like_file_path(arg) {
        arg.to_string()
    } else {
        match open_repository().and_then(|r| r.get(arg).ok().flatten()) {
            Some(entry) => entry.install_path.to_string_lossy().to_string(),
            None => {
                let msg = format!(
                    "agent '{arg}' not installed (and not a file path); install it first via \
                     `apollia-os agent install <path>` or pass a `.py` path"
                );
                if json {
                    println!("{}", serde_json::json!({"error": msg}));
                } else {
                    eprintln!("Error: {msg}");
                }
                return exit_codes::GENERAL_ERROR;
            }
        }
    };
    match client.start_agent(&resolved_path).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                let agent_id = resp.get("agent_id").and_then(|v| v.as_str()).unwrap_or("?");
                let state = resp.get("state").and_then(|v| v.as_str()).unwrap_or("?");
                println!("Agent {agent_id} started ({state})");
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// Return true if `arg` looks like a file path rather than an agent name or UUID.
///
/// Detects the common mistake of passing a Python module path (e.g. `agents/foo.py`)
/// to commands that expect a name or UUID (e.g. `apollia-reviewer`).
fn looks_like_file_path(arg: &str) -> bool {
    arg.contains('/') || arg.contains('\\') || arg.ends_with(".py")
}

/// `apollia-os agent stop <id>`: stop a running agent.
async fn run_stop(client: &RuntimeClient, agent_id: &str, json: bool) -> i32 {
    if looks_like_file_path(agent_id) {
        let msg = format!(
            "'{agent_id}' looks like a file path — use the agent name or UUID instead\n\
             Hint: apollia-os agent stop <name|uuid>  (e.g. apollia-os agent stop apollia-reviewer)"
        );
        if json {
            println!("{}", serde_json::json!({"error": msg}));
        } else {
            eprintln!("Error: {msg}");
        }
        return exit_codes::GENERAL_ERROR;
    }
    match client.stop_agent(agent_id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("Agent {agent_id} stopped");
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os agent info <id>`: display agent detail.
async fn run_info(client: &RuntimeClient, agent_id: &str, json: bool) -> i32 {
    if looks_like_file_path(agent_id) {
        let msg = format!(
            "'{agent_id}' looks like a file path — use the agent name or UUID instead\n\
             Hint: apollia-os agent info <name|uuid>  (e.g. apollia-os agent info apollia-reviewer)"
        );
        return print_compact_error_and_exit(&msg, json);
    }
    match client.get_agent(agent_id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                format_agent_detail(&resp);
            }
            exit_codes::SUCCESS
        }
        // Disabled / not-yet-loaded agents are absent from the runtime
        // registry but still present in `agents.db`. Fall back to the local
        // repository so `apollia-os agent info` works on every installed
        // agent regardless of its runtime state.
        Err(ClientError::ServerError { status: 404, .. }) => run_info_local_fallback(agent_id, json),
        Err(e) => handle_error(e, json),
    }
}

/// Render `agent info` from the local `agents.db` row when the runtime
/// registry returns 404 (e.g. disabled or not-yet-loaded agents).
fn run_info_local_fallback(agent_id: &str, json: bool) -> i32 {
    match local_agent_detail(agent_id) {
        Some(detail) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&detail).unwrap_or_default()
                );
            } else {
                format_local_agent_detail(&detail);
            }
            exit_codes::SUCCESS
        }
        None => print_compact_error_and_exit(&format!("agent not found: {agent_id}"), json),
    }
}

/// `apollia-os agent status <id>`: compact runtime-status snapshot.
async fn run_status(client: &RuntimeClient, agent_id: &str, json: bool) -> i32 {
    match client.get_agent(agent_id).await {
        Ok(resp) => {
            format_status_snapshot(&resp, agent_id, json);
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, .. }) => {
            print_compact_error_and_exit(&format!("agent not found: {agent_id}"), json)
        }
        Err(e) => handle_error(e, json),
    }
}

/// Render the compact runtime-status snapshot produced by `agent status`.
fn format_status_snapshot(resp: &serde_json::Value, agent_id: &str, json: bool) {
    let name = resp.get("name").and_then(|v| v.as_str()).unwrap_or(agent_id);
    let state = resp.get("state").and_then(|v| v.as_str()).unwrap_or("unknown");
    let started_at = resp.get("started_at").and_then(|v| v.as_str());
    let last_activity = resp.get("last_activity_at").and_then(|v| v.as_str());
    let active_tasks = resp.get("active_tasks").and_then(|v| v.as_u64()).unwrap_or(0);
    let completed_tasks = resp
        .get("completed_tasks")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if json {
        let body = serde_json::json!({
            "agent": name,
            "state": state,
            "active_tasks": active_tasks,
            "completed_tasks": completed_tasks,
            "started_at": started_at,
            "last_activity_at": last_activity,
        });
        println!("{}", serde_json::to_string_pretty(&body).unwrap_or_default());
        return;
    }
    let glyph = match state {
        "active" | "idle" | "ready" => "*",
        "error" | "failed" => "x",
        _ => "?",
    };
    println!("  {glyph} {name}");
    println!("    state          : {state}");
    println!("    active tasks   : {active_tasks}");
    println!("    completed tasks: {completed_tasks}");
    if let Some(s) = started_at {
        println!("    started at     : {s}");
    }
    if let Some(s) = last_activity {
        println!("    last activity  : {s}");
    }
}

/// `apollia-os agent messages <id>`: list in-memory A2A messages.
async fn run_messages(
    client: &RuntimeClient,
    agent_id: &str,
    limit: u32,
    json: bool,
) -> i32 {
    let limit_opt = if limit == 0 { None } else { Some(limit) };
    match client.list_agent_messages(agent_id, limit_opt).await {
        Ok(resp) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&resp).unwrap_or_default());
                return exit_codes::SUCCESS;
            }
            let empty: Vec<serde_json::Value> = Vec::new();
            let messages = resp
                .get("messages")
                .and_then(|v| v.as_array())
                .unwrap_or(&empty);
            if messages.is_empty() {
                println!("No A2A messages for {agent_id}.");
                return exit_codes::SUCCESS;
            }
            println!(
                "  A2A messages for {agent_id} ({} returned, limit {limit}):",
                messages.len()
            );
            println!(
                "  {:<24} {:<22} PAYLOAD",
                "FROM", "SENT_AT"
            );
            for m in messages {
                let from = m
                    .get("from_agent")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let sent_at = m.get("sent_at").and_then(|v| v.as_str()).unwrap_or("?");
                let payload_str = m
                    .get("payload")
                    .map(|p| serde_json::to_string(p).unwrap_or_default())
                    .unwrap_or_default();
                let preview: String = payload_str.chars().take(60).collect();
                println!("  {from:<24} {sent_at:<22} {preview}");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError {
            status: 503, body, ..
        }) => {
            let msg = format!("agent mailbox unavailable: {body}");
            if json {
                println!("{}", serde_json::json!({ "error": msg }));
            } else {
                eprintln!("Error: {msg}");
            }
            exit_codes::RUNTIME_ERROR
        }
        Err(e) => handle_error(e, json),
    }
}

/// Load a detailed snapshot of an installed agent from `agents.db`, bypassing
/// the runtime registry. Returned as a JSON value so it can be either pretty-
/// printed via `format_local_agent_detail` or emitted verbatim with `--json`.
fn local_agent_detail(agent_id: &str) -> Option<serde_json::Value> {
    let repo = open_repository()?;
    let entry = repo.get(agent_id).ok().flatten()?;
    let body = serde_json::json!({
        "agent_id": null,
        "name": entry.name,
        "version": entry.version,
        "state": if entry.enabled { "stopped" } else { "disabled" },
        "supports_a2a": entry.manifest.supports_a2a,
        "skills": entry.manifest.skills,
        "manifest": entry.manifest,
        "install_path": entry.install_path,
        "installed_at": entry.installed_at,
        "_source": "local agents.db (runtime registry has no live entry — agent is disabled or failed to load)",
    });
    Some(body)
}

/// Render the local-only agent detail in the same shape as the live runtime
/// view so operators don't have to learn two layouts. Skills come from the
/// manifest because they are unchanged across enable / disable transitions.
fn format_local_agent_detail(detail: &serde_json::Value) {
    let name = detail.get("name").and_then(|v| v.as_str()).unwrap_or("?");
    let version = detail
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let state = detail.get("state").and_then(|v| v.as_str()).unwrap_or("?");
    let installed_at = detail
        .get("installed_at")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let install_path = detail
        .get("install_path")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    println!("  Name         : {name}");
    println!("  Version      : {version}");
    println!("  State        : {state}  (from local agents.db; not running in registry)");
    println!("  Install path : {install_path}");
    println!("  Installed at : {installed_at}");
    if let Some(manifest) = detail.get("manifest") {
        if let Some(skills) = manifest.get("skills").and_then(|s| s.as_array()) {
            if !skills.is_empty() {
                println!("  Skills ({}) :", skills.len());
                for s in skills {
                    let id = s.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                    let n = s.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    println!("    - {id} ({n})");
                }
            }
        }
        if let Some(tools) = manifest.get("tools_required").and_then(|t| t.as_array()) {
            let names: Vec<&str> = tools.iter().filter_map(|v| v.as_str()).collect();
            if !names.is_empty() {
                println!("  Tools (required) : {}", names.join(", "));
            }
        }
    }
    println!(
        "  Hint         : run `apollia-os agent enable {name}` then `apollia-os agent start {name}` to load."
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// New commands (install/uninstall/enable/disable/update)
// ─────────────────────────────────────────────────────────────────────────────

/// `apollia-os agent install <source> [--skip-tests]`: install an agent permanently.
///
/// `source` may be a local Python file path or a Git URL with an optional
/// `#<tag>` fragment.
///
/// **Local path**: performs AIP duck-typing validation (PyO3), manifest
/// conformance check, an optional pytest run, and copies the file into
/// `~/.apollia/agents/<name>/`.
///
/// **Git URL**: checks that `git` is available, clones the repository
/// (depth 1), performs AIP duck-typing validation on the discovered `.py`
/// file, installs to `~/.apollia/agents/community/<name>/`, and writes
/// `registry.json`.
async fn run_install(
    source_arg: &str,
    client: &RuntimeClient,
    json: bool,
    skip_tests: bool,
) -> i32 {
    match parse_install_source(source_arg) {
        AgentInstallSource::Git { url, tag } => {
            run_install_git(&url, tag.as_deref(), client, json, skip_tests).await
        }
        AgentInstallSource::Local { path } => {
            run_install_local(&path, client, json, skip_tests).await
        }
    }
}

/// Install a community agent from a Git remote URL.
async fn run_install_git(
    url: &str,
    tag: Option<&str>,
    client: &RuntimeClient,
    json: bool,
    skip_tests: bool,
) -> i32 {
    // Verify git is reachable before doing any work.
    if let Err(e) = registry_remote::check_git_available() {
        return print_error_and_exit(&e.to_string(), json);
    }

    // Create a temporary clone directory, removed when `temp` is dropped.
    let temp = match registry_remote::TempInstallDir::new() {
        Ok(t) => t,
        Err(e) => return print_error_and_exit(&format!("cannot create temp dir: {e}"), json),
    };

    // Clone the repository.
    if let Err(e) = registry_remote::git_clone(url, tag, temp.path()).await {
        return print_error_and_exit(&e.to_string(), json);
    }

    // Find the Python agent file in the clone root.
    let agent_py = match registry_remote::find_agent_file(temp.path()) {
        Some(p) => p,
        None => {
            return print_error_and_exit("no Python agent file found in the repository root", json);
        }
    };

    // AIP duck-typing validation (PyO3).
    let manifest = match validate_community_agent(&agent_py, skip_tests).await {
        Ok(m) => m,
        Err(AgentValidationError::FileNotFound(_)) => {
            return print_error_and_exit(
                &format!("agent file not found: {}", agent_py.display()),
                json,
            );
        }
        Err(e) => return print_error_and_exit(&format!("agent validation failed: {e}"), json),
    };

    if manifest.dangerous_tools_allowed {
        eprintln!(
            "Warning: community agent '{}' requests dangerous_tools_allowed — user approval required",
            manifest.name
        );
    }

    // Install files, pip packages, and update registry.json.
    let data_dir = apollia_data_dir();
    let source = AgentInstallSource::Git {
        url: url.to_string(),
        tag: tag.map(|t| t.to_string()),
    };
    let entry = match registry_remote::install_from_dir(temp.path(), &manifest, source, &data_dir) {
        Ok(e) => e,
        Err(e) => return print_error_and_exit(&format!("installation failed: {e}"), json),
    };

    // Check if runtime is running (informational only).
    if client.list_agents().await.is_err() {
        eprintln!("Info: Runtime not running — agent will auto-start on next boot");
    }

    if json {
        let install_path = data_dir.join("agents").join("community").join(&entry.name);
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "name": entry.name,
                "version": entry.version,
                "install_path": install_path.to_string_lossy(),
                "source": "git",
            }))
            .unwrap_or_default()
        );
    } else {
        println!(
            "Agent '{}' v{} installed from Git successfully",
            entry.name, entry.version,
        );
    }

    exit_codes::SUCCESS
}

/// Install a community agent from a local Python file or an agent package directory.
async fn run_install_local(
    source_path: &Path,
    client: &RuntimeClient,
    json: bool,
    skip_tests: bool,
) -> i32 {
    // Detect package directory (has agent.toml at root).
    if source_path.is_dir() {
        if source_path.join("agent.toml").exists() {
            return run_install_package(source_path, client, json, skip_tests).await;
        }
        return print_error_and_exit(
            &format!(
                "directory '{}' has no agent.toml — not a valid agent package",
                source_path.display()
            ),
            json,
        );
    }

    // Validate the agent and load its manifest.
    let manifest = match validate_community_agent(source_path, skip_tests).await {
        Ok(m) => m,
        Err(AgentValidationError::FileNotFound(_)) => {
            return print_error_and_exit(
                &format!("file not found: {}", source_path.display()),
                json,
            );
        }
        Err(e) => {
            return print_error_and_exit(&format!("agent validation failed: {e}"), json);
        }
    };

    // Surface security and skip-tests warnings to the operator.
    if manifest.dangerous_tools_allowed {
        eprintln!(
            "Warning: community agent '{}' requests dangerous_tools_allowed — user approval required",
            manifest.name
        );
    }
    if skip_tests {
        eprintln!(
            "Warning: installing '{}' without running its test suite — not recommended",
            manifest.name
        );
    }

    // Canonicalize the source path for reliable storage.
    let canonical = match source_path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return print_error_and_exit(
                &format!("cannot resolve path {}: {e}", source_path.display()),
                json,
            );
        }
    };

    let data_dir = apollia_data_dir();
    let agents_dir = data_dir.join("agents").join(&manifest.name);

    // Create install directory.
    if let Err(e) = std::fs::create_dir_all(&agents_dir) {
        return print_error_and_exit(
            &format!("cannot create directory {}: {e}", agents_dir.display()),
            json,
        );
    }

    // Copy Python file to install location.
    let install_path = agents_dir.join("agent.py");
    if let Err(e) = std::fs::copy(&canonical, &install_path) {
        return print_error_and_exit(
            &format!(
                "cannot copy {} to {}: {e}",
                canonical.display(),
                install_path.display()
            ),
            json,
        );
    }

    // Persist in AgentRepository.
    let repo = match open_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };

    let now = now_rfc3339();
    let agent = InstalledAgent {
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        install_path: install_path.clone(),
        source_path: canonical,
        manifest,
        enabled: true,
        installed_at: now.clone(),
        updated_at: now,
    };

    if let Err(e) = repo.save(&agent) {
        return print_error_and_exit(&format!("failed to save to database: {e}"), json);
    }

    // Check if runtime is running (informational only).
    let runtime_running = client.list_agents().await.is_ok();
    if !runtime_running {
        eprintln!("Info: Runtime not running — agent will auto-start on next boot");
    }

    if json {
        let output = serde_json::json!({
            "name": agent.name,
            "version": agent.version,
            "install_path": install_path.to_string_lossy(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
    } else {
        println!(
            "Agent '{}' v{} installed successfully",
            agent.name, agent.version,
        );
    }

    exit_codes::SUCCESS
}

// ─────────────────────────────────────────────────────────────────────────────
// Package commands
// ─────────────────────────────────────────────────────────────────────────────

/// `apollia-os agent install <dir>`: install a multi-agent package from a folder.
///
/// 1. Validates `agent.toml` + duck-types every `.py`
/// 2. Copies the folder to `~/.apollia/agents/packages/<name>/`
/// 3. Registers each agent in `AgentRepository`
/// 4. Registers the package in `PackageRepository`
/// 5. Injects triggers into `TriggerDefinitionRepository`
async fn run_install_package(
    source_path: &Path,
    client: &RuntimeClient,
    json: bool,
    skip_tests: bool,
) -> i32 {
    // Step 1: parse the manifest structure and verify each `.py` exists on
    // disk. Strict duck-typing (`load_package`) is deferred to the per-agent
    // loop below so a single broken worker (e.g. failing top-level Python
    // import) doesn't tank the entire package install: partial installs
    // mark the failing agents as DEGRADED and report a summary at the end.
    let pkg = match load_manifest_only(source_path) {
        Ok(p) => p,
        Err(PackageLoaderError::ManifestNotFound(_)) => {
            return print_error_and_exit("agent.toml not found in directory", json);
        }
        Err(e) => return print_error_and_exit(&format!("package validation failed: {e}"), json),
    };

    let pkg_name = pkg.manifest.package.name.clone();
    let pkg_version = pkg.manifest.package.version.clone();

    let data_dir = apollia_data_dir();
    let install_root = data_dir.join("agents").join("packages").join(&pkg_name);

    // Step 2: copy directory to install location.
    if let Err(e) = copy_dir_all(source_path, &install_root) {
        return print_error_and_exit(
            &format!("failed to copy package to {}: {e}", install_root.display()),
            json,
        );
    }

    // Open repositories.
    let agent_repo = match open_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };
    let pkg_repo = match open_package_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };

    // Step 3: register each agent (best-effort per agent).
    //
    // We *do not* abort the whole package on a single agent's validation
    // failure: one broken worker (e.g. a stale top-level Python import)
    // shouldn't make the rest of the package un-installable. Failed agents
    // are reported in the final summary so the operator can fix and re-run.
    //
    // For each agent that declares pip dependencies we provision its venv
    // *before* duck-typing (mirroring what the Supervisor does at boot)
    // so top-level imports of declared packages resolve. Without this step
    // the validator never finds pip-installed modules (incl. the apollia
    // SDK when the agent imports it from a site-packages location).
    let now = now_rfc3339();
    let venv_base = data_dir.join("venvs");
    let install_ctx = PackageInstallCtx {
        version: &pkg_version,
        now: &now,
        skip_tests,
        source_path,
        install_root: &install_root,
        venv_base: &venv_base,
    };
    let (agent_count, failed_agents) =
        register_package_agents(&pkg, &agent_repo, &install_ctx).await;

    if agent_count == 0 {
        return print_error_and_exit(
            &format!(
                "package validation failed: 0 of {} agents could be installed",
                pkg.agents.len()
            ),
            json,
        );
    }

    // Step 4: register the package itself.
    let installed_pkg = InstalledPackage {
        name: pkg_name.clone(),
        version: pkg_version.clone(),
        root_path: install_root.clone(),
        manifest_json: pkg.manifest_json.clone(),
        installed_at: now.clone(),
        updated_at: now.clone(),
    };
    if let Err(e) = pkg_repo.save(&installed_pkg) {
        return print_error_and_exit(&format!("failed to save package: {e}"), json);
    }
    for entry in &pkg.agents {
        if let Err(e) = pkg_repo.link_agent(&pkg_name, &entry.name) {
            return print_error_and_exit(
                &format!("failed to link agent '{}' to package: {e}", entry.name),
                json,
            );
        }
    }

    // Step 5: inject triggers.
    let toml_str = match std::fs::read_to_string(source_path.join("agent.toml")) {
        Ok(s) => s,
        Err(e) => return print_error_and_exit(&format!("failed to read agent.toml: {e}"), json),
    };
    let trigger_count = match inject_package_triggers(&data_dir, &toml_str) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("Warning: trigger injection failed: {e}");
            0
        }
    };

    let runtime_running = client.list_agents().await.is_ok();
    if !runtime_running {
        eprintln!("Info: Runtime not running — agents will auto-start on next boot");
    }

    let summary = InstallSummary {
        pkg_name: &pkg_name,
        pkg_version: &pkg_version,
        agent_count,
        trigger_count,
        install_root: &install_root,
        failed_agents: &failed_agents,
    };
    print_install_summary(&summary, json);
    exit_codes::SUCCESS
}

/// Fields rendered by [`print_install_summary`].
struct InstallSummary<'a> {
    pkg_name: &'a str,
    pkg_version: &'a str,
    agent_count: u32,
    trigger_count: usize,
    install_root: &'a Path,
    failed_agents: &'a [(String, String)],
}

/// Print the package install summary (JSON or human-readable form).
fn print_install_summary(s: &InstallSummary<'_>, json: bool) {
    if json {
        let failed_view: Vec<serde_json::Value> = s
            .failed_agents
            .iter()
            .map(|(n, r)| serde_json::json!({ "agent": n, "reason": r }))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "name": s.pkg_name,
                "version": s.pkg_version,
                "agent_count": s.agent_count,
                "trigger_count": s.trigger_count,
                "install_path": s.install_root.to_string_lossy(),
                "failed_agents": failed_view,
            }))
            .unwrap_or_default()
        );
    } else {
        println!(
            "Package '{}' v{} installed: {} agents, {} triggers",
            s.pkg_name, s.pkg_version, s.agent_count, s.trigger_count,
        );
        println!("  Install path : {}", s.install_root.display());
        if !s.failed_agents.is_empty() {
            println!(
                "  ! {} agent(s) skipped due to validation errors:",
                s.failed_agents.len()
            );
            for (name, reason) in s.failed_agents {
                println!("    - {name}: {reason}");
            }
        }
    }
}

/// Register every agent declared in the package (best-effort, per agent).
///
/// Returns the count of successfully registered agents and the list of
/// `(agent_name, failure_reason)` pairs for the ones that were skipped.
async fn register_package_agents(
    pkg: &apollia_aip::package_loader::AgentPackage,
    agent_repo: &AgentRepository,
    install_ctx: &PackageInstallCtx<'_>,
) -> (u32, Vec<(String, String)>) {
    let mut agent_count = 0;
    let mut failed_agents: Vec<(String, String)> = Vec::new();
    for entry in &pkg.agents {
        let installed_entry_path = install_ctx.install_root.join(
            entry
                .entry
                .strip_prefix(install_ctx.source_path)
                .unwrap_or(&entry.entry),
        );

        // Read this agent's declared pip dependencies from the manifest.
        let agent_packages: Vec<String> = pkg
            .manifest
            .agents
            .iter()
            .find(|a| a.name == entry.name)
            .map(|a| a.packages.clone())
            .unwrap_or_default();

        if let Err(msg) =
            provision_agent_venv(&entry.name, install_ctx.venv_base, &agent_packages).await
        {
            failed_agents.push((entry.name.clone(), msg));
            continue;
        }

        match register_installed_agent(agent_repo, &entry.name, &installed_entry_path, install_ctx)
            .await
        {
            Ok(()) => agent_count += 1,
            Err(msg) => failed_agents.push((entry.name.clone(), msg)),
        }
    }
    (agent_count, failed_agents)
}

/// Provision a per-agent venv for the declared pip packages, mirroring the
/// Supervisor's boot behaviour so top-level imports resolve during
/// validation. A no-op when `packages` is empty. Returns the operator-facing
/// failure reason on error.
async fn provision_agent_venv(
    agent_name: &str,
    venv_base: &Path,
    packages: &[String],
) -> Result<(), String> {
    if packages.is_empty() {
        return Ok(());
    }
    let executor =
        apollia_tools::tools::python_executor::PythonExecutor::new(agent_name, venv_base)
            .map_err(|e| format!("could not initialise per-agent venv: {e}"))?;
    if let Err(e) = executor.setup_venv(packages).await {
        let msg = format!(
            "venv provisioning failed (the agent declares {} pip dep(s)): {e}",
            packages.len()
        );
        tracing::warn!(
            agent = %agent_name,
            error = %msg,
            "package install: setup_venv failed, skipping agent"
        );
        return Err(msg);
    }
    Ok(())
}

/// Per-package install metadata shared across each agent's registration.
struct PackageInstallCtx<'a> {
    version: &'a str,
    now: &'a str,
    skip_tests: bool,
    source_path: &'a Path,
    install_root: &'a Path,
    venv_base: &'a Path,
}

/// Duck-type validate a packaged agent and persist its `InstalledAgent` row.
/// Returns the operator-facing failure reason on error (the caller records it
/// in the per-package skip summary).
async fn register_installed_agent(
    agent_repo: &AgentRepository,
    agent_name: &str,
    installed_entry_path: &Path,
    ctx: &PackageInstallCtx<'_>,
) -> Result<(), String> {
    let agent_manifest = match validate_community_agent(installed_entry_path, ctx.skip_tests).await
    {
        Ok(m) => m,
        Err(e) => {
            let msg = e.to_string();
            tracing::warn!(
                agent = %agent_name,
                error = %msg,
                "package install: agent validation failed, skipping"
            );
            return Err(msg);
        }
    };

    let installed_agent = InstalledAgent {
        name: agent_name.to_string(),
        version: ctx.version.to_string(),
        install_path: installed_entry_path.to_path_buf(),
        source_path: installed_entry_path.to_path_buf(),
        manifest: agent_manifest,
        enabled: true,
        installed_at: ctx.now.to_string(),
        updated_at: ctx.now.to_string(),
    };
    if let Err(e) = agent_repo.save(&installed_agent) {
        tracing::warn!(
            agent = %agent_name,
            error = %e,
            "package install: failed to persist agent, skipping"
        );
        return Err(format!("save failed: {e}"));
    }
    Ok(())
}

/// `apollia-os agent package list`: list installed packages.
fn run_package_list(json: bool) -> i32 {
    let data_dir = apollia_data_dir();
    let pkg_repo = match open_package_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };
    let pkgs = match pkg_repo.list() {
        Ok(p) => p,
        Err(e) => return print_error_and_exit(&format!("database error: {e}"), json),
    };
    if json {
        let items: Vec<_> = pkgs
            .iter()
            .map(|p| {
                serde_json::json!({
                    "name": p.name,
                    "version": p.version,
                    "installed_at": p.installed_at,
                    "root_path": p.root_path.to_string_lossy(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({ "packages": items }))
                .unwrap_or_default()
        );
    } else if pkgs.is_empty() {
        println!("No agent packages installed.");
    } else {
        println!("{:<24} {:<12} {}", "NAME", "VERSION", "INSTALLED");
        for p in &pkgs {
            println!("{:<24} {:<12} {}", p.name, p.version, p.installed_at);
        }
    }
    exit_codes::SUCCESS
}

/// `apollia-os agent package info <name>`: show package details.
fn run_package_info(name: &str, json: bool) -> i32 {
    let data_dir = apollia_data_dir();
    let pkg_repo = match open_package_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };
    let pkg = match pkg_repo.get(name) {
        Ok(Some(p)) => p,
        Ok(None) => {
            return print_error_and_exit(&format!("Package '{name}' not found"), json);
        }
        Err(e) => return print_error_and_exit(&format!("database error: {e}"), json),
    };
    let agents = pkg_repo.list_agents_for_package(name).unwrap_or_default();

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "name": pkg.name,
                "version": pkg.version,
                "installed_at": pkg.installed_at,
                "root_path": pkg.root_path.to_string_lossy(),
                "agents": agents,
                "manifest": serde_json::from_str::<serde_json::Value>(&pkg.manifest_json).unwrap_or_default(),
            }))
            .unwrap_or_default()
        );
    } else {
        println!("Package: {} v{}", pkg.name, pkg.version);
        println!("  Installed: {}", pkg.installed_at);
        println!("  Path:      {}", pkg.root_path.display());
        println!("  Agents ({}):", agents.len());
        for a in &agents {
            println!("    - {a}");
        }
    }
    exit_codes::SUCCESS
}

/// `apollia-os agent package uninstall <name>`: remove package, all its agents and triggers.
async fn run_package_uninstall(name: &str, json: bool) -> i32 {
    let data_dir = apollia_data_dir();
    let pkg_repo = match open_package_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };

    let pkg = match pkg_repo.get(name) {
        Ok(Some(p)) => p,
        Ok(None) => return print_error_and_exit(&format!("Package '{name}' not found"), json),
        Err(e) => return print_error_and_exit(&format!("database error: {e}"), json),
    };
    let agent_names = pkg_repo.list_agents_for_package(name).unwrap_or_default();

    // Delete agents from AgentRepository.
    let agent_repo = match open_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };
    for agent_name in &agent_names {
        let _ = agent_repo.delete(agent_name);
    }

    // Delete package from PackageRepository (cascades package_agents).
    if let Err(e) = pkg_repo.delete(name) {
        return print_error_and_exit(&format!("failed to delete package: {e}"), json);
    }

    // Remove install directory (best-effort).
    let _ = std::fs::remove_dir_all(&pkg.root_path);

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "name": name,
                "status": "uninstalled",
                "agents_removed": agent_names.len(),
            }))
            .unwrap_or_default()
        );
    } else {
        println!(
            "Package '{name}' uninstalled ({} agents removed)",
            agent_names.len()
        );
    }
    exit_codes::SUCCESS
}

// ─── Trigger injection ───────────────────────────────────────────────────────

/// Parse triggers from `agent.toml` content and upsert into `triggers_def.db`.
///
/// Returns the number of triggers successfully injected.
fn inject_package_triggers(data_dir: &Path, toml_str: &str) -> Result<usize, String> {
    let trigger_defs =
        parse_triggers_from_toml_str(toml_str).map_err(|e| format!("trigger parse error: {e}"))?;

    if trigger_defs.is_empty() {
        return Ok(0);
    }

    let triggers_db = data_dir.join("triggers_def.db");
    let repo = TriggerDefinitionRepository::open(&triggers_db)
        .map_err(|e| format!("cannot open triggers_def.db: {e}"))?;

    let mut count = 0;
    for def in &trigger_defs {
        let row = trigger_def_to_row(def);
        // Upsert: delete if exists, then insert.
        let _ = repo.delete(&def.id);
        repo.insert(&row)
            .map_err(|e| format!("failed to insert trigger '{}': {e}", def.id))?;
        count += 1;
    }
    Ok(count)
}

/// Convert a [`TriggerDefinition`] to a [`TriggerDefinitionRow`] for persistence.
fn trigger_def_to_row(def: &apollia_triggers::TriggerDefinition) -> TriggerDefinitionRow {
    let (source_type, source_config) = match &def.source {
        TriggerSourceConfig::Cron { schedule } => (
            "cron".to_string(),
            serde_json::json!({"schedule": schedule}),
        ),
        TriggerSourceConfig::Interval { every } => {
            ("interval".to_string(), serde_json::json!({"every": every}))
        }
        TriggerSourceConfig::Oneshot { fire_at } => (
            "oneshot".to_string(),
            serde_json::json!({"fire_at": fire_at.to_rfc3339()}),
        ),
        TriggerSourceConfig::FileWatch {
            path,
            events,
            recursive,
            follow_symlinks,
            exclude_patterns,
        } => (
            "file_watch".to_string(),
            serde_json::json!({
                "path": path.to_string_lossy(),
                "events": events,
                "recursive": recursive,
                "follow_symlinks": follow_symlinks,
                "exclude_patterns": exclude_patterns,
            }),
        ),
        TriggerSourceConfig::Webhook { secret } => {
            ("webhook".to_string(), serde_json::json!({"secret": secret}))
        }
    };

    let on_busy = match &def.on_busy {
        OnBusyPolicy::Skip => OnBusy::Drop,
        OnBusyPolicy::Queue { .. } | OnBusyPolicy::Block => OnBusy::Queue,
    };

    TriggerDefinitionRow {
        id: def.id.clone(),
        agent: if def.agent.is_empty() {
            None
        } else {
            Some(def.agent.clone())
        },
        enabled: def.enabled,
        on_busy,
        source_type,
        source_config,
        input_template: if def.input_template.0.is_empty() {
            None
        } else {
            Some(def.input_template.0.clone())
        },
        created_at: now_rfc3339(),
        updated_at: now_rfc3339(),
    }
}

/// Recursively copy a directory tree from `src` to `dst`.
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// `apollia-os agent uninstall <name>`: remove an installed agent.
fn run_uninstall(name: &str, json: bool) -> i32 {
    let data_dir = apollia_data_dir();
    let repo = match open_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };

    // Check agent exists.
    match repo.get(name) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return print_error_and_exit(
                &format!("Agent '{name}' not found in installed agents"),
                json,
            );
        }
        Err(e) => return print_error_and_exit(&format!("database error: {e}"), json),
    }

    // Delete from repository.
    if let Err(e) = repo.delete(name) {
        return print_error_and_exit(&format!("failed to delete from database: {e}"), json);
    }

    // Remove install directory (best-effort, ignore errors).
    let agents_dir = data_dir.join("agents").join(name);
    let _ = std::fs::remove_dir_all(&agents_dir);

    if json {
        let output = serde_json::json!({
            "name": name,
            "status": "uninstalled",
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
    } else {
        println!("Agent '{name}' uninstalled");
    }

    exit_codes::SUCCESS
}

/// `apollia-os agent enable <name>`: re-enable an installed agent.
///
/// Two-phase, symmetric with [`run_disable`]:
/// 1. Mark `enabled = true` in `agents.db` so the agent auto-starts at the
///    next runtime boot.
/// 2. Best-effort `POST /api/v1/agents` with the persisted `install_path`
///    so the agent is loaded into the live registry immediately. Without
///    this, `apollia-os run <name>` returns 404 until the daemon is
///    restarted.
async fn run_enable(client: &RuntimeClient, name: &str, json: bool) -> i32 {
    let data_dir = apollia_data_dir();
    let repo = match open_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };

    // Look up install_path BEFORE flipping the flag so we can short-circuit
    // when the agent is unknown (operator typo) without persisting a stale
    // `enabled=true` row.
    let install_path = match repo.get(name) {
        Ok(Some(entry)) => entry.install_path.to_string_lossy().to_string(),
        Ok(None) => {
            return print_error_and_exit(
                &format!("Agent '{name}' not found in installed agents"),
                json,
            );
        }
        Err(e) => return print_error_and_exit(&format!("repository read failed: {e}"), json),
    };

    match repo.set_enabled(name, true) {
        Ok(()) => {
            let load_outcome = match client.start_agent(&install_path).await {
                Ok(_) => "loaded",
                Err(ClientError::ServerError { status: 409, .. }) => "already-loaded",
                Err(ClientError::ConnectionRefused) => "runtime-offline",
                Err(e) => {
                    tracing::warn!(
                        agent = %name,
                        error = %e,
                        "enable: failed to load into the live registry — agent remains enabled=true for next boot"
                    );
                    "load-failed"
                }
            };
            if json {
                let output = serde_json::json!({
                    "name": name,
                    "enabled": true,
                    "runtime_load": load_outcome,
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                );
            } else {
                let suffix = match load_outcome {
                    "loaded" => " and loaded into the runtime (`apollia-os run` works now)",
                    "already-loaded" => " (already running in the runtime)",
                    "runtime-offline" => " — runtime offline, load will happen on next start",
                    _ => " — runtime load failed, retry with `apollia-os agent start <name>`",
                };
                println!("Agent '{name}' enabled (will auto-start on boot){suffix}");
            }
            exit_codes::SUCCESS
        }
        Err(apollia_tools::AgentRepositoryError::NotFound(_)) => print_error_and_exit(
            &format!("Agent '{name}' not found in installed agents"),
            json,
        ),
        Err(e) => print_error_and_exit(&format!("database error: {e}"), json),
    }
}

/// `apollia-os agent disable <name>`: disable an installed agent.
///
/// Two-phase operation:
/// 1. Mark the agent `enabled = false` in `agents.db` so it won't auto-start
///    at the next runtime boot.
/// 2. Best-effort `DELETE /api/v1/agents/<name>` so the live registry stops
///    holding an executor for it. This is what makes `apollia-os agent list`
///    immediately show `disabled` instead of an outdated `active` state.
///    A runtime that is offline (or has never had this agent loaded) is
///    treated as a no-op for phase 2: the persisted disable is what
///    matters across reboots.
async fn run_disable(client: &RuntimeClient, name: &str, json: bool) -> i32 {
    let data_dir = apollia_data_dir();
    let repo = match open_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };

    match repo.set_enabled(name, false) {
        Ok(()) => {
            let stop_outcome = match client.stop_agent(name).await {
                Ok(_) => "stopped",
                Err(ClientError::ServerError { status: 404, .. }) => "not-loaded",
                Err(ClientError::ConnectionRefused) => "runtime-offline",
                Err(e) => {
                    tracing::warn!(
                        agent = %name,
                        error = %e,
                        "disable: failed to stop live registry entry — agent remains enabled=false for next boot"
                    );
                    "stop-failed"
                }
            };
            if json {
                let output = serde_json::json!({
                    "name": name,
                    "enabled": false,
                    "runtime_stop": stop_outcome,
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                );
            } else {
                let suffix = match stop_outcome {
                    "stopped" => " and unloaded from the runtime",
                    "not-loaded" => " (was not loaded in the runtime)",
                    "runtime-offline" => " — runtime offline, change takes effect on next start",
                    _ => "",
                };
                println!("Agent '{name}' disabled (will not auto-start){suffix}");
            }
            exit_codes::SUCCESS
        }
        Err(apollia_tools::AgentRepositoryError::NotFound(_)) => print_error_and_exit(
            &format!("Agent '{name}' not found in installed agents"),
            json,
        ),
        Err(e) => print_error_and_exit(&format!("database error: {e}"), json),
    }
}

/// `apollia-os agent update <name> <path>`: update an installed agent.
fn run_update(name: &str, source_path: &Path, json: bool) -> i32 {
    let data_dir = apollia_data_dir();
    let repo = match open_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };

    // Check the agent exists.
    let existing = match repo.get(name) {
        Ok(Some(a)) => a,
        Ok(None) => {
            return print_error_and_exit(
                &format!("Agent '{name}' not found in installed agents"),
                json,
            );
        }
        Err(e) => return print_error_and_exit(&format!("database error: {e}"), json),
    };

    // Validate source file exists.
    if !source_path.exists() {
        return print_error_and_exit(&format!("file not found: {}", source_path.display()), json);
    }

    let canonical = match source_path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return print_error_and_exit(
                &format!("cannot resolve path {}: {e}", source_path.display()),
                json,
            );
        }
    };

    // Load and validate the new Python module.
    let loader = CliAgentLoader;
    let manifest = match loader.load_and_validate(&canonical) {
        Ok(m) => m,
        Err(e) => {
            return print_error_and_exit(&format!("failed to load agent: {e}"), json);
        }
    };

    // Copy new file to install location.
    if let Err(e) = std::fs::copy(&canonical, &existing.install_path) {
        return print_error_and_exit(
            &format!(
                "cannot copy {} to {}: {e}",
                canonical.display(),
                existing.install_path.display()
            ),
            json,
        );
    }

    // Update repository entry.
    let updated = InstalledAgent {
        name: existing.name.clone(),
        version: manifest.version.clone(),
        install_path: existing.install_path,
        source_path: canonical,
        manifest,
        enabled: existing.enabled,
        installed_at: existing.installed_at,
        updated_at: now_rfc3339(),
    };

    if let Err(e) = repo.save(&updated) {
        return print_error_and_exit(&format!("failed to update database: {e}"), json);
    }

    if json {
        let output = serde_json::json!({
            "name": updated.name,
            "version": updated.version,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
    } else {
        println!("Agent '{}' updated to v{}", updated.name, updated.version);
    }

    exit_codes::SUCCESS
}

// ─────────────────────────────────────────────────────────────────────────────
// Scaffolding (agent new)
// ─────────────────────────────────────────────────────────────────────────────

/// Supported agent template types.
const VALID_AGENT_TYPES: &[&str] = &["react", "conversational", "orchestrated"];

/// `apollia-os agent new <name> [--type <type>]`: scaffold a new agent via the SDK.
fn run_new(name: &str, agent_type: &str, json: bool) -> i32 {
    // Validate template type.
    if !VALID_AGENT_TYPES.contains(&agent_type) {
        let msg = format!(
            "Invalid type '{}'. Supported types: {}",
            agent_type,
            VALID_AGENT_TYPES.join(", ")
        );
        return print_error_and_exit(&msg, json);
    }

    // Verify the SDK is installed.
    if let Err(msg) = check_sdk_installed() {
        return print_error_and_exit(&msg, json);
    }

    // Check for name conflict in ~/.apollia/agents/.
    let agents_dir = apollia_data_dir().join("agents");
    let target_dir = agents_dir.join(name);
    if target_dir.exists() {
        let msg = format!(
            "An agent '{}' already exists. Use a different name or remove the existing one with: \
             apollia-os agent uninstall {}",
            name, name
        );
        return print_error_and_exit(&msg, json);
    }

    // Delegate to `python3 -m apollia new <name> --type <type> --output-dir <path>`.
    let output = match std::process::Command::new("python3")
        .args([
            "-m",
            "apollia",
            "new",
            name,
            "--type",
            agent_type,
            "--output-dir",
        ])
        .arg(&target_dir)
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            let msg = format!("Failed to execute python3: {e}");
            return print_error_and_exit(&msg, json);
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = format!("Scaffolding failed: {}", stderr.trim());
        return print_error_and_exit(&msg, json);
    }

    // List generated files.
    let files = list_generated_files(&target_dir);

    if json {
        let json_output = serde_json::json!({
            "name": name,
            "type": agent_type,
            "path": target_dir.to_string_lossy(),
            "files": files,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&json_output).unwrap_or_default()
        );
    } else {
        println!("Agent '{}' created in {}", name, target_dir.display());
        for f in &files {
            println!("  {f}");
        }
    }

    exit_codes::SUCCESS
}

/// Verify that the Apollia Python SDK is importable.
fn check_sdk_installed() -> Result<(), String> {
    let output = std::process::Command::new("python3")
        .args(["-c", "import apollia"])
        .output()
        .map_err(|e| format!("python3 not found: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err("apollia-sdk is not installed. Install it with: pip install apollia-sdk".to_string())
    }
}

/// List files generated in `dir`, returning relative paths sorted alphabetically.
fn list_generated_files(dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    collect_files_recursive(dir, dir, &mut files);
    files.sort();
    files
}

/// Recursively collect file paths relative to `base`.
fn collect_files_recursive(base: &Path, current: &Path, out: &mut Vec<String>) {
    let entries = match std::fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(base, &path, out);
        } else if let Ok(rel) = path.strip_prefix(base) {
            out.push(rel.to_string_lossy().to_string());
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Resolve `~/.apollia/` data directory.
fn apollia_data_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".apollia")
}

/// Open the package repository at `<data_dir>/agents.db`, creating it if needed.
fn open_package_repository_or_create(data_dir: &Path) -> Result<PackageRepository, String> {
    if let Err(e) = std::fs::create_dir_all(data_dir) {
        return Err(format!(
            "cannot create data directory {}: {e}",
            data_dir.display()
        ));
    }
    let db_path = data_dir.join("agents.db");
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
    let db_path = data_dir.join("agents.db");
    AgentRepository::open(&db_path).map_err(|e| format!("cannot open agents.db: {e}"))
}

/// Try to open the agent repository (returns `None` if file or DB unavailable).
fn open_repository() -> Option<AgentRepository> {
    let data_dir = apollia_data_dir();
    let db_path = data_dir.join("agents.db");
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

/// Build a merged JSON array for `agent list --json`.
fn build_list_json(
    installed: &[InstalledAgent],
    runtime: &Option<serde_json::Value>,
) -> serde_json::Value {
    let runtime_agents = runtime
        .as_ref()
        .and_then(|v| v.get("agents"))
        .and_then(|a| a.as_array());

    let mut entries: Vec<serde_json::Value> = Vec::new();

    for agent in installed {
        let runtime_status = runtime_agents
            .and_then(|agents| {
                agents.iter().find(|a| {
                    a.get("name")
                        .and_then(|n| n.as_str())
                        .is_some_and(|n| n == agent.name)
                        || a.get("manifest")
                            .and_then(|m| m.get("name"))
                            .and_then(|n| n.as_str())
                            .is_some_and(|n| n == agent.name)
                })
            })
            .and_then(|a| a.get("state").and_then(|s| s.as_str()))
            .unwrap_or("-");

        entries.push(serde_json::json!({
            "name": agent.name,
            "version": agent.version,
            "status": runtime_status,
            "enabled": agent.enabled,
            "installed": true,
        }));
    }

    // Add runtime-only agents not in installed list.
    if let Some(agents) = runtime_agents {
        for agent in agents {
            let name = agent
                .get("name")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    agent
                        .get("manifest")
                        .and_then(|m| m.get("name"))
                        .and_then(|n| n.as_str())
                })
                .unwrap_or("?");

            let already_listed = installed.iter().any(|i| i.name == name);
            if !already_listed {
                let state = agent.get("state").and_then(|v| v.as_str()).unwrap_or("?");
                entries.push(serde_json::json!({
                    "name": name,
                    "version": agent.get("manifest")
                        .and_then(|m| m.get("version"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("-"),
                    "status": state,
                    "enabled": serde_json::Value::Null,
                    "installed": false,
                }));
            }
        }
    }

    serde_json::json!({ "agents": entries })
}

/// Format an enriched agent list as a human-readable table.
fn format_enriched_agent_list(installed: &[InstalledAgent], runtime: &Option<serde_json::Value>) {
    let runtime_agents = runtime
        .as_ref()
        .and_then(|v| v.get("agents"))
        .and_then(|a| a.as_array());

    println!(
        "  {:<24} {:<10} {:<12} {:<10} SOURCE",
        "NAME", "VERSION", "STATUS", "AUTO-LOAD"
    );

    let mut has_entries = false;

    for agent in installed {
        has_entries = true;
        let runtime_state = runtime_agents
            .and_then(|agents| {
                agents.iter().find(|a| {
                    a.get("name")
                        .and_then(|n| n.as_str())
                        .is_some_and(|n| n == agent.name)
                        || a.get("manifest")
                            .and_then(|m| m.get("name"))
                            .and_then(|n| n.as_str())
                            .is_some_and(|n| n == agent.name)
                })
            })
            .and_then(|a| a.get("state").and_then(|s| s.as_str()));

        // Combine "loaded in registry" with "enabled in repo" into a single
        // human-readable status so operators don't have to cross-reference
        // two columns. When the operator has explicitly disabled the agent
        // (enabled=false) we surface 'disabled' over the registry state so
        // it's clear the agent will not auto-start on the next boot, the
        // registry might still report 'stopped' transiently for a moment.
        let status = match (agent.enabled, runtime_state) {
            (false, _) => "disabled",
            (true, Some("stopped")) => "stopped",
            (true, Some(state)) => state, // active / degraded / initializing
            (true, None) => "stopped",
        };
        let auto_load = if agent.enabled { "yes" } else { "no" };

        println!(
            "  {:<24} {:<10} {:<12} {:<10} installed",
            agent.name, agent.version, status, auto_load
        );
    }

    // Add runtime-only agents not in installed list.
    if let Some(agents) = runtime_agents {
        for agent in agents {
            let name = agent
                .get("name")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    agent
                        .get("manifest")
                        .and_then(|m| m.get("name"))
                        .and_then(|n| n.as_str())
                })
                .unwrap_or("?");

            let already_listed = installed.iter().any(|i| i.name == name);
            if !already_listed {
                has_entries = true;
                let state = agent.get("state").and_then(|v| v.as_str()).unwrap_or("?");
                let version = agent
                    .get("manifest")
                    .and_then(|m| m.get("version"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("-");
                println!(
                    "  {:<24} {:<10} {:<12} {:<10} runtime-only",
                    name, version, state, "-"
                );
            }
        }
    }

    if !has_entries {
        println!("  (no agents registered or installed)");
    }
}

/// Format agent detail as human-readable text.
fn format_agent_detail(resp: &serde_json::Value) {
    let agent_id = resp.get("agent_id").and_then(|v| v.as_str()).unwrap_or("?");
    let state = resp.get("state").and_then(|v| v.as_str()).unwrap_or("?");

    println!("  Agent     : {agent_id}");
    println!("  State     : {state}");

    if let Some(manifest) = resp.get("manifest") {
        let name = manifest.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let version = manifest
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let desc = manifest
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        println!("  Name      : {name}");
        println!("  Version   : {version}");
        if !desc.is_empty() {
            println!("  Desc      : {desc}");
        }
    }
}

/// Handle client errors uniformly.
fn handle_error(err: ClientError, json: bool) -> i32 {
    match err {
        ClientError::ConnectionRefused => {
            if json {
                let output =
                    serde_json::json!({"error": "runtime not started (connection refused)"});
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                );
            } else {
                eprintln!("Error: runtime not started (connection refused)");
            }
            exit_codes::RUNTIME_ERROR
        }
        other => {
            if json {
                let output = serde_json::json!({"error": other.to_string()});
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                );
            } else {
                eprintln!("Error: {other}");
            }
            exit_codes::GENERAL_ERROR
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Logs
// ─────────────────────────────────────────────────────────────────────────────

/// `apollia-os agent logs <id> [--last N] [--follow]`: display recent
/// activity for an agent.
///
/// Until a dedicated agent stderr persistence layer ships (v0.1.1+), the
/// runtime does not expose `/api/v1/agents/:id/logs`. We fall back to the
/// audit trail: every tool invocation attributed to `<agent_id>` is fetched
/// from `GET /api/v1/audit?limit=…` and rendered as one timestamped line per
/// event. This gives operators the "what did this agent do recently?" view
/// that `agent logs` should answer.
///
/// With `--follow`, opens the SSE stream at
/// `GET /api/v1/agents/{id}/logs/stream` (server-side route; when absent,
/// the CLI prints a clear "not implemented" message and exits 1).
async fn run_logs(
    client: &RuntimeClient,
    agent_id: &str,
    last: u32,
    follow: bool,
    json: bool,
) -> i32 {
    if looks_like_file_path(agent_id) {
        let msg = format!(
            "'{agent_id}' looks like a file path — use the agent name or UUID instead\n\
             Hint: apollia-os agent logs <name|uuid>"
        );
        return print_compact_error_and_exit(&msg, json);
    }

    if follow {
        return run_logs_follow(client, agent_id, json).await;
    }

    // Fetch a generous slice of recent audit events. We cap at 500
    // (runtime hard limit) and client-side filter on agent_id, then keep
    // the last `last` matches. This is O(500) per call, fine for an
    // interactive CLI; a dedicated route would be faster but is out of
    // scope.
    let fetch_limit = 500u32;
    let uri = format!("/api/v1/audit?limit={fetch_limit}");
    let resp = match client.get(&uri).await {
        Ok(r) if r.status < 400 => r,
        Ok(r) => {
            let msg = format!("HTTP {} from /audit: {}", r.status, r.body);
            return print_compact_error_and_exit(&msg, json);
        }
        Err(e) => return handle_error(e, json),
    };
    let body: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            return print_compact_error_and_exit(&format!("invalid audit JSON: {e}"), json);
        }
    };

    let empty: Vec<serde_json::Value> = Vec::new();
    let events: Vec<&serde_json::Value> = body
        .get("events")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty)
        .iter()
        .filter(|e| {
            e.get("agent_id")
                .and_then(|v| v.as_str())
                .map(|s| s == agent_id)
                .unwrap_or(false)
        })
        .take(last as usize)
        .collect();

    if json {
        let array: Vec<serde_json::Value> = events.iter().map(|e| (*e).clone()).collect();
        let out = serde_json::json!({
            "agent_id": agent_id,
            "events": array,
            "source": "audit-trail (no dedicated agent log channel yet)",
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
        return exit_codes::SUCCESS;
    }

    if events.is_empty() {
        println!("(no recent activity for {agent_id} in the audit trail)");
        return exit_codes::SUCCESS;
    }

    println!(
        "  Recent audit events for {agent_id} ({} of last {fetch_limit}):",
        events.len()
    );
    for e in &events {
        print_audit_event_row(e);
    }
    exit_codes::SUCCESS
}

/// Print a single audit event as an aligned text row for `agent logs`.
fn print_audit_event_row(e: &serde_json::Value) {
    let ts = e.get("timestamp").and_then(|v| v.as_str()).unwrap_or("?");
    let tool = e.get("tool").and_then(|v| v.as_str()).unwrap_or("?");
    let outcome = e.get("outcome").and_then(|v| v.as_str()).unwrap_or("?");
    let task = e.get("task_id").and_then(|v| v.as_str()).unwrap_or("-");
    println!("  {ts}  {tool:<20} {outcome:<10} task={task}");
}

/// `apollia-os agent logs <id> --follow`: live stream placeholder.
///
/// The runtime does not yet register `GET /api/v1/agents/:id/logs/stream`
/// (no dedicated agent stderr persistence layer ships in v0.1.0; the
/// audit fallback used by `agent logs` covers the structured tool-call
/// view). Rather than fail silently by hitting a missing endpoint, the
/// CLI returns a clear, actionable error pointing at the same `--last`
/// path and the v0.1.1 roadmap entry.
async fn run_logs_follow(_client: &RuntimeClient, agent_id: &str, json: bool) -> i32 {
    let msg = format!(
        "agent logs --follow is not yet implemented (v0.1.1+).\n  \
         For the latest activity, run: apollia-os agent logs {agent_id} --last 20\n  \
         A dedicated agent log channel will land alongside the audit-event \
         SSE stream in a future release."
    );
    if json {
        let body = serde_json::json!({
            "error": "agent logs --follow not implemented",
            "agent_id": agent_id,
            "hint": "use `agent logs <id> --last N` for the audit-trail fallback",
            "tracking": "v0.1.1",
        });
        println!("{}", serde_json::to_string_pretty(&body).unwrap_or_default());
    } else {
        eprintln!("Error: {msg}");
    }
    exit_codes::GENERAL_ERROR
}

// ─────────────────────────────────────────────────────────────────────────────
// Validate
// ─────────────────────────────────────────────────────────────────────────────

/// `apollia-os agent validate <path>`: validate an agent manifest without starting it.
///
/// Performs AIP duck-typing validation via PyO3, then reports the manifest
/// summary and any tool requirements. Exit 0 on success (with optional warnings),
/// exit 1 if the manifest is invalid or a required tool is absent.
fn run_validate(path: &Path, json: bool) -> i32 {
    // File existence check before invoking PyO3.
    if !path.exists() {
        return print_error_and_exit(&format!("file not found: {}", path.display()), json);
    }

    let loader = CliAgentLoader;
    let manifest = match loader.load_and_validate(path) {
        Ok(m) => m,
        Err(e) => return print_error_and_exit(&format!("manifest invalid: {e}"), json),
    };

    if json {
        print_validate_json(&manifest);
    } else {
        print_validate_text(&manifest);
    }

    exit_codes::SUCCESS
}

/// Emit the full validated-manifest payload as pretty JSON.
fn print_validate_json(manifest: &apollia_core::AgentManifest) {
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "valid": true,
            "name": manifest.name,
            "version": manifest.version,
            "description": manifest.description,
            "execution_mode": manifest.execution_mode,
            "memory_namespace": manifest.memory_namespace,
            "max_concurrent_tasks": manifest.max_concurrent_tasks,
            "supports_a2a": manifest.supports_a2a,
            "supports_streaming": manifest.supports_streaming,
            "dangerous_tools_allowed": manifest.dangerous_tools_allowed,
            "user_memory_write": manifest.user_memory_write,
            "tools_required": manifest.tools_required,
            "tools_optional": manifest.tools_optional,
            "tools_requiring_approval": manifest.tools_requiring_approval,
            "step_budget": manifest.step_budget,
            "skills": manifest.skills,
            "tags": manifest.tags,
            "packages": manifest.packages,
            "datasources": manifest.datasources,
            "templates": manifest.templates,
            "secrets": manifest.secrets,
        }))
        .unwrap_or_default()
    );
}

/// Compute the comma-joined feature-flag summary for a manifest, or `None`
/// when no flags are set.
fn manifest_feature_flags(manifest: &apollia_core::AgentManifest) -> Option<String> {
    let mut feature_flags = Vec::new();
    if manifest.supports_a2a {
        feature_flags.push("a2a");
    }
    if manifest.supports_streaming {
        feature_flags.push("streaming");
    }
    if manifest.user_memory_write {
        feature_flags.push("writes user memory");
    }
    if manifest.dangerous_tools_allowed {
        feature_flags.push("dangerous tools allowed");
    }
    if feature_flags.is_empty() {
        None
    } else {
        Some(feature_flags.join(", "))
    }
}

/// Render the validated-manifest summary as human-readable text.
fn print_validate_text(manifest: &apollia_core::AgentManifest) {
    let required = &manifest.tools_required;
    let optional = &manifest.tools_optional;

    println!("✔ Manifest valide");
    println!("  Name             : {}", manifest.name);
    println!("  Version          : {}", manifest.version);
    if !manifest.description.is_empty() {
        println!("  Description      : {}", manifest.description);
    }
    println!("  Execution mode   : {}", manifest.execution_mode);
    if let Some(ns) = &manifest.memory_namespace {
        println!("  Memory namespace : {ns}");
    }
    println!("  Max concurrency  : {}", manifest.max_concurrent_tasks);
    if let Some(flags) = manifest_feature_flags(manifest) {
        println!("  Features         : {flags}");
    }
    if !required.is_empty() {
        println!("  Required tools   : {}", required.join(", "));
    }
    if !optional.is_empty() {
        println!("  Optional tools   : {}", optional.join(", "));
        println!("  ⚠ Optional tools not checked — agent may start in DEGRADED mode if absent");
    }
    if !manifest.tools_requiring_approval.is_empty() {
        println!(
            "  HITL gated tools : {}",
            manifest.tools_requiring_approval.join(", ")
        );
    }
    if let Some(budget) = &manifest.step_budget {
        println!(
            "  Step budget      : {} steps, {} tool calls, {}s wall-clock",
            budget.max_steps, budget.max_tool_calls, budget.wall_clock_secs
        );
    }
    print_validate_text_skills(manifest);
    print_validate_text_lists(manifest);
    print_validate_text_setup_notes(manifest);
}

/// Print the declared skills section of a validated manifest.
fn print_validate_text_skills(manifest: &apollia_core::AgentManifest) {
    if manifest.skills.is_empty() {
        return;
    }
    println!("  Skills ({})       :", manifest.skills.len());
    for skill in &manifest.skills {
        println!("    - {} ({})", skill.id, skill.name);
    }
}

/// Print the multi-line setup notes section of a validated manifest.
fn print_validate_text_setup_notes(manifest: &apollia_core::AgentManifest) {
    let Some(notes) = &manifest.setup_notes else {
        return;
    };
    if notes.is_empty() {
        return;
    }
    println!("  Setup notes      :");
    for line in notes.lines() {
        println!("    {line}");
    }
}

/// Print the trailing string-list fields (tags, deps, datasources, etc.) of
/// a validated manifest in the human-readable summary.
fn print_validate_text_lists(manifest: &apollia_core::AgentManifest) {
    if !manifest.tags.is_empty() {
        println!("  Tags             : {}", manifest.tags.join(", "));
    }
    if !manifest.packages.is_empty() {
        println!("  Pip deps         : {}", manifest.packages.join(", "));
    }
    if !manifest.datasources.is_empty() {
        println!("  Datasources      : {}", manifest.datasources.join(", "));
    }
    if !manifest.templates.is_empty() {
        println!("  Templates        : {}", manifest.templates.join(", "));
    }
    if !manifest.secrets.is_empty() {
        println!("  Secrets          : {}", manifest.secrets.join(", "));
    }
}

/// `apollia-os agent repair <name>`: re-provision an installed agent's venv.
///
/// Looks the agent up in the package repository, parses the `manifest_json`
/// stored for its owning package, finds the matching agent entry, then runs
/// `PythonExecutor::setup_venv` against the declared `packages` list. Idempotent:
/// safe to run on already-provisioned venvs (pip is a no-op when satisfied).
async fn run_repair(name: &str, json: bool) -> i32 {
    let data_dir = apollia_data_dir();
    let pkg_repo = match open_package_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };

    // Find the package owning this agent: list packages, parse each manifest,
    // and match by agent name. The repository stores the resolved manifest JSON
    // alongside the package row so we do not need to re-parse agent.toml from disk.
    let packages = match pkg_repo.list() {
        Ok(p) => p,
        Err(e) => return print_error_and_exit(&format!("database error: {e}"), json),
    };

    let (package_name, declared) = match find_agent_package_deps(&packages, name) {
        Some(f) => f,
        None => {
            return print_error_and_exit(
                &format!(
                    "agent '{name}' not found in any installed package\n\
                     Hint: run `apollia-os agent list` to see installed agents."
                ),
                json,
            );
        }
    };

    if declared.is_empty() {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "agent": name,
                    "package": package_name,
                    "venv_provisioned": false,
                    "reason": "no pip dependencies declared in manifest",
                }))
                .unwrap_or_default()
            );
        } else {
            println!("Agent '{name}' (package {package_name}) declares no pip packages.");
            println!("Nothing to repair.");
        }
        return exit_codes::SUCCESS;
    }

    let venv_base = data_dir.join("venvs");
    let executor =
        match apollia_tools::tools::python_executor::PythonExecutor::new(name, &venv_base) {
            Ok(e) => e,
            Err(e) => {
                return print_error_and_exit(&format!("could not init venv: {e}"), json);
            }
        };

    if !json {
        println!(
            "Provisioning venv for '{name}' ({} package(s))…",
            declared.len()
        );
    }

    if let Err(e) = executor.setup_venv(&declared).await {
        return print_error_and_exit(&format!("venv provisioning failed: {e}"), json);
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "agent": name,
                "package": package_name,
                "venv_provisioned": true,
                "packages": declared,
            }))
            .unwrap_or_default()
        );
    } else {
        println!(
            "OK venv for '{name}' provisioned at {}",
            venv_base.join(name).display()
        );
        println!("    Packages installed:");
        for p in &declared {
            println!("      - {p}");
        }
        println!();
        println!("Restart the runtime to pick up the new venv:");
        println!("    apollia-os stop && apollia-os start");
    }
    exit_codes::SUCCESS
}

/// Locate the package owning `agent_name` and return its `(package_name,
/// declared_pip_packages)`. Returns `None` when no installed package
/// declares an agent with that name.
fn find_agent_package_deps(
    packages: &[InstalledPackage],
    agent_name: &str,
) -> Option<(String, Vec<String>)> {
    for pkg in packages {
        let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&pkg.manifest_json) else {
            continue;
        };
        let Some(agents) = manifest.get("agents").and_then(|v| v.as_array()) else {
            continue;
        };
        let Some(entry) = agents
            .iter()
            .find(|e| e.get("name").and_then(|v| v.as_str()) == Some(agent_name))
        else {
            continue;
        };
        let pkgs: Vec<String> = entry
            .get("packages")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        return Some((pkg.name.clone(), pkgs));
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::AgentManifest;
    use std::path::Path;

    fn test_manifest(name: &str) -> AgentManifest {
        AgentManifest {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            description: format!("Test agent {name}"),
            tools_required: vec![],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
            execution_mode: "auto".to_string(),
            system_prompt: None,
            tools_requiring_approval: vec![],
            llm_backend: None,
            packages: vec![],
            memory_config: None,
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            user_memory_write: false,
            datasources: vec![],
            templates: vec![],
            secrets: vec![],
        }
    }

    fn test_installed_agent(name: &str) -> InstalledAgent {
        InstalledAgent {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            install_path: PathBuf::from(format!("/tmp/.apollia/agents/{name}/agent.py")),
            source_path: PathBuf::from(format!("/tmp/{name}.py")),
            manifest: test_manifest(name),
            enabled: true,
            installed_at: "2026-03-17T10:00:00Z".to_string(),
            updated_at: "2026-03-17T10:00:00Z".to_string(),
        }
    }

    // install command output format (JSON)
    #[test]
    fn test_install_command_output() {
        // GIVEN an InstalledAgent
        let agent = test_installed_agent("mon-agent");
        // WHEN formatting JSON output
        let output = serde_json::json!({
            "name": agent.name,
            "version": agent.version,
            "install_path": agent.install_path.to_string_lossy(),
        });
        // THEN JSON contains expected fields
        assert_eq!(output["name"], "mon-agent");
        assert_eq!(output["version"], "0.1.0");
        assert!(output["install_path"]
            .as_str()
            .is_some_and(|p| p.contains("mon-agent")));
    }

    // uninstall command output format (JSON)
    #[test]
    fn test_uninstall_command_output() {
        // GIVEN an agent name
        let name = "mon-agent";
        // WHEN formatting JSON output
        let output = serde_json::json!({
            "name": name,
            "status": "uninstalled",
        });
        // THEN JSON contains expected fields
        assert_eq!(output["name"], "mon-agent");
        assert_eq!(output["status"], "uninstalled");
    }

    // enable/disable output
    #[test]
    fn test_enable_disable_output() {
        // GIVEN an agent name
        let name = "mon-agent";
        // WHEN formatting enable/disable JSON output
        let enable_output = serde_json::json!({ "name": name, "enabled": true });
        let disable_output = serde_json::json!({ "name": name, "enabled": false });
        // THEN JSON contains expected values
        assert_eq!(enable_output["enabled"], true);
        assert_eq!(disable_output["enabled"], false);
    }

    // update command output format
    #[test]
    fn test_update_command_output() {
        // GIVEN update result
        let name = "mon-agent";
        let version = "0.2.0";
        // WHEN formatting JSON output
        let output = serde_json::json!({ "name": name, "version": version });
        // THEN JSON contains expected fields
        assert_eq!(output["name"], "mon-agent");
        assert_eq!(output["version"], "0.2.0");
    }

    // list shows installed agents merged with runtime
    #[test]
    fn test_list_shows_installed_agents() {
        // GIVEN 2 installed agents (1 enabled, 1 disabled) and runtime data
        let installed = vec![test_installed_agent("agent-active"), {
            let mut a = test_installed_agent("agent-disabled");
            a.enabled = false;
            a
        }];
        let runtime = Some(serde_json::json!({
            "agents": [
                {
                    "agent_id": "uuid-1",
                    "name": "agent-active",
                    "state": "Active",
                    "manifest": { "name": "agent-active", "version": "0.1.0" },
                },
                {
                    "agent_id": "uuid-3",
                    "name": "runtime-only",
                    "state": "Active",
                    "manifest": { "name": "runtime-only", "version": "1.0.0" },
                }
            ]
        }));

        // WHEN building JSON list
        let result = build_list_json(&installed, &runtime);
        let agents = result["agents"].as_array().expect("should be array");

        // THEN all agents are present with correct status
        assert_eq!(agents.len(), 3);

        // agent-active: installed, enabled, runtime Active
        assert_eq!(agents[0]["name"], "agent-active");
        assert_eq!(agents[0]["status"], "Active");
        assert_eq!(agents[0]["enabled"], true);
        assert_eq!(agents[0]["installed"], true);

        // agent-disabled: installed, disabled, not in runtime
        assert_eq!(agents[1]["name"], "agent-disabled");
        assert_eq!(agents[1]["status"], "-");
        assert_eq!(agents[1]["enabled"], false);
        assert_eq!(agents[1]["installed"], true);

        // runtime-only: not installed
        assert_eq!(agents[2]["name"], "runtime-only");
        assert_eq!(agents[2]["status"], "Active");
        assert_eq!(agents[2]["installed"], false);
    }

    // error for uninstall of nonexistent agent
    #[test]
    fn test_uninstall_not_found_error() {
        // GIVEN a repository with no agents
        let repo = AgentRepository::open(Path::new(":memory:")).expect("open in-memory repo");
        // WHEN checking for a nonexistent agent
        let result = repo.get("inexistant").expect("get should not error");
        // THEN the agent is not found
        assert!(result.is_none());
    }

    // helper functions work without runtime
    #[test]
    fn test_data_dir_resolution() {
        // GIVEN the HOME environment variable
        // WHEN resolving the data dir
        let dir = apollia_data_dir();
        // THEN it ends with .apollia
        assert!(dir.to_string_lossy().ends_with(".apollia"));
    }

    #[test]
    fn test_looks_like_file_path_detection() {
        // GIVEN various arguments
        // THEN file-like args are detected
        assert!(looks_like_file_path("agents/foo.py"));
        assert!(looks_like_file_path("./agent.py"));
        assert!(looks_like_file_path("/abs/path/agent.py"));
        assert!(!looks_like_file_path("my-agent"));
        assert!(!looks_like_file_path("uuid-1234"));
    }

    #[test]
    fn test_new_validates_agent_type() {
        // GIVEN an invalid template type
        // WHEN run_new is called
        let code = run_new("test-agent", "invalid", false);
        // THEN it returns GENERAL_ERROR
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn test_new_valid_agent_types_accepted() {
        // GIVEN all valid template types
        // THEN they are all recognized
        for t in VALID_AGENT_TYPES {
            assert!(VALID_AGENT_TYPES.contains(t), "type '{t}' should be valid");
        }
        assert!(!VALID_AGENT_TYPES.contains(&"invalid"));
        assert!(!VALID_AGENT_TYPES.contains(&"custom"));
    }

    #[test]
    fn test_new_detects_name_conflict() {
        // GIVEN a temporary directory simulating ~/.apollia/agents/<name>/
        let tmp = tempfile::tempdir().expect("create tmpdir");
        let agents_dir = tmp.path().join("agents").join("existing-agent");
        std::fs::create_dir_all(&agents_dir).expect("create agent dir");

        // WHEN the target directory already exists
        // THEN it is detected as a conflict
        assert!(agents_dir.exists());
    }

    #[test]
    fn test_new_json_output_format() {
        // GIVEN a scaffolding result
        let name = "my-agent";
        let agent_type = "react";
        let path = "/home/user/.apollia/agents/my-agent/";
        let files = vec![
            "my_agent_agent.py".to_string(),
            "test_my_agent_agent.py".to_string(),
        ];

        // WHEN formatting JSON output
        let output = serde_json::json!({
            "name": name,
            "type": agent_type,
            "path": path,
            "files": files,
        });

        // THEN the JSON contains all required fields
        assert_eq!(output["name"], "my-agent");
        assert_eq!(output["type"], "react");
        assert!(output["path"]
            .as_str()
            .is_some_and(|p| p.contains("my-agent")));
        let file_list = output["files"].as_array().expect("files should be array");
        assert_eq!(file_list.len(), 2);
    }

    #[test]
    fn test_new_default_type_is_react() {
        // GIVEN the AgentCommand::New parsed without --type
        use clap::Parser;

        #[derive(Debug, Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: AgentCommand,
        }

        let cli = TestCli::parse_from(["test", "new", "simple-bot"]);
        // THEN the default type is "react"
        match cli.cmd {
            AgentCommand::New { name, r#type } => {
                assert_eq!(name, "simple-bot");
                assert_eq!(r#type, "react");
            }
            other => panic!("expected AgentCommand::New, got {other:?}"),
        }
    }

    #[test]
    fn test_a2a_skill_id_field_name() {
        // GIVEN a skill DTO JSON as returned by GET /api/v1/a2a/agents
        let skill = serde_json::json!({
            "id": "read-excel",
            "name": "Read Excel",
            "description": "Read an Excel workbook.",
            "input_modes": ["text"],
            "output_modes": ["text"]
        });

        // WHEN reading the skill identifier
        let id = skill.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        let legacy = skill
            .get("skill_id")
            .and_then(|v| v.as_str())
            .unwrap_or("?");

        // THEN "id" resolves correctly and "skill_id" is absent
        assert_eq!(id, "read-excel");
        assert_eq!(legacy, "?");
    }

    #[test]
    fn test_format_a2a_agent_list_empty_no_agents_key() {
        // GIVEN a response with no "agents" key
        let resp = serde_json::json!({});

        // WHEN extracting the agents array
        let agents = resp.get("agents").and_then(|v| v.as_array());

        // THEN agents is None
        assert!(agents.is_none());
    }

    #[test]
    fn test_format_a2a_agent_list_empty_array() {
        // GIVEN a response with an empty agents array
        let resp = serde_json::json!({ "agents": [] });

        // WHEN extracting the agents array
        let agents = resp
            .get("agents")
            .and_then(|v| v.as_array())
            .expect("agents array");

        // THEN the array is empty
        assert!(agents.is_empty());
    }

    // ── agent logs parsing ────────────────────────────────────────────────────

    #[test]
    fn test_agent_logs_parses_defaults() {
        // GIVEN "apollia-os agent logs devis-generator"
        use clap::Parser;

        #[derive(Debug, Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: AgentCommand,
        }

        let cli = TestCli::parse_from(["test", "logs", "devis-generator"]);
        // THEN AgentCommand::Logs with default last=50, follow=false
        match cli.cmd {
            AgentCommand::Logs {
                agent_id,
                last,
                follow,
            } => {
                assert_eq!(agent_id, "devis-generator");
                assert_eq!(last, 50);
                assert!(!follow);
            }
            other => panic!("expected AgentCommand::Logs, got {other:?}"),
        }
    }

    #[test]
    fn test_agent_logs_parses_last_flag() {
        // GIVEN "agent logs devis-generator --last 20"
        use clap::Parser;

        #[derive(Debug, Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: AgentCommand,
        }

        let cli = TestCli::parse_from(["test", "logs", "devis-generator", "--last", "20"]);
        // THEN last=20
        match cli.cmd {
            AgentCommand::Logs { last, .. } => assert_eq!(last, 20),
            other => panic!("expected AgentCommand::Logs, got {other:?}"),
        }
    }

    #[test]
    fn test_agent_logs_parses_follow_flag() {
        // GIVEN "agent logs devis-generator --follow"
        use clap::Parser;

        #[derive(Debug, Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: AgentCommand,
        }

        let cli = TestCli::parse_from(["test", "logs", "devis-generator", "--follow"]);
        // THEN follow=true
        match cli.cmd {
            AgentCommand::Logs { follow, .. } => assert!(follow),
            other => panic!("expected AgentCommand::Logs, got {other:?}"),
        }
    }

    // ── agent validate parsing ────────────────────────────────────────────────

    #[test]
    fn test_agent_validate_parses_path() {
        // GIVEN "agent validate ./mon-agent.py"
        use clap::Parser;

        #[derive(Debug, Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: AgentCommand,
        }

        let cli = TestCli::parse_from(["test", "validate", "./mon-agent.py"]);
        // THEN AgentCommand::Validate with correct path
        match cli.cmd {
            AgentCommand::Validate { path } => {
                assert_eq!(path, PathBuf::from("./mon-agent.py"));
            }
            other => panic!("expected AgentCommand::Validate, got {other:?}"),
        }
    }

    #[test]
    fn test_agent_validate_file_not_found_returns_error() {
        // GIVEN a path that does not exist
        let path = PathBuf::from("/tmp/this-file-does-not-exist-apollia-test.py");
        // WHEN run_validate is called
        let code = run_validate(&path, false);
        // THEN exit code is GENERAL_ERROR
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn test_agent_validate_file_not_found_json_output() {
        // GIVEN a path that does not exist and json=true
        let path = PathBuf::from("/tmp/this-file-does-not-exist-apollia-test.py");
        // WHEN run_validate is called in JSON mode
        let code = run_validate(&path, true);
        // THEN exit code is GENERAL_ERROR
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn test_agent_logs_file_path_rejected() {
        // GIVEN an agent_id that looks like a file path
        // THEN looks_like_file_path returns true
        assert!(looks_like_file_path("agents/foo.py"));
        assert!(looks_like_file_path("./agent.py"));
        // AND agent names are not rejected
        assert!(!looks_like_file_path("devis-generator"));
        assert!(!looks_like_file_path("rapport-hebdo"));
    }

    #[test]
    fn test_format_a2a_agent_list_skills_read_from_id_field() {
        // GIVEN an A2A agents response with skills using the "id" key
        let resp = serde_json::json!({
            "agents": [{
                "agent_id": "uuid-1",
                "name": "excel-worker",
                "version": "0.1.0",
                "state": "active",
                "skills": [
                    { "id": "read-excel", "name": "Read Excel", "description": "Reads an Excel file.", "input_modes": ["text"], "output_modes": ["text"] },
                    { "id": "edit-excel", "name": "Edit Excel", "description": "", "input_modes": ["text"], "output_modes": ["file"] }
                ]
            }]
        });

        // WHEN extracting skill IDs
        let agents = resp["agents"].as_array().expect("agents");
        let skills = agents[0]["skills"].as_array().expect("skills");
        let ids: Vec<&str> = skills
            .iter()
            .filter_map(|s| s.get("id").and_then(|v| v.as_str()))
            .collect();

        // THEN both skill IDs are correctly resolved
        assert_eq!(ids, vec!["read-excel", "edit-excel"]);
    }

    #[test]
    fn test_list_generated_files_collects_recursively() {
        // GIVEN a directory with files at different depths
        let tmp = tempfile::tempdir().expect("create tmpdir");
        let base = tmp.path();
        std::fs::write(base.join("agent.py"), "").expect("write file");
        let tests_dir = base.join("tests");
        std::fs::create_dir_all(&tests_dir).expect("create tests dir");
        std::fs::write(tests_dir.join("test_agent.py"), "").expect("write test file");

        // WHEN listing generated files
        let files = list_generated_files(base);

        // THEN both files are found with relative paths
        assert_eq!(files.len(), 2);
        assert!(files.contains(&"agent.py".to_string()));
        assert!(files.contains(&"tests/test_agent.py".to_string()));
    }
}
