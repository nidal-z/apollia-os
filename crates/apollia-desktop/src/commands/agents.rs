//! Tauri IPC commands for agent management.
//!
//! Each command delegates to the embedded runtime handles for runtime agents
//! (list/start/stop) and to the [`AgentRepository`] + [`AgentLoader`]
//! for persistence operations (install/uninstall/enable/disable/update).

use std::path::PathBuf;
use std::sync::Arc;

use apollia_core::ProcessState;
use apollia_runtime::embedded::RuntimeHandle;
use apollia_tools::AgentRepository;
use serde::Serialize;
use tauri::State;

use super::http_post_json;

/// The state-changing commands live in `lifecycle`, the agent-to-agent mailbox
/// view in `messages`.
pub mod lifecycle;
pub mod messages;

// ─────────────────────────────────────────────────────────────────────────────
// Response types
// ─────────────────────────────────────────────────────────────────────────────

/// Response to an agent install or update.
#[derive(Debug, Serialize)]
pub struct InstallAgentResponse {
    /// Unique name of the installed agent.
    pub name: String,
    /// Semver version of the agent.
    pub version: String,
    /// Install path on disk.
    pub install_path: String,
}

/// What happened to the live runtime instance while an agent's module was
/// replaced.
///
/// The embedded interpreter keeps the imported module in memory, so writing a
/// new `.py` over the installed one changes nothing for an agent that is
/// already running. The replacement therefore stops and restarts it, and this
/// value is what lets the window say which of the two versions is now serving
/// tasks instead of announcing a success it cannot vouch for.
pub mod restart_outcome {
    /// No live runtime entry: the new module loads at the next start.
    pub const NOT_RUNNING: &str = "not_running";
    /// Stopped and started again on the new module.
    pub const RESTARTED: &str = "restarted";
    /// The stop was refused: the previous module is still running.
    pub const STOP_FAILED: &str = "stop_failed";
    /// Stopped, but it did not come back up: nothing is running.
    pub const START_FAILED: &str = "start_failed";
}

/// Response to an agent update.
///
/// Distinct from [`InstallAgentResponse`] because an update has an outcome an
/// install cannot have: a runtime instance that had to be cycled.
#[derive(Debug, Serialize)]
pub struct UpdateAgentResponse {
    /// Unique name of the updated agent.
    pub name: String,
    /// Semver version read from the new module's manifest.
    pub version: String,
    /// Install path on disk.
    pub install_path: String,
    /// One of the [`restart_outcome`] constants.
    pub restart_outcome: String,
    /// Raw cause when the stop or the start failed, `None` otherwise.
    pub restart_error: Option<String>,
}

/// Skill declared by a worker agent in its manifest.
#[derive(Debug, Serialize)]
pub struct AgentSkillView {
    /// Unique skill identifier (e.g. `"read-excel"`).
    pub id: String,
    /// Human-readable skill name.
    pub name: String,
    /// Short skill description.
    pub description: String,
}

/// Item in the enriched agent list.
///
/// Merges installed agents (persisted in `agents.db`) with agents active in
/// the runtime, giving the frontend a unified view.
#[derive(Debug, Serialize)]
pub struct AgentListItem {
    /// Runtime UUID of the agent (`None` if installed but not loaded).
    pub id: Option<String>,
    /// Unique agent name.
    pub name: String,
    /// Semver version.
    pub version: String,
    /// Whether the agent is enabled for loading at boot.
    pub enabled: bool,
    /// Runtime state (`"active"`, `"degraded"`, `"stopped"`, or `None` if not loaded).
    pub runtime_status: Option<String>,
    /// Install timestamp (RFC 3339, `None` for runtime-only agents).
    pub installed_at: Option<String>,
    /// Human-readable agent description (from the manifest).
    pub description: Option<String>,
    /// Free-form tags for routing/discovery.
    pub tags: Vec<String>,
    /// Tools required by the agent.
    pub tools_required: Vec<String>,
    /// Optional tools for the agent.
    pub tools_optional: Vec<String>,
    /// Execution mode (`"auto"`, `"direct"`, `"orchestrated"`).
    pub execution_mode: Option<String>,
    /// Install path on disk (`None` for runtime-only agents).
    pub install_path: Option<String>,
    /// Whether the agent supports inter-agent communication (A2A).
    pub supports_a2a: bool,
    /// Declared A2A skills (empty if `supports_a2a` is `false`).
    pub skills: Vec<AgentSkillView>,
    /// Semantic role of the agent: `"worker"` | `"assistant"` | `"system"` | `None`.
    /// Used by the UI to categorize agents independently of `supports_a2a`.
    pub agent_type: Option<String>,
    /// Prompt examples illustrating typical uses (empty = not provided).
    pub examples: Vec<String>,
    /// Explicit limitations: what the agent does not do (empty = not provided).
    pub limitations: Vec<String>,
    /// Configuration note required before first use (`None` = no prerequisite).
    pub setup_notes: Option<String>,
    /// Name of the agent's source Python class (e.g. `"VeilleIaAgent"`,
    /// `"ApolliaGuide"`, `"DocxWorker"`).
    /// Filled in by the AIP validator. `None` for agents built outside the
    /// PyO3 pipeline.
    pub agent_class: Option<String>,
    /// Primary memory namespace declared in the manifest. `None` = the agent
    /// has no access to persistent memory.
    ///
    /// Several agents from the same package often share the same namespace
    /// (convention: all declare `memory_namespace = "<package-slug>"`), so this
    /// value is never derived from `name` on the UI side; it must come from the
    /// manifest to match the SQLite key.
    pub memory_namespace: Option<String>,
    /// Shared memory namespaces the agent can read from.
    pub shared_memory_namespaces: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Converts a `ProcessState` into a string for the frontend.
fn state_to_string(state: &ProcessState) -> String {
    match state {
        ProcessState::Initializing => "initializing".to_string(),
        ProcessState::Active => "active".to_string(),
        ProcessState::Degraded => "degraded".to_string(),
        ProcessState::Stopping => "stopping".to_string(),
        ProcessState::Stopped => "stopped".to_string(),
    }
}

/// Resolves the `~/.apollia/` directory.
fn apollia_data_dir() -> PathBuf {
    let home = apollia_core::paths::home_dir_or_temp()
        .display()
        .to_string();
    apollia_core::paths::data_dir_under(home)
}

/// Current timestamp in RFC 3339.
fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Recursively copy `.py` files and Python sub-packages from *src_dir* into *dst_dir*.
///
/// Skips *exclude* (the main agent file already copied as `agent.py`).
/// A directory is considered a Python package if it contains `__init__.py`.
fn copy_python_tree(
    src_dir: &std::path::Path,
    dst_dir: &std::path::Path,
    exclude: &std::path::Path,
) {
    let Ok(entries) = std::fs::read_dir(src_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == exclude {
            continue;
        }
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("py") {
            if let Some(name) = path.file_name() {
                let _ = std::fs::copy(&path, dst_dir.join(name));
            }
        } else if path.is_dir() && path.join("__init__.py").exists() {
            // Python package: copy recursively.
            if let Some(dir_name) = path.file_name() {
                let sub_dst = dst_dir.join(dir_name);
                let _ = std::fs::create_dir_all(&sub_dst);
                copy_python_tree(&path, &sub_dst, exclude);
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Existing commands
// ─────────────────────────────────────────────────────────────────────────────

/// Lists all agents, merging installed agents and runtime agents.
///
/// Installed agents (persisted in `agents.db`) are enriched with their runtime
/// state when currently loaded. Runtime-only agents (started manually via
/// `start_agent`) also appear in the list.
#[tauri::command]
pub async fn list_agents(
    runtime: State<'_, RuntimeHandle>,
    repo: State<'_, Arc<std::sync::Mutex<AgentRepository>>>,
) -> Result<Vec<AgentListItem>, String> {
    // Fetch installed agents from SQLite (sync, lightweight).
    let installed = tokio::task::spawn_blocking({
        let repo = Arc::clone(&repo);
        move || {
            let repo = repo.lock().map_err(|e| format!("mutex poisoned: {e}"))?;
            repo.list().map_err(|e| e.to_string())
        }
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))??;

    // Fetch runtime agents.
    let runtime_entries = runtime
        .registry_handle
        .list_agents()
        .await
        .map_err(|e| e.to_string())?;

    let mut items: Vec<AgentListItem> = Vec::new();

    // Build items from installed agents, enriching with runtime status.
    for agent in &installed {
        let runtime_entry = runtime_entries
            .iter()
            .find(|e| e.manifest.name == agent.name);

        // Prefer the runtime manifest when the agent is loaded. Package
        // installation (`install_agent_package`) stores a stub manifest with
        // hardcoded defaults (memory_namespace=None, tools_required=[], ...)
        // because it never reads the per-agent Python file. The runtime, on
        // the other hand, has the real manifest parsed by apollia-aip when
        // the agent was started. Falling back to the SQLite manifest only
        // when the agent isn't loaded keeps offline metadata available.
        let manifest = runtime_entry
            .map(|e| &e.manifest)
            .unwrap_or(&agent.manifest);
        items.push(AgentListItem {
            id: runtime_entry.map(|e| e.id.to_string()),
            name: agent.name.clone(),
            version: agent.version.clone(),
            enabled: agent.enabled,
            runtime_status: runtime_entry.map(|e| state_to_string(&e.process_state)),
            installed_at: Some(agent.installed_at.clone()),
            description: if manifest.description.is_empty() {
                None
            } else {
                Some(manifest.description.clone())
            },
            tags: manifest.tags.clone(),
            tools_required: manifest.tools_required.clone(),
            tools_optional: manifest.tools_optional.clone(),
            execution_mode: Some(manifest.execution_mode.clone()),
            install_path: Some(agent.install_path.to_string_lossy().to_string()),
            supports_a2a: manifest.supports_a2a,
            skills: manifest
                .skills
                .iter()
                .map(|s| AgentSkillView {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    description: s.description.clone(),
                })
                .collect(),
            agent_type: manifest.agent_type.clone(),
            examples: manifest.examples.clone(),
            limitations: manifest.limitations.clone(),
            setup_notes: manifest.setup_notes.clone(),
            agent_class: manifest.agent_class.clone(),
            memory_namespace: manifest.memory_namespace.clone(),
            shared_memory_namespaces: manifest.shared_memory_namespaces.clone(),
        });
    }

    // Add runtime-only agents not in the installed list.
    for entry in &runtime_entries {
        let already_listed = installed.iter().any(|i| i.name == entry.manifest.name);
        if !already_listed {
            let manifest = &entry.manifest;
            items.push(AgentListItem {
                id: Some(entry.id.to_string()),
                name: manifest.name.clone(),
                version: manifest.version.clone(),
                enabled: true,
                runtime_status: Some(state_to_string(&entry.process_state)),
                installed_at: None,
                description: if manifest.description.is_empty() {
                    None
                } else {
                    Some(manifest.description.clone())
                },
                tags: manifest.tags.clone(),
                tools_required: manifest.tools_required.clone(),
                tools_optional: manifest.tools_optional.clone(),
                execution_mode: Some(manifest.execution_mode.clone()),
                install_path: None,
                supports_a2a: manifest.supports_a2a,
                skills: manifest
                    .skills
                    .iter()
                    .map(|s| AgentSkillView {
                        id: s.id.clone(),
                        name: s.name.clone(),
                        description: s.description.clone(),
                    })
                    .collect(),
                agent_type: manifest.agent_type.clone(),
                examples: manifest.examples.clone(),
                limitations: manifest.limitations.clone(),
                setup_notes: manifest.setup_notes.clone(),
                agent_class: manifest.agent_class.clone(),
                memory_namespace: manifest.memory_namespace.clone(),
                shared_memory_namespaces: manifest.shared_memory_namespaces.clone(),
            });
        }
    }

    Ok(items)
}

/// Lightweight snapshot of the live status of all agents.
///
/// Projects the runtime state (`ProcessState`) onto the 4 statuses exposed to
/// the QuickPicker (`online` / `busy` / `offline` / `error`). Used for fast
/// frontend polling without loading the full [`list_agents`] payload.
///
/// Mapping:
/// - `Active`   → `"online"`
/// - `Degraded` → `"error"` (last run failed)
/// - `Stopping` | `Stopped` | not loaded → `"offline"`
/// - `Initializing` → `"busy"`
#[derive(Debug, Serialize)]
pub struct AgentStatusSnapshot {
    /// Unique agent name.
    pub name: String,
    /// Status normalized for the UI: `online` | `busy` | `offline` | `error`.
    pub status: String,
}

#[tauri::command]
pub async fn agent_status_snapshot(
    runtime: State<'_, RuntimeHandle>,
    repo: State<'_, Arc<std::sync::Mutex<AgentRepository>>>,
) -> Result<Vec<AgentStatusSnapshot>, String> {
    let installed = tokio::task::spawn_blocking({
        let repo = Arc::clone(&repo);
        move || {
            let repo = repo.lock().map_err(|e| format!("mutex poisoned: {e}"))?;
            repo.list().map_err(|e| e.to_string())
        }
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))??;

    let runtime_entries = runtime
        .registry_handle
        .list_agents()
        .await
        .map_err(|e| e.to_string())?;

    let mut items: Vec<AgentStatusSnapshot> = Vec::new();

    for agent in &installed {
        let status = runtime_entries
            .iter()
            .find(|e| e.manifest.name == agent.name)
            .map(|e| process_state_to_ui_status(&e.process_state))
            .unwrap_or_else(|| "offline".to_string());
        items.push(AgentStatusSnapshot {
            name: agent.name.clone(),
            status,
        });
    }

    for entry in &runtime_entries {
        if !installed.iter().any(|i| i.name == entry.manifest.name) {
            items.push(AgentStatusSnapshot {
                name: entry.manifest.name.clone(),
                status: process_state_to_ui_status(&entry.process_state),
            });
        }
    }

    Ok(items)
}

fn process_state_to_ui_status(state: &ProcessState) -> String {
    match state {
        ProcessState::Active => "online",
        ProcessState::Degraded => "error",
        ProcessState::Initializing => "busy",
        ProcessState::Stopping | ProcessState::Stopped => "offline",
    }
    .to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_to_string_all_variants() {
        // GIVEN all ProcessState variants
        // WHEN converted to string
        // THEN each produces the expected snake_case representation
        assert_eq!(state_to_string(&ProcessState::Initializing), "initializing");
        assert_eq!(state_to_string(&ProcessState::Active), "active");
        assert_eq!(state_to_string(&ProcessState::Degraded), "degraded");
        assert_eq!(state_to_string(&ProcessState::Stopping), "stopping");
        assert_eq!(state_to_string(&ProcessState::Stopped), "stopped");
    }

    // InstallAgentResponse serialization
    #[test]
    fn test_install_agent_response_serialization() {
        // GIVEN an InstallAgentResponse
        let resp = InstallAgentResponse {
            name: "mon-agent".to_string(),
            version: "1.0.0".to_string(),
            install_path: "/home/user/.apollia/agents/mon-agent/agent.py".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&resp).expect("serialize");

        // THEN all fields are present
        assert_eq!(json["name"], "mon-agent");
        assert_eq!(json["version"], "1.0.0");
        assert!(json["install_path"]
            .as_str()
            .is_some_and(|p| p.contains("mon-agent")));
    }

    // AgentListItem includes installed fields
    #[test]
    fn test_agent_list_item_includes_installed_fields() {
        // GIVEN an AgentListItem with all fields populated
        let item = AgentListItem {
            id: Some("uuid-123".to_string()),
            name: "hello-agent".to_string(),
            version: "0.2.0".to_string(),
            enabled: true,
            runtime_status: Some("active".to_string()),
            installed_at: Some("2026-03-17T10:00:00Z".to_string()),
            description: Some("A test agent".to_string()),
            tags: vec!["test".to_string(), "demo".to_string()],
            tools_required: vec!["bash".to_string()],
            tools_optional: vec!["file_io".to_string()],
            execution_mode: Some("auto".to_string()),
            install_path: Some("/home/user/.apollia/agents/hello-agent/agent.py".to_string()),
            supports_a2a: true,
            skills: vec![],
            agent_type: Some("assistant".to_string()),
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&item).expect("serialize");

        // THEN all fields are present with correct types
        assert_eq!(json["id"], "uuid-123");
        assert_eq!(json["name"], "hello-agent");
        assert_eq!(json["version"], "0.2.0");
        assert_eq!(json["enabled"], true);
        assert_eq!(json["runtime_status"], "active");
        assert_eq!(json["installed_at"], "2026-03-17T10:00:00Z");
        assert_eq!(json["description"], "A test agent");
        assert_eq!(json["tags"], serde_json::json!(["test", "demo"]));
        assert_eq!(json["tools_required"], serde_json::json!(["bash"]));
        assert_eq!(json["execution_mode"], "auto");
    }

    // AgentListItem with no runtime (installed but not loaded)
    #[test]
    fn test_agent_list_item_without_runtime() {
        // GIVEN an installed agent not currently loaded in runtime
        let item = AgentListItem {
            id: None,
            name: "disabled-agent".to_string(),
            version: "1.0.0".to_string(),
            enabled: false,
            runtime_status: None,
            installed_at: Some("2026-03-17T09:00:00Z".to_string()),
            description: None,
            tags: vec![],
            tools_required: vec![],
            tools_optional: vec![],
            execution_mode: Some("direct".to_string()),
            install_path: Some("/home/user/.apollia/agents/disabled-agent/agent.py".to_string()),
            supports_a2a: false,
            skills: vec![],
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&item).expect("serialize");

        // THEN optional fields are null
        assert!(json["id"].is_null());
        assert!(json["runtime_status"].is_null());
        assert_eq!(json["enabled"], false);
        assert!(json["description"].is_null());
        assert_eq!(json["tags"], serde_json::json!([]));
    }

    // Runtime-only agent (not installed)
    #[test]
    fn test_agent_list_item_runtime_only() {
        // GIVEN a runtime-only agent (not persisted)
        let item = AgentListItem {
            id: Some("uuid-456".to_string()),
            name: "ephemeral-agent".to_string(),
            version: "0.1.0".to_string(),
            enabled: true,
            runtime_status: Some("active".to_string()),
            installed_at: None,
            description: Some("Ephemeral agent".to_string()),
            tags: vec![],
            tools_required: vec![],
            tools_optional: vec![],
            execution_mode: Some("auto".to_string()),
            install_path: None,
            supports_a2a: false,
            skills: vec![],
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&item).expect("serialize");

        // THEN installed_at is null
        assert!(json["installed_at"].is_null());
        assert_eq!(json["runtime_status"], "active");
        assert!(json["install_path"].is_null());
    }

    #[test]
    fn test_update_agent_response_reports_a_completed_restart() {
        // GIVEN an update that cycled the running agent
        let resp = UpdateAgentResponse {
            name: "mon-agent".to_string(),
            version: "2.0.0".to_string(),
            install_path: "/home/user/.apollia/agents/mon-agent/agent.py".to_string(),
            restart_outcome: restart_outcome::RESTARTED.to_string(),
            restart_error: None,
        };

        // WHEN serialized for the window
        let json = serde_json::to_value(&resp).expect("serialize");

        // THEN the outcome is explicit and no cause is carried
        assert_eq!(json["restart_outcome"], "restarted");
        assert!(json["restart_error"].is_null());
        assert_eq!(json["version"], "2.0.0");
    }

    #[test]
    fn test_update_agent_response_keeps_the_restart_cause() {
        // GIVEN an update whose restart failed after the stop
        let resp = UpdateAgentResponse {
            name: "mon-agent".to_string(),
            version: "2.0.0".to_string(),
            install_path: "/home/user/.apollia/agents/mon-agent/agent.py".to_string(),
            restart_outcome: restart_outcome::START_FAILED.to_string(),
            restart_error: Some("tool resolution failed: missing bash".to_string()),
        };

        // WHEN serialized for the window
        let json = serde_json::to_value(&resp).expect("serialize");

        // THEN the raw cause survives the boundary, it is what the operator acts on
        assert_eq!(json["restart_outcome"], "start_failed");
        assert_eq!(
            json["restart_error"],
            "tool resolution failed: missing bash"
        );
    }

    #[test]
    fn test_restart_outcome_constants_are_distinct() {
        // GIVEN the four outcomes the window switches on
        let all = [
            restart_outcome::NOT_RUNNING,
            restart_outcome::RESTARTED,
            restart_outcome::STOP_FAILED,
            restart_outcome::START_FAILED,
        ];

        // WHEN collected into a set
        let unique: std::collections::HashSet<&str> = all.iter().copied().collect();

        // THEN none collapses onto another
        assert_eq!(unique.len(), all.len());
    }
}
