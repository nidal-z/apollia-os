//! Versioned schema of `agents.db`, shared by every store in the file.
//!
//! `agents.db` holds the installed-agent registry (`installed_agents`, owned
//! by [`crate::agent_repository`]) and the package tables
//! (`installed_packages`, `package_agents`, owned by
//! [`crate::package_repository`]). `PRAGMA user_version` belongs to the
//! database file, not to a table, so the two repositories share one
//! migration list: whichever opens first brings the whole file to the
//! current version, and both refuse a file written by a newer binary.

use rusqlite::Connection;

use apollia_core::schema::Migration;

/// Current schema version of `agents.db`.
pub(crate) const AGENTS_DB_SCHEMA_VERSION: u32 = 1;

/// Numbered migration steps of `agents.db`.
///
/// Step `k` migrates the file from version `k` to `k + 1`; the list length
/// always equals [`AGENTS_DB_SCHEMA_VERSION`].
pub(crate) const AGENTS_DB_MIGRATIONS: &[Migration] = &[migrate_v1];

/// v1: the pre-versioning lineage of the file, replayed idempotently.
///
/// Every `agents.db` written before the versioned layer is at
/// `user_version = 0`; this step brings any of them (and a fresh file) to
/// the full current schema, `007_agent_tables` then `008_package_tables`.
fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(include_str!("../migrations/007_agent_tables.sql"))?;
    conn.execute_batch(include_str!("../migrations/008_package_tables.sql"))
}

/// Opens the file's schema at the current version, or refuses it.
///
/// Shared entry point for the two `agents.db` repositories.
///
/// # Errors
///
/// Returns [`apollia_core::schema::SchemaError`] when the migration fails or
/// the database was written by a newer binary.
pub(crate) fn open_agents_schema(
    conn: &Connection,
) -> Result<(), apollia_core::schema::SchemaError> {
    apollia_core::schema::open_versioned(
        conn,
        apollia_core::paths::DataFile::Agents.file_name(),
        AGENTS_DB_SCHEMA_VERSION,
        AGENTS_DB_MIGRATIONS,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::schema::SchemaError;

    /// Pre-versioning `agents.db` schema (`user_version = 0`): the same
    /// tables, written by a binary that did not stamp a version.
    const AGENTS_V0_SQL: &str = include_str!("../tests/fixtures/schemas/agents_v0.sql");

    // GIVEN a database written by a pre-versioning binary, with rows
    // WHEN opening it through the versioned layer
    // THEN the rows survive and the version is stamped
    #[test]
    fn test_agents_db_old_format_migrates_and_keeps_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(AGENTS_V0_SQL).unwrap();
        conn.execute(
            "INSERT INTO installed_agents (name, version, install_path, source_path, manifest_json)
             VALUES ('mailer', '1.0.0', '/a/agent.py', '/src/agent.py', '{}')",
            [],
        )
        .unwrap();

        open_agents_schema(&conn).unwrap();

        let name: String = conn
            .query_row("SELECT name FROM installed_agents", [], |row| row.get(0))
            .unwrap();
        assert_eq!(name, "mailer");
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, i64::from(AGENTS_DB_SCHEMA_VERSION));
    }

    // GIVEN a database stamped by a newer binary
    // WHEN opening it through the versioned layer
    // THEN the open is refused
    #[test]
    fn test_agents_db_newer_version_is_refused() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", AGENTS_DB_SCHEMA_VERSION + 1)
            .unwrap();

        let err = open_agents_schema(&conn).unwrap_err();

        assert!(matches!(err, SchemaError::NewerThanBinary { .. }));
    }
}
