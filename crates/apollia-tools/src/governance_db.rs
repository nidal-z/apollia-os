//! Initialization and migration of the consolidated `governance.db` SQLite store.
//!
//! This single store replaces the former `permissions.db` and gathers every
//! runtime governance table under `~/.apollia/governance.db`:
//!
//! - `permission_rules`: scope-aware prefix rules (session/project/global);
//! - `permission_audit`: immutable decision log, append-only via triggers;
//! - `tools`: enabled/disabled state and per-tool JSON configuration;
//! - `tool_credentials`: per-tool secrets, AES-256-GCM encrypted values.
//!
//! ## Migration from `permissions.db`
//!
//! On first startup with an existing `permissions.db`, the file is copied to
//! `governance.db` then renamed `permissions.db.bak`. The backup is kept but no
//! longer used by the runtime. The schema itself lives in
//! `apollia_permissions::governance_schema`: the file is also opened by
//! `PrefixRuleEngine` and `PermissionAuditLog`, so the three openers share
//! one versioned migration list.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

/// Error returned by [`GovernanceDb`].
#[derive(Debug, thiserror::Error)]
pub enum GovernanceError {
    /// I/O error while creating the directory or migrating the file.
    #[error("governance.db I/O error at {path}: {source}")]
    Io {
        /// Path involved in the I/O error.
        path: PathBuf,
        /// Underlying cause.
        #[source]
        source: std::io::Error,
    },
    /// SQLite error while opening or migrating the schema.
    #[error("governance.db SQLite error: {0}")]
    Database(#[from] rusqlite::Error),
    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] apollia_core::schema::SchemaError),
}

/// Filename of the consolidated store.
pub const GOVERNANCE_DB_FILENAME: &str = apollia_core::paths::DataFile::Governance.file_name();

/// Filename of the legacy permissions store.
pub const LEGACY_PERMISSIONS_FILENAME: &str = apollia_core::paths::LEGACY_PERMISSIONS_DB_NAME;

/// Filename of the backup created after migrating from `permissions.db`.
pub const LEGACY_BACKUP_FILENAME: &str = "permissions.db.bak";

/// Consolidated SQLite store for tool and permission governance.
///
/// `GovernanceDb` is responsible for:
/// - migrating any legacy `permissions.db` to `governance.db`;
/// - ensuring every table and trigger of the target schema exists;
/// - exposing the connection and path to downstream components (registry,
///   credential store, prefix-rule engine, audit log).
pub struct GovernanceDb {
    path: PathBuf,
    conn: Connection,
}

impl GovernanceDb {
    /// Opens (or creates) `<base_dir>/governance.db` and runs the schema migration.
    ///
    /// If `governance.db` does not exist but a legacy `permissions.db` is present
    /// in `base_dir`, the latter is copied to `governance.db` then renamed to
    /// `permissions.db.bak`. The backup is kept.
    ///
    /// The schema migration is idempotent: calling `open` several times on an
    /// already-migrated store produces no change.
    ///
    /// # Errors
    ///
    /// - [`GovernanceError::Io`] if creating the directory or the copy/rename fails.
    /// - [`GovernanceError::Database`] if SQLite fails to open the file or apply
    ///   the migration.
    pub fn open(base_dir: &Path) -> Result<Self, GovernanceError> {
        if !base_dir.exists() {
            std::fs::create_dir_all(base_dir).map_err(|e| GovernanceError::Io {
                path: base_dir.to_path_buf(),
                source: e,
            })?;
        }

        let path = base_dir.join(GOVERNANCE_DB_FILENAME);
        let legacy = base_dir.join(LEGACY_PERMISSIONS_FILENAME);
        let backup = base_dir.join(LEGACY_BACKUP_FILENAME);

        if !path.exists() && legacy.exists() {
            std::fs::copy(&legacy, &path).map_err(|e| GovernanceError::Io {
                path: path.clone(),
                source: e,
            })?;
            std::fs::rename(&legacy, &backup).map_err(|e| GovernanceError::Io {
                path: backup.clone(),
                source: e,
            })?;
            tracing::info!(
                from = %legacy.display(),
                to = %path.display(),
                backup = %backup.display(),
                "migrated permissions.db to governance.db"
            );
        }

        let conn = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

        apollia_permissions::governance_schema::open_governance_schema(&conn)?;

        Ok(Self { path, conn })
    }

    /// Absolute path of the open `governance.db` file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Underlying SQLite connection, read/write.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use tempfile::TempDir;

    fn count_tables(conn: &Connection, name: &str) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
            params![name],
            |row| row.get(0),
        )
        .expect("query sqlite_master")
    }

    #[test]
    fn test_fresh_create_all_tables() {
        // GIVEN an empty directory with no existing store.
        let dir = TempDir::new().expect("tempdir");
        // WHEN a GovernanceDb is opened for the first time.
        let db = GovernanceDb::open(dir.path()).expect("open governance.db");
        // THEN governance.db is created with the four target tables.
        let conn = db.connection();
        assert_eq!(count_tables(conn, "permission_rules"), 1);
        assert_eq!(count_tables(conn, "permission_audit"), 1);
        assert_eq!(count_tables(conn, "tools"), 1);
        assert_eq!(count_tables(conn, "tool_credentials"), 1);
        assert!(db.path().ends_with(GOVERNANCE_DB_FILENAME));
        assert!(db.path().exists());
    }

    #[test]
    fn test_migration_from_permissions_db() {
        // GIVEN a legacy permissions.db seeded with an existing rule.
        let dir = TempDir::new().expect("tempdir");
        let legacy = dir.path().join(LEGACY_PERMISSIONS_FILENAME);
        {
            let conn = Connection::open(&legacy).expect("create legacy");
            conn.execute_batch(
                "CREATE TABLE permission_rules (
                    id          INTEGER PRIMARY KEY AUTOINCREMENT,
                    tool_name   TEXT NOT NULL,
                    arg_prefix  TEXT,
                    action      TEXT NOT NULL,
                    created_at  INTEGER NOT NULL,
                    created_by  TEXT
                );",
            )
            .expect("legacy schema");
            conn.execute(
                "INSERT INTO permission_rules (tool_name, arg_prefix, action, created_at, created_by) \
                 VALUES ('bash_executor', 'git', 'allow', 1700000000, 'operator')",
                [],
            )
            .expect("seed legacy rule");
        }

        // WHEN GovernanceDb::open migrates that store.
        let db = GovernanceDb::open(dir.path()).expect("migrate");

        // THEN governance.db exists, permissions.db has been renamed to .bak,
        //      and the existing rule is present with scope='global'.
        let governance = dir.path().join(GOVERNANCE_DB_FILENAME);
        let backup = dir.path().join(LEGACY_BACKUP_FILENAME);
        assert!(governance.exists(), "governance.db must exist");
        assert!(backup.exists(), "permissions.db.bak must exist");
        assert!(!legacy.exists(), "permissions.db must have been renamed");

        let (tool, scope, project_path, expires_at): (String, String, Option<String>, Option<i64>) =
            db.connection()
                .query_row(
                    "SELECT tool_name, scope, project_path, expires_at FROM permission_rules",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .expect("query migrated rule");
        assert_eq!(tool, "bash_executor");
        assert_eq!(scope, "global");
        assert!(project_path.is_none());
        assert!(expires_at.is_none());
    }

    #[test]
    fn test_idempotent_migration() {
        // GIVEN a directory that has already been migrated once.
        let dir = TempDir::new().expect("tempdir");
        {
            let _first = GovernanceDb::open(dir.path()).expect("first open");
        }
        // WHEN the GovernanceDb is reopened.
        let second = GovernanceDb::open(dir.path()).expect("second open");
        // THEN no error, identical schema, no duplicated tables.
        let conn = second.connection();
        for table in [
            "permission_rules",
            "permission_audit",
            "tools",
            "tool_credentials",
        ] {
            assert_eq!(count_tables(conn, table), 1, "table {table} must be unique");
        }
    }

    #[test]
    fn test_audit_trigger_blocks_update() {
        // GIVEN a fresh GovernanceDb with one audit entry inserted.
        let dir = TempDir::new().expect("tempdir");
        let db = GovernanceDb::open(dir.path()).expect("open");
        db.connection()
            .execute(
                "INSERT INTO permission_audit (tool_name, first_arg, decision, decided_at) \
                 VALUES ('bash_executor', 'git status', 'AutoAllowedSafeList', 1700000000)",
                [],
            )
            .expect("insert audit row");

        // WHEN attempting to modify the decision...
        let update_result = db
            .connection()
            .execute("UPDATE permission_audit SET decision = 'NeedsApproval'", []);
        // ...or to delete it.
        let delete_result = db.connection().execute("DELETE FROM permission_audit", []);

        // THEN both operations are blocked by the append-only triggers.
        assert!(update_result.is_err(), "UPDATE must be blocked");
        assert!(delete_result.is_err(), "DELETE must be blocked");
    }
}
