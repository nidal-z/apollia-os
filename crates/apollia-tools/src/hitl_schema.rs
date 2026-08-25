//! Versioned schema of `hitl.db`, shared by every store in the file.
//!
//! `hitl.db` holds the HITL task tables (`tasks`, `task_approvals`, owned by
//! [`crate::task_repository`]) and the notification delivery log
//! (`notification_logs`, written by the `apollia-notifications` engine and
//! read by the runtime's notification routes). `PRAGMA user_version` belongs
//! to the database file, not to a table, so every opener goes through this
//! one migration list: whichever opens first brings the whole file to the
//! current version, and each refuses a file written by a newer binary.

use rusqlite::Connection;

use apollia_core::schema::{add_column_if_missing, Migration};

/// Current schema version of `hitl.db`.
pub const HITL_SCHEMA_VERSION: u32 = 1;

/// Numbered migration steps of `hitl.db`.
///
/// Step `k` migrates the file from version `k` to `k + 1`; the list length
/// always equals [`HITL_SCHEMA_VERSION`].
pub const HITL_MIGRATIONS: &[Migration] = &[migrate_v1];

/// v1: the pre-versioning lineage of the file, replayed idempotently.
///
/// Every `hitl.db` written before the versioned layer is at
/// `user_version = 0` whatever columns it carries, so this step must accept
/// a fresh file, the initial `005_hitl_tables` shape, and every intermediate
/// one (observability columns, HITL timing columns, notification log), and
/// bring each of them to the same state.
fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(include_str!("../migrations/005_hitl_tables.sql"))?;

    // Observability columns added to `tasks` after 005 shipped.
    add_column_if_missing(conn, "ALTER TABLE tasks ADD COLUMN input_text TEXT")?;
    add_column_if_missing(
        conn,
        "ALTER TABLE tasks ADD COLUMN input_truncated INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(conn, "ALTER TABLE tasks ADD COLUMN output_text TEXT")?;
    add_column_if_missing(
        conn,
        "ALTER TABLE tasks ADD COLUMN output_truncated INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(conn, "ALTER TABLE tasks ADD COLUMN duration_ms INTEGER")?;
    add_column_if_missing(conn, "ALTER TABLE tasks ADD COLUMN transitions_json TEXT")?;
    add_column_if_missing(conn, "ALTER TABLE tasks ADD COLUMN run_id TEXT")?;

    // HITL timing columns added to `task_approvals` after 005 shipped.
    add_column_if_missing(
        conn,
        "ALTER TABLE task_approvals ADD COLUMN suspended_at TEXT",
    )?;
    add_column_if_missing(
        conn,
        "ALTER TABLE task_approvals ADD COLUMN wait_duration_ms INTEGER",
    )?;

    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_task_approvals_pending
             ON task_approvals(task_id) WHERE approved IS NULL;

         CREATE TABLE IF NOT EXISTS notification_logs (
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
}

/// Opens the file's schema at the current version, or refuses it.
///
/// Shared entry point for every `hitl.db` opener.
///
/// # Errors
///
/// Returns [`apollia_core::schema::SchemaError`] when the migration fails or
/// the database was written by a newer binary.
pub fn open_hitl_schema(conn: &Connection) -> Result<(), apollia_core::schema::SchemaError> {
    apollia_core::schema::open_versioned(
        conn,
        apollia_core::paths::DataFile::Hitl.file_name(),
        HITL_SCHEMA_VERSION,
        HITL_MIGRATIONS,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::schema::SchemaError;

    /// Pre-versioning `hitl.db` schema, frozen as the `005_hitl_tables`
    /// shape a published binary wrote (`user_version = 0`, no observability
    /// columns, no timing columns, no notification log).
    const HITL_V0_SQL: &str = include_str!("../tests/fixtures/schemas/hitl_v0.sql");

    // GIVEN a database written by a pre-versioning binary, with a task row
    // WHEN opening it through the versioned layer
    // THEN the row survives, the missing columns and table appear and the version is stamped
    #[test]
    fn test_hitl_db_old_format_migrates_and_keeps_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(HITL_V0_SQL).unwrap();
        conn.execute(
            "INSERT INTO tasks (task_id, agent_name, status) VALUES ('t-1', 'mailer', 'completed')",
            [],
        )
        .unwrap();

        open_hitl_schema(&conn).unwrap();

        let (task, run_id): (String, Option<String>) = conn
            .query_row("SELECT task_id, run_id FROM tasks", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(task, "t-1");
        assert!(run_id.is_none());
        let logs: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='notification_logs'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(logs, 1);
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, i64::from(HITL_SCHEMA_VERSION));
    }

    // GIVEN a database stamped by a newer binary
    // WHEN opening it through the versioned layer
    // THEN the open is refused
    #[test]
    fn test_hitl_db_newer_version_is_refused() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", HITL_SCHEMA_VERSION + 1)
            .unwrap();

        let err = open_hitl_schema(&conn).unwrap_err();

        assert!(matches!(err, SchemaError::NewerThanBinary { .. }));
    }
}
