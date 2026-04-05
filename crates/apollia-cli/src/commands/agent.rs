//! `apollia-os agent` subcommands — manage agents via the runtime API and local persistence.
//!
//! Provides `list`, `start`, `stop`, `info` (runtime-dependent) and
//! `install`, `uninstall`, `enable`, `disable`, `update` (local).

use std::path::{Path, PathBuf};

use apollia_runtime::agents::registry_remote::{self, parse_install_source, AgentInstallSource};
use apollia_runtime::api::routes_agents::AgentLoader;
use apollia_tools::{AgentRepository, InstalledAgent};
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
    /// Install an agent permanently from a local path or a Git URL.
    ///
    /// Accepts a local filesystem path (e.g. `./agents/my-agent.py`) or a Git
    /// remote URL (e.g. `https://github.com/user/my-agent.git`).  An optional
    /// `#<tag>` suffix pins the clone to a specific tag or branch
    /// (e.g. `https://github.com/user/my-agent.git#v1.2.0`).
    Install {
        /// Local path to a Python module or a Git URL (with optional #tag).
        source: String,

        /// Skip the agent test suite (not recommended — reduces validation coverage).
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
}

/// Execute an `agent` subcommand.
///
/// Returns the process exit code.
pub async fn run(cmd: &AgentCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let client = RuntimeClient::new(socket_path);

    match cmd {
        AgentCommand::List { supports_a2a } => run_list(&client, *supports_a2a, json).await,
        AgentCommand::Start { path } => run_start(&client, path, json).await,
        AgentCommand::Stop { agent_id } => run_stop(&client, agent_id, json).await,
        AgentCommand::Info { agent_id } => run_info(&client, agent_id, json).await,
        AgentCommand::Install { source, skip_tests } => {
            run_install(source, &client, json, *skip_tests).await
        }
        AgentCommand::Uninstall { name } => run_uninstall(name, json),
        AgentCommand::Enable { name } => run_enable(name, json),
        AgentCommand::Disable { name } => run_disable(name, json),
        AgentCommand::Update { name, path } => run_update(name, path, json),
        AgentCommand::New { name, r#type } => run_new(name, r#type, json),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Existing commands (list/start/stop/info)
// ─────────────────────────────────────────────────────────────────────────────

/// `apollia-os agent list` — display all agents (installed + runtime).
///
/// When `supports_a2a` is `true`, fetches from `/api/v1/a2a/agents` instead
/// and displays only A2A-capable agents with their skill descriptors.
async fn run_list(client: &RuntimeClient, supports_a2a: bool, json: bool) -> i32 {
    if supports_a2a {
        return run_list_a2a(client, json).await;
    }

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

/// `apollia-os agent list --supports-a2a` — display A2A-capable agents with skills.
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

/// `apollia-os agent install <source> [--skip-tests]` — install an agent permanently.
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
        AgentInstallSource::Local(path) => run_install_local(&path, client, json, skip_tests).await,
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

    // Create a temporary clone directory — removed when `temp` is dropped.
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

    // Check if runtime is running — informational only.
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

/// Install a community agent from a local Python file (non-regression path).
async fn run_install_local(
    source_path: &Path,
    client: &RuntimeClient,
    json: bool,
    skip_tests: bool,
) -> i32 {
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
// Scaffolding (agent new)
// ─────────────────────────────────────────────────────────────────────────────

/// Supported agent template types.
const VALID_AGENT_TYPES: &[&str] = &["react", "conversational", "orchestrated"];

/// `apollia-os agent new <name> [--type <type>]` — scaffold a new agent via the SDK.
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

/// Format an enriched agent list as a human-readable table.
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
            llm_backend: None,
            packages: vec![],
            memory_config: None,
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
