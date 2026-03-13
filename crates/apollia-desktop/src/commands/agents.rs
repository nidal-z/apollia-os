//! Commandes IPC Tauri pour la gestion des agents.
//!
//! Chaque commande délègue aux handles du runtime embarqué. `start_agent` passe
//! par l'API REST interne car elle requiert `AgentLoader` et `BackendFactory`
//! (non exposés sur `RuntimeHandle`).

use apollia_core::{ProcessState, TaskStatus};
use apollia_runtime::embedded::RuntimeHandle;
use serde::Serialize;
use tauri::State;

use super::http_post_json;

/// Informations d'un agent pour l'affichage dans l'UI.
#[derive(Debug, Serialize)]
pub struct AgentInfo {
    /// Identifiant unique de l'agent.
    pub id: String,
    /// Nom de l'agent (depuis le manifest).
    pub name: String,
    /// État courant (initializing, active, degraded, stopping, stopped).
    pub state: String,
    /// Durée depuis le démarrage en secondes.
    pub uptime_secs: u64,
    /// Nombre de tâches complétées.
    pub tasks_completed: u64,
    /// Nombre de tâches échouées.
    pub tasks_failed: u64,
    /// Raison de dégradation si l'agent est en état Degraded.
    pub degraded_reason: Option<String>,
}

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

/// Liste tous les agents enregistrés avec leur état et leurs statistiques.
///
/// Délègue à `AgentRegistryHandle::list_agents()` pour les entrées et à
/// `TaskRouterHandle::all_tasks()` pour les compteurs de tâches par agent.
#[tauri::command]
pub async fn list_agents(state: State<'_, RuntimeHandle>) -> Result<Vec<AgentInfo>, String> {
    let entries = state
        .registry_handle
        .list_agents()
        .await
        .map_err(|e| e.to_string())?;

    let all_tasks = state.router_handle.all_tasks().await.unwrap_or_default();

    let agents = entries
        .into_iter()
        .map(|entry| {
            let agent_id_str = entry.id.to_string();

            let tasks_completed = all_tasks
                .iter()
                .filter(|(_, aid, s)| aid.as_str() == agent_id_str && *s == TaskStatus::Completed)
                .count() as u64;

            let tasks_failed = all_tasks
                .iter()
                .filter(|(_, aid, s)| aid.as_str() == agent_id_str && *s == TaskStatus::Failed)
                .count() as u64;

            let uptime_secs = entry.registered_at.elapsed().as_secs();

            let degraded_reason = if entry.process_state == ProcessState::Degraded {
                Some("optional tools unavailable".to_string())
            } else {
                None
            };

            AgentInfo {
                id: agent_id_str,
                name: entry.manifest.name.clone(),
                state: state_to_string(&entry.process_state),
                uptime_secs,
                tasks_completed,
                tasks_failed,
                degraded_reason,
            }
        })
        .collect();

    Ok(agents)
}

/// Démarre un agent depuis un fichier Python.
///
/// Délègue à `POST /api/v1/agents` car le chargement Python nécessite
/// `AgentLoader` et `BackendFactory` qui ne sont pas sur `RuntimeHandle`.
/// Retourne l'`AgentId` (UUID) du nouvel agent.
#[tauri::command]
pub async fn start_agent(state: State<'_, RuntimeHandle>, path: String) -> Result<String, String> {
    let body = serde_json::json!({ "agent_path": path });
    let resp = http_post_json(state.api_port, "/api/v1/agents", &body).await?;

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
pub async fn stop_agent(state: State<'_, RuntimeHandle>, agent_id: String) -> Result<(), String> {
    let entry = state
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

    state
        .registry_handle
        .update_state(canonical_id.as_str(), ProcessState::Stopping)
        .await
        .map_err(|e| e.to_string())?;

    let _ = state
        .router_handle
        .unregister_coordinator(&canonical_id)
        .await;

    state
        .registry_handle
        .update_state(canonical_id.as_str(), ProcessState::Stopped)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

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

    #[test]
    fn test_agent_info_serializes_to_json() {
        // GIVEN an AgentInfo struct
        let info = AgentInfo {
            id: "abc-123".to_string(),
            name: "hello-agent".to_string(),
            state: "active".to_string(),
            uptime_secs: 3600,
            tasks_completed: 5,
            tasks_failed: 1,
            degraded_reason: None,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&info).expect("serialize");

        // THEN all fields are present with correct values
        assert_eq!(json["id"], "abc-123");
        assert_eq!(json["name"], "hello-agent");
        assert_eq!(json["state"], "active");
        assert_eq!(json["uptime_secs"], 3600);
        assert_eq!(json["tasks_completed"], 5);
        assert_eq!(json["tasks_failed"], 1);
        assert!(json["degraded_reason"].is_null());
    }

    #[test]
    fn test_agent_info_degraded_reason_serialized() {
        // GIVEN an AgentInfo with degraded_reason set
        let info = AgentInfo {
            id: "def-456".to_string(),
            name: "crm-agent".to_string(),
            state: "degraded".to_string(),
            uptime_secs: 120,
            tasks_completed: 0,
            tasks_failed: 0,
            degraded_reason: Some("optional tools unavailable".to_string()),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&info).expect("serialize");

        // THEN degraded_reason is present
        assert_eq!(json["degraded_reason"], "optional tools unavailable");
    }
}
