//! Commandes IPC Tauri pour la gestion des notifications.
//!
//! Chaque commande délègue à l'API REST interne (`/api/v1/notifications/*`)
//! via les helpers `http_get_json` / `http_post_json`. Les données transitent
//! en JSON brut (`serde_json::Value`) pour éviter de dupliquer les types
//! Rust déjà définis dans `apollia-notifications`.

use apollia_runtime::embedded::RuntimeHandle;
use serde::Serialize;
use tauri::State;

use super::{http_get_json, http_post_json};

/// Description publique d'un canal de notification pour l'UI.
#[derive(Debug, Serialize)]
pub struct NotificationChannel {
    /// Identifiant unique du canal (ex: `"desktop"`, `"slack"`).
    pub channel_id: String,
    /// Type de canal : `"desktop"`, `"webhook"`, ou `"sse"`.
    #[serde(rename = "type")]
    pub kind: String,
    /// `true` si le canal est activé dans la configuration.
    pub enabled: bool,
    /// Liste des événements que ce canal accepte.
    pub events: Vec<String>,
}

/// Résultat du test d'un canal individuel.
#[derive(Debug, Serialize)]
pub struct ChannelTestResult {
    /// Identifiant unique du canal.
    pub channel_id: String,
    /// Statut du test : `"ok"`, `"error"`, ou `"disabled"`.
    pub status: String,
    /// Message d'erreur si `status == "error"`.
    pub error: Option<String>,
    /// Latence mesurée en millisecondes.
    pub latency_ms: Option<u64>,
}

/// Entrée de l'historique des notifications pour l'UI.
#[derive(Debug, Serialize)]
pub struct NotificationLogEntry {
    /// Identifiant unique de l'entrée.
    pub id: String,
    /// Nom de l'événement déclencheur (ex: `"task.completed"`).
    pub event_name: String,
    /// Identifiant de la tâche concernée, si applicable.
    pub task_id: Option<String>,
    /// Horodatage d'envoi (ISO 8601).
    pub sent_at: String,
    /// Résultats par canal : `{"canal_id": "ok" | "error"}`.
    pub channels: serde_json::Value,
    /// Erreur globale si la notification n'a pas pu être dispatchée.
    pub error: Option<String>,
}

/// Liste tous les canaux de notification configurés.
///
/// Délègue à `GET /api/v1/notifications/channels` sur l'API REST interne.
#[tauri::command]
pub async fn list_notification_channels(
    state: State<'_, RuntimeHandle>,
) -> Result<Vec<NotificationChannel>, String> {
    let json = http_get_json(state.api_port, "/api/v1/notifications/channels").await?;

    let channels = json
        .get("channels")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let result = channels
        .into_iter()
        .map(|ch| NotificationChannel {
            channel_id: ch
                .get("channel_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            kind: ch
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            enabled: ch.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false),
            events: ch
                .get("events")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|e| e.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect();

    Ok(result)
}

/// Teste un canal de notification spécifique.
///
/// Délègue à `POST /api/v1/notifications/test` et filtre le résultat
/// pour ne retourner que le canal demandé.
#[tauri::command]
pub async fn test_notification_channel(
    state: State<'_, RuntimeHandle>,
    channel_id: String,
) -> Result<ChannelTestResult, String> {
    let body = serde_json::json!({});
    let json = http_post_json(state.api_port, "/api/v1/notifications/test", &body).await?;

    let results = json
        .get("results")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let found = results.into_iter().find(|r| {
        r.get("channel_id")
            .and_then(|v| v.as_str())
            .is_some_and(|id| id == channel_id)
    });

    match found {
        Some(r) => Ok(ChannelTestResult {
            channel_id: r
                .get("channel_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            status: r
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("error")
                .to_string(),
            error: r.get("error").and_then(|v| v.as_str()).map(String::from),
            latency_ms: r.get("latency_ms").and_then(|v| v.as_u64()),
        }),
        None => Err(format!("channel '{channel_id}' not found in test results")),
    }
}

/// Récupère l'historique des notifications.
///
/// Délègue à `GET /api/v1/notifications/logs?last=N`.
#[tauri::command]
pub async fn get_notification_logs(
    state: State<'_, RuntimeHandle>,
    limit: Option<u32>,
) -> Result<Vec<NotificationLogEntry>, String> {
    let l = limit.unwrap_or(50);
    let path = format!("/api/v1/notifications/logs?last={l}");
    let json = http_get_json(state.api_port, &path).await?;

    let entries = json
        .get("entries")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let result = entries
        .into_iter()
        .map(|e| NotificationLogEntry {
            id: e
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            event_name: e
                .get("event_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            task_id: e.get("task_id").and_then(|v| v.as_str()).map(String::from),
            sent_at: e
                .get("sent_at")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            channels: e
                .get("channels")
                .cloned()
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
            error: e.get("error").and_then(|v| v.as_str()).map(String::from),
        })
        .collect();

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_channel_serializes() {
        // GIVEN a NotificationChannel
        let channel = NotificationChannel {
            channel_id: "desktop".to_string(),
            kind: "desktop".to_string(),
            enabled: true,
            events: vec!["task.completed".to_string(), "pipeline.failed".to_string()],
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&channel).expect("serialize");

        // THEN all fields are present with correct values
        assert_eq!(json["channel_id"], "desktop");
        assert_eq!(json["type"], "desktop");
        assert_eq!(json["enabled"], true);
        assert_eq!(json["events"][0], "task.completed");
        assert_eq!(json["events"][1], "pipeline.failed");
    }

    #[test]
    fn test_notification_channel_serializes_empty_events() {
        // GIVEN a NotificationChannel with no event filters
        let channel = NotificationChannel {
            channel_id: "slack".to_string(),
            kind: "webhook".to_string(),
            enabled: false,
            events: vec![],
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&channel).expect("serialize");

        // THEN events is an empty array
        assert_eq!(json["enabled"], false);
        assert!(json["events"].as_array().expect("array").is_empty());
    }

    #[test]
    fn test_channel_test_result_serializes_ok() {
        // GIVEN a successful ChannelTestResult
        let result = ChannelTestResult {
            channel_id: "desktop".to_string(),
            status: "ok".to_string(),
            error: None,
            latency_ms: Some(12),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN status is ok and error is null
        assert_eq!(json["status"], "ok");
        assert!(json["error"].is_null());
        assert_eq!(json["latency_ms"], 12);
    }

    #[test]
    fn test_channel_test_result_serializes_error() {
        // GIVEN a failed ChannelTestResult
        let result = ChannelTestResult {
            channel_id: "slack".to_string(),
            status: "error".to_string(),
            error: Some("connection refused".to_string()),
            latency_ms: Some(5001),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN status is error and error message is present
        assert_eq!(json["status"], "error");
        assert_eq!(json["error"], "connection refused");
    }

    #[test]
    fn test_notification_log_entry_serializes() {
        // GIVEN a NotificationLogEntry
        let entry = NotificationLogEntry {
            id: "abc123".to_string(),
            event_name: "task.completed".to_string(),
            task_id: Some("task-456".to_string()),
            sent_at: "2026-03-13T10:00:00Z".to_string(),
            channels: serde_json::json!({"desktop": "ok", "slack": "error"}),
            error: None,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&entry).expect("serialize");

        // THEN all fields are correct
        assert_eq!(json["event_name"], "task.completed");
        assert_eq!(json["task_id"], "task-456");
        assert_eq!(json["channels"]["desktop"], "ok");
        assert_eq!(json["channels"]["slack"], "error");
        assert!(json["error"].is_null());
    }

    #[test]
    fn test_notification_log_entry_serializes_with_error() {
        // GIVEN a NotificationLogEntry with a global error
        let entry = NotificationLogEntry {
            id: "def789".to_string(),
            event_name: "pipeline.failed".to_string(),
            task_id: None,
            sent_at: "2026-03-13T11:00:00Z".to_string(),
            channels: serde_json::json!({}),
            error: Some("all channels down".to_string()),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&entry).expect("serialize");

        // THEN task_id is null and error is present
        assert!(json["task_id"].is_null());
        assert_eq!(json["error"], "all channels down");
    }
}
