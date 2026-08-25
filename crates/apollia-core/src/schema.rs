//! Versioned opening of the SQLite stores under the data directory.
//!
//! Every store used to create its schema ad hoc at open time: no version on
//! disk, no refusal of a database written by a newer binary, and additive
//! `ALTER TABLE` migrations swallowed under `let _ =`, which hid every failure
//! and not just the duplicate-column one. This module is the common layer:
//! [`open_versioned`] stamps `PRAGMA user_version`, applies the missing
//! migrations in order, and refuses a database more recent than the binary;
//! [`add_column_if_missing`] is the one tolerance an additive migration is
//! allowed.
//!
//! Adoption is per store: a store passes its numbered migration list here
//! instead of running DDL by hand. `scripts/check_sqlite_schema_versioning.py`
//! holds the list of stores that have not adopted the layer yet, and that list
//! only shrinks.

use rusqlite::Connection;

/// One numbered migration step.
///
/// Step `k` (0-based index `k - 1` in the slice) brings a database from
/// version `k - 1` to version `k`. Steps must be idempotent (`IF NOT EXISTS`
/// DDL, or [`add_column_if_missing`] for additive columns): a crash between a
/// step and its version stamp replays the step on the next open.
pub type Migration = fn(&Connection) -> Result<(), rusqlite::Error>;

/// Failure to open a store at a supported schema version.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SchemaError {
    /// The database on disk carries a version this binary does not know.
    ///
    /// Opening it anyway could misread or destroy rows written by the newer
    /// binary, so the caller must surface this instead of recreating anything.
    #[error(
        "{name}: schema version {found} on disk is newer than the supported version {supported}; refusing to open"
    )]
    NewerThanBinary {
        /// Name of the store, for the operator-facing message.
        name: &'static str,
        /// Version read from `PRAGMA user_version`.
        found: i64,
        /// Highest version this binary supports.
        supported: u32,
    },
    /// The declared version and the migration list disagree.
    ///
    /// Fail fast: a missing step would leave every fresh database silently
    /// below the declared version.
    #[error(
        "{name}: schema version {declared} declared but {provided} migration step(s) provided"
    )]
    MigrationCountMismatch {
        /// Name of the store.
        name: &'static str,
        /// Declared current version.
        declared: u32,
        /// Number of migration steps provided.
        provided: usize,
    },
    /// Reading `PRAGMA user_version` failed.
    #[error("{name}: cannot read the schema version")]
    Version {
        /// Name of the store.
        name: &'static str,
        /// Underlying SQLite failure.
        #[source]
        source: rusqlite::Error,
    },
    /// A migration step or its version stamp failed.
    #[error("{name}: migration to schema version {target} failed")]
    Migration {
        /// Name of the store.
        name: &'static str,
        /// Version the failing step was migrating to.
        target: u32,
        /// Underlying SQLite failure.
        #[source]
        source: rusqlite::Error,
    },
}

/// Bring an open connection to `schema_version`, or refuse it.
///
/// Reads `PRAGMA user_version`, refuses a database more recent than
/// `schema_version`, then applies the missing steps of `migrations` in order,
/// stamping the version after each one. A fresh database runs every step; an
/// up-to-date database runs none.
///
/// `migrations.len()` must equal `schema_version`: step `k` of the slice
/// migrates to version `k + 1`.
///
/// # Errors
///
/// - [`SchemaError::MigrationCountMismatch`] when the list and the declared
///   version disagree.
/// - [`SchemaError::Version`] when the version pragma cannot be read.
/// - [`SchemaError::NewerThanBinary`] when the database was written by a newer
///   binary.
/// - [`SchemaError::Migration`] when a step or its version stamp fails.
pub fn open_versioned(
    conn: &Connection,
    name: &'static str,
    schema_version: u32,
    migrations: &[Migration],
) -> Result<(), SchemaError> {
    if migrations.len() != schema_version as usize {
        return Err(SchemaError::MigrationCountMismatch {
            name,
            declared: schema_version,
            provided: migrations.len(),
        });
    }
    let found: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|source| SchemaError::Version { name, source })?;
    if found > i64::from(schema_version) {
        return Err(SchemaError::NewerThanBinary {
            name,
            found,
            supported: schema_version,
        });
    }
    let start = usize::try_from(found).unwrap_or(0);
    for (index, migrate) in migrations.iter().enumerate().skip(start) {
        let target = index as u32 + 1;
        migrate(conn).map_err(|source| SchemaError::Migration {
            name,
            target,
            source,
        })?;
        conn.pragma_update(None, "user_version", target)
            .map_err(|source| SchemaError::Migration {
                name,
                target,
                source,
            })?;
    }
    Ok(())
}

/// Run an additive `ALTER TABLE ... ADD COLUMN`, tolerating only the
/// duplicate-column failure.
///
/// The previous shape, `let _ = conn.execute_batch(...)`, swallowed every
/// failure: a locked file or an I/O error read as "column already there".
/// Here the one expected failure, "duplicate column name" (SQLite primary
/// code 1 with no extended detail), is a no-op, and everything else
/// propagates.
///
/// # Errors
///
/// Any SQLite failure other than the duplicate-column one.
pub fn add_column_if_missing(conn: &Connection, sql: &str) -> Result<(), rusqlite::Error> {
    match conn.execute_batch(sql) {
        Ok(()) => Ok(()),
        Err(rusqlite::Error::SqliteFailure(err, ref message))
            if err.extended_code == 1
                && message
                    .as_deref()
                    .is_some_and(|m| m.contains("duplicate column name")) =>
        {
            Ok(())
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAME: &str = "test.db";

    fn v1(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch("CREATE TABLE IF NOT EXISTS t (id INTEGER PRIMARY KEY, a TEXT)")
    }

    fn v2(conn: &Connection) -> Result<(), rusqlite::Error> {
        add_column_if_missing(conn, "ALTER TABLE t ADD COLUMN b TEXT")
    }

    fn failing(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch("THIS IS NOT SQL")
    }

    fn user_version(conn: &Connection) -> i64 {
        conn.query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap()
    }

    // GIVEN a fresh database and a two-step migration list
    // WHEN opening at version 2
    // THEN both steps run and the stamped version is 2
    #[test]
    fn test_open_versioned_fresh_database_runs_every_step() {
        let conn = Connection::open_in_memory().unwrap();

        open_versioned(&conn, NAME, 2, &[v1, v2]).unwrap();

        assert_eq!(user_version(&conn), 2);
        conn.execute("INSERT INTO t (a, b) VALUES ('x', 'y')", [])
            .unwrap();
    }

    // GIVEN a database already migrated to version 1
    // WHEN opening at version 2
    // THEN only the missing step runs and existing rows survive
    #[test]
    fn test_open_versioned_partial_database_resumes_where_it_stopped() {
        let conn = Connection::open_in_memory().unwrap();
        open_versioned(&conn, NAME, 1, &[v1]).unwrap();
        conn.execute("INSERT INTO t (a) VALUES ('kept')", [])
            .unwrap();

        open_versioned(&conn, NAME, 2, &[v1, v2]).unwrap();

        assert_eq!(user_version(&conn), 2);
        let kept: String = conn
            .query_row("SELECT a FROM t", [], |row| row.get(0))
            .unwrap();
        assert_eq!(kept, "kept");
    }

    // GIVEN a database stamped by a newer binary
    // WHEN opening at an older version
    // THEN the open is refused and nothing is migrated
    #[test]
    fn test_open_versioned_newer_database_is_refused() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", 3).unwrap();

        let err = open_versioned(&conn, NAME, 2, &[v1, v2]).unwrap_err();

        assert!(matches!(
            err,
            SchemaError::NewerThanBinary {
                found: 3,
                supported: 2,
                ..
            }
        ));
    }

    // GIVEN a migration list shorter than the declared version
    // WHEN opening
    // THEN the mismatch is refused before touching the database
    #[test]
    fn test_open_versioned_short_migration_list_is_refused() {
        let conn = Connection::open_in_memory().unwrap();

        let err = open_versioned(&conn, NAME, 2, &[v1]).unwrap_err();

        assert!(matches!(
            err,
            SchemaError::MigrationCountMismatch {
                declared: 2,
                provided: 1,
                ..
            }
        ));
        assert_eq!(user_version(&conn), 0);
    }

    // GIVEN a step that fails
    // WHEN opening
    // THEN the failure names the target version and the version stays behind
    #[test]
    fn test_open_versioned_failing_step_reports_target_version() {
        let conn = Connection::open_in_memory().unwrap();

        let err = open_versioned(&conn, NAME, 2, &[v1, failing]).unwrap_err();

        assert!(matches!(err, SchemaError::Migration { target: 2, .. }));
        assert_eq!(user_version(&conn), 1);
    }

    // GIVEN a table that already carries the column
    // WHEN adding it again
    // THEN the duplicate is tolerated as a no-op
    #[test]
    fn test_add_column_if_missing_tolerates_duplicate_column() {
        let conn = Connection::open_in_memory().unwrap();
        v1(&conn).unwrap();

        add_column_if_missing(&conn, "ALTER TABLE t ADD COLUMN a TEXT").unwrap();
    }

    // GIVEN a statement that fails for another reason
    // WHEN adding the column
    // THEN the failure propagates instead of being swallowed
    #[test]
    fn test_add_column_if_missing_propagates_other_failures() {
        let conn = Connection::open_in_memory().unwrap();

        let err = add_column_if_missing(&conn, "ALTER TABLE absent ADD COLUMN a TEXT");

        assert!(err.is_err());
    }
}
