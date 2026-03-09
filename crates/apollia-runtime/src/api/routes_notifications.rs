//! Routes REST pour les notifications — STORY-104.
//!
//! Expose trois endpoints :
//! - `GET  /api/v1/notifications/channels` — liste des canaux configurés
//! - `POST /api/v1/notifications/test`     — envoi d'une notification de test
//! - `GET  /api/v1/notifications/logs`     — historique depuis SQLite

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use rusqlite::params;
use serde::{Deserialize, Serialize};

use apollia_notifications::{
    build_channels,
    config::{ChannelKind, NotificationConfig},
    engine::Notification,
    Severity,
};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

// ─── Response types ───────────────────────────────────────────────────────────

/// Description publique d'un canal de notification retournée par `GET /channels`.
#[derive(Debug, Serialize)]
pub struct ChannelInfo {
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

/// Résultat du test d'un canal individuel retourné par `POST /test`.
#[derive(Debug, Serialize, Deserialize)]
pub struct ChannelTestResult {
    /// Identifiant unique du canal.
    pub channel_id: String,
    /// Type de canal.
    #[serde(rename = "type")]
    pub kind: String,
    /// Statut du test : `"ok"`, `"error"`, ou `"disabled"`.
    pub status: String,
    /// Message d'erreur si `status == "error"`.
    pub error: Option<String>,
    /// Latence mesurée en millisecondes (`None` si le canal est désactivé).
    pub latency_ms: Option<u64>,
}

/// Entrée de l'historique des notifications retournée par `GET /logs`.
#[derive(Debug, Serialize)]
pub struct NotifLogEntry {
    /// Identifiant unique de l'entrée.
    pub id: String,
    /// Nom de l'événement (ex: `"task.input_required"`).
    pub event_name: String,
    /// Identifiant de la tâche concernée, si applicable.
    pub task_id: Option<String>,
    /// Identifiant de l'agent concerné, si applicable.
    pub agent_id: Option<String>,
    /// Horodatage d'envoi (ISO 8601).
    pub sent_at: String,
    /// Résultats par canal : `{"canal_id": "ok" | "error"}`.
    pub channels: serde_json::Value,
    /// Erreur globale si la notification n'a pas pu être dispatché.
    pub error: Option<String>,
}

// ─── Query params ─────────────────────────────────────────────────────────────

/// Paramètres de requête pour `GET /api/v1/notifications/logs`.
#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    /// Nombre maximal d'entrées à retourner (défaut : 20, max : 1000).
    #[serde(default = "default_last")]
    pub last: usize,
}

fn default_last() -> usize {
    20
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// `GET /api/v1/notifications/channels` — liste des canaux configurés.
///
/// Retourne la liste complète des canaux (actifs et inactifs) depuis
/// `notification_config` dans [`AppState`]. Si aucune configuration n'est
/// présente, retourne un tableau vide.
pub async fn list_channels<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Json<serde_json::Value> {
    let Some(config) = &state.notification_config else {
        return Json(serde_json::json!({ "channels": [] }));
    };

    let channels: Vec<ChannelInfo> = config
        .channels
        .iter()
        .map(|ch| ChannelInfo {
            channel_id: ch.id.clone(),
            kind: channel_kind_str(&ch.kind),
            enabled: ch.enabled,
            events: ch
                .events
                .clone()
                .unwrap_or_else(|| config.events.clone()),
        })
        .collect();

    Json(serde_json::json!({ "channels": channels }))
}

/// `POST /api/v1/notifications/test` — envoi d'une notification de test.
///
/// Pour chaque canal activé dans la config :
/// - Instancie le canal via [`build_channels`]
/// - Envoie une [`Notification`] de test avec l'événement `"test.ping"`
/// - Mesure la latence et collecte le statut (`"ok"`, `"error"`, `"disabled"`)
///
/// Les canaux désactivés apparaissent dans le résultat avec `status = "disabled"`
/// sans tentative d'envoi.
pub async fn test_channels<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let Some(config) = state.notification_config.clone() else {
        return (
            StatusCode::OK,
            Json(serde_json::json!({ "results": [] })),
        );
    };

    let mut results: Vec<ChannelTestResult> = Vec::new();

    // Collect disabled channels (reported without testing)
    for ch in &config.channels {
        if !ch.enabled {
            results.push(ChannelTestResult {
                channel_id: ch.id.clone(),
                kind: channel_kind_str(&ch.kind),
                status: "disabled".to_string(),
                error: None,
                latency_ms: None,
            });
        }
    }

    // Build active channel instances
    let channels = match build_channels(&config.channels) {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    let test_notif = make_test_notification();

    // Test each active channel
    for channel in &channels {
        let start = Instant::now();
        let outcome = channel.send(&test_notif).await;
        let latency_ms = start.elapsed().as_millis() as u64;
        let kind = channel_kind_by_id(channel.id(), &config);

        let (status, error) = match outcome {
            Ok(()) => ("ok".to_string(), None),
            Err(e) => ("error".to_string(), Some(e.to_string())),
        };

        results.push(ChannelTestResult {
            channel_id: channel.id().to_string(),
            kind,
            status,
            error,
            latency_ms: Some(latency_ms),
        });
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({ "results": results })),
    )
}

/// `GET /api/v1/notifications/logs?last=N` — historique des notifications.
///
/// Lit depuis la table `notification_logs` dans `~/.apollia/hitl.db`.
/// La table est créée idempotentiellement si elle n'existe pas encore.
/// Retourne les `last` entrées les plus récentes (défaut 20, max 1000).
pub async fn notification_logs<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(params): Query<LogsQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    let last = params.last.min(1000);
    let db_path = resolve_notif_db_path(&state);

    let entries_result =
        tokio::task::spawn_blocking(move || query_notification_logs(&db_path, last)).await;

    match entries_result {
        Ok(Ok(entries)) => (
            StatusCode::OK,
            Json(serde_json::json!({ "entries": entries })),
        ),
        Ok(Err(msg)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": msg })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("spawn_blocking failed: {e}") })),
        ),
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Build a test [`Notification`] for the `"test.ping"` event.
fn make_test_notification() -> Notification {
    Notification {
        event: "test.ping".to_string(),
        timestamp: chrono::Utc::now(),
        task_id: None,
        agent: None,
        message: "Notification de test Apollia OS".to_string(),
        metadata: HashMap::new(),
        severity: Severity::Info,
    }
}

/// Return the string representation of a [`ChannelKind`].
fn channel_kind_str(kind: &ChannelKind) -> String {
    match kind {
        ChannelKind::Desktop => "desktop".to_string(),
        ChannelKind::Webhook => "webhook".to_string(),
        ChannelKind::Sse => "sse".to_string(),
    }
}

/// Return the `kind` string for the channel identified by `id` in `config`.
///
/// Falls back to `"unknown"` if the ID is not found (should not happen in practice).
fn channel_kind_by_id(id: &str, config: &NotificationConfig) -> String {
    config
        .channels
        .iter()
        .find(|ch| ch.id == id)
        .map(|ch| channel_kind_str(&ch.kind))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Resolve the path of the notification log database.
///
/// Uses `~/.apollia/hitl.db` by default.
fn resolve_notif_db_path<B: ExecutionBackend + Clone>(state: &AppState<B>) -> PathBuf {
    state
        .task_repository
        .as_ref()
        .and_then(|_| {
            // TaskRepository doesn't expose the DB path publicly;
            // derive the default path from the HOME directory.
            std::env::var("HOME").ok()
        })
        .map(|home| PathBuf::from(format!("{home}/.apollia/hitl.db")))
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| PathBuf::from(format!("{home}/.apollia/hitl.db")))
        })
        .unwrap_or_else(|| PathBuf::from("/tmp/apollia-notif.db"))
}

/// Open `db_path`, create `notification_logs` if needed, and return the last `N` entries.
fn query_notification_logs(db_path: &PathBuf, last: usize) -> Result<Vec<serde_json::Value>, String> {
    let conn = rusqlite::Connection::open(db_path)
        .map_err(|e| format!("impossible d'ouvrir la base : {e}"))?;

    // Lazy creation — idempotent
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS notification_logs (
            id          TEXT    PRIMARY KEY,
            event_name  TEXT    NOT NULL,
            task_id     TEXT,
            agent_id    TEXT,
            sent_at     TEXT    NOT NULL DEFAULT (datetime('now')),
            channels    TEXT    NOT NULL DEFAULT '{}',
            error       TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_notif_logs_sent_at ON notification_logs(sent_at);",
    )
    .map_err(|e| format!("migration notification_logs échouée : {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT id, event_name, task_id, agent_id, sent_at, channels, error
               FROM notification_logs
              ORDER BY sent_at DESC
              LIMIT ?1",
        )
        .map_err(|e| format!("prepare échoué : {e}"))?;

    let rows = stmt
        .query_map(params![last as i64], |row| {
            let channels_raw: String = row.get(5)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                channels_raw,
                row.get::<_, Option<String>>(6)?,
            ))
        })
        .map_err(|e| format!("query échouée : {e}"))?;

    let mut entries = Vec::new();
    for row_result in rows {
        let (id, event_name, task_id, agent_id, sent_at, channels_raw, error) =
            row_result.map_err(|e| format!("lecture ligne échouée : {e}"))?;

        let channels = serde_json::from_str(&channels_raw).unwrap_or(serde_json::Value::Object(
            serde_json::Map::new(),
        ));

        entries.push(serde_json::json!({
            "id": id,
            "event_name": event_name,
            "task_id": task_id,
            "agent_id": agent_id,
            "sent_at": sent_at,
            "channels": channels,
            "error": error,
        }));
    }

    Ok(entries)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_notifications::config::{ChannelConfig, ChannelKind, NotificationConfig};

    // ── AC-5 — JSON structure de ChannelTestResult ────────────────────────────

    // GIVEN un ChannelTestResult en succès
    // WHEN sérialisé en JSON
    // THEN tous les champs attendus sont présents avec les bonnes valeurs
    #[test]
    fn test_ac5_channel_test_result_json_structure_ok() {
        // GIVEN
        let result = ChannelTestResult {
            channel_id: "desktop".to_string(),
            kind: "desktop".to_string(),
            status: "ok".to_string(),
            error: None,
            latency_ms: Some(12),
        };

        // WHEN
        let json = serde_json::to_value(&result).expect("sérialisation échoue");

        // THEN
        assert_eq!(json["channel_id"], "desktop");
        assert_eq!(json["type"], "desktop");
        assert_eq!(json["status"], "ok");
        assert!(json["error"].is_null());
        assert_eq!(json["latency_ms"], 12);
    }

    // GIVEN un ChannelTestResult en erreur
    // WHEN sérialisé en JSON
    // THEN status = "error" et error contient le message
    #[test]
    fn test_ac5_channel_test_result_json_structure_error() {
        // GIVEN
        let result = ChannelTestResult {
            channel_id: "slack".to_string(),
            kind: "webhook".to_string(),
            status: "error".to_string(),
            error: Some("connexion refusée".to_string()),
            latency_ms: Some(5001),
        };

        // WHEN
        let json = serde_json::to_value(&result).expect("sérialisation échoue");

        // THEN
        assert_eq!(json["status"], "error");
        assert_eq!(json["error"], "connexion refusée");
        assert_eq!(json["latency_ms"], 5001);
    }

    // GIVEN un ChannelTestResult désactivé
    // WHEN sérialisé en JSON
    // THEN status = "disabled", latency_ms = null, error = null
    #[test]
    fn test_ac1_disabled_channel_result_structure() {
        // GIVEN
        let result = ChannelTestResult {
            channel_id: "monitoring".to_string(),
            kind: "webhook".to_string(),
            status: "disabled".to_string(),
            error: None,
            latency_ms: None,
        };

        // WHEN
        let json = serde_json::to_value(&result).expect("sérialisation échoue");

        // THEN
        assert_eq!(json["status"], "disabled");
        assert!(json["latency_ms"].is_null());
        assert!(json["error"].is_null());
    }

    // ── channel_kind_str helper ───────────────────────────────────────────────

    // GIVEN les 3 variantes de ChannelKind
    // WHEN channel_kind_str est appelé
    // THEN les chaînes attendues sont retournées
    #[test]
    fn test_channel_kind_str_all_variants() {
        // GIVEN / WHEN / THEN
        assert_eq!(channel_kind_str(&ChannelKind::Desktop), "desktop");
        assert_eq!(channel_kind_str(&ChannelKind::Webhook), "webhook");
        assert_eq!(channel_kind_str(&ChannelKind::Sse), "sse");
    }

    // ── channel_kind_by_id helper ─────────────────────────────────────────────

    // GIVEN une config avec un canal desktop "mon-desktop"
    // WHEN channel_kind_by_id est appelé avec "mon-desktop"
    // THEN "desktop" est retourné
    #[test]
    fn test_channel_kind_by_id_found() {
        // GIVEN
        let config = NotificationConfig {
            events: vec![],
            channels: vec![ChannelConfig {
                id: "mon-desktop".to_string(),
                kind: ChannelKind::Desktop,
                enabled: true,
                events: None,
                url: None,
            }],
        };

        // WHEN
        let kind = channel_kind_by_id("mon-desktop", &config);

        // THEN
        assert_eq!(kind, "desktop");
    }

    // GIVEN une config sans canal "inconnu"
    // WHEN channel_kind_by_id est appelé avec "inconnu"
    // THEN "unknown" est retourné (fallback)
    #[test]
    fn test_channel_kind_by_id_not_found_returns_unknown() {
        // GIVEN
        let config = NotificationConfig {
            events: vec![],
            channels: vec![],
        };

        // WHEN
        let kind = channel_kind_by_id("inconnu", &config);

        // THEN
        assert_eq!(kind, "unknown");
    }

    // ── notification_logs lazy creation ──────────────────────────────────────

    // GIVEN un chemin de base SQLite vide (en mémoire via fichier temp)
    // WHEN query_notification_logs est appelé
    // THEN la table est créée et le tableau retourné est vide
    #[test]
    fn test_logs_lazy_table_creation_returns_empty() {
        // GIVEN
        let dir = tempfile::tempdir().expect("tempdir échoue");
        let db_path = dir.path().join("test.db");

        // WHEN
        let result = query_notification_logs(&db_path, 20);

        // THEN
        let entries = result.expect("query_notification_logs ne doit pas échouer");
        assert!(entries.is_empty(), "table vide → aucune entrée");
    }
}
