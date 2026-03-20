//! Repository SQLite pour la configuration des notifications.
//!
//! Ce module gère la persistance des canaux de notification, des événements
//! globaux et des logs de notification dans une base `notifications.db` dédiée.
//!
//! Les opérations d'écriture sont validées via [`crate::validation`] avant
//! insertion en base (fail fast).

use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::validation::{validate_channel, validate_events, NotificationConfigError};

/// Canal de notification persisté en base SQLite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationChannelRow {
    /// Identifiant unique du canal (ex: `"slack-ops"`, `"desktop"`).
    pub id: String,
    /// Type de canal : `"desktop"` ou `"webhook"`.
    pub channel_type: String,
    /// Si `false`, le canal est ignoré lors du dispatch.
    pub enabled: bool,
    /// Configuration spécifique au type de canal (ex: `{"url": "..."}` pour webhook).
    pub config_json: serde_json::Value,
    /// Liste d'événements spécifiques à ce canal. `None` = utilise les événements globaux.
    pub events_json: Option<Vec<String>>,
    /// Date de création (ISO 8601).
    pub created_at: String,
    /// Date de dernière mise à jour (ISO 8601).
    pub updated_at: String,
}

/// Log d'envoi de notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationLogRow {
    /// Identifiant unique du log (UUID v4).
    pub id: String,
    /// Nom de l'événement déclencheur (ex: `"task.completed"`).
    pub event_name: String,
    /// Identifiant de la tâche concernée, si applicable.
    pub task_id: Option<String>,
    /// Identifiant de l'agent concerné, si applicable.
    pub agent_id: Option<String>,
    /// Horodatage d'envoi (ISO 8601).
    pub sent_at: String,
    /// Résultats par canal au format JSON.
    pub channels: String,
    /// Message d'erreur global, si au moins un canal a échoué.
    pub error: Option<String>,
}

/// Repository SQLite pour la configuration des notifications.
///
/// Gère trois tables dans `notifications.db` :
/// - `notification_channels` — canaux de livraison configurés
/// - `notification_global_events` — événements activés globalement
/// - `notification_logs` — historique des envois
pub struct NotificationConfigRepository {
    conn: Connection,
}

impl NotificationConfigRepository {
    /// Ouvre (ou crée) la base `notifications.db` au chemin donné.
    ///
    /// Active WAL, crée les 3 tables et l'index si absents.
    pub fn open(path: &Path) -> Result<Self, NotificationConfigError> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(Self::MIGRATION_SQL)?;
        Ok(Self { conn })
    }

    /// SQL de migration appliqué à l'ouverture.
    const MIGRATION_SQL: &'static str = "
        CREATE TABLE IF NOT EXISTS notification_channels (
            id              TEXT PRIMARY KEY,
            channel_type    TEXT NOT NULL,
            enabled         BOOLEAN NOT NULL DEFAULT 1,
            config_json     TEXT NOT NULL DEFAULT '{}',
            events_json     TEXT,
            created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            CHECK (channel_type IN ('desktop', 'webhook'))
        );

        CREATE TABLE IF NOT EXISTS notification_global_events (
            event_name      TEXT PRIMARY KEY
        );

        CREATE TABLE IF NOT EXISTS notification_logs (
            id              TEXT PRIMARY KEY,
            event_name      TEXT NOT NULL,
            task_id         TEXT,
            agent_id        TEXT,
            sent_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            channels        TEXT NOT NULL DEFAULT '{}',
            error           TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_notif_logs_sent_at
            ON notification_logs(sent_at);
    ";

    // ── Channels ─────────────────────────────────────────────────────────

    /// Insère un nouveau canal de notification.
    ///
    /// Valide le canal avant insertion. Retourne [`NotificationConfigError::DuplicateId`]
    /// si un canal avec le même identifiant existe déjà.
    pub fn insert_channel(
        &self,
        ch: &NotificationChannelRow,
    ) -> Result<(), NotificationConfigError> {
        validate_channel(ch)?;

        if self.get_channel(&ch.id)?.is_some() {
            return Err(NotificationConfigError::DuplicateId(ch.id.clone()));
        }

        let events_str = ch
            .events_json
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "[]".into()));
        let config_str = serde_json::to_string(&ch.config_json).unwrap_or_else(|_| "{}".into());

        self.conn.execute(
            "INSERT INTO notification_channels (id, channel_type, enabled, config_json, events_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![ch.id, ch.channel_type, ch.enabled, config_str, events_str],
        )?;

        Ok(())
    }

    /// Met à jour un canal existant.
    ///
    /// Valide le canal avant mise à jour. Retourne [`NotificationConfigError::NotFound`]
    /// si le canal n'existe pas.
    pub fn update_channel(
        &self,
        id: &str,
        ch: &NotificationChannelRow,
    ) -> Result<(), NotificationConfigError> {
        validate_channel(ch)?;

        if self.get_channel(id)?.is_none() {
            return Err(NotificationConfigError::NotFound(id.into()));
        }

        let events_str = ch
            .events_json
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "[]".into()));
        let config_str = serde_json::to_string(&ch.config_json).unwrap_or_else(|_| "{}".into());

        self.conn.execute(
            "UPDATE notification_channels
             SET channel_type = ?1, enabled = ?2, config_json = ?3, events_json = ?4,
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?5",
            params![ch.channel_type, ch.enabled, config_str, events_str, id],
        )?;

        Ok(())
    }

    /// Supprime un canal par identifiant.
    ///
    /// Retourne [`NotificationConfigError::NotFound`] si le canal n'existe pas.
    pub fn delete_channel(&self, id: &str) -> Result<(), NotificationConfigError> {
        let count = self.conn.execute(
            "DELETE FROM notification_channels WHERE id = ?1",
            params![id],
        )?;
        if count == 0 {
            return Err(NotificationConfigError::NotFound(id.into()));
        }
        Ok(())
    }

    /// Récupère un canal par identifiant.
    ///
    /// Retourne `None` si le canal n'existe pas.
    pub fn get_channel(
        &self,
        id: &str,
    ) -> Result<Option<NotificationChannelRow>, NotificationConfigError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, channel_type, enabled, config_json, events_json, created_at, updated_at
             FROM notification_channels WHERE id = ?1",
        )?;

        let mut rows = stmt.query_map(params![id], row_to_channel)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Liste tous les canaux enregistrés.
    pub fn list_channels(&self) -> Result<Vec<NotificationChannelRow>, NotificationConfigError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, channel_type, enabled, config_json, events_json, created_at, updated_at
             FROM notification_channels ORDER BY id",
        )?;

        let rows = stmt.query_map([], row_to_channel)?;
        let mut channels = Vec::new();
        for row in rows {
            channels.push(row?);
        }
        Ok(channels)
    }

    // ── Global events ────────────────────────────────────────────────────

    /// Remplace l'ensemble des événements globaux (delete + re-insert).
    ///
    /// Valide chaque nom d'événement contre [`crate::validation::KNOWN_EVENTS`].
    pub fn set_global_events(&self, events: &[String]) -> Result<(), NotificationConfigError> {
        validate_events(events)?;

        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM notification_global_events", [])?;
        let mut stmt =
            tx.prepare("INSERT INTO notification_global_events (event_name) VALUES (?1)")?;
        for event in events {
            stmt.execute(params![event])?;
        }
        drop(stmt);
        tx.commit()?;
        Ok(())
    }

    /// Récupère la liste des événements globaux.
    pub fn get_global_events(&self) -> Result<Vec<String>, NotificationConfigError> {
        let mut stmt = self
            .conn
            .prepare("SELECT event_name FROM notification_global_events ORDER BY event_name")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    // ── Logs ─────────────────────────────────────────────────────────────

    /// Écrit un log de notification.
    pub fn write_log(&self, log: &NotificationLogRow) -> Result<(), NotificationConfigError> {
        self.conn.execute(
            "INSERT INTO notification_logs (id, event_name, task_id, agent_id, sent_at, channels, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                log.id,
                log.event_name,
                log.task_id,
                log.agent_id,
                log.sent_at,
                log.channels,
                log.error,
            ],
        )?;
        Ok(())
    }

    /// Récupère les derniers logs de notification, triés par date d'envoi décroissante.
    pub fn query_logs(
        &self,
        limit: usize,
    ) -> Result<Vec<NotificationLogRow>, NotificationConfigError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, event_name, task_id, agent_id, sent_at, channels, error
             FROM notification_logs ORDER BY sent_at DESC LIMIT ?1",
        )?;

        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(NotificationLogRow {
                id: row.get(0)?,
                event_name: row.get(1)?,
                task_id: row.get(2)?,
                agent_id: row.get(3)?,
                sent_at: row.get(4)?,
                channels: row.get(5)?,
                error: row.get(6)?,
            })
        })?;

        let mut logs = Vec::new();
        for row in rows {
            logs.push(row?);
        }
        Ok(logs)
    }
}

// ─── Conversion Row → ChannelConfig ─────────────────────────────────────────

impl NotificationChannelRow {
    /// Convertit une [`NotificationChannelRow`] en [`crate::config::ChannelConfig`].
    ///
    /// Utilisé par le boot Supervisor (STORY-187) pour reconstruire la configuration
    /// de notification depuis SQLite.
    pub fn to_channel_config(&self) -> crate::config::ChannelConfig {
        let kind = match self.channel_type.as_str() {
            "webhook" => crate::config::ChannelKind::Webhook,
            "sse" => crate::config::ChannelKind::Sse,
            _ => crate::config::ChannelKind::Desktop,
        };
        let url = self
            .config_json
            .get("url")
            .and_then(|v| v.as_str())
            .map(String::from);
        crate::config::ChannelConfig {
            id: self.id.clone(),
            kind,
            enabled: self.enabled,
            events: self.events_json.clone(),
            url,
        }
    }
}

/// Convertit une ligne SQLite en [`NotificationChannelRow`].
fn row_to_channel(row: &rusqlite::Row<'_>) -> rusqlite::Result<NotificationChannelRow> {
    let config_str: String = row.get(3)?;
    let events_str: Option<String> = row.get(4)?;

    let config_json: serde_json::Value =
        serde_json::from_str(&config_str).unwrap_or(serde_json::Value::Object(Default::default()));

    let events_json: Option<Vec<String>> = events_str.and_then(|s| serde_json::from_str(&s).ok());

    Ok(NotificationChannelRow {
        id: row.get(0)?,
        channel_type: row.get(1)?,
        enabled: row.get(2)?,
        config_json,
        events_json,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_db() -> (PathBuf, NotificationConfigRepository) {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("notifications.db");
        let repo =
            NotificationConfigRepository::open(&path).expect("failed to open notifications.db");
        // Leak the dir so it's not cleaned up before we're done
        std::mem::forget(dir);
        (path, repo)
    }

    fn make_channel(id: &str, channel_type: &str) -> NotificationChannelRow {
        NotificationChannelRow {
            id: id.into(),
            channel_type: channel_type.into(),
            enabled: true,
            config_json: if channel_type == "webhook" {
                serde_json::json!({"url": "https://hooks.slack.com/test"})
            } else {
                serde_json::json!({})
            },
            events_json: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn make_log(id: &str, event: &str) -> NotificationLogRow {
        NotificationLogRow {
            id: id.into(),
            event_name: event.into(),
            task_id: Some("t-001".into()),
            agent_id: None,
            sent_at: format!("2026-03-20T10:0{id}:00Z"),
            channels: r#"{"desktop":"ok"}"#.into(),
            error: None,
        }
    }

    // AC-1 — Création notifications.db avec 3 tables
    #[test]
    fn test_open_creates_tables() {
        // GIVEN un chemin vers une nouvelle base
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("notifications.db");

        // WHEN
        let repo = NotificationConfigRepository::open(&path).expect("open");

        // THEN les 3 tables existent
        let tables: Vec<String> = repo
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .expect("prepare")
            .query_map([], |row| row.get(0))
            .expect("query")
            .filter_map(|r| r.ok())
            .collect();

        assert!(tables.contains(&"notification_channels".into()));
        assert!(tables.contains(&"notification_global_events".into()));
        assert!(tables.contains(&"notification_logs".into()));

        // ET l'index existe
        let indices: Vec<String> = repo
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type='index' AND name = 'idx_notif_logs_sent_at'")
            .expect("prepare")
            .query_map([], |row| row.get(0))
            .expect("query")
            .filter_map(|r| r.ok())
            .collect();
        assert_eq!(indices.len(), 1);
    }

    // AC-2 — Insert, get, list, update, delete channels
    #[test]
    fn test_insert_and_get_channel() {
        // GIVEN
        let (_path, repo) = temp_db();
        let ch = make_channel("slack-ops", "webhook");

        // WHEN
        repo.insert_channel(&ch).expect("insert");

        // THEN
        let found = repo.get_channel("slack-ops").expect("get").expect("Some");
        assert_eq!(found.id, "slack-ops");
        assert_eq!(found.channel_type, "webhook");
        assert!(found.enabled);
        assert!(!found.created_at.is_empty());

        let all = repo.list_channels().expect("list");
        assert_eq!(all.len(), 1);
    }

    // AC-2 — Update channel
    #[test]
    fn test_update_channel() {
        // GIVEN
        let (_path, repo) = temp_db();
        let ch = make_channel("slack-ops", "webhook");
        repo.insert_channel(&ch).expect("insert");

        let original = repo.get_channel("slack-ops").expect("get").expect("Some");

        // WHEN
        let mut updated = ch.clone();
        updated.enabled = false;
        repo.update_channel("slack-ops", &updated).expect("update");

        // THEN
        let result = repo.get_channel("slack-ops").expect("get").expect("Some");
        assert!(!result.enabled);
        assert!(result.updated_at >= original.updated_at);
    }

    // AC-2 — Delete channel
    #[test]
    fn test_delete_channel() {
        // GIVEN
        let (_path, repo) = temp_db();
        let ch = make_channel("slack-ops", "webhook");
        repo.insert_channel(&ch).expect("insert");

        // WHEN
        repo.delete_channel("slack-ops").expect("delete");

        // THEN
        assert!(repo.get_channel("slack-ops").expect("get").is_none());
    }

    // AC-3 — Duplicate channel ID
    #[test]
    fn test_duplicate_channel_id() {
        // GIVEN
        let (_path, repo) = temp_db();
        let ch = make_channel("slack-ops", "webhook");
        repo.insert_channel(&ch).expect("insert");

        // WHEN
        let err = repo.insert_channel(&ch).unwrap_err();

        // THEN
        assert!(
            matches!(&err, NotificationConfigError::DuplicateId(id) if id == "slack-ops"),
            "expected DuplicateId, got: {err:?}"
        );
    }

    // AC-4 — Global events CRUD
    #[test]
    fn test_global_events_crud() {
        // GIVEN
        let (_path, repo) = temp_db();

        // WHEN — set initial events
        repo.set_global_events(&["task.completed".into(), "task.failed".into()])
            .expect("set");

        // THEN
        let events = repo.get_global_events().expect("get");
        assert_eq!(events, vec!["task.completed", "task.failed"]);

        // WHEN — replace events
        repo.set_global_events(&["task.completed".into()])
            .expect("set");

        // THEN — complete replacement
        let events = repo.get_global_events().expect("get");
        assert_eq!(events, vec!["task.completed"]);
    }

    // AC-5 — Validation webhook sans URL
    #[test]
    fn test_validation_webhook_no_url() {
        // GIVEN
        let (_path, repo) = temp_db();
        let ch = NotificationChannelRow {
            id: "bad-webhook".into(),
            channel_type: "webhook".into(),
            enabled: true,
            config_json: serde_json::json!({}),
            events_json: None,
            created_at: String::new(),
            updated_at: String::new(),
        };

        // WHEN
        let err = repo.insert_channel(&ch).unwrap_err();

        // THEN
        assert!(
            matches!(&err, NotificationConfigError::ValidationError(msg) if msg.contains("url")),
            "expected ValidationError about url, got: {err:?}"
        );
    }

    // AC-6 — Validation event name inconnu
    #[test]
    fn test_validation_unknown_event() {
        // GIVEN
        let (_path, repo) = temp_db();

        // WHEN
        let err = repo
            .set_global_events(&["unknown.event".into()])
            .unwrap_err();

        // THEN
        assert!(
            matches!(&err, NotificationConfigError::ValidationError(msg) if msg.contains("unknown.event")),
            "expected ValidationError about unknown.event, got: {err:?}"
        );
    }

    // AC-7 — Write log + query logs
    #[test]
    fn test_write_and_query_logs() {
        // GIVEN
        let (_path, repo) = temp_db();
        repo.write_log(&make_log("1", "task.completed"))
            .expect("write");
        repo.write_log(&make_log("2", "task.failed"))
            .expect("write");
        repo.write_log(&make_log("3", "agent.degraded"))
            .expect("write");

        // WHEN
        let logs = repo.query_logs(2).expect("query");

        // THEN
        assert_eq!(logs.len(), 2);
        // Ordered by sent_at DESC
        assert_eq!(logs[0].id, "3");
        assert_eq!(logs[1].id, "2");
    }

    // DB vide — list_channels retourne vide
    #[test]
    fn test_list_channels_empty() {
        // GIVEN
        let (_path, repo) = temp_db();

        // WHEN
        let channels = repo.list_channels().expect("list");

        // THEN
        assert!(channels.is_empty());
    }
}
