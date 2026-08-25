//! Versioned schema of `system.db`, shared by every store in the file.
//!
//! `system.db` holds two singleton stores: the LLM backend registry
//! ([`crate::llm_backend`]) and the STT configuration ([`crate::stt_config`]).
//! `PRAGMA user_version` belongs to the database file, not to a table, so the
//! stores share one migration list: whichever opens first brings the whole
//! file to the current version, and both refuse a file written by a newer
//! binary.

use rusqlite::Connection;

use crate::schema::{add_column_if_missing, Migration};

/// Current schema version of `system.db`.
pub(crate) const SYSTEM_DB_SCHEMA_VERSION: u32 = 1;

/// Numbered migration steps of `system.db`.
///
/// Step `k` migrates the file from version `k` to `k + 1`; the list length
/// always equals [`SYSTEM_DB_SCHEMA_VERSION`].
pub(crate) const SYSTEM_DB_MIGRATIONS: &[Migration] = &[migrate_v1];

/// v1: the pre-versioning lineage of the file, replayed idempotently.
///
/// Every `system.db` written before the versioned layer is at
/// `user_version = 0` whatever columns it carries, so this step must accept
/// a fresh file, the initial schema, and the schema after the additive
/// `input_device` column, and bring each of them to the same state.
fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS llm_backends (
            name         TEXT PRIMARY KEY,
            provider     TEXT NOT NULL,
            model        TEXT NOT NULL,
            config_json  TEXT NOT NULL DEFAULT '{}',
            enabled      INTEGER NOT NULL DEFAULT 1,
            is_default   INTEGER NOT NULL DEFAULT 0,
            created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            CHECK (provider IN ('llama-cpp', 'openai', 'mistral', 'anthropic', 'ollama'))
        );

        CREATE TABLE IF NOT EXISTS stt_config (
            id                   INTEGER PRIMARY KEY CHECK (id = 1),
            enabled              INTEGER NOT NULL DEFAULT 0,
            model_path           TEXT    NOT NULL DEFAULT '',
            hotkey               TEXT    NOT NULL DEFAULT 'ctrl+shift+space',
            clipboard_mode       TEXT    NOT NULL DEFAULT 'paste',
            clipboard_restore    INTEGER NOT NULL DEFAULT 1,
            silence_threshold_db REAL    NOT NULL DEFAULT -40.0,
            max_recording_sec    INTEGER NOT NULL DEFAULT 60,
            language             TEXT,
            trigger_mode         TEXT    NOT NULL DEFAULT 'toggle',
            input_device         TEXT,
            updated_at           TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );",
    )?;
    add_column_if_missing(conn, "ALTER TABLE stt_config ADD COLUMN input_device TEXT")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::DataFile;
    use crate::schema::{open_versioned, SchemaError};

    /// Pre-versioning `system.db` schema, frozen as the oldest shape a
    /// published binary wrote (no `input_device` column, `user_version = 0`).
    const SYSTEM_V0_SQL: &str = include_str!("../tests/fixtures/schemas/system_v0.sql");

    fn open_file_db(dir: &tempfile::TempDir) -> Connection {
        Connection::open(dir.path().join(DataFile::System.file_name())).unwrap()
    }

    // GIVEN a database written by a pre-versioning binary, with rows
    // WHEN opening it through the versioned layer
    // THEN the rows survive, the missing column is added and the version is stamped
    #[test]
    fn test_system_db_old_format_migrates_and_keeps_rows() {
        let dir = tempfile::TempDir::new().unwrap();
        {
            let conn = open_file_db(&dir);
            conn.execute_batch(SYSTEM_V0_SQL).unwrap();
            conn.execute(
                "INSERT INTO llm_backends (name, provider, model, is_default)
                 VALUES ('local', 'llama-cpp', '/m.gguf', 1)",
                [],
            )
            .unwrap();
            conn.execute("INSERT INTO stt_config (id, enabled) VALUES (1, 1)", [])
                .unwrap();
        }

        let conn = open_file_db(&dir);
        open_versioned(
            &conn,
            DataFile::System.file_name(),
            SYSTEM_DB_SCHEMA_VERSION,
            SYSTEM_DB_MIGRATIONS,
        )
        .unwrap();

        let name: String = conn
            .query_row("SELECT name FROM llm_backends", [], |row| row.get(0))
            .unwrap();
        assert_eq!(name, "local");
        let device: Option<String> = conn
            .query_row("SELECT input_device FROM stt_config", [], |row| row.get(0))
            .unwrap();
        assert!(device.is_none());
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, i64::from(SYSTEM_DB_SCHEMA_VERSION));
    }

    // GIVEN a database stamped by a newer binary
    // WHEN opening it through the versioned layer
    // THEN the open is refused
    #[test]
    fn test_system_db_newer_version_is_refused() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", SYSTEM_DB_SCHEMA_VERSION + 1)
            .unwrap();

        let err = open_versioned(
            &conn,
            DataFile::System.file_name(),
            SYSTEM_DB_SCHEMA_VERSION,
            SYSTEM_DB_MIGRATIONS,
        )
        .unwrap_err();

        assert!(matches!(err, SchemaError::NewerThanBinary { .. }));
    }
}
