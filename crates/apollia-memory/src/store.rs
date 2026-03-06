//! MemoryStore — SQLite schema management and versioned migrations.
//!
//! A `MemoryStore` corresponds to a single `<namespace>.db` file.
//! It owns the `rusqlite::Connection` and guarantees the schema is
//! correctly initialised and migrated at startup.

use std::path::Path;

use rusqlite::Connection;

/// Current schema version for the memory database.
const SCHEMA_VERSION: u32 = 1;

/// SQL statements for schema version 1.
const SCHEMA_V1: &str = "
    CREATE TABLE IF NOT EXISTS _schema_version (
        version INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS episodic_memories (
        id         TEXT PRIMARY KEY,
        namespace  TEXT NOT NULL,
        task_id    TEXT,
        agent_id   TEXT NOT NULL,
        content    TEXT NOT NULL,
        importance REAL NOT NULL DEFAULT 0.5,
        created_at TEXT NOT NULL,
        expires_at TEXT,
        metadata   TEXT NOT NULL DEFAULT '{}'
    );

    CREATE TABLE IF NOT EXISTS semantic_memories (
        id         TEXT PRIMARY KEY,
        namespace  TEXT NOT NULL,
        key        TEXT NOT NULL,
        value      TEXT NOT NULL,
        source     TEXT,
        confidence REAL NOT NULL DEFAULT 1.0,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        expires_at TEXT,
        UNIQUE(namespace, key)
    );

    CREATE TABLE IF NOT EXISTS procedural_memories (
        id            TEXT PRIMARY KEY,
        namespace     TEXT NOT NULL,
        trigger_text  TEXT NOT NULL,
        steps         TEXT NOT NULL,
        success_count INTEGER NOT NULL DEFAULT 1,
        last_used_at  TEXT NOT NULL,
        created_at    TEXT NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_episodic_namespace_created
        ON episodic_memories(namespace, created_at DESC);
    CREATE INDEX IF NOT EXISTS idx_semantic_namespace_key
        ON semantic_memories(namespace, key);
    CREATE INDEX IF NOT EXISTS idx_procedural_namespace_trigger
        ON procedural_memories(namespace, trigger_text);
";

/// FTS5 virtual table (must be executed separately from `execute_batch`
/// because some SQLite builds require individual statement execution for
/// virtual table DDL).
const FTS5_DDL: &str = "\
    CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(\
        content,\
        source_table UNINDEXED,\
        source_id UNINDEXED,\
        tokenize='unicode61'\
    )";

/// Manager for a single namespace's SQLite memory database.
///
/// One `MemoryStore` corresponds to one `<namespace>.db` file.
/// It owns the [`Connection`] and ensures the schema is correctly
/// initialised and migrated on open.
pub struct MemoryStore {
    conn: Connection,
}

/// Errors from [`MemoryStore`] operations.
#[derive(Debug, thiserror::Error)]
pub enum MemoryStoreError {
    /// The database file could not be opened or created.
    #[error("failed to open memory database: {0}")]
    OpenFailed(String),

    /// A schema migration step failed.
    #[error("schema migration failed at version {version}: {reason}")]
    MigrationFailed {
        /// The migration version that failed.
        version: u32,
        /// Human-readable reason.
        reason: String,
    },

    /// A raw SQLite error propagated from rusqlite.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl MemoryStore {
    /// Opens (or creates) the SQLite database for a memory namespace.
    ///
    /// Activates WAL mode, creates the schema if absent, and applies
    /// any pending migrations up to [`SCHEMA_VERSION`].
    pub fn open(path: &Path) -> Result<Self, MemoryStoreError> {
        let conn =
            Connection::open(path).map_err(|e| MemoryStoreError::OpenFailed(e.to_string()))?;

        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| MemoryStoreError::MigrationFailed {
                version: 0,
                reason: format!("failed to enable WAL: {e}"),
            })?;

        let current_version = Self::read_schema_version(&conn)?;

        if current_version < SCHEMA_VERSION {
            Self::apply_migrations(&conn, current_version)?;
        }

        tracing::info!(
            path = %path.display(),
            schema_version = SCHEMA_VERSION,
            "memory store opened"
        );

        Ok(Self { conn })
    }

    /// Returns the current schema version stored in `_schema_version`.
    pub fn schema_version(&self) -> Result<u32, MemoryStoreError> {
        Self::read_schema_version(&self.conn)
    }

    /// Provides direct access to the underlying connection for backends.
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Reads the schema version from `_schema_version`, returning 0 if the
    /// table does not exist or is empty.
    fn read_schema_version(conn: &Connection) -> Result<u32, MemoryStoreError> {
        let table_exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='_schema_version'",
            [],
            |row| row.get(0),
        )?;

        if !table_exists {
            return Ok(0);
        }

        let version: Option<u32> = conn
            .query_row("SELECT version FROM _schema_version LIMIT 1", [], |row| {
                row.get(0)
            })
            .ok();

        Ok(version.unwrap_or(0))
    }

    /// Applies migrations from `current_version` up to [`SCHEMA_VERSION`].
    fn apply_migrations(conn: &Connection, current_version: u32) -> Result<(), MemoryStoreError> {
        if current_version < 1 {
            Self::migrate_to_v1(conn)?;
        }
        Ok(())
    }

    /// Migration to schema version 1: creates all tables, indexes, and FTS5.
    fn migrate_to_v1(conn: &Connection) -> Result<(), MemoryStoreError> {
        conn.execute_batch(SCHEMA_V1)
            .map_err(|e| MemoryStoreError::MigrationFailed {
                version: 1,
                reason: e.to_string(),
            })?;

        conn.execute(FTS5_DDL, [])
            .map_err(|e| MemoryStoreError::MigrationFailed {
                version: 1,
                reason: format!("FTS5 creation failed: {e}"),
            })?;

        // Seed the schema version.
        let existing: i64 =
            conn.query_row("SELECT COUNT(*) FROM _schema_version", [], |row| row.get(0))?;

        if existing == 0 {
            conn.execute(
                "INSERT INTO _schema_version (version) VALUES (?1)",
                [SCHEMA_VERSION],
            )?;
        } else {
            conn.execute("UPDATE _schema_version SET version = ?1", [SCHEMA_VERSION])?;
        }

        tracing::info!(version = SCHEMA_VERSION, "schema migrated");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_db_path() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("apollia_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("test.db")
    }

    // AC-1 — All tables created on fresh open
    #[test]
    fn test_ac1_schema_created_on_open() {
        // GIVEN
        let path = temp_db_path();
        // WHEN
        let store = MemoryStore::open(&path).unwrap();
        // THEN — all tables exist
        let tables: Vec<String> = store
            .conn()
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(tables.contains(&"episodic_memories".to_string()));
        assert!(tables.contains(&"semantic_memories".to_string()));
        assert!(tables.contains(&"procedural_memories".to_string()));
        assert!(tables.contains(&"_schema_version".to_string()));
    }

    // AC-1 — WAL mode is active
    #[test]
    fn test_ac1_wal_mode_active() {
        // GIVEN
        let path = temp_db_path();
        let store = MemoryStore::open(&path).unwrap();
        // WHEN
        let mode: String = store
            .conn()
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        // THEN
        assert_eq!(mode, "wal");
    }

    // AC-2 — FTS5 with unicode61 tokenizer works
    #[test]
    fn test_ac2_fts5_unicode61_works() {
        // GIVEN
        let path = temp_db_path();
        let store = MemoryStore::open(&path).unwrap();
        // WHEN — insert accented content
        store
            .conn()
            .execute(
                "INSERT INTO memory_fts(content, source_table, source_id) VALUES (?1, ?2, ?3)",
                ("reunion avec societe", "episodic", "test-id"),
            )
            .unwrap();
        // THEN — search finds it
        let count: i64 = store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM memory_fts WHERE memory_fts MATCH 'reunion'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(count >= 1);
    }

    // AC-3 — Reopen existing DB without re-migration
    #[test]
    fn test_ac3_reopen_existing_db() {
        // GIVEN
        let path = temp_db_path();
        let _ = MemoryStore::open(&path).unwrap();
        // WHEN
        let store2 = MemoryStore::open(&path).unwrap();
        // THEN
        assert_eq!(store2.schema_version().unwrap(), SCHEMA_VERSION);
    }

    // AC-4 — Invalid path returns error
    #[test]
    fn test_ac4_invalid_path_returns_error() {
        // GIVEN
        let path = PathBuf::from("/nonexistent/dir/that/does/not/exist/memory.db");
        // WHEN
        let result = MemoryStore::open(&path);
        // THEN
        assert!(result.is_err());
    }

    // AC-5 — schema_version() returns current version
    #[test]
    fn test_ac5_schema_version_returns_current() {
        // GIVEN
        let path = temp_db_path();
        let store = MemoryStore::open(&path).unwrap();
        // WHEN
        let version = store.schema_version().unwrap();
        // THEN
        assert_eq!(version, SCHEMA_VERSION);
    }

    // FTS5 virtual table is listed in sqlite_master
    #[test]
    fn test_fts5_table_exists_in_schema() {
        // GIVEN
        let path = temp_db_path();
        let store = MemoryStore::open(&path).unwrap();
        // WHEN
        let tables: Vec<String> = store
            .conn()
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        // THEN
        assert!(tables.contains(&"memory_fts".to_string()));
    }
}
