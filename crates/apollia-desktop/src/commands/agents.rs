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

/// Réponse d'une installation ou mise à jour d'agent.
#[derive(Debug, Serialize)]
pub struct InstallAgentResponse {
    /// Nom unique de l'agent installé.
    pub name: String,
    /// Version semver de l'agent.
    pub version: String,
    /// Chemin d'installation sur le disque.
    pub install_path: String,
}

/// Élément de la liste enrichie des agents.
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
    /// Description humaine de l'agent (du manifest).
    pub description: Option<String>,
    /// Tags libres pour le routing/découverte.
    pub tags: Vec<String>,
    /// Outils requis par l'agent.
    pub tools_required: Vec<String>,
    /// Outils optionnels de l'agent.
    pub tools_optional: Vec<String>,
    /// Mode d'exécution (`"auto"`, `"direct"`, `"orchestrated"`).
    pub execution_mode: Option<String>,
    /// Chemin d'installation sur disque (`None` pour les agents runtime-only).
    pub install_path: Option<String>,
    /// Indique si l'agent supporte la communication inter-agents (A2A).
    pub supports_a2a: bool,
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

/// Liste tous les agents — fusionne agents installés et agents runtime.
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

        let manifest = &agent.manifest;
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
// Commandes de persistance
// ─────────────────────────────────────────────────────────────────────────────

/// Installe un agent de façon permanente depuis un fichier Python.
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

    // Copy sibling .py files from the source directory so local imports
    // (e.g. `from apollia_base import ...`) resolve at runtime.
    if let Some(source_dir) = canonical.parent() {
        if let Ok(entries) = std::fs::read_dir(source_dir) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path == canonical {
                    continue;
                }
                if entry_path.extension().and_then(|e| e.to_str()) == Some("py") {
                    if let Some(file_name) = entry_path.file_name() {
                        let dest = agents_dir.join(file_name);
                        let _ = std::fs::copy(&entry_path, &dest);
                    }
                }
            }
        }
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

/// Désinstalle un agent installé.
///
/// 1. Supprime l'entrée de `agents.db` et le répertoire d'installation.
/// 2. Retire l'agent du registry runtime (évite le ghost dans `list_agents`).
/// 3. Émet un `RuntimeEvent::AgentUninstalled` sur l'EventBus.
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
    // find_by_name returns None if the agent was never started — that is fine.
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

/// Active un agent installé pour l'auto-start au boot.
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

/// Désactive un agent installé (ne sera plus chargé au boot).
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

/// Met à jour un agent installé avec un nouveau fichier Python.
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

    // Copy sibling .py files from the source directory so local imports resolve.
    if let Some(source_dir) = canonical.parent() {
        if let Some(install_dir) = existing.install_path.parent() {
            if let Ok(entries) = std::fs::read_dir(source_dir) {
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    if entry_path == canonical {
                        continue;
                    }
                    if entry_path.extension().and_then(|e| e.to_str()) == Some("py") {
                        if let Some(file_name) = entry_path.file_name() {
                            let dest = install_dir.join(file_name);
                            let _ = std::fs::copy(&entry_path, &dest);
                        }
                    }
                }
            }
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

/// Message échangé entre deux agents.
#[derive(Debug, Serialize)]
pub struct AgentMessageView {
    /// Nom de l'agent émetteur.
    pub from_agent: String,
    /// Nom de l'agent destinataire.
    pub to_agent: String,
    /// Contenu JSON du message.
    pub payload: serde_json::Value,
    /// Horodatage d'envoi (RFC 3339).
    pub sent_at: String,
}

/// Plafond maximal de messages retournés.
const MAX_MESSAGE_LIMIT: u32 = 200;
/// Limite par défaut si non spécifiée ou invalide.
const DEFAULT_MESSAGE_LIMIT: u32 = 50;

/// Retourne les messages reçus par un agent, triés par `sent_at` descendant.
///
/// Le `limit` est plafonné à 200 ; si `<= 0` ou non fourni, la valeur par
/// défaut (50) s'applique. Délègue à `GET /api/v1/agents/{name}/messages`.
#[tauri::command]
pub async fn list_agent_messages(
    runtime: State<'_, RuntimeHandle>,
    agent_name: String,
    limit: u32,
) -> Result<Vec<AgentMessageView>, String> {
    list_agent_messages_inner(runtime.api_port, &agent_name, limit).await
}

/// Logique interne pour `list_agent_messages`, testable sans Tauri State.
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

/// Types de template supportés pour le scaffolding d'agent.
const VALID_TEMPLATE_TYPES: &[&str] = &["react", "conversational", "orchestrated"];

/// Regex de validation du nom d'agent : lettres minuscules, chiffres, tirets.
/// Commence par une lettre, finit par une lettre ou un chiffre, minimum 2 caractères.
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

/// Résultat de la création d'un agent depuis un template SDK.
#[derive(Debug, Serialize)]
pub struct CreateAgentResult {
    /// Nom de l'agent créé.
    pub name: String,
    /// Type de template utilisé.
    pub template_type: String,
    /// Chemin du dossier créé sur le disque.
    pub path: String,
}

/// Crée un nouvel agent depuis un template SDK.
///
/// Délègue à `python3 -m apollia new` pour la génération effective.
/// Retourne le chemin du dossier créé en cas de succès.
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

    let output = tokio::process::Command::new("python3")
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
        .map_err(|e| format!("Erreur execution Python : {e}"))?;

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

/// Vérifie si le SDK Python apollia est installé.
///
/// Tente d'importer le module `apollia` via Python et retourne `true` si
/// l'import réussit.
#[tauri::command]
pub async fn check_sdk_available() -> Result<bool, String> {
    let output = tokio::process::Command::new("python3")
        .args(["-c", "import apollia; print(apollia.__version__)"])
        .output()
        .await
        .map_err(|e| format!("Python non disponible : {e}"))?;

    Ok(output.status.success())
}

/// Vérifie si un nom d'agent est disponible (pas déjà utilisé).
///
/// Retourne `true` si le répertoire `~/.apollia/agents/<name>` n'existe pas.
#[tauri::command]
pub async fn check_agent_name_available(name: String) -> Result<bool, String> {
    let target = apollia_data_dir().join("agents").join(&name);
    Ok(!target.exists())
}

// ─────────────────────────────────────────────────────────────────────────────
// Détail agent
// ─────────────────────────────────────────────────────────────────────────────

/// Récupère les détails complets d'un agent par son ID.
///
/// Appelle `GET /api/v1/agents/{agent_id}` et retourne les données enrichies
/// en JSON brut pour éviter de dupliquer la structure de données côté Tauri.
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
