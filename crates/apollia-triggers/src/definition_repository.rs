//! SQLite CRUD repository for trigger definitions.
//!
//! This module provides [`TriggerDefinitionRepository`], a synchronous
//! persistence interface for trigger definitions in `triggers.db`. Business
//! validation is delegated to the [`crate::validation`] module and runs
//! automatically before each write (insert/update).
//!
//! This repository is used by Supervisor boot and the REST CRUD routes.

use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::types::{
    FileEventKind, InputTemplate, OnBusyPolicy, TriggerDefinition, TriggerSourceConfig,
};
use crate::validation;

// --- Migration ---------------------------------------------------------------

/// Idempotent migration for the `trigger_definitions` table.
const MIGRATION_008: &str = "\
CREATE TABLE IF NOT EXISTS trigger_definitions (
    id              TEXT PRIMARY KEY,
    agent           TEXT NOT NULL,
    enabled         BOOLEAN NOT NULL DEFAULT 1,
    on_busy         TEXT NOT NULL DEFAULT 'queue',
    source_type     TEXT NOT NULL,
    source_config   TEXT NOT NULL,
    input_template  TEXT,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (on_busy IN ('queue', 'drop')),
    CHECK (source_type IN ('cron', 'interval', 'oneshot', 'file_watch', 'webhook'))
);";

// --- Types -------------------------------------------------------------------

/// Persisted trigger definition (flat SQLite representation).
///
/// Unlike [`crate::types::TriggerDefinition`], which uses rich types
/// (`TriggerSourceConfig`, `OnBusyPolicy`), this struct uses flat types
/// (`source_type` + `source_config` JSON) suited to SQLite storage and REST
/// serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerDefinitionRow {
    /// Unique trigger identifier (primary key).
    pub id: String,
    /// Target agent.
    pub agent: Option<String>,
    /// Whether the trigger is active.
    pub enabled: bool,
    /// Behavior when the target agent is busy.
    pub on_busy: OnBusy,
    /// Source type: `"cron"`, `"interval"`, `"oneshot"`, `"file_watch"`, `"webhook"`.
    pub source_type: String,
    /// Source JSON configuration (structure depends on `source_type`).
    pub source_config: serde_json::Value,
    /// Input message template (`{{variables}}` substitution).
    pub input_template: Option<String>,
    /// Creation timestamp (ISO 8601, filled automatically).
    pub created_at: String,
    /// Last-modified timestamp (ISO 8601, refreshed on each update).
    pub updated_at: String,
}

/// Behavior when the target agent is busy at fire time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OnBusy {
    /// Submit anyway; `TaskRouter` handles the queue.
    Queue,
    /// Drop the fire and record "skipped" in the audit log.
    Drop,
}

impl OnBusy {
    /// Returns the SQLite representation of the on_busy behavior.
    fn as_sql(&self) -> &'static str {
        match self {
            OnBusy::Queue => "queue",
            OnBusy::Drop => "drop",
        }
    }

    /// Parses a SQLite value into [`OnBusy`], defaulting to `Queue` if unrecognized.
    fn from_sql(s: &str) -> Self {
        match s {
            "drop" => OnBusy::Drop,
            _ => OnBusy::Queue,
        }
    }
}

// --- Conversion Row -> TriggerDefinition -------------------------------------

impl TryFrom<TriggerDefinitionRow> for TriggerDefinition {
    type Error = TriggerDefinitionError;

    /// Converts a persisted [`TriggerDefinitionRow`] into a rich [`TriggerDefinition`].
    ///
    /// Parses `source_type` + `source_config` JSON into a typed [`TriggerSourceConfig`].
    /// Returns [`TriggerDefinitionError::ValidationError`] if the conversion fails.
    fn try_from(row: TriggerDefinitionRow) -> Result<Self, Self::Error> {
        let source = parse_source_config(&row.source_type, &row.source_config)?;
        let on_busy = match row.on_busy {
            OnBusy::Queue => OnBusyPolicy::Queue {
                max_depth: crate::DEFAULT_QUEUE_MAX_DEPTH,
            },
            OnBusy::Drop => OnBusyPolicy::Skip,
        };
        Ok(TriggerDefinition {
            id: row.id,
            agent: row.agent.unwrap_or_default(),
            enabled: row.enabled,
            on_busy,
            source,
            input_template: InputTemplate(row.input_template.unwrap_or_default()),
        })
    }
}

/// Parses `source_type` and `source_config` JSON into a [`TriggerSourceConfig`].
fn parse_source_config(
    source_type: &str,
    source_config: &serde_json::Value,
) -> Result<TriggerSourceConfig, TriggerDefinitionError> {
    match source_type {
        "cron" => {
            let schedule = source_config
                .get("schedule")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            Ok(TriggerSourceConfig::Cron { schedule })
        }
        "interval" => {
            let every = source_config
                .get("every")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            Ok(TriggerSourceConfig::Interval { every })
        }
        "oneshot" => {
            let fire_at_str = source_config
                .get("fire_at")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    TriggerDefinitionError::ValidationError(
                        "oneshot source requires 'fire_at' field".into(),
                    )
                })?;
            let fire_at = fire_at_str.parse().map_err(|e| {
                TriggerDefinitionError::ValidationError(format!("invalid fire_at datetime: {e}"))
            })?;
            Ok(TriggerSourceConfig::Oneshot { fire_at })
        }
        "file_watch" => {
            let path = source_config
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let events: Vec<FileEventKind> = source_config
                .get("events")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_else(|| vec![FileEventKind::Any]);
            let recursive = source_config
                .get("recursive")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let follow_symlinks = source_config
                .get("follow_symlinks")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let exclude_patterns = source_config
                .get("exclude_patterns")
                .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
                .unwrap_or_else(crate::config::default_exclude_patterns);
            Ok(TriggerSourceConfig::FileWatch {
                path: std::path::PathBuf::from(path),
                events,
                recursive,
                follow_symlinks,
                exclude_patterns,
            })
        }
        "webhook" => {
            let secret = source_config
                .get("secret")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            // An empty secret is refused here, as it already is on the TOML path.
            // The reception route is exempt from bearer authentication precisely
            // because it authenticates itself, by HMAC under this secret. With an
            // empty one the HMAC is computable by anyone who knows the trigger id,
            // so accepting it would turn `POST /webhooks/:id` into an open route
            // that starts agents. `unwrap_or_default` used to make an absent
            // secret indistinguishable from an empty one, and both passed.
            if secret.is_empty() {
                return Err(TriggerDefinitionError::ValidationError(
                    crate::types::TriggerDefinitionError::EmptyWebhookSecret.to_string(),
                ));
            }
            Ok(TriggerSourceConfig::Webhook { secret })
        }
        other => Err(TriggerDefinitionError::ValidationError(format!(
            "unknown source_type: {other}"
        ))),
    }
}

// --- Errors ------------------------------------------------------------------

/// Errors from the trigger definition repository.
#[derive(Debug, thiserror::Error)]
pub enum TriggerDefinitionError {
    /// The requested trigger does not exist.
    #[error("trigger not found: {0}")]
    NotFound(String),
    /// A trigger with this identifier already exists.
    #[error("duplicate trigger id: {0}")]
    DuplicateId(String),
    /// The definition does not satisfy the business validation rules.
    #[error("validation error: {0}")]
    ValidationError(String),
    /// Underlying SQLite error.
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
}

// --- Repository --------------------------------------------------------------

/// CRUD repository for trigger definitions in SQLite.
///
/// Synchronous struct (no Tokio actor). The SQLite connection is `Send`,
/// compatible with `spawn_blocking` if needed.
pub struct TriggerDefinitionRepository {
    conn: Connection,
}

impl TriggerDefinitionRepository {
    /// Opens (or creates) the SQLite database and applies the idempotent migration.
    ///
    /// Enables WAL for better concurrent write performance. Creates the
    /// `trigger_definitions` table if it does not exist (idempotent).
    pub fn open(path: &Path) -> Result<Self, TriggerDefinitionError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(MIGRATION_008)?;
        Ok(Self { conn })
    }

    /// Inserts a new trigger definition after validation.
    ///
    /// The `created_at` and `updated_at` fields are filled automatically by the
    /// SQLite DEFAULTs. Returns [`TriggerDefinitionError::DuplicateId`] if the
    /// identifier already exists.
    pub fn insert(&self, def: &TriggerDefinitionRow) -> Result<(), TriggerDefinitionError> {
        validation::validate_trigger(def)?;

        let source_config =
            validation::normalized_source_config(&def.source_type, &def.source_config)?;
        let source_config_json = serde_json::to_string(&source_config).map_err(|e| {
            TriggerDefinitionError::ValidationError(format!("invalid source_config JSON: {e}"))
        })?;

        self.conn
            .execute(
                "INSERT INTO trigger_definitions \
                 (id, agent, enabled, on_busy, source_type, source_config, input_template) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    def.id,
                    def.agent,
                    def.enabled,
                    def.on_busy.as_sql(),
                    def.source_type,
                    source_config_json,
                    def.input_template,
                ],
            )
            .map_err(|e| {
                if let rusqlite::Error::SqliteFailure(err, _) = &e {
                    if err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY {
                        return TriggerDefinitionError::DuplicateId(def.id.clone());
                    }
                }
                TriggerDefinitionError::Database(e)
            })?;

        Ok(())
    }

    /// Updates an existing definition after validation.
    ///
    /// Refreshes `updated_at` automatically. Returns
    /// [`TriggerDefinitionError::NotFound`] if the identifier does not exist.
    pub fn update(
        &self,
        id: &str,
        def: &TriggerDefinitionRow,
    ) -> Result<(), TriggerDefinitionError> {
        validation::validate_trigger(def)?;

        let source_config =
            validation::normalized_source_config(&def.source_type, &def.source_config)?;
        let source_config_json = serde_json::to_string(&source_config).map_err(|e| {
            TriggerDefinitionError::ValidationError(format!("invalid source_config JSON: {e}"))
        })?;

        let rows = self.conn.execute(
            "UPDATE trigger_definitions \
             SET agent = ?1, enabled = ?2, on_busy = ?3, \
                 source_type = ?4, source_config = ?5, input_template = ?6, \
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
             WHERE id = ?7",
            params![
                def.agent,
                def.enabled,
                def.on_busy.as_sql(),
                def.source_type,
                source_config_json,
                def.input_template,
                id,
            ],
        )?;

        if rows == 0 {
            return Err(TriggerDefinitionError::NotFound(id.to_string()));
        }

        Ok(())
    }

    /// Deletes a trigger definition.
    ///
    /// Returns [`TriggerDefinitionError::NotFound`] if the identifier does not exist.
    pub fn delete(&self, id: &str) -> Result<(), TriggerDefinitionError> {
        let rows = self
            .conn
            .execute("DELETE FROM trigger_definitions WHERE id = ?1", params![id])?;

        if rows == 0 {
            return Err(TriggerDefinitionError::NotFound(id.to_string()));
        }

        Ok(())
    }

    /// Returns the definition of a trigger by its identifier.
    ///
    /// Returns `None` if no trigger matches.
    pub fn get(&self, id: &str) -> Result<Option<TriggerDefinitionRow>, TriggerDefinitionError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, agent, enabled, on_busy, source_type, \
                    source_config, input_template, created_at, updated_at \
             FROM trigger_definitions WHERE id = ?1",
        )?;

        let mut rows = stmt.query_map(params![id], row_to_definition)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Lists all trigger definitions, sorted by identifier.
    pub fn list(&self) -> Result<Vec<TriggerDefinitionRow>, TriggerDefinitionError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, agent, enabled, on_busy, source_type, \
                    source_config, input_template, created_at, updated_at \
             FROM trigger_definitions ORDER BY id",
        )?;

        let rows = stmt.query_map([], row_to_definition)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }
}

/// Converts a SQLite row into a [`TriggerDefinitionRow`].
///
/// The stored `source_config` is handed back in the form the runtime readers
/// accept verbatim, through [`validation::normalized_source_config`]. The write
/// path normalizes a cron schedule before storing it, but nothing rewrites the
/// rows persisted before it did: `MIGRATION_008` creates the table and no
/// statement updates `source_config` outside [`TriggerDefinitionRepository::update`].
/// Such a row keeps its 5-field expression, which `sources/cron.rs` refuses,
/// so the trigger stays listed, with a readable schedule, and never fires. The
/// repair therefore belongs to the read path as well.
///
/// An expression no normalization can rescue is returned unchanged: refusing it
/// here would fail the whole listing over a single row, where today it costs one
/// silent trigger.
fn row_to_definition(row: &rusqlite::Row) -> rusqlite::Result<TriggerDefinitionRow> {
    let on_busy_str: String = row.get(3)?;
    let source_type: String = row.get(4)?;
    let source_config_str: String = row.get(5)?;

    let stored_config: serde_json::Value =
        serde_json::from_str(&source_config_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
        })?;

    let source_config =
        validation::normalized_source_config(&source_type, &stored_config).unwrap_or(stored_config);

    Ok(TriggerDefinitionRow {
        id: row.get(0)?,
        agent: row.get(1)?,
        enabled: row.get(2)?,
        on_busy: OnBusy::from_sql(&on_busy_str),
        source_type,
        source_config,
        input_template: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

// --- Tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use tempfile::TempDir;

    /// Writes a row straight into SQLite, bypassing [`TriggerDefinitionRepository::insert`]
    /// and its normalization, the way a build older than that normalization left it.
    fn insert_raw_row(repo: &TriggerDefinitionRepository, id: &str, source_config: &str) {
        repo.conn
            .execute(
                "INSERT INTO trigger_definitions \
                 (id, agent, enabled, on_busy, source_type, source_config, input_template) \
                 VALUES (?1, 'rapport-agent', 1, 'queue', 'cron', ?2, NULL)",
                params![id, source_config],
            )
            .expect("direct insert bypassing the repository");
    }

    /// Opens a test repository in a temporary directory.
    fn open_test_repo() -> (TempDir, TriggerDefinitionRepository) {
        let dir = TempDir::new().expect("tempdir creation");
        let repo =
            TriggerDefinitionRepository::open(&dir.path().join("triggers.db")).expect("repo open");
        (dir, repo)
    }

    /// Creates a valid cron definition for tests.
    fn make_cron_def(id: &str, agent: &str, schedule: &str) -> TriggerDefinitionRow {
        TriggerDefinitionRow {
            id: id.to_string(),
            agent: Some(agent.to_string()),
            enabled: true,
            on_busy: OnBusy::Queue,
            source_type: "cron".to_string(),
            source_config: serde_json::json!({ "schedule": schedule }),
            input_template: Some("Rapport du {{scheduled_at}}".to_string()),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    // --- Insert + Get ----------------------------------------------------

    #[test]
    fn test_insert_and_get() {
        // GIVEN an open repository
        let (_dir, repo) = open_test_repo();
        let def = make_cron_def("rapport-hebdo", "rapport-agent", "0 0 8 * * MON *");

        // WHEN insert then get
        repo.insert(&def).expect("insert");
        let got = repo.get("rapport-hebdo").expect("get");

        // THEN the definition is found with the same fields
        let got = got.expect("should exist");
        assert_eq!(got.id, "rapport-hebdo");
        assert_eq!(got.agent.as_deref(), Some("rapport-agent"));
        assert!(got.enabled);
        assert_eq!(got.on_busy, OnBusy::Queue);
        assert_eq!(got.source_type, "cron");
        assert_eq!(
            got.source_config.get("schedule").and_then(|v| v.as_str()),
            Some("0 0 8 * * MON *")
        );
        assert!(!got.created_at.is_empty(), "created_at doit être renseigné");
        assert!(!got.updated_at.is_empty(), "updated_at doit être renseigné");
    }

    // --- Insert duplicate ID ---------------------------------------------

    #[test]
    fn test_insert_duplicate_id() {
        // GIVEN a repository containing "rapport-hebdo"
        let (_dir, repo) = open_test_repo();
        let def = make_cron_def("rapport-hebdo", "agent", "0 0 8 * * MON *");
        repo.insert(&def).expect("first insert");

        // WHEN insert with the same ID
        let result = repo.insert(&def);

        // THEN DuplicateId error
        assert!(
            matches!(result, Err(TriggerDefinitionError::DuplicateId(ref id)) if id == "rapport-hebdo"),
            "expected DuplicateId, got: {result:?}"
        );
    }

    // --- Update existing -------------------------------------------------

    #[test]
    fn test_update_existing() {
        // GIVEN a trigger with schedule "0 0 8 * * MON *"
        let (_dir, repo) = open_test_repo();
        let def = make_cron_def("rapport-hebdo", "agent", "0 0 8 * * MON *");
        repo.insert(&def).expect("insert");

        let original = repo.get("rapport-hebdo").expect("get").expect("exists");
        let original_updated_at = original.updated_at.clone();

        // WHEN update with schedule "0 0 9 * * MON *"
        // (small delay so updated_at changes; SQLite subsecond precision)
        std::thread::sleep(std::time::Duration::from_millis(10));
        let updated_def = make_cron_def("rapport-hebdo", "agent", "0 0 9 * * MON *");
        repo.update("rapport-hebdo", &updated_def).expect("update");

        // THEN source_config is updated and updated_at refreshed
        let got = repo.get("rapport-hebdo").expect("get").expect("exists");
        assert_eq!(
            got.source_config.get("schedule").and_then(|v| v.as_str()),
            Some("0 0 9 * * MON *")
        );
        assert!(
            got.updated_at >= original_updated_at,
            "updated_at doit être rafraîchi"
        );
    }

    // --- Update non-existent ID ------------------------------------------

    #[test]
    fn test_update_not_found() {
        // GIVEN an empty repository
        let (_dir, repo) = open_test_repo();
        let def = make_cron_def("inconnu", "agent", "0 0 8 * * MON *");

        // WHEN update("inconnu")
        let result = repo.update("inconnu", &def);

        // THEN NotFound error
        assert!(
            matches!(result, Err(TriggerDefinitionError::NotFound(ref id)) if id == "inconnu"),
            "expected NotFound, got: {result:?}"
        );
    }

    // --- Delete + Get + List ---------------------------------------------

    #[test]
    fn test_delete_and_list() {
        // GIVEN a repository containing 3 triggers
        let (_dir, repo) = open_test_repo();
        for i in 1..=3 {
            let def = make_cron_def(
                &format!("trigger-{i}"),
                &format!("agent-{i}"),
                "0 0 8 * * MON *",
            );
            repo.insert(&def).expect("insert");
        }
        assert_eq!(repo.list().expect("list").len(), 3);

        // WHEN delete("trigger-2")
        repo.delete("trigger-2").expect("delete");

        // THEN list() returns 2, get("trigger-2") returns None
        let all = repo.list().expect("list");
        assert_eq!(all.len(), 2);
        assert!(repo.get("trigger-2").expect("get").is_none());

        // AND delete("trigger-2") again returns NotFound
        let result = repo.delete("trigger-2");
        assert!(
            matches!(result, Err(TriggerDefinitionError::NotFound(ref id)) if id == "trigger-2"),
            "expected NotFound on double delete, got: {result:?}"
        );
    }

    // --- Desktop write path: 5-field cron persisted in the reader's form --

    #[test]
    fn test_insert_five_field_cron_persists_schedule_the_reader_accepts() {
        use std::str::FromStr;

        // GIVEN a 5-field scheduler-preset expression, as the desktop sends it
        let (_dir, repo) = open_test_repo();
        let def = make_cron_def("bureau-15m", "agent", "*/15 * * * *");

        // WHEN insert then get
        repo.insert(&def).expect("insert");
        let got = repo.get("bureau-15m").expect("get").expect("exists");

        // THEN the stored schedule parses verbatim with the runtime reader's parser
        let stored = got
            .source_config
            .get("schedule")
            .and_then(|v| v.as_str())
            .expect("schedule present");
        assert_eq!(stored, "0 */15 * * * *");
        assert!(
            cron::Schedule::from_str(stored).is_ok(),
            "stored schedule must be accepted verbatim by Schedule::from_str: {stored:?}"
        );
    }

    #[test]
    fn test_update_five_field_cron_persists_schedule_the_reader_accepts() {
        use std::str::FromStr;

        // GIVEN an existing trigger with a directly parseable schedule
        let (_dir, repo) = open_test_repo();
        let def = make_cron_def("bureau-daily", "agent", "0 0 8 * * MON *");
        repo.insert(&def).expect("insert");

        // WHEN update with a 5-field expression, as the desktop sends it
        let updated_def = make_cron_def("bureau-daily", "agent", "30 8 * * *");
        repo.update("bureau-daily", &updated_def).expect("update");

        // THEN the stored schedule parses verbatim with the runtime reader's parser
        let got = repo.get("bureau-daily").expect("get").expect("exists");
        let stored = got
            .source_config
            .get("schedule")
            .and_then(|v| v.as_str())
            .expect("schedule present");
        assert_eq!(stored, "0 30 8 * * *");
        assert!(
            cron::Schedule::from_str(stored).is_ok(),
            "stored schedule must be accepted verbatim by Schedule::from_str: {stored:?}"
        );
    }

    // --- Invalid cron validation -----------------------------------------

    #[test]
    fn test_validation_invalid_cron() {
        // GIVEN an open repository
        let (_dir, repo) = open_test_repo();
        let def = make_cron_def("bad-cron", "agent", "not-a-cron");

        // WHEN insert with an invalid cron
        let result = repo.insert(&def);

        // THEN ValidationError containing "invalid cron expression"
        assert!(
            matches!(result, Err(TriggerDefinitionError::ValidationError(ref msg)) if msg.contains("invalid cron expression")),
            "expected ValidationError with cron message, got: {result:?}"
        );
    }

    // --- Validation: agent is mandatory ----------------------------------
    //
    // A trigger once could target either an agent or a pipeline (XOR
    // validation). Pipelines were absorbed into A2A agents, and the
    // `pipeline` field was removed from `TriggerDefinitionRow`. Only `agent`
    // remains, and it is mandatory.

    #[test]
    fn test_validation_agent_required() {
        let (_dir, repo) = open_test_repo();

        // GIVEN agent=None
        let def_no_agent = TriggerDefinitionRow {
            id: "test-no-agent".to_string(),
            agent: None,
            enabled: true,
            on_busy: OnBusy::Queue,
            source_type: "cron".to_string(),
            source_config: serde_json::json!({ "schedule": "0 0 8 * * MON *" }),
            input_template: None,
            created_at: String::new(),
            updated_at: String::new(),
        };

        // WHEN insert, THEN error "agent must be set"
        let result = repo.insert(&def_no_agent);
        assert!(
            matches!(result, Err(TriggerDefinitionError::ValidationError(ref msg)) if msg.contains("agent must be set")),
            "expected 'agent must be set', got: {result:?}"
        );

        // GIVEN agent=Some("") (empty string); same rejection expected.
        let def_empty_agent = TriggerDefinitionRow {
            id: "test-empty-agent".to_string(),
            agent: Some(String::new()),
            enabled: true,
            on_busy: OnBusy::Queue,
            source_type: "cron".to_string(),
            source_config: serde_json::json!({ "schedule": "0 0 8 * * MON *" }),
            input_template: None,
            created_at: String::new(),
            updated_at: String::new(),
        };

        let result = repo.insert(&def_empty_agent);
        assert!(
            matches!(result, Err(TriggerDefinitionError::ValidationError(ref msg)) if msg.contains("agent must be set")),
            "expected 'agent must be set' for empty agent, got: {result:?}"
        );
    }

    // --- Validation webhook secret < 32 chars ----------------------------

    #[test]
    fn test_validation_webhook_short_secret() {
        let (_dir, repo) = open_test_repo();
        let def = TriggerDefinitionRow {
            id: "webhook-short".to_string(),
            agent: Some("agent".to_string()),
            enabled: true,
            on_busy: OnBusy::Queue,
            source_type: "webhook".to_string(),
            source_config: serde_json::json!({ "secret": "short" }),
            input_template: None,
            created_at: String::new(),
            updated_at: String::new(),
        };

        // WHEN insert, THEN error "webhook secret must be at least 32 characters"
        let result = repo.insert(&def);
        assert!(
            matches!(result, Err(TriggerDefinitionError::ValidationError(ref msg)) if msg.contains("webhook secret must be at least 32 characters")),
            "expected webhook secret validation error, got: {result:?}"
        );
    }

    // --- Extra: empty list -----------------------------------------------

    #[test]
    fn test_list_empty() {
        // GIVEN an empty repository
        let (_dir, repo) = open_test_repo();

        // WHEN list
        let all = repo.list().expect("list");

        // THEN empty Vec
        assert!(all.is_empty());
    }

    // --- Extra: idempotent open ------------------------------------------

    #[test]
    fn test_open_idempotent() {
        // GIVEN an already-migrated database
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("triggers.db");
        {
            let _repo = TriggerDefinitionRepository::open(&path).expect("first open");
        }

        // WHEN opening a second time
        let result = TriggerDefinitionRepository::open(&path);

        // THEN no error
        assert!(result.is_ok(), "second open should succeed");
    }

    // --- Extra: webhook with a valid secret (>= 32 chars) ----------------

    #[test]
    fn test_webhook_valid_secret() {
        let (_dir, repo) = open_test_repo();
        let secret = "a".repeat(32);
        let def = TriggerDefinitionRow {
            id: "webhook-valid".to_string(),
            agent: Some("agent".to_string()),
            enabled: true,
            on_busy: OnBusy::Queue,
            source_type: "webhook".to_string(),
            source_config: serde_json::json!({ "secret": secret }),
            input_template: None,
            created_at: String::new(),
            updated_at: String::new(),
        };

        repo.insert(&def).expect("insert valid webhook");
        let got = repo.get("webhook-valid").expect("get").expect("exists");
        assert_eq!(got.source_type, "webhook");
    }

    // --- Read path: a schedule persisted before the write path normalized it ---

    #[test]
    fn test_get_returns_a_five_field_schedule_the_cron_reader_accepts() {
        // GIVEN a cron row persisted with a 5-field schedule, written without insert()
        let (_dir, repo) = open_test_repo();
        insert_raw_row(&repo, "legacy-cron", r#"{"schedule":"*/15 * * * *"}"#);

        // WHEN the read path returns it
        let got = repo.get("legacy-cron").expect("get").expect("should exist");
        let schedule = got
            .source_config
            .get("schedule")
            .and_then(|v| v.as_str())
            .expect("schedule field");

        // THEN the expression is the one `sources/cron.rs` parses verbatim
        assert_eq!(schedule, "0 */15 * * * *");
        assert!(
            cron::Schedule::from_str(schedule).is_ok(),
            "the runtime cron reader must accept the schedule the read path returns: {schedule}"
        );
    }

    #[test]
    fn test_list_feeds_the_boot_path_a_firing_cron_definition() {
        // GIVEN the same row, read through the listing the supervisor boots on
        let (_dir, repo) = open_test_repo();
        insert_raw_row(&repo, "legacy-cron", r#"{"schedule":"*/15 * * * *"}"#);

        // WHEN the row is converted the way `load_trigger_definitions` converts it
        let rows = repo.list().expect("list");
        let row = rows.into_iter().next().expect("one row");
        let def = TriggerDefinition::try_from(row).expect("conversion");

        // THEN the source carries a schedule the cron trigger can spawn on
        let schedule = match def.source {
            TriggerSourceConfig::Cron { schedule } => Some(schedule),
            _ => None,
        }
        .expect("the stored row is a cron source");
        assert!(
            cron::Schedule::from_str(&schedule).is_ok(),
            "CronTrigger::spawn returns without firing when this fails: {schedule}"
        );
    }

    #[test]
    fn test_list_survives_a_schedule_no_normalization_can_rescue() {
        // GIVEN a stored schedule that is neither 5-field nor parsable
        let (_dir, repo) = open_test_repo();
        insert_raw_row(&repo, "broken-cron", r#"{"schedule":"every tuesday"}"#);

        // WHEN the listing runs
        let rows = repo
            .list()
            .expect("list must not fail over one unreadable row");

        // THEN the row is returned unchanged, and the listing still holds it
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0]
                .source_config
                .get("schedule")
                .and_then(|v| v.as_str()),
            Some("every tuesday")
        );
    }
}
