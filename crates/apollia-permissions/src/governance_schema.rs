//! Versioned schema of `governance.db`, shared by every store in the file.
//!
//! `governance.db` gathers the runtime governance tables: `permission_rules`,
//! `permission_audit`, `family_consents` and `governance_upgrades` (owned by
//! this crate), plus `tools` and
//! `tool_credentials` (owned by `apollia-tools`, whose `GovernanceDb` reuses
//! this list). Since v3 `tool_credentials` is an index of the credentials,
//! their values living in the secret store; the encrypted values of an older
//! file wait in `tool_credentials_legacy` until `apollia-tools` moves them. The legacy `chat_libre_config` table that v1 creates is read
//! once by [`crate::chat_config_upgrade`], then dropped by the runtime once
//! carried over to the Apollia agent. `PRAGMA user_version` belongs to
//! the database file, not to a table, so the three openers
//! ([`crate::prefix_rule_engine::PrefixRuleEngine`],
//! [`crate::audit_log::PermissionAuditLog`], and `GovernanceDb`) share one
//! migration list: whichever opens first brings the whole file to the current
//! version, and each refuses a file written by a newer binary.

use rusqlite::Connection;

use apollia_core::schema::{add_column_if_missing, Migration};

/// Current schema version of `governance.db`.
pub const GOVERNANCE_SCHEMA_VERSION: u32 = 3;

/// Numbered migration steps of `governance.db`.
///
/// Step `k` migrates the file from version `k` to `k + 1`; the list length
/// always equals [`GOVERNANCE_SCHEMA_VERSION`].
pub const GOVERNANCE_MIGRATIONS: &[Migration] = &[migrate_v1, migrate_v2, migrate_v3];

/// Name, in `governance_upgrades`, of the data upgrade that moves the
/// encrypted tool credentials of `tool_credentials_legacy` into the secret
/// store. Consumed by `apollia-tools`, which holds both the old key and the
/// store.
pub const TOOL_CREDENTIALS_UPGRADE: &str = "tool_credentials_secret_store";

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

/// v2: the store of standing consents ("always", per agent and action family)
/// and the marker of the one-shot access upgrade.
///
/// `family_consents` holds one row per (agent, family, scope): `scope` is
/// `permanent` or `lot:<id>`, so every row of one batch run goes in one
/// statement. `governance_upgrades` names the data upgrades still to run; the
/// row inserted here is consumed by
/// [`crate::access_upgrade::run_access_upgrade`], which needs the tool
/// descriptors and therefore runs in the runtime, not in this step.
fn migrate_v2(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS family_consents (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            agent_id    TEXT NOT NULL,
            family      TEXT NOT NULL,
            scope       TEXT NOT NULL,
            granted_at  INTEGER NOT NULL,
            granted_by  TEXT NOT NULL,
            UNIQUE(agent_id, family, scope)
        );
        CREATE INDEX IF NOT EXISTS idx_family_consents_scope ON family_consents(scope);

        CREATE TABLE IF NOT EXISTS governance_upgrades (
            name     TEXT PRIMARY KEY,
            done_at  INTEGER
        );
        INSERT OR IGNORE INTO governance_upgrades (name, done_at)
            VALUES ('access_upgrade', NULL);",
    )
}

/// v3: `tool_credentials` keeps no value any more, only the index of what is
/// stored (tool, key, dates), because the values move to the secret store.
///
/// The encrypted table is renamed `tool_credentials_legacy` and kept for the
/// data upgrade named [`TOOL_CREDENTIALS_UPGRADE`], which needs the key file
/// and the secret store and therefore runs outside this step. Each statement
/// is guarded, so a step interrupted half way is replayed to the same state.
fn migrate_v3(conn: &Connection) -> Result<(), rusqlite::Error> {
    let has_value_column: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM pragma_table_info('tool_credentials')
         WHERE name = 'value_encrypted'",
        [],
        |row| row.get(0),
    )?;
    let has_legacy: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master
         WHERE type = 'table' AND name = 'tool_credentials_legacy'",
        [],
        |row| row.get(0),
    )?;
    if has_value_column && !has_legacy {
        conn.execute_batch("ALTER TABLE tool_credentials RENAME TO tool_credentials_legacy;")?;
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS tool_credentials (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            tool_name       TEXT NOT NULL,
            key_name        TEXT NOT NULL,
            created_at      INTEGER NOT NULL DEFAULT (unixepoch()),
            last_used_at    INTEGER,
            UNIQUE(tool_name, key_name)
        );",
    )?;
    let has_legacy: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master
         WHERE type = 'table' AND name = 'tool_credentials_legacy'",
        [],
        |row| row.get(0),
    )?;
    if has_legacy {
        conn.execute_batch(
            "INSERT OR IGNORE INTO tool_credentials (tool_name, key_name, created_at, last_used_at)
             SELECT tool_name, key_name, created_at, last_used_at FROM tool_credentials_legacy;",
        )?;
    }
    conn.execute(
        "INSERT OR IGNORE INTO governance_upgrades (name, done_at) VALUES (?1, NULL)",
        [TOOL_CREDENTIALS_UPGRADE],
    )?;
    Ok(())
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

    // GIVEN a v2 file holding an encrypted tool credential
    // WHEN it is opened through the versioned layer
    // THEN the index keeps the credential without its value, the encrypted
    // row waits in the legacy table, and the data upgrade is pending
    #[test]
    fn test_governance_v3_turns_tool_credentials_into_an_index() {
        let conn = Connection::open_in_memory().unwrap();
        (GOVERNANCE_MIGRATIONS[0])(&conn).unwrap();
        (GOVERNANCE_MIGRATIONS[1])(&conn).unwrap();
        conn.pragma_update(None, "user_version", 2).unwrap();
        conn.execute(
            "INSERT INTO tool_credentials (tool_name, key_name, value_encrypted, created_at)
             VALUES ('web_search', 'brave.api_key', X'00112233', 1700000000)",
            [],
        )
        .unwrap();

        open_governance_schema(&conn).unwrap();

        let columns: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('tool_credentials')")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert!(!columns.contains(&"value_encrypted".to_string()));
        let indexed: (String, i64) = conn
            .query_row(
                "SELECT key_name, created_at FROM tool_credentials",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(indexed, ("brave.api_key".to_string(), 1_700_000_000));
        let legacy: i64 = conn
            .query_row("SELECT COUNT(*) FROM tool_credentials_legacy", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(legacy, 1);
        let pending: Option<i64> = conn
            .query_row(
                "SELECT done_at FROM governance_upgrades WHERE name = ?1",
                [TOOL_CREDENTIALS_UPGRADE],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pending, None);
    }

    // GIVEN the v3 step already applied once
    // WHEN it is replayed on the same file
    // THEN it changes nothing and fails on nothing
    #[test]
    fn test_governance_v3_replayed_is_a_no_op() {
        let conn = Connection::open_in_memory().unwrap();
        open_governance_schema(&conn).unwrap();

        migrate_v3(&conn).unwrap();

        let index: i64 = conn
            .query_row("SELECT COUNT(*) FROM tool_credentials", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(index, 0);
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
