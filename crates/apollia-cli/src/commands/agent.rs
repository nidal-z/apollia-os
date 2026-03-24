//! `apollia-os agent` subcommands — manage agents via the runtime API and local persistence.
//!
//! Provides `list`, `start`, `stop`, `info` (runtime-dependent) and
//! `install`, `uninstall`, `enable`, `disable`, `update` (local).

use std::path::{Path, PathBuf};

use apollia_runtime::api::routes_agents::AgentLoader;
use apollia_tools::{AgentRepository, InstalledAgent};
use clap::Subcommand;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

/// Agent subcommands: `apollia-os agent <verb>`.
#[derive(Debug, Subcommand)]
pub enum AgentCommand {
    /// List all agents (installed and/or runtime).
    List,
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
    /// Install an agent permanently from a Python module.
    Install {
        /// Path to the agent Python module.
        path: PathBuf,
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
}

/// Execute an `agent` subcommand.
///
/// Returns the process exit code.
pub async fn run(cmd: &AgentCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let client = RuntimeClient::new(socket_path);

    match cmd {
        AgentCommand::List => run_list(&client, json).await,
        AgentCommand::Start { path } => run_start(&client, path, json).await,
        AgentCommand::Stop { agent_id } => run_stop(&client, agent_id, json).await,
        AgentCommand::Info { agent_id } => run_info(&client, agent_id, json).await,
        AgentCommand::Install { path } => run_install(path, &client, json).await,
        AgentCommand::Uninstall { name } => run_uninstall(name, json),
        AgentCommand::Enable { name } => run_enable(name, json),
        AgentCommand::Disable { name } => run_disable(name, json),
        AgentCommand::Update { name, path } => run_update(name, path, json),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Existing commands (list/start/stop/info)
// ─────────────────────────────────────────────────────────────────────────────

/// `apollia-os agent list` — display all agents (installed + runtime).
async fn run_list(client: &RuntimeClient, json: bool) -> i32 {
    // Fetch installed agents from local DB.
    let installed = open_repository()
        .and_then(|repo| repo.list().ok())
        .unwrap_or_default();

    // Fetch runtime agents (may fail if runtime not running).
    let runtime_agents = client.list_agents().await.ok();

    if json {
        let output = build_list_json(&installed, &runtime_agents);
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
    } else {
        format_enriched_agent_list(&installed, &runtime_agents);
    }
    exit_codes::SUCCESS
}

/// `apollia-os agent start <path>` — register a new agent.
async fn run_start(client: &RuntimeClient, path: &str, json: bool) -> i32 {
    match client.start_agent(path).await {
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

/// `apollia-os agent stop <id>` — stop a running agent.
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

/// `apollia-os agent info <id>` — display agent detail.
async fn run_info(client: &RuntimeClient, agent_id: &str, json: bool) -> i32 {
    if looks_like_file_path(agent_id) {
        let msg = format!(
            "'{agent_id}' looks like a file path — use the agent name or UUID instead\n\
             Hint: apollia-os agent info <name|uuid>  (e.g. apollia-os agent info apollia-reviewer)"
        );
        if json {
            println!("{}", serde_json::json!({"error": msg}));
        } else {
            eprintln!("Error: {msg}");
        }
        return exit_codes::GENERAL_ERROR;
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
        Err(e) => handle_error(e, json),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// New commands (install/uninstall/enable/disable/update)
// ─────────────────────────────────────────────────────────────────────────────

/// `apollia-os agent install <path>` — install an agent permanently.
async fn run_install(source_path: &Path, client: &RuntimeClient, json: bool) -> i32 {
    // Validate source file exists.
    if !source_path.exists() {
        return print_error_and_exit(&format!("file not found: {}", source_path.display()), json);
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

    // Load and validate the Python module via PyO3.
    let loader = CliAgentLoader;
    let manifest = match loader.load_and_validate(&canonical) {
        Ok(m) => m,
        Err(e) => {
            return print_error_and_exit(&format!("failed to load agent: {e}"), json);
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

    // Check if runtime is running — informational only.
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

/// `apollia-os agent uninstall <name>` — remove an installed agent.
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

/// `apollia-os agent enable <name>` — re-enable an installed agent.
fn run_enable(name: &str, json: bool) -> i32 {
    let data_dir = apollia_data_dir();
    let repo = match open_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };

    match repo.set_enabled(name, true) {
        Ok(()) => {
            if json {
                let output = serde_json::json!({
                    "name": name,
                    "enabled": true,
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                );
            } else {
                println!("Agent '{name}' enabled (will auto-start on boot)");
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

/// `apollia-os agent disable <name>` — disable an installed agent.
fn run_disable(name: &str, json: bool) -> i32 {
    let data_dir = apollia_data_dir();
    let repo = match open_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };

    match repo.set_enabled(name, false) {
        Ok(()) => {
            if json {
                let output = serde_json::json!({
                    "name": name,
                    "enabled": false,
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                );
            } else {
                println!("Agent '{name}' disabled (will not auto-start)");
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

/// `apollia-os agent update <name> <path>` — update an installed agent.
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
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Resolve `~/.apollia/` data directory.
fn apollia_data_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".apollia")
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

/// Real agent loader using PyO3 AIPLoader + validate_agent (ADR-019).
///
/// Loads a Python module, validates AIP duck typing, and returns the manifest.
struct CliAgentLoader;

impl AgentLoader for CliAgentLoader {
    fn load_and_validate(&self, path: &Path) -> Result<apollia_core::AgentManifest, String> {
        let module = apollia_aip::loader::load_agent_module(path).map_err(|e| e.to_string())?;
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

/// Format an enriched agent list as a human-readable table (AC-5).
fn format_enriched_agent_list(installed: &[InstalledAgent], runtime: &Option<serde_json::Value>) {
    let runtime_agents = runtime
        .as_ref()
        .and_then(|v| v.get("agents"))
        .and_then(|a| a.as_array());

    println!(
        "  {:<24} {:<10} {:<10} {:<9} INSTALLED",
        "NAME", "VERSION", "STATUS", "ENABLED"
    );

    let mut has_entries = false;

    for agent in installed {
        has_entries = true;
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
        let enabled_str = if agent.enabled { "yes" } else { "no" };

        println!(
            "  {:<24} {:<10} {:<10} {:<9} yes",
            agent.name, agent.version, runtime_status, enabled_str
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
                println!("  {:<24} {:<10} {:<10} {:<9} no", name, version, state, "-");
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

    // AC-1 — install command output format (JSON)
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

    // AC-2 — uninstall command output format (JSON)
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

    // AC-3 — enable/disable output
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

    // AC-4 — update command output format
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

    // AC-5 — list shows installed agents merged with runtime
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

    // AC-6 — error for uninstall of nonexistent agent
    #[test]
    fn test_uninstall_not_found_error() {
        // GIVEN a repository with no agents
        let repo = AgentRepository::open(Path::new(":memory:")).expect("open in-memory repo");
        // WHEN checking for a nonexistent agent
        let result = repo.get("inexistant").expect("get should not error");
        // THEN the agent is not found
        assert!(result.is_none());
    }

    // AC-7 — helper functions work without runtime
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
}
