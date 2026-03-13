//! Commandes IPC Tauri pour la gestion des triggers.
//!
//! Chaque commande délègue à l'API REST interne (`/api/v1/triggers/*`) via
//! les helpers `http_get_json` / `http_post_json`. Les données transitent
//! en JSON brut (`serde_json::Value`) pour éviter de dupliquer les types
//! Rust déjà définis dans `apollia-triggers`.

use apollia_runtime::embedded::RuntimeHandle;
use serde::Serialize;
use tauri::State;

use super::{http_get_json, http_post_json};

/// Statut d'un trigger pour l'affichage dans l'UI.
#[derive(Debug, Serialize)]
pub struct TriggerStatus {
    /// Identifiant du trigger.
    pub id: String,
    /// Nom de l'agent cible.
    pub agent: String,
    /// Type de source : `"cron"` | `"interval"` | `"file_watch"` | `"webhook"` | `"oneshot"`.
    pub source_kind: String,
    /// Trigger actif ou non.
    pub enabled: bool,
    /// Nombre de fires réussis.
    pub fire_count: u64,
    /// Nombre de skips.
    pub skip_count: u64,
    /// Horodatage du dernier fire (RFC3339) ou `null`.
    pub last_fired: Option<String>,
}

/// Entrée d'historique d'un trigger.
#[derive(Debug, Serialize)]
pub struct TriggerLogEntry {
    /// Identifiant unique de l'entrée.
    pub id: String,
    /// Identifiant du trigger.
    pub trigger_id: String,
    /// Nom de l'agent cible.
    pub agent_name: String,
    /// Horodatage du déclenchement (RFC3339).
    pub fired_at: String,
    /// Identifiant de la tâche soumise (si `status` est `"fired"`).
    pub task_id: Option<String>,
    /// Statut : `"fired"` | `"skipped"` | `"error"`.
    pub status: String,
    /// Raison du skip ou de l'erreur.
    pub reason: Option<String>,
}

/// Résultat d'un fire manuel.
#[derive(Debug, Serialize)]
pub struct FireResult {
    /// Identifiant de la tâche créée.
    pub task_id: String,
}

/// Résultat du rechargement de la configuration.
#[derive(Debug, Serialize)]
pub struct ReloadResult {
    /// Nombre de triggers actifs après rechargement.
    pub reloaded: u64,
}

/// Liste tous les triggers configurés avec leur statut.
///
/// Délègue à `GET /api/v1/triggers` sur l'API REST interne.
#[tauri::command]
pub async fn list_triggers(state: State<'_, RuntimeHandle>) -> Result<Vec<TriggerStatus>, String> {
    let json = http_get_json(state.api_port, "/api/v1/triggers").await?;

    let triggers = json
        .get("triggers")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let result = triggers
        .into_iter()
        .map(|t| TriggerStatus {
            id: t
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            agent: t
                .get("agent")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            source_kind: t
                .get("source_kind")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            enabled: t.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false),
            fire_count: t.get("fire_count").and_then(|v| v.as_u64()).unwrap_or(0),
            skip_count: t.get("skip_count").and_then(|v| v.as_u64()).unwrap_or(0),
            last_fired: t
                .get("last_fired")
                .and_then(|v| v.as_str())
                .map(String::from),
        })
        .collect();

    Ok(result)
}

/// Active ou désactive un trigger.
///
/// Délègue à `POST /api/v1/triggers/:id/enable` ou `POST /api/v1/triggers/:id/disable`.
#[tauri::command]
pub async fn set_trigger_enabled(
    state: State<'_, RuntimeHandle>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    let action = if enabled { "enable" } else { "disable" };
    let path = format!("/api/v1/triggers/{id}/{action}");
    let body = serde_json::json!({});
    http_post_json(state.api_port, &path, &body).await?;
    Ok(())
}

/// Déclenche un trigger manuellement.
///
/// Délègue à `POST /api/v1/triggers/:id/fire`.
#[tauri::command]
pub async fn fire_trigger(
    state: State<'_, RuntimeHandle>,
    id: String,
) -> Result<FireResult, String> {
    let body = serde_json::json!({});
    let json = http_post_json(
        state.api_port,
        &format!("/api/v1/triggers/{id}/fire"),
        &body,
    )
    .await?;

    let task_id = json
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(FireResult { task_id })
}

/// Récupère les logs d'un trigger.
///
/// Délègue à `GET /api/v1/triggers/:id/logs?last=N`.
#[tauri::command]
pub async fn get_trigger_logs(
    state: State<'_, RuntimeHandle>,
    id: String,
    limit: Option<u32>,
) -> Result<Vec<TriggerLogEntry>, String> {
    let l = limit.unwrap_or(20);
    let path = format!("/api/v1/triggers/{id}/logs?last={l}");
    let json = http_get_json(state.api_port, &path).await?;

    let entries = json
        .get("entries")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let result = entries
        .into_iter()
        .map(|e| TriggerLogEntry {
            id: e
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            trigger_id: e
                .get("trigger_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            agent_name: e
                .get("agent_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            fired_at: e
                .get("fired_at")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            task_id: e.get("task_id").and_then(|v| v.as_str()).map(String::from),
            status: e
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            reason: e.get("reason").and_then(|v| v.as_str()).map(String::from),
        })
        .collect();

    Ok(result)
}

/// Recharge la configuration des triggers depuis apollia.toml.
///
/// Délègue à `POST /api/v1/triggers/reload`.
#[tauri::command]
pub async fn reload_triggers(state: State<'_, RuntimeHandle>) -> Result<ReloadResult, String> {
    let body = serde_json::json!({});
    let json = http_post_json(state.api_port, "/api/v1/triggers/reload", &body).await?;

    let reloaded = json.get("reloaded").and_then(|v| v.as_u64()).unwrap_or(0);

    Ok(ReloadResult { reloaded })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_status_serializes() {
        // GIVEN a TriggerStatus
        let status = TriggerStatus {
            id: "daily-report".to_string(),
            agent: "report-agent".to_string(),
            source_kind: "cron".to_string(),
            enabled: true,
            fire_count: 42,
            skip_count: 3,
            last_fired: Some("2026-03-13T08:00:00Z".to_string()),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&status).expect("serialize");

        // THEN all fields are present and correct
        assert_eq!(json["id"], "daily-report");
        assert_eq!(json["agent"], "report-agent");
        assert_eq!(json["source_kind"], "cron");
        assert_eq!(json["enabled"], true);
        assert_eq!(json["fire_count"], 42);
        assert_eq!(json["skip_count"], 3);
        assert_eq!(json["last_fired"], "2026-03-13T08:00:00Z");
    }

    #[test]
    fn test_trigger_status_serializes_no_last_fired() {
        // GIVEN a TriggerStatus that never fired
        let status = TriggerStatus {
            id: "watcher".to_string(),
            agent: "file-agent".to_string(),
            source_kind: "file_watch".to_string(),
            enabled: false,
            fire_count: 0,
            skip_count: 0,
            last_fired: None,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&status).expect("serialize");

        // THEN last_fired is null
        assert!(json["last_fired"].is_null());
        assert_eq!(json["enabled"], false);
    }

    #[test]
    fn test_trigger_log_entry_serializes() {
        // GIVEN a TriggerLogEntry for a fired event
        let entry = TriggerLogEntry {
            id: "abc123".to_string(),
            trigger_id: "daily-report".to_string(),
            agent_name: "report-agent".to_string(),
            fired_at: "2026-03-13T08:00:00Z".to_string(),
            task_id: Some("task-456".to_string()),
            status: "fired".to_string(),
            reason: None,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&entry).expect("serialize");

        // THEN all fields are correct
        assert_eq!(json["status"], "fired");
        assert_eq!(json["task_id"], "task-456");
        assert!(json["reason"].is_null());
    }

    #[test]
    fn test_trigger_log_entry_serializes_error() {
        // GIVEN a TriggerLogEntry for an error event
        let entry = TriggerLogEntry {
            id: "def789".to_string(),
            trigger_id: "webhook-trigger".to_string(),
            agent_name: "api-agent".to_string(),
            fired_at: "2026-03-13T09:00:00Z".to_string(),
            task_id: None,
            status: "error".to_string(),
            reason: Some("agent not running".to_string()),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&entry).expect("serialize");

        // THEN task_id is null and reason is set
        assert!(json["task_id"].is_null());
        assert_eq!(json["reason"], "agent not running");
    }

    #[test]
    fn test_fire_result_serializes() {
        // GIVEN a FireResult
        let result = FireResult {
            task_id: "task-abc".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN task_id is present
        assert_eq!(json["task_id"], "task-abc");
    }

    #[test]
    fn test_reload_result_serializes() {
        // GIVEN a ReloadResult
        let result = ReloadResult { reloaded: 5 };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN reloaded count is correct
        assert_eq!(json["reloaded"], 5);
    }
}
