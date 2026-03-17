//! Commandes IPC Tauri pour la gestion des agents.
//!
//! Chaque commande délègue aux handles du runtime embarqué pour les agents
//! runtime (list/start/stop) et à l'[`AgentRepository`] + [`AgentLoader`]
//! pour les opérations de persistance (install/uninstall/enable/disable/update).

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
// Types réponse
// ─────────────────────────────────────────────────────────────────────────────

/// Réponse d'une installation ou mise à jour d'agent (AC-1, AC-4).
#[derive(Debug, Serialize)]
pub struct InstallAgentResponse {
    /// Nom unique de l'agent installé.
    pub name: String,
    /// Version semver de l'agent.
    pub version: String,
    /// Chemin d'installation sur le disque.
    pub install_path: String,
}

/// Élément de la liste enrichie des agents (AC-5).
///
/// Fusionne les agents installés (persistés dans `agents.db`) avec les agents
/// actifs dans le runtime, offrant une vue unifiée au frontend.
#[derive(Debug, Serialize)]
pub struct AgentListItem {
    /// UUID runtime de l'agent (`None` si l'agent est installé mais pas chargé).
    pub id: Option<String>,
    /// Nom unique de l'agent.
    pub name: String,
    /// Version semver.
    pub version: String,
    /// Indique si l'agent est activé pour le chargement au boot.
    pub enabled: bool,
    /// État runtime (`"active"`, `"degraded"`, `"stopped"`, ou `None` si non chargé).
    pub runtime_status: Option<String>,
    /// Horodatage d'installation (RFC 3339, `None` pour les agents runtime-only).
    pub installed_at: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Convertit un `ProcessState` en chaîne pour le frontend.
fn state_to_string(state: &ProcessState) -> String {
    match state {
        ProcessState::Initializing => "initializing".to_string(),
        ProcessState::Active => "active".to_string(),
        ProcessState::Degraded => "degraded".to_string(),
        ProcessState::Stopping => "stopping".to_string(),
        ProcessState::Stopped => "stopped".to_string(),
    }
}

/// Résout le répertoire `~/.apollia/`.
fn apollia_data_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".apollia")
}

/// Horodatage courant en RFC 3339.
fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

// ─────────────────────────────────────────────────────────────────────────────
// Commandes existantes
// ─────────────────────────────────────────────────────────────────────────────

/// Liste tous les agents — fusionne agents installés et agents runtime (AC-5).
///
/// Les agents installés (persistés dans `agents.db`) sont enrichis avec leur
/// état runtime s'ils sont actuellement chargés. Les agents runtime-only
/// (démarrés manuellement via `start_agent`) apparaissent aussi dans la liste.
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

        items.push(AgentListItem {
            id: runtime_entry.map(|e| e.id.to_string()),
            name: agent.name.clone(),
            version: agent.version.clone(),
            enabled: agent.enabled,
            runtime_status: runtime_entry.map(|e| state_to_string(&e.process_state)),
            installed_at: Some(agent.installed_at.clone()),
        });
    }

    // Add runtime-only agents not in the installed list.
    for entry in &runtime_entries {
        let already_listed = installed.iter().any(|i| i.name == entry.manifest.name);
        if !already_listed {
            items.push(AgentListItem {
                id: Some(entry.id.to_string()),
                name: entry.manifest.name.clone(),
                version: entry.manifest.version.clone(),
                enabled: true,
                runtime_status: Some(state_to_string(&entry.process_state)),
                installed_at: None,
            });
        }
    }

    Ok(items)
}

/// Démarre un agent depuis un fichier Python.
///
/// Délègue à `POST /api/v1/agents` car le chargement Python nécessite
/// `AgentLoader` et `BackendFactory` qui ne sont pas sur `RuntimeHandle`.
/// Retourne l'`AgentId` (UUID) du nouvel agent.
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

/// Arrête un agent (transition Stopping → Stopped).
///
/// Délègue directement aux handles du registry et du TaskRouter, sans passer
/// par l'API REST — le cycle complet est répliqué pour garantir l'ordre des
/// événements sur l'EventBus.
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
// Commandes de persistance (STORY-181)
// ─────────────────────────────────────────────────────────────────────────────

/// Installe un agent de façon permanente depuis un fichier Python (AC-1).
///
/// Valide le module Python via `AgentLoader`, copie le fichier dans
/// `~/.apollia/agents/<name>/agent.py`, et persiste l'entrée dans `agents.db`.
/// Émet un `RuntimeEvent::AgentInstalled` sur l'EventBus.
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

    // Validate the Python module.
    let manifest = loader.load_and_validate(&canonical)?;

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

/// Désinstalle un agent installé (AC-2).
///
/// Supprime l'entrée de `agents.db` et le répertoire d'installation.
/// Émet un `RuntimeEvent::AgentUninstalled` sur l'EventBus.
#[tauri::command]
pub async fn uninstall_agent(
    name: String,
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

    let _ = event_bus.send(apollia_core::events::RuntimeEvent::AgentUninstalled { name });

    Ok(())
}

/// Active un agent installé pour l'auto-start au boot (AC-3).
///
/// Met `enabled = true` dans `agents.db`.
/// Émet un `RuntimeEvent::AgentEnabled` sur l'EventBus.
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

/// Désactive un agent installé (ne sera plus chargé au boot) (AC-3).
///
/// Met `enabled = false` dans `agents.db`.
/// Émet un `RuntimeEvent::AgentDisabled` sur l'EventBus.
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

/// Met à jour un agent installé avec un nouveau fichier Python (AC-4).
///
/// Valide le nouveau module via `AgentLoader`, remplace le fichier, et met à
/// jour l'entrée dans `agents.db`. Émet `RuntimeEvent::AgentInstalled`.
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

    // Validate the new Python module.
    let manifest = loader.load_and_validate(&canonical)?;

    // Copy new file to install location.
    std::fs::copy(&canonical, &existing.install_path).map_err(|e| {
        format!(
            "cannot copy {} to {}: {e}",
            canonical.display(),
            existing.install_path.display()
        )
    })?;

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

    // AC-1 — InstallAgentResponse serialization
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

    // AC-5 — AgentListItem includes installed fields
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
    }

    // AC-5 — AgentListItem with no runtime (installed but not loaded)
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
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&item).expect("serialize");

        // THEN optional fields are null
        assert!(json["id"].is_null());
        assert!(json["runtime_status"].is_null());
        assert_eq!(json["enabled"], false);
    }

    // AC-5 — Runtime-only agent (not installed)
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
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&item).expect("serialize");

        // THEN installed_at is null
        assert!(json["installed_at"].is_null());
        assert_eq!(json["runtime_status"], "active");
    }
}
