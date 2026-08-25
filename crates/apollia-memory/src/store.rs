//! MemoryStore: SQLite schema management and versioned migrations.
//!
//! A `MemoryStore` corresponds to a single `<namespace>.db` file.
//! It owns the `rusqlite::Connection` and guarantees the schema is
//! correctly initialised and migrated at startup.

use std::path::Path;

use rusqlite::Connection;

/// Statistics for a memory namespace database.
///
/// Returned by [`MemoryStore::stats`] and used by the CLI `memory inspect` command.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryStats {
    /// Namespace name.
    pub namespace: String,
    /// Number of episodic memory entries.
    pub episodic_count: u64,
    /// Number of semantic memory entries.
    pub semantic_count: u64,
    /// Number of procedural memory entries.
    pub procedural_count: u64,
    /// Number of FTS5 index entries.
    pub fts_entries: u64,
    /// Size of the `.db` file in bytes.
    pub db_size_bytes: u64,
}

/// Current schema version for the memory database.
const SCHEMA_VERSION: u32 = 2;

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

    /// The database on disk carries a version this binary does not know.
    #[error(
        "memory database schema version {found} on disk is newer than the supported version {supported}; refusing to open"
    )]
    NewerThanBinary {
        /// Version read from the `_schema_version` table.
        found: u32,
        /// Highest version this binary supports.
        supported: u32,
    },
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

        if current_version > SCHEMA_VERSION {
            // Opening anyway could misread or destroy rows written by the
            // newer binary, so surface the refusal instead of migrating.
            return Err(MemoryStoreError::NewerThanBinary {
                found: current_version,
                supported: SCHEMA_VERSION,
            });
        }
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

    /// Returns memory statistics for the given namespace.
    ///
    /// Queries row counts in each memory table and reads the file size
    /// from `db_path`. This is used by the CLI `memory inspect` command.
    pub fn stats(&self, namespace: &str, db_path: &Path) -> Result<MemoryStats, MemoryStoreError> {
        let episodic_count: u64 = self.conn.query_row(
            "SELECT COUNT(*) FROM episodic_memories WHERE namespace = ?1",
            rusqlite::params![namespace],
            |row| row.get(0),
        )?;

        let semantic_count: u64 = self.conn.query_row(
            "SELECT COUNT(*) FROM semantic_memories WHERE namespace = ?1",
            rusqlite::params![namespace],
            |row| row.get(0),
        )?;

        let procedural_count: u64 = self.conn.query_row(
            "SELECT COUNT(*) FROM procedural_memories WHERE namespace = ?1",
            rusqlite::params![namespace],
            |row| row.get(0),
        )?;

        let fts_entries: u64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM memory_fts", [], |row| row.get(0))?;

        let db_size_bytes = std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);

        Ok(MemoryStats {
            namespace: namespace.to_string(),
            episodic_count,
            semantic_count,
            procedural_count,
            fts_entries,
            db_size_bytes,
        })
    }

    /// Deletes a memory entry by its UUID across all tables.
    ///
    /// Tries `episodic_memories`, `semantic_memories`, and `procedural_memories`
    /// in order. Also removes the corresponding FTS5 index entry.
    /// Returns `true` if an entry was found and deleted, `false` otherwise.
    pub fn delete_entry_by_id(&self, id: &str) -> Result<bool, MemoryStoreError> {
        let tables = [
            "episodic_memories",
            "semantic_memories",
            "procedural_memories",
        ];

        for table in &tables {
            let sql = format!("DELETE FROM {table} WHERE id = ?1");
            let deleted = self.conn.execute(&sql, rusqlite::params![id])?;
            if deleted > 0 {
                self.conn.execute(
                    "DELETE FROM memory_fts WHERE source_id = ?1",
                    rusqlite::params![id],
                )?;
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Deletes all episodic memory entries for the given namespace.
    ///
    /// Returns the number of rows deleted.
    pub fn clear_episodic(&self, namespace: &str) -> Result<u64, MemoryStoreError> {
        let n = self.conn.execute(
            "DELETE FROM episodic_memories WHERE namespace = ?1",
            rusqlite::params![namespace],
        )?;
        Ok(n as u64)
    }

    /// Deletes all semantic memory entries for the given namespace.
    ///
    /// Returns the number of rows deleted.
    pub fn clear_semantic(&self, namespace: &str) -> Result<u64, MemoryStoreError> {
        let n = self.conn.execute(
            "DELETE FROM semantic_memories WHERE namespace = ?1",
            rusqlite::params![namespace],
        )?;
        Ok(n as u64)
    }

    /// Deletes all procedural memory entries for the given namespace.
    ///
    /// Returns the number of rows deleted.
    pub fn clear_procedural(&self, namespace: &str) -> Result<u64, MemoryStoreError> {
        let n = self.conn.execute(
            "DELETE FROM procedural_memories WHERE namespace = ?1",
            rusqlite::params![namespace],
        )?;
        Ok(n as u64)
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
        if current_version < 2 {
            Self::migrate_to_v2(conn)?;
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

    /// Migration to schema version 2: adds `plan_choices` for binary feedback RLHF signals.
    fn migrate_to_v2(conn: &Connection) -> Result<(), MemoryStoreError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS plan_choices (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id  TEXT NOT NULL UNIQUE,
                chosen      TEXT NOT NULL,
                plan_a_json TEXT NOT NULL,
                plan_b_json TEXT NOT NULL,
                chosen_at   INTEGER NOT NULL
            );",
        )
        .map_err(|e| MemoryStoreError::MigrationFailed {
            version: 2,
            reason: e.to_string(),
        })?;

        conn.execute("UPDATE _schema_version SET version = 2", [])
            .map_err(|e| MemoryStoreError::MigrationFailed {
                version: 2,
                reason: format!("version bump failed: {e}"),
            })?;

        tracing::info!(
            version = 2,
            "schema migrated to v2: plan_choices table added"
        );
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

    // All tables created on fresh open
    #[test]
    fn test_schema_created_on_open() {
        // GIVEN
        let path = temp_db_path();
        // WHEN
        let store = MemoryStore::open(&path).unwrap();
        // THEN: all tables exist
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

    // WAL mode is active
    #[test]
    fn test_wal_mode_active() {
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

    // FTS5 with unicode61 tokenizer works
    #[test]
    fn test_fts5_unicode61_works() {
        // GIVEN
        let path = temp_db_path();
        let store = MemoryStore::open(&path).unwrap();
        // WHEN: insert accented content
        store
            .conn()
            .execute(
                "INSERT INTO memory_fts(content, source_table, source_id) VALUES (?1, ?2, ?3)",
                ("reunion avec societe", "episodic", "test-id"),
            )
            .unwrap();
        // THEN: search finds it
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

    // GIVEN a database migrated to v1 only, holding a row
    // WHEN opening it (v2 is current)
    // THEN the v1 rows survive, plan_choices appears and the version is bumped
    #[test]
    fn test_v1_database_migrates_to_v2_and_keeps_rows() {
        let path = temp_db_path();
        {
            // Build the database exactly as a v1 binary left it: v1 schema,
            // stamped 1 (migrate_to_v1 cannot be used here: it stamps the
            // current SCHEMA_VERSION).
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(SCHEMA_V1).unwrap();
            conn.execute(FTS5_DDL, []).unwrap();
            conn.execute("INSERT INTO _schema_version (version) VALUES (1)", [])
                .unwrap();
            conn.execute(
                "INSERT INTO episodic_memories (id, namespace, agent_id, content, created_at)
                 VALUES ('m-1', 'ns', 'agent', 'remembered', '2026-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
        }

        let store = MemoryStore::open(&path).unwrap();

        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        let kept: String = store
            .conn()
            .query_row("SELECT content FROM episodic_memories", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(kept, "remembered");
        let plan_choices: i64 = store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='plan_choices'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(plan_choices, 1);
    }

    // GIVEN a database stamped by a newer binary
    // WHEN opening it
    // THEN the open is refused
    #[test]
    fn test_newer_database_is_refused() {
        let path = temp_db_path();
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(SCHEMA_V1).unwrap();
            conn.execute(
                "INSERT INTO _schema_version (version) VALUES (?1)",
                [SCHEMA_VERSION + 1],
            )
            .unwrap();
        }

        let err = MemoryStore::open(&path).map(|_| ()).unwrap_err();

        assert!(matches!(
            err,
            MemoryStoreError::NewerThanBinary {
                supported: SCHEMA_VERSION,
                ..
            }
        ));
    }

    // Reopen existing DB without re-migration
    #[test]
    fn test_reopen_existing_db() {
        // GIVEN
        let path = temp_db_path();
        let _ = MemoryStore::open(&path).unwrap();
        // WHEN
        let store2 = MemoryStore::open(&path).unwrap();
        // THEN
        assert_eq!(store2.schema_version().unwrap(), SCHEMA_VERSION);
    }

    // Invalid path returns error
    #[test]
    fn test_invalid_path_returns_error() {
        // GIVEN
        let path = PathBuf::from("/nonexistent/dir/that/does/not/exist/memory.db");
        // WHEN
        let result = MemoryStore::open(&path);
        // THEN
        assert!(result.is_err());
    }

    // schema_version() returns current version
    #[test]
    fn test_schema_version_returns_current() {
        // GIVEN
        let path = temp_db_path();
        let store = MemoryStore::open(&path).unwrap();
        // WHEN
        let version = store.schema_version().unwrap();
        // THEN
        assert_eq!(version, SCHEMA_VERSION);
    }

    // delete_entry_by_id deletes episodic entry and FTS
    #[test]
    fn test_delete_entry_by_id_episodic() {
        // GIVEN: an episodic entry
        let path = temp_db_path();
        let store = MemoryStore::open(&path).unwrap();
        let id = "ep-test-001";
        store
            .conn()
            .execute(
                "INSERT INTO episodic_memories (id, namespace, agent_id, content, importance, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![id, "ns", "agent-1", "test content", 0.5, "2026-01-01T00:00:00Z"],
            )
            .unwrap();
        store
            .conn()
            .execute(
                "INSERT INTO memory_fts (content, source_table, source_id) VALUES (?1, ?2, ?3)",
                rusqlite::params!["test content", "episodic", id],
            )
            .unwrap();

        // WHEN
        let deleted = store.delete_entry_by_id(id).unwrap();

        // THEN
        assert!(deleted);
        let count: i64 = store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM episodic_memories WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        let fts_count: i64 = store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM memory_fts WHERE source_id = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fts_count, 0);
    }

    // delete_entry_by_id returns false for nonexistent entry
    #[test]
    fn test_delete_entry_by_id_nonexistent() {
        // GIVEN
        let path = temp_db_path();
        let store = MemoryStore::open(&path).unwrap();

        // WHEN
        let deleted = store.delete_entry_by_id("nonexistent-id").unwrap();

        // THEN
        assert!(!deleted);
    }

    // clear_episodic returns the count of deleted rows
    #[test]
    fn test_clear_episodic_returns_count() {
        // GIVEN 5 episodic entries in namespace "test"
        let path = temp_db_path();
        let store = MemoryStore::open(&path).unwrap();
        for i in 0..5u32 {
            store
                .conn()
                .execute(
                    "INSERT INTO episodic_memories (id, namespace, agent_id, content, importance, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![
                        format!("ep-{i}"),
                        "test",
                        "agent-1",
                        format!("content {i}"),
                        0.5_f64,
                        "2026-01-01T00:00:00Z"
                    ],
                )
                .unwrap();
        }
        // WHEN
        let n = store.clear_episodic("test").unwrap();
        // THEN
        assert_eq!(n, 5);
        let remaining: u64 = store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM episodic_memories WHERE namespace = 'test'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 0);
    }

    // clear_semantic returns the count of deleted rows
    #[test]
    fn test_clear_semantic_returns_count() {
        // GIVEN 3 semantic entries in namespace "test"
        let path = temp_db_path();
        let store = MemoryStore::open(&path).unwrap();
        for i in 0..3u32 {
            store
                .conn()
                .execute(
                    "INSERT INTO semantic_memories (id, namespace, key, value, confidence, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![
                        format!("sem-{i}"),
                        "test",
                        format!("key-{i}"),
                        format!("value-{i}"),
                        1.0_f64,
                        "2026-01-01T00:00:00Z",
                        "2026-01-01T00:00:00Z"
                    ],
                )
                .unwrap();
        }
        // WHEN
        let n = store.clear_semantic("test").unwrap();
        // THEN
        assert_eq!(n, 3);
    }

    // clear_procedural returns the count of deleted rows
    #[test]
    fn test_clear_procedural_returns_count() {
        // GIVEN 2 procedural entries in namespace "test"
        let path = temp_db_path();
        let store = MemoryStore::open(&path).unwrap();
        for i in 0..2u32 {
            store
                .conn()
                .execute(
                    "INSERT INTO procedural_memories (id, namespace, trigger_text, steps, last_used_at, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![
                        format!("proc-{i}"),
                        "test",
                        format!("trigger {i}"),
                        "[]",
                        "2026-01-01T00:00:00Z",
                        "2026-01-01T00:00:00Z"
                    ],
                )
                .unwrap();
        }
        // WHEN
        let n = store.clear_procedural("test").unwrap();
        // THEN
        assert_eq!(n, 2);
    }

    // clear_episodic only affects the given namespace
    #[test]
    fn test_clear_episodic_namespace_isolation() {
        // GIVEN entries in two namespaces
        let path = temp_db_path();
        let store = MemoryStore::open(&path).unwrap();
        for ns in ["ns-a", "ns-b"] {
            store
                .conn()
                .execute(
                    "INSERT INTO episodic_memories (id, namespace, agent_id, content, importance, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![
                        format!("ep-{ns}"),
                        ns,
                        "agent-1",
                        "content",
                        0.5_f64,
                        "2026-01-01T00:00:00Z"
                    ],
                )
                .unwrap();
        }
        // WHEN clear only ns-a
        let n = store.clear_episodic("ns-a").unwrap();
        // THEN ns-a is empty, ns-b untouched
        assert_eq!(n, 1);
        let remaining: u64 = store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM episodic_memories WHERE namespace = 'ns-b'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 1);
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
