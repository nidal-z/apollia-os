//! The commands that change an agent's state: start, stop, install, uninstall,
//! enable, disable, update, and the restart an update owes the operator when it
//! cycled a running agent.

use std::path::PathBuf;
use std::sync::Arc;

use apollia_core::{EventBusSender, ProcessState};
use apollia_runtime::api::routes_agents::AgentLoader;
use apollia_runtime::embedded::RuntimeHandle;
use apollia_tools::{AgentRepository, InstalledAgent};
use tauri::State;

use super::{
    apollia_data_dir, copy_python_tree, http_post_json, now_rfc3339, restart_outcome,
    InstallAgentResponse, UpdateAgentResponse,
};

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
    start_agent_inner(runtime.api_port, &path).await
}

/// Inner logic for [`start_agent`], callable without Tauri `State`.
async fn start_agent_inner(api_port: u16, path: &str) -> Result<String, String> {
    let body = serde_json::json!({ "agent_path": path });
    let resp = http_post_json(api_port, "/api/v1/agents", &body).await?;

    resp.get("agent_id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| "missing agent_id in response".to_string())
}

/// True while the agent still answers tasks.
///
/// `Stopping` counts as not running: the transition is already under way and
/// the registry refuses a second stop, so the caller must not try to cycle it.
fn is_running(state: &ProcessState) -> bool {
    matches!(
        state,
        ProcessState::Initializing | ProcessState::Active | ProcessState::Degraded
    )
}

/// Stops an agent (transition Stopping → Stopped).
///
/// Delegates directly to the registry and TaskRouter handles, bypassing the
/// REST API; the full cycle is replicated to guarantee event ordering on the
/// EventBus.
#[tauri::command]
pub async fn stop_agent(runtime: State<'_, RuntimeHandle>, agent_id: String) -> Result<(), String> {
    stop_agent_inner(&runtime, &agent_id).await
}

/// Inner logic for [`stop_agent`], callable without Tauri `State`.
///
/// The order is load-bearing: `Stopping` first so the window sees the
/// transition, then the coordinator leaves the router so no new task is
/// dispatched, then `Stopped`. Unregistering before the state change would let
/// a task be routed to a coordinator that is already gone.
async fn stop_agent_inner(runtime: &RuntimeHandle, agent_id: &str) -> Result<(), String> {
    let entry = runtime
        .registry_handle
        .get_agent(agent_id)
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
/// Validates the new module via `AgentLoader`, replaces the file, updates the
/// entry in `agents.db`, and cycles the runtime instance when there is one.
///
/// The cycle is the point of the command, not a courtesy. The embedded
/// interpreter holds the module it imported at start time, so replacing the
/// file on disk leaves a running agent serving the previous code: without the
/// stop and the start, the window would report a new version while the old one
/// keeps answering. The start drops the cached modules of the agent directory
/// before importing, so the helpers copied next to the entry file are re-read
/// too and not only the entry file. When the cycle cannot be completed the
/// answer says so through `restart_outcome`, and the caller renders the real
/// situation instead of a success.
///
/// Emits `RuntimeEvent::AgentInstalled` once the runtime has settled, so a list
/// refresh triggered by the event reads the final state and not the gap between
/// the stop and the start.
// REASON: four of the six are Tauri-injected State, not a call signature an
// author picks; splitting them into a bag would only hide the injection.
// REASON: Tauri command: each parameter is one invoke key or injected State; a struct would change the IPC contract.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn update_agent(
    name: String,
    path: String,
    runtime: State<'_, RuntimeHandle>,
    loader: State<'_, Arc<dyn AgentLoader>>,
    repo: State<'_, Arc<std::sync::Mutex<AgentRepository>>>,
    event_bus: State<'_, EventBusSender>,
) -> Result<UpdateAgentResponse, String> {
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

    let install_path_str = existing.install_path.to_string_lossy().to_string();
    let (outcome, restart_error) = restart_after_update(&runtime, &name, &install_path_str).await;

    let _ = event_bus.send(apollia_core::events::RuntimeEvent::AgentInstalled {
        name: updated.name.clone(),
        version: updated.version.clone(),
    });

    Ok(UpdateAgentResponse {
        name: updated.name,
        version: updated.version,
        install_path: install_path_str,
        restart_outcome: outcome.to_string(),
        restart_error,
    })
}

/// Cycle the runtime instance of *name* so it picks up the module just written.
///
/// Returns the [`restart_outcome`] constant and the raw cause of a failure. A
/// registry lookup that itself fails is reported as "not running" rather than
/// as a failed restart: nothing was stopped, so the previous instance, if any,
/// is untouched, and the operator is told the new module applies at the next
/// start.
async fn restart_after_update(
    runtime: &RuntimeHandle,
    name: &str,
    install_path: &str,
) -> (&'static str, Option<String>) {
    let live_id = match runtime.registry_handle.find_by_name(name).await {
        Ok(Some(id)) => id,
        Ok(None) => return (restart_outcome::NOT_RUNNING, None),
        Err(e) => {
            tracing::warn!(agent = %name, cause = %e, "agent.update.registry_unreachable");
            return (restart_outcome::NOT_RUNNING, None);
        }
    };

    let running = matches!(
        runtime.registry_handle.get_agent(live_id.as_str()).await,
        Ok(Some(ref entry)) if is_running(&entry.process_state)
    );
    if !running {
        return (restart_outcome::NOT_RUNNING, None);
    }

    if let Err(e) = stop_agent_inner(runtime, live_id.as_str()).await {
        tracing::warn!(agent = %name, cause = %e, "agent.update.stop_failed");
        return (restart_outcome::STOP_FAILED, Some(e));
    }

    // `register` evicts the entry that carries the same manifest name, so the
    // start replaces the stopped one instead of leaving two rows behind.
    match start_agent_inner(runtime.api_port, install_path).await {
        Ok(agent_id) => {
            tracing::info!(agent = %name, agent_id = %agent_id, "agent.update.restarted");
            (restart_outcome::RESTARTED, None)
        }
        Err(e) => {
            tracing::warn!(agent = %name, cause = %e, "agent.update.start_failed");
            (restart_outcome::START_FAILED, Some(e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_running_covers_live_states_only() {
        // GIVEN every ProcessState variant
        // WHEN asked whether the agent still answers tasks
        // THEN only the live ones qualify, Stopping included in the negatives
        assert!(is_running(&ProcessState::Initializing));
        assert!(is_running(&ProcessState::Active));
        assert!(is_running(&ProcessState::Degraded));
        assert!(!is_running(&ProcessState::Stopping));
        assert!(!is_running(&ProcessState::Stopped));
    }
}
