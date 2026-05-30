//! SQLite repository for the notifications configuration.
//!
//! This module handles persistence of the notification channels, global events,
//! and notification logs in a dedicated `notifications.db` database.
//!
//! Write operations are validated via [`crate::validation`] before insertion
//! (fail fast).

use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::validation::{validate_channel, validate_events, NotificationConfigError};

/// Notification channel persisted in the SQLite database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationChannelRow {
    /// Unique channel identifier (e.g. `"slack-ops"`, `"desktop"`).
    pub id: String,
    /// Free-form display name for the UI (spaces, accents, emojis allowed).
    /// `None` = fall back to `id` on the UI side. Max 80 Unicode characters.
    #[serde(default)]
    pub label: Option<String>,
    /// Channel type: `"desktop"` or `"webhook"`.
    pub channel_type: String,
    /// If `false`, the channel is ignored during dispatch.
    pub enabled: bool,
    /// Channel-type-specific configuration (e.g. `{"url": "..."}` for webhook).
    pub config_json: serde_json::Value,
    /// Channel-specific event list. `None` = uses the global events.
    pub events_json: Option<Vec<String>>,
    /// Minimum interval in seconds between two notifications for the same
    /// `(channel, event)` pair. `0` = no throttling. Capped by validation at
    /// [`crate::validation::MAX_MIN_INTERVAL_SECONDS`].
    #[serde(default)]
    pub min_interval_seconds: u32,
    /// Creation date (ISO 8601).
    pub created_at: String,
    /// Last-update date (ISO 8601).
    pub updated_at: String,
}

/// Notification delivery log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationLogRow {
    /// Unique log identifier (UUID v4).
    pub id: String,
    /// Name of the triggering event (e.g. `"task.completed"`).
    pub event_name: String,
    /// Identifier of the relevant task, if applicable.
    pub task_id: Option<String>,
    /// Identifier of the relevant agent, if applicable.
    pub agent_id: Option<String>,
    /// Delivery timestamp (ISO 8601).
    pub sent_at: String,
    /// Per-channel results as JSON.
    pub channels: String,
    /// Global error message, if at least one channel failed.
    pub error: Option<String>,
}

/// SQLite repository for the notifications configuration.
///
/// Manages three tables in `notifications.db`:
/// - `notification_channels`: configured delivery channels
/// - `notification_global_events`: globally enabled events
/// - `notification_logs`: delivery history
pub struct NotificationConfigRepository {
    conn: Connection,
}

impl NotificationConfigRepository {
    /// Opens (or creates) the `notifications.db` database at the given path.
    ///
    /// Enables WAL, creates the 3 tables and the index if absent, then applies
    /// the incremental migrations (column additions via [`ensure_columns`]).
    pub fn open(path: &Path) -> Result<Self, NotificationConfigError> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(Self::MIGRATION_SQL)?;
        ensure_columns(&conn)?;
        Ok(Self { conn })
    }

    /// Migration SQL applied at open.
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

    // --- Channels --------------------------------------------------------

    /// Inserts a new notification channel.
    ///
    /// Validates the channel before insertion. Returns
    /// [`NotificationConfigError::DuplicateId`] if a channel with the same
    /// identifier already exists.
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
            "INSERT INTO notification_channels (id, label, channel_type, enabled, config_json, events_json, min_interval_seconds)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![ch.id, ch.label, ch.channel_type, ch.enabled, config_str, events_str, ch.min_interval_seconds],
        )?;

        Ok(())
    }

    /// Updates an existing channel.
    ///
    /// Validates the channel before update. Returns
    /// [`NotificationConfigError::NotFound`] if the channel does not exist.
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
             SET label = ?1, channel_type = ?2, enabled = ?3, config_json = ?4, events_json = ?5,
                 min_interval_seconds = ?6,
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?7",
            params![
                ch.label,
                ch.channel_type,
                ch.enabled,
                config_str,
                events_str,
                ch.min_interval_seconds,
                id
            ],
        )?;

        Ok(())
    }

    /// Deletes a channel by identifier.
    ///
    /// Returns [`NotificationConfigError::NotFound`] if the channel does not exist.
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

    /// Fetches a channel by identifier.
    ///
    /// Returns `None` if the channel does not exist.
    pub fn get_channel(
        &self,
        id: &str,
    ) -> Result<Option<NotificationChannelRow>, NotificationConfigError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, label, channel_type, enabled, config_json, events_json,
                    COALESCE(min_interval_seconds, 0), created_at, updated_at
             FROM notification_channels WHERE id = ?1",
        )?;

        let mut rows = stmt.query_map(params![id], row_to_channel)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Lists all registered channels.
    pub fn list_channels(&self) -> Result<Vec<NotificationChannelRow>, NotificationConfigError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, label, channel_type, enabled, config_json, events_json,
                    COALESCE(min_interval_seconds, 0), created_at, updated_at
             FROM notification_channels ORDER BY id",
        )?;

        let rows = stmt.query_map([], row_to_channel)?;
        let mut channels = Vec::new();
        for row in rows {
            channels.push(row?);
        }
        Ok(channels)
    }

    // --- Global events ---------------------------------------------------

    /// Replaces the entire set of global events (delete + re-insert).
    ///
    /// Validates each event name against [`crate::validation::KNOWN_EVENTS`].
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

    /// Fetches the list of global events.
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

    // --- Logs ------------------------------------------------------------

    /// Writes a notification log.
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

    /// Fetches the latest notification logs, sorted by descending delivery date.
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

// --- Conversion Row -> ChannelConfig -----------------------------------------

impl NotificationChannelRow {
    /// Converts a [`NotificationChannelRow`] into a [`crate::config::ChannelConfig`].
    ///
    /// Used by Supervisor boot to rebuild the notification configuration from
    /// SQLite.
    pub fn to_channel_config(&self) -> crate::config::ChannelConfig {
        let kind = match self.channel_type.as_str() {
            "webhook" => crate::config::ChannelKind::Webhook,
            "terminal" => crate::config::ChannelKind::Terminal,
            _ => crate::config::ChannelKind::Desktop,
        };
        let url = self
            .config_json
            .get("url")
            .and_then(|v| v.as_str())
            .map(String::from);
        let signing_secret = self
            .config_json
            .get("signing_secret")
            .and_then(|v| v.as_str())
            .map(String::from);
        let min_severity = self
            .config_json
            .get("min_severity")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_value(serde_json::Value::String(s.to_string())).ok());
        crate::config::ChannelConfig {
            id: self.id.clone(),
            kind,
            enabled: self.enabled,
            events: self.events_json.clone(),
            url,
            signing_secret,
            min_severity,
            min_interval_seconds: self.min_interval_seconds,
        }
    }
}

/// Converts a SQLite row into a [`NotificationChannelRow`].
///
/// Expected order: `id, label, channel_type, enabled, config_json, events_json,
/// min_interval_seconds, created_at, updated_at`.
fn row_to_channel(row: &rusqlite::Row<'_>) -> rusqlite::Result<NotificationChannelRow> {
    let config_str: String = row.get(4)?;
    let events_str: Option<String> = row.get(5)?;

    let config_json: serde_json::Value =
        serde_json::from_str(&config_str).unwrap_or(serde_json::Value::Object(Default::default()));

    let events_json: Option<Vec<String>> = events_str.and_then(|s| serde_json::from_str(&s).ok());

    let min_interval_seconds: u32 = row.get::<_, i64>(6)?.try_into().unwrap_or(0);

    Ok(NotificationChannelRow {
        id: row.get(0)?,
        label: row.get(1)?,
        channel_type: row.get(2)?,
        enabled: row.get(3)?,
        config_json,
        events_json,
        min_interval_seconds,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

/// Applies the incremental migrations (column additions).
///
/// SQLite does not support `ALTER TABLE ADD COLUMN IF NOT EXISTS`; we read
/// `PRAGMA table_info` and only attempt the `ALTER` if the column is missing.
/// Idempotent: safe to call on every open.
fn ensure_columns(conn: &Connection) -> Result<(), NotificationConfigError> {
    let mut stmt = conn.prepare("PRAGMA table_info(notification_channels)")?;
    let existing: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<Result<_, _>>()?;
    drop(stmt);

    if !existing.iter().any(|c| c == "label") {
        conn.execute(
            "ALTER TABLE notification_channels ADD COLUMN label TEXT",
            [],
        )?;
    }
    if !existing.iter().any(|c| c == "min_interval_seconds") {
        conn.execute(
            "ALTER TABLE notification_channels ADD COLUMN min_interval_seconds INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    Ok(())
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
            label: None,
            channel_type: channel_type.into(),
            enabled: true,
            config_json: if channel_type == "webhook" {
                serde_json::json!({"url": "https://hooks.slack.com/test"})
            } else {
                serde_json::json!({})
            },
            events_json: None,
            min_interval_seconds: 0,
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

    // Creation of notifications.db with 3 tables
    #[test]
    fn test_open_creates_tables() {
        // GIVEN a path to a new database
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("notifications.db");

        // WHEN
        let repo = NotificationConfigRepository::open(&path).expect("open");

        // THEN the 3 tables exist
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

        // AND the index exists
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

    // Insert, get, list, update, delete channels
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

    // Update channel
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

    // Delete channel
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

    // Duplicate channel ID
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

    // Global events CRUD
    #[test]
    fn test_global_events_crud() {
        // GIVEN
        let (_path, repo) = temp_db();

        // WHEN set initial events
        repo.set_global_events(&["task.completed".into(), "task.failed".into()])
            .expect("set");

        // THEN
        let events = repo.get_global_events().expect("get");
        assert_eq!(events, vec!["task.completed", "task.failed"]);

        // WHEN replace events
        repo.set_global_events(&["task.completed".into()])
            .expect("set");

        // THEN complete replacement
        let events = repo.get_global_events().expect("get");
        assert_eq!(events, vec!["task.completed"]);
    }

    // Validation webhook without URL
    #[test]
    fn test_validation_webhook_no_url() {
        // GIVEN
        let (_path, repo) = temp_db();
        let ch = NotificationChannelRow {
            id: "bad-webhook".into(),
            label: None,
            channel_type: "webhook".into(),
            enabled: true,
            config_json: serde_json::json!({}),
            events_json: None,
            min_interval_seconds: 0,
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

    // Validation of an unknown event name
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

    // Write log + query logs
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

    // Empty DB: list_channels returns empty
    #[test]
    fn test_list_channels_empty() {
        // GIVEN
        let (_path, repo) = temp_db();

        // WHEN
        let channels = repo.list_channels().expect("list");

        // THEN
        assert!(channels.is_empty());
    }

    // Roundtrip of a channel with a human label
    #[test]
    fn test_insert_and_get_channel_with_label() {
        // GIVEN a channel with a free-form label (spaces, accents)
        let (_path, repo) = temp_db();
        let mut ch = make_channel("alertes-slack-equipe", "webhook");
        ch.label = Some("Alertes Slack équipe".into());

        // WHEN insert then read
        repo.insert_channel(&ch).expect("insert");
        let found = repo
            .get_channel("alertes-slack-equipe")
            .expect("get")
            .expect("Some");

        // THEN the label is preserved as-is
        assert_eq!(found.label.as_deref(), Some("Alertes Slack équipe"));
        assert_eq!(found.id, "alertes-slack-equipe");
    }

    // Update the label alone, without touching the rest
    #[test]
    fn test_update_channel_label() {
        // GIVEN a channel without a label
        let (_path, repo) = temp_db();
        let ch = make_channel("desktop", "desktop");
        repo.insert_channel(&ch).expect("insert");

        // WHEN a label is added
        let mut updated = ch.clone();
        updated.label = Some("Bureau de Nidal".into());
        repo.update_channel("desktop", &updated).expect("update");

        // THEN the label is persisted
        let result = repo.get_channel("desktop").expect("get").expect("Some");
        assert_eq!(result.label.as_deref(), Some("Bureau de Nidal"));
    }

    // Idempotent migration: open() on a v1 database adds the label column
    // without breaking the existing channels.
    #[test]
    fn test_migration_adds_label_column_idempotent() {
        // GIVEN a "v1" database without the label column, containing a channel
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("notifications.db");
        {
            let conn = rusqlite::Connection::open(&path).expect("open");
            conn.execute_batch(
                "CREATE TABLE notification_channels (
                    id              TEXT PRIMARY KEY,
                    channel_type    TEXT NOT NULL,
                    enabled         BOOLEAN NOT NULL DEFAULT 1,
                    config_json     TEXT NOT NULL DEFAULT '{}',
                    events_json     TEXT,
                    created_at      TEXT NOT NULL DEFAULT '2024-01-01T00:00:00Z',
                    updated_at      TEXT NOT NULL DEFAULT '2024-01-01T00:00:00Z',
                    CHECK (channel_type IN ('desktop', 'webhook'))
                );
                INSERT INTO notification_channels (id, channel_type, enabled, config_json)
                VALUES ('legacy', 'desktop', 1, '{}');
                CREATE TABLE notification_global_events (event_name TEXT PRIMARY KEY);
                CREATE TABLE notification_logs (
                    id TEXT PRIMARY KEY,
                    event_name TEXT NOT NULL,
                    task_id TEXT,
                    agent_id TEXT,
                    sent_at TEXT NOT NULL DEFAULT '2024-01-01T00:00:00Z',
                    channels TEXT NOT NULL DEFAULT '{}',
                    error TEXT
                );",
            )
            .expect("seed v1");
        }
        std::mem::forget(dir);

        // WHEN opening via NotificationConfigRepository (which calls ensure_columns)
        let repo = NotificationConfigRepository::open(&path).expect("open v2");

        // THEN the existing channel is still there, with label = None
        let legacy = repo.get_channel("legacy").expect("get").expect("Some");
        assert_eq!(legacy.label, None);
        assert_eq!(legacy.channel_type, "desktop");

        // AND reopening a second time does not crash (idempotent)
        drop(repo);
        let _repo2 = NotificationConfigRepository::open(&path).expect("open again");
    }
}
