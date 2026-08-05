//! Tauri IPC commands for agent management.
//!
//! Each command delegates to the embedded runtime handles for runtime agents
//! (list/start/stop) and to the [`AgentRepository`] + [`AgentLoader`]
//! for persistence operations (install/uninstall/enable/disable/update).

use std::path::PathBuf;
use std::sync::Arc;

use apollia_core::{EventBusSender, ProcessState};
use apollia_runtime::api::routes_agents::AgentLoader;
use apollia_runtime::embedded::RuntimeHandle;
use apollia_tools::{AgentRepository, InstalledAgent};
use serde::Serialize;
use tauri::State;

use super::http_post_json;

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
    PathBuf::from(home).join(".apollia")
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

/// Starts an agent from a Python file.
///
/// Delegates to `POST /api/v1/agents` because loading Python requires
/// `AgentLoader` and `BackendFactory`, which are not on `RuntimeHandle`.
/// Returns the new agent's `AgentId` (UUID).
#[tauri::command]
pub async fn start_agent(
    runtime: State<'_, RuntimeHandle>,
    path: String,
) -> Result<String, String> {
    let body = serde_json::json!({ "agent_path": path });
    let resp = http_post_json(runtime.api_port, "/api/v1/agents", &body).await?;

    resp.get("agent_id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| "missing agent_id in response".to_string())
}

/// Stops an agent (transition Stopping → Stopped).
///
/// Delegates directly to the registry and TaskRouter handles, bypassing the
/// REST API; the full cycle is replicated to guarantee event ordering on the
/// EventBus.
#[tauri::command]
pub async fn stop_agent(runtime: State<'_, RuntimeHandle>, agent_id: String) -> Result<(), String> {
    let entry = runtime
        .registry_handle
        .get_agent(&agent_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("agent not found: {agent_id}"))?;

    if entry.process_state == ProcessState::Stopped {
        return Err(format!("agent already stopped: {agent_id}"));
    }
    if entry.process_state == ProcessState::Stopping {
        return Err(format!("agent already stopping: {agent_id}"));
    }

    let canonical_id = entry.id;

    runtime
        .registry_handle
        .update_state(canonical_id.as_str(), ProcessState::Stopping)
        .await
        .map_err(|e| e.to_string())?;

    let _ = runtime
        .router_handle
        .unregister_coordinator(&canonical_id)
        .await;

    runtime
        .registry_handle
        .update_state(canonical_id.as_str(), ProcessState::Stopped)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Commandes de persistance
// ─────────────────────────────────────────────────────────────────────────────

/// Installs an agent permanently from a Python file.
///
/// Validates the Python module via `AgentLoader`, copies the file to
/// `~/.apollia/agents/<name>/agent.py`, and persists the entry in `agents.db`.
/// Emits a `RuntimeEvent::AgentInstalled` on the EventBus.
#[tauri::command]
pub async fn install_agent(
    path: String,
    loader: State<'_, Arc<dyn AgentLoader>>,
    repo: State<'_, Arc<std::sync::Mutex<AgentRepository>>>,
    event_bus: State<'_, EventBusSender>,
) -> Result<InstallAgentResponse, String> {
    let source_path = PathBuf::from(&path);
    if !source_path.exists() {
        return Err(format!("file not found: {path}"));
    }

    let canonical = source_path
        .canonicalize()
        .map_err(|e| format!("cannot resolve path {path}: {e}"))?;

    // Validate the Python module. A rejection here carries the Python error
    // verbatim (a missing decorator argument, a bad import), which is the only
    // thing that tells the operator what to change. Log it: without this line
    // the refusal reached the window as one generic sentence and left nothing
    // behind on either side.
    let manifest = loader.load_and_validate(&canonical).map_err(|e| {
        tracing::warn!(
            path = %canonical.display(),
            cause = %e,
            "agent.install.rejected"
        );
        e
    })?;

    let data_dir = apollia_data_dir();
    let agents_dir = data_dir.join("agents").join(&manifest.name);
    std::fs::create_dir_all(&agents_dir)
        .map_err(|e| format!("cannot create directory {}: {e}", agents_dir.display()))?;

    let install_path = agents_dir.join("agent.py");
    std::fs::copy(&canonical, &install_path).map_err(|e| {
        format!(
            "cannot copy {} to {}: {e}",
            canonical.display(),
            install_path.display()
        )
    })?;

    // Copy sibling .py files and Python sub-packages from the source directory
    // so local imports (e.g. `from assistants.shared import ...`) resolve at runtime.
    if let Some(source_dir) = canonical.parent() {
        copy_python_tree(source_dir, &agents_dir, &canonical);
    }

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

    let agent_for_db = agent.clone();
    let repo_clone = Arc::clone(&repo);
    tokio::task::spawn_blocking(move || {
        let repo = repo_clone
            .lock()
            .map_err(|e| format!("mutex poisoned: {e}"))?;
        repo.save(&agent_for_db)
            .map_err(|e| format!("failed to save to database: {e}"))
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))??;

    let _ = event_bus.send(apollia_core::events::RuntimeEvent::AgentInstalled {
        name: agent.name.clone(),
        version: agent.version.clone(),
    });

    Ok(InstallAgentResponse {
        name: agent.name,
        version: agent.version,
        install_path: install_path.to_string_lossy().to_string(),
    })
}

/// Uninstalls an installed agent.
///
/// 1. Removes the entry from `agents.db` and the install directory.
/// 2. Removes the agent from the runtime registry (avoids a ghost in `list_agents`).
/// 3. Emits a `RuntimeEvent::AgentUninstalled` on the EventBus.
#[tauri::command]
pub async fn uninstall_agent(
    name: String,
    runtime: State<'_, RuntimeHandle>,
    repo: State<'_, Arc<std::sync::Mutex<AgentRepository>>>,
    event_bus: State<'_, EventBusSender>,
) -> Result<(), String> {
    let repo_clone = Arc::clone(&repo);
    let name_clone = name.clone();

    // Verify agent exists, then delete in one blocking call.
    let existing = tokio::task::spawn_blocking({
        let repo = Arc::clone(&repo_clone);
        let name = name_clone.clone();
        move || {
            let repo = repo.lock().map_err(|e| format!("mutex poisoned: {e}"))?;
            let agent = repo
                .get(&name)
                .map_err(|e| format!("database error: {e}"))?
                .ok_or_else(|| format!("agent '{name}' not found in installed agents"))?;
            repo.delete(&name)
                .map_err(|e| format!("failed to delete from database: {e}"))?;
            Ok::<_, String>(agent)
        }
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))??;

    // Remove install directory (best-effort).
    if let Some(parent) = existing.install_path.parent() {
        let _ = std::fs::remove_dir_all(parent);
    }

    // Remove from in-memory registry so list_agents no longer returns it.
    // find_by_name returns None if the agent was never started; that is fine.
    if let Ok(Some(agent_id)) = runtime.registry_handle.find_by_name(&name).await {
        let _ = runtime
            .router_handle
            .unregister_coordinator(&agent_id)
            .await;
        let _ = runtime.registry_handle.unregister(agent_id.as_str()).await;
    }

    let _ = event_bus.send(apollia_core::events::RuntimeEvent::AgentUninstalled { name });

    Ok(())
}

/// Enables an installed agent for auto-start at boot.
///
/// Sets `enabled = true` in `agents.db`.
/// Emits a `RuntimeEvent::AgentEnabled` on the EventBus.
#[tauri::command]
pub async fn enable_agent(
    name: String,
    repo: State<'_, Arc<std::sync::Mutex<AgentRepository>>>,
    event_bus: State<'_, EventBusSender>,
) -> Result<(), String> {
    let repo_clone = Arc::clone(&repo);
    let name_clone = name.clone();
    tokio::task::spawn_blocking(move || {
        let repo = repo_clone
            .lock()
            .map_err(|e| format!("mutex poisoned: {e}"))?;
        repo.set_enabled(&name_clone, true).map_err(|e| match e {
            apollia_tools::AgentRepositoryError::NotFound(n) => {
                format!("agent '{n}' not found in installed agents")
            }
            other => format!("database error: {other}"),
        })
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))??;

    let _ = event_bus.send(apollia_core::events::RuntimeEvent::AgentEnabled { name: name.clone() });

    Ok(())
}

/// Disables an installed agent (it will no longer load at boot).
///
/// Sets `enabled = false` in `agents.db`.
/// Emits a `RuntimeEvent::AgentDisabled` on the EventBus.
#[tauri::command]
pub async fn disable_agent(
    name: String,
    repo: State<'_, Arc<std::sync::Mutex<AgentRepository>>>,
    event_bus: State<'_, EventBusSender>,
) -> Result<(), String> {
    let repo_clone = Arc::clone(&repo);
    let name_clone = name.clone();
    tokio::task::spawn_blocking(move || {
        let repo = repo_clone
            .lock()
            .map_err(|e| format!("mutex poisoned: {e}"))?;
        repo.set_enabled(&name_clone, false).map_err(|e| match e {
            apollia_tools::AgentRepositoryError::NotFound(n) => {
                format!("agent '{n}' not found in installed agents")
            }
            other => format!("database error: {other}"),
        })
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))??;

    let _ =
        event_bus.send(apollia_core::events::RuntimeEvent::AgentDisabled { name: name.clone() });

    Ok(())
}

/// Updates an installed agent with a new Python file.
///
/// Validates the new module via `AgentLoader`, replaces the file, and updates
/// the entry in `agents.db`. Emits `RuntimeEvent::AgentInstalled`.
#[tauri::command]
pub async fn update_agent(
    name: String,
    path: String,
    loader: State<'_, Arc<dyn AgentLoader>>,
    repo: State<'_, Arc<std::sync::Mutex<AgentRepository>>>,
    event_bus: State<'_, EventBusSender>,
) -> Result<InstallAgentResponse, String> {
    let source_path = PathBuf::from(&path);
    if !source_path.exists() {
        return Err(format!("file not found: {path}"));
    }

    let canonical = source_path
        .canonicalize()
        .map_err(|e| format!("cannot resolve path {path}: {e}"))?;

    // Verify agent exists.
    let existing = {
        let repo_clone = Arc::clone(&repo);
        let name_clone = name.clone();
        tokio::task::spawn_blocking(move || {
            let repo = repo_clone
                .lock()
                .map_err(|e| format!("mutex poisoned: {e}"))?;
            repo.get(&name_clone)
                .map_err(|e| format!("database error: {e}"))
        })
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))??
        .ok_or_else(|| format!("agent '{name}' not found in installed agents"))?
    };

    // Validate the new Python module. Same reasoning as the install path: the
    // Python cause is the actionable part and must survive the boundary.
    let manifest = loader.load_and_validate(&canonical).map_err(|e| {
        tracing::warn!(
            agent = %name,
            path = %canonical.display(),
            cause = %e,
            "agent.update.rejected"
        );
        e
    })?;

    // Copy new file to install location.
    std::fs::copy(&canonical, &existing.install_path).map_err(|e| {
        format!(
            "cannot copy {} to {}: {e}",
            canonical.display(),
            existing.install_path.display()
        )
    })?;

    // Copy sibling .py files and Python sub-packages so local imports resolve.
    if let Some(source_dir) = canonical.parent() {
        if let Some(install_dir) = existing.install_path.parent() {
            copy_python_tree(source_dir, install_dir, &canonical);
        }
    }

    let updated = InstalledAgent {
        name: existing.name.clone(),
        version: manifest.version.clone(),
        install_path: existing.install_path.clone(),
        source_path: canonical,
        manifest,
        enabled: existing.enabled,
        installed_at: existing.installed_at,
        updated_at: now_rfc3339(),
    };

    let updated_for_db = updated.clone();
    let repo_clone = Arc::clone(&repo);
    tokio::task::spawn_blocking(move || {
        let repo = repo_clone
            .lock()
            .map_err(|e| format!("mutex poisoned: {e}"))?;
        repo.save(&updated_for_db)
            .map_err(|e| format!("failed to update database: {e}"))
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))??;

    let _ = event_bus.send(apollia_core::events::RuntimeEvent::AgentInstalled {
        name: updated.name.clone(),
        version: updated.version.clone(),
    });

    Ok(InstallAgentResponse {
        name: updated.name,
        version: updated.version,
        install_path: existing.install_path.to_string_lossy().to_string(),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Agent messages
// ─────────────────────────────────────────────────────────────────────────────

/// Message exchanged between two agents.
#[derive(Debug, Serialize)]
pub struct AgentMessageView {
    /// Name of the sending agent.
    pub from_agent: String,
    /// Name of the receiving agent.
    pub to_agent: String,
    /// JSON content of the message.
    pub payload: serde_json::Value,
    /// Send timestamp (RFC 3339).
    pub sent_at: String,
}

/// Maximum cap on returned messages.
const MAX_MESSAGE_LIMIT: u32 = 200;
/// Default limit when unspecified or invalid.
const DEFAULT_MESSAGE_LIMIT: u32 = 50;

/// Returns the messages received by an agent, sorted by `sent_at` descending.
///
/// `limit` is capped at 200; if `<= 0` or not provided, the default (50)
/// applies. Delegates to `GET /api/v1/agents/{name}/messages`.
#[tauri::command]
pub async fn list_agent_messages(
    runtime: State<'_, RuntimeHandle>,
    agent_name: String,
    limit: u32,
) -> Result<Vec<AgentMessageView>, String> {
    list_agent_messages_inner(runtime.api_port, &agent_name, limit).await
}

/// Inner logic for `list_agent_messages`, testable without Tauri State.
async fn list_agent_messages_inner(
    port: u16,
    agent_name: &str,
    limit: u32,
) -> Result<Vec<AgentMessageView>, String> {
    let effective = if limit == 0 {
        DEFAULT_MESSAGE_LIMIT
    } else if limit > MAX_MESSAGE_LIMIT {
        MAX_MESSAGE_LIMIT
    } else {
        limit
    };

    let path = format!("/api/v1/agents/{agent_name}/messages?limit={effective}");
    match super::http_get_json(port, &path).await {
        Ok(json) => {
            let messages = json
                .get("messages")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            let views: Vec<AgentMessageView> = messages
                .into_iter()
                .map(|m| AgentMessageView {
                    from_agent: m
                        .get("from_agent")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    to_agent: m
                        .get("to_agent")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    payload: m.get("payload").cloned().unwrap_or(serde_json::Value::Null),
                    sent_at: m
                        .get("sent_at")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
                .collect();

            Ok(views)
        }
        Err(e) if e.contains("404") => Ok(vec![]),
        Err(e) => Err(format!("list_agent_messages: {e}")),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Scaffolding from template
// ─────────────────────────────────────────────────────────────────────────────

/// Template types supported for agent scaffolding.
const VALID_TEMPLATE_TYPES: &[&str] = &["react", "conversational", "orchestrated"];

/// Validates the agent name: lowercase letters, digits, hyphens. Starts with a
/// letter, ends with a letter or digit, minimum 2 characters.
fn is_valid_agent_name(name: &str) -> bool {
    if name.len() < 2 {
        return false;
    }
    let bytes = name.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    let last = bytes[bytes.len() - 1];
    if !last.is_ascii_lowercase() && !last.is_ascii_digit() {
        return false;
    }
    name.bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Result of creating an agent from an SDK template.
#[derive(Debug, Serialize)]
pub struct CreateAgentResult {
    /// Name of the created agent.
    pub name: String,
    /// Template type used.
    pub template_type: String,
    /// Path of the directory created on disk.
    pub path: String,
}

/// Creates a new agent from an SDK template.
///
/// Delegates to `python3 -m apollia new` for the actual generation.
/// Returns the path of the created directory on success.
#[tauri::command]
pub async fn create_agent_from_template(
    name: String,
    template_type: String,
) -> Result<CreateAgentResult, String> {
    if !VALID_TEMPLATE_TYPES.contains(&template_type.as_str()) {
        return Err(format!(
            "Type invalide '{}'. Types supportes : {}",
            template_type,
            VALID_TEMPLATE_TYPES.join(", ")
        ));
    }

    if !is_valid_agent_name(&name) {
        return Err(
            "Le nom ne doit contenir que des lettres minuscules, chiffres et tirets".to_string(),
        );
    }

    let agents_dir = apollia_data_dir().join("agents");
    let target_dir = agents_dir.join(&name);
    if target_dir.exists() {
        return Err(format!("Un agent '{}' existe deja", name));
    }

    let mut scaffold = tokio::process::Command::new(sdk_interpreter());
    apollia_core::subprocess_window::hide_console_async(&mut scaffold);
    let output = scaffold
        .args([
            "-m",
            "apollia",
            "new",
            &name,
            "--type",
            &template_type,
            "--output",
        ])
        .arg(&target_dir)
        .output()
        .await
        .map_err(|e| format!("Python execution error: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Echec du scaffolding : {stderr}"));
    }

    Ok(CreateAgentResult {
        name,
        template_type,
        path: target_dir.display().to_string(),
    })
}

/// The interpreter to use when the bundled Python environment is the point.
///
/// Two call sites here run the `apollia` SDK, which ships inside the bundle's
/// site-packages, so they want the bundled interpreter and its `PYTHONHOME`.
/// They used to reach it as `python3`, which resolves to the *system*
/// interpreter everywhere except Windows: it only worked when that interpreter
/// happened to share the bundle's minor version, and crashed on startup
/// otherwise. `APOLLIA_BUNDLED_PYTHON` is exported at boot with the absolute
/// path, so the intent is now explicit rather than accidental.
///
/// Falls back to `python3` when the variable is absent (a source build with no
/// bundle), which is the pre-existing behaviour and the only case where the
/// SDK might come from the system environment instead.
fn sdk_interpreter() -> std::ffi::OsString {
    std::env::var_os("APOLLIA_BUNDLED_PYTHON").unwrap_or_else(|| "python3".into())
}

/// Checks whether the apollia Python SDK is installed.
///
/// Attempts to import the `apollia` module via Python and returns `true` if
/// the import succeeds.
#[tauri::command]
pub async fn check_sdk_available() -> Result<bool, String> {
    let mut probe = tokio::process::Command::new(sdk_interpreter());
    apollia_core::subprocess_window::hide_console_async(&mut probe);
    let output = probe
        .args(["-c", "import apollia; print(apollia.__version__)"])
        .output()
        .await
        .map_err(|e| format!("Python non disponible : {e}"))?;

    Ok(output.status.success())
}

/// Checks whether an agent name is available (not already in use).
///
/// Returns `true` if the `~/.apollia/agents/<name>` directory does not exist.
#[tauri::command]
pub async fn check_agent_name_available(name: String) -> Result<bool, String> {
    let target = apollia_data_dir().join("agents").join(&name);
    Ok(!target.exists())
}

// ─────────────────────────────────────────────────────────────────────────────
// Agent detail
// ─────────────────────────────────────────────────────────────────────────────

/// Fetches the full details of an agent by its ID.
///
/// Calls `GET /api/v1/agents/{agent_id}` and returns the enriched data as raw
/// JSON to avoid duplicating the data structure on the Tauri side.
#[tauri::command]
pub async fn get_agent_detail(
    runtime: State<'_, RuntimeHandle>,
    agent_id: String,
) -> Result<serde_json::Value, String> {
    let path = format!("/api/v1/agents/{agent_id}");
    super::http_get_json(runtime.api_port, &path).await
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
    fn test_agent_message_view_serializes() {
        // GIVEN an AgentMessageView
        let view = AgentMessageView {
            from_agent: "agent-a".to_string(),
            to_agent: "agent-b".to_string(),
            payload: serde_json::json!({"data": "hello"}),
            sent_at: "2026-03-24T10:00:00Z".to_string(),
        };

        // WHEN serialized
        let json = serde_json::to_value(&view).expect("serialize");

        // THEN all fields are present
        assert_eq!(json["from_agent"], "agent-a");
        assert_eq!(json["to_agent"], "agent-b");
        assert_eq!(json["payload"]["data"], "hello");
        assert_eq!(json["sent_at"], "2026-03-24T10:00:00Z");
    }

    #[test]
    fn test_message_limit_constants() {
        // GIVEN the limit constants
        // THEN they have expected values
        assert_eq!(MAX_MESSAGE_LIMIT, 200);
        assert_eq!(DEFAULT_MESSAGE_LIMIT, 50);
    }

    #[test]
    fn test_is_valid_agent_name_accepts_valid_names() {
        // GIVEN valid agent names
        // WHEN validated
        // THEN they are accepted
        assert!(is_valid_agent_name("my-agent"));
        assert!(is_valid_agent_name("agent1"));
        assert!(is_valid_agent_name("my-cool-agent-42"));
        assert!(is_valid_agent_name("ab"));
    }

    #[test]
    fn test_is_valid_agent_name_rejects_invalid_names() {
        // GIVEN invalid agent names
        // WHEN validated
        // THEN they are rejected
        assert!(!is_valid_agent_name(""));
        assert!(!is_valid_agent_name("a"));
        assert!(!is_valid_agent_name("MyAgent"));
        assert!(!is_valid_agent_name("my agent"));
        assert!(!is_valid_agent_name("1agent"));
        assert!(!is_valid_agent_name("agent-"));
        assert!(!is_valid_agent_name("my_agent"));
        assert!(!is_valid_agent_name("my-agent!"));
    }

    #[test]
    fn test_create_agent_result_serializes() {
        // GIVEN a CreateAgentResult
        let result = CreateAgentResult {
            name: "my-agent".to_string(),
            template_type: "react".to_string(),
            path: "/home/user/.apollia/agents/my-agent".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN all fields are present with correct values
        assert_eq!(json["name"], "my-agent");
        assert_eq!(json["template_type"], "react");
        assert!(json["path"]
            .as_str()
            .is_some_and(|p| p.contains("my-agent")));
    }

    #[test]
    fn test_valid_template_types_constant() {
        // GIVEN the template type list
        // THEN it contains exactly the 3 expected types
        assert_eq!(VALID_TEMPLATE_TYPES.len(), 3);
        assert!(VALID_TEMPLATE_TYPES.contains(&"react"));
        assert!(VALID_TEMPLATE_TYPES.contains(&"conversational"));
        assert!(VALID_TEMPLATE_TYPES.contains(&"orchestrated"));
    }
}
