//! Versioned schema of `governance.db`, shared by every store in the file.
//!
//! `governance.db` gathers the runtime governance tables: `permission_rules`
//! and `permission_audit` (owned by this crate), plus `tools`,
//! `tool_credentials` and `chat_libre_config` (owned by `apollia-tools`,
//! whose `GovernanceDb` reuses this list). `PRAGMA user_version` belongs to
//! the database file, not to a table, so the three openers
//! ([`crate::prefix_rule_engine::PrefixRuleEngine`],
//! [`crate::audit_log::PermissionAuditLog`], and `GovernanceDb`) share one
//! migration list: whichever opens first brings the whole file to the current
//! version, and each refuses a file written by a newer binary.

use rusqlite::Connection;

use apollia_core::schema::{add_column_if_missing, Migration};

/// Current schema version of `governance.db`.
pub const GOVERNANCE_SCHEMA_VERSION: u32 = 1;

/// Numbered migration steps of `governance.db`.
///
/// Step `k` migrates the file from version `k` to `k + 1`; the list length
/// always equals [`GOVERNANCE_SCHEMA_VERSION`].
pub const GOVERNANCE_MIGRATIONS: &[Migration] = &[migrate_v1];

/// v1: the pre-versioning lineage of the file, replayed idempotently.
///
/// Every `governance.db` (including a legacy `permissions.db` copied over)
/// written before the versioned layer is at `user_version = 0` whatever
/// columns it carries, so this step must accept a fresh file, the legacy
/// shape, and every intermediate one, and bring each of them to the same
/// state.
fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS permission_rules (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            tool_name    TEXT NOT NULL,
            arg_prefix   TEXT,
            action       TEXT NOT NULL,
            created_at   INTEGER NOT NULL,
            created_by   TEXT,
            scope        TEXT NOT NULL DEFAULT 'global',
            project_path TEXT,
            agent_id     TEXT,
            expires_at   INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_rules_tool ON permission_rules(tool_name);

        CREATE TABLE IF NOT EXISTS permission_audit (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            tool_name   TEXT NOT NULL,
            first_arg   TEXT,
            decision    TEXT NOT NULL,
            decided_at  INTEGER NOT NULL,
            scope       TEXT,
            rule_id     INTEGER,
            agent       TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_audit_tool ON permission_audit(tool_name, decided_at);

        CREATE TABLE IF NOT EXISTS tools (
            name        TEXT PRIMARY KEY,
            enabled     BOOLEAN NOT NULL DEFAULT TRUE,
            config_json TEXT,
            updated_at  INTEGER NOT NULL DEFAULT (unixepoch())
        );

        CREATE TABLE IF NOT EXISTS tool_credentials (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            tool_name       TEXT NOT NULL,
            key_name        TEXT NOT NULL,
            value_encrypted BLOB NOT NULL,
            created_at      INTEGER NOT NULL DEFAULT (unixepoch()),
            last_used_at    INTEGER,
            UNIQUE(tool_name, key_name)
        );

        CREATE TABLE IF NOT EXISTS chat_libre_config (
            id              INTEGER PRIMARY KEY CHECK (id = 1),
            system_prompt   TEXT NOT NULL DEFAULT '',
            allowed_tools   TEXT NOT NULL DEFAULT '[]',
            llm_backend     TEXT,
            updated_at      TEXT NOT NULL
                            DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
        );
        INSERT OR IGNORE INTO chat_libre_config (id) VALUES (1);",
    )?;

    // Columns a legacy `permissions.db` (or an early `governance.db`) lacks.
    add_column_if_missing(
        conn,
        "ALTER TABLE permission_rules ADD COLUMN scope TEXT NOT NULL DEFAULT 'global'",
    )?;
    add_column_if_missing(
        conn,
        "ALTER TABLE permission_rules ADD COLUMN project_path TEXT",
    )?;
    add_column_if_missing(
        conn,
        "ALTER TABLE permission_rules ADD COLUMN agent_id TEXT",
    )?;
    add_column_if_missing(
        conn,
        "ALTER TABLE permission_rules ADD COLUMN expires_at INTEGER",
    )?;
    add_column_if_missing(conn, "ALTER TABLE permission_audit ADD COLUMN scope TEXT")?;
    add_column_if_missing(
        conn,
        "ALTER TABLE permission_audit ADD COLUMN rule_id INTEGER",
    )?;
    add_column_if_missing(conn, "ALTER TABLE permission_audit ADD COLUMN agent TEXT")?;

    // This index references the migrated columns, so it can only exist after
    // the legacy file has received them.
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_rules_scope_project
             ON permission_rules(scope, project_path);",
    )?;

    conn.execute_batch(
        "CREATE TRIGGER IF NOT EXISTS no_update_audit
         BEFORE UPDATE ON permission_audit BEGIN
             SELECT RAISE(ABORT, 'permission_audit is append-only');
         END;
         CREATE TRIGGER IF NOT EXISTS no_delete_audit
         BEFORE DELETE ON permission_audit BEGIN
             SELECT RAISE(ABORT, 'permission_audit is append-only');
         END;",
    )
}

/// Opens the file's schema at the current version, or refuses it.
///
/// Shared entry point for the three `governance.db` openers.
///
/// # Errors
///
/// Returns [`apollia_core::schema::SchemaError`] when the migration fails or
/// the database was written by a newer binary.
pub fn open_governance_schema(conn: &Connection) -> Result<(), apollia_core::schema::SchemaError> {
    apollia_core::schema::open_versioned(
        conn,
        apollia_core::paths::DataFile::Governance.file_name(),
        GOVERNANCE_SCHEMA_VERSION,
        GOVERNANCE_MIGRATIONS,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::schema::SchemaError;

    /// Pre-versioning `governance.db` schema, frozen as the legacy
    /// `permissions.db` shape a published binary wrote (`user_version = 0`,
    /// no scope / project_path / agent_id / expires_at on the rules, no
    /// scope / rule_id / agent on the audit rows).
    const GOVERNANCE_V0_SQL: &str = include_str!("../tests/fixtures/schemas/governance_v0.sql");

    // GIVEN a legacy permissions.db copied to governance.db, with rows
    // WHEN opening it through the versioned layer
    // THEN the rows survive, the missing columns are added and the version is stamped
    #[test]
    fn test_governance_db_old_format_migrates_and_keeps_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(GOVERNANCE_V0_SQL).unwrap();
        conn.execute(
            "INSERT INTO permission_rules (tool_name, arg_prefix, action, created_at)
             VALUES ('shell_command', 'git ', 'allow', 1700000000)",
            [],
        )
        .unwrap();

        open_governance_schema(&conn).unwrap();

        let (tool, scope): (String, String) = conn
            .query_row("SELECT tool_name, scope FROM permission_rules", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(tool, "shell_command");
        assert_eq!(scope, "global");
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, i64::from(GOVERNANCE_SCHEMA_VERSION));
    }

    // GIVEN a database stamped by a newer binary
    // WHEN opening it through the versioned layer
    // THEN the open is refused
    #[test]
    fn test_governance_db_newer_version_is_refused() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", GOVERNANCE_SCHEMA_VERSION + 1)
            .unwrap();

        let err = open_governance_schema(&conn).unwrap_err();

        assert!(matches!(err, SchemaError::NewerThanBinary { .. }));
    }
}
