//! SQLite repository for persisting STT transcriptions.
//!
//! Provides [`SttRepository`] with full CRUD operations on the
//! `stt_transcriptions` table, and [`TranscriptRow`] as the
//! persistence-layer representation of a transcription.

use std::path::Path;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::types::{SttError, TranscriptResult};

/// Current schema version for the STT transcriptions database.
const SCHEMA_VERSION: u32 = 1;

/// DDL for schema version 1: transcription table + index.
const SCHEMA_V1: &str = "
    CREATE TABLE IF NOT EXISTS _schema_version (
        version INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS stt_transcriptions (
        id                  TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
        full_text           TEXT NOT NULL,
        language            TEXT,
        source              TEXT NOT NULL DEFAULT 'hotkey',
        audio_duration_ms   INTEGER NOT NULL DEFAULT 0,
        processing_time_ms  INTEGER NOT NULL DEFAULT 0,
        model_name          TEXT,
        created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        CHECK (source IN ('hotkey', 'file', 'api'))
    );

    CREATE INDEX IF NOT EXISTS idx_stt_transcriptions_created
        ON stt_transcriptions(created_at DESC);
";

/// A persisted transcription row from the `stt_transcriptions` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptRow {
    /// Unique identifier (hex-encoded random bytes).
    pub id: String,
    /// Full transcribed text.
    pub full_text: String,
    /// Detected or specified language (ISO 639-1 code).
    pub language: Option<String>,
    /// Transcription source: `"hotkey"`, `"file"`, or `"api"`.
    pub source: String,
    /// Duration of the source audio in milliseconds.
    pub audio_duration_ms: i64,
    /// Time spent on transcription processing in milliseconds.
    pub processing_time_ms: i64,
    /// Name of the model used for transcription.
    pub model_name: Option<String>,
    /// ISO 8601 timestamp of when the transcription was created.
    pub created_at: String,
}

/// SQLite-backed repository for STT transcriptions.
///
/// Owns a single [`Connection`] to a `.db` file. On [`open`](Self::open),
/// the schema is created or migrated to the current version.
pub struct SttRepository {
    conn: Connection,
}

impl SttRepository {
    /// Opens (or creates) the SQLite database at `path` and applies migrations.
    ///
    /// Enables WAL journal mode for concurrent-read performance, then ensures
    /// the schema is up to date with [`SCHEMA_VERSION`].
    pub fn open(path: &Path) -> Result<Self, SttError> {
        let conn = Connection::open(path).map_err(|e| SttError::Repository {
            reason: format!("failed to open database: {e}"),
        })?;

        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| SttError::Repository {
                reason: format!("failed to enable WAL: {e}"),
            })?;

        let current_version = Self::read_schema_version(&conn)?;

        if current_version > SCHEMA_VERSION {
            // Opening anyway could misread or destroy rows written by the
            // newer binary, so surface the refusal instead of migrating.
            return Err(SttError::NewerThanBinary {
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
            "stt.repository.opened"
        );

        Ok(Self { conn })
    }

    /// Persists a transcription and returns its generated ID.
    ///
    /// The `source` parameter must be one of `"hotkey"`, `"file"`, or `"api"`.
    /// The `model_name` is stored alongside the result for traceability.
    pub fn insert(
        &self,
        source: &str,
        result: &TranscriptResult,
        model_name: Option<&str>,
    ) -> Result<String, SttError> {
        self.conn
            .execute(
                "INSERT INTO stt_transcriptions (full_text, language, source, audio_duration_ms, processing_time_ms, model_name)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    result.full_text,
                    result.language,
                    source,
                    result.audio_duration_ms as i64,
                    result.processing_time_ms as i64,
                    model_name,
                ],
            )
            .map_err(|e| SttError::Repository {
                reason: format!("insert failed: {e}"),
            })?;

        let id: String = self
            .conn
            .query_row(
                "SELECT id FROM stt_transcriptions WHERE rowid = last_insert_rowid()",
                [],
                |row| row.get(0),
            )
            .map_err(|e| SttError::Repository {
                reason: format!("failed to retrieve inserted id: {e}"),
            })?;

        tracing::info!(transcript_id = %id, source = %source, "stt.transcription.persisted");

        Ok(id)
    }

    /// Returns transcriptions ordered by `created_at DESC` with pagination.
    pub fn list(&self, limit: u32, offset: u32) -> Result<Vec<TranscriptRow>, SttError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, full_text, language, source, audio_duration_ms, processing_time_ms, model_name, created_at
                 FROM stt_transcriptions
                 ORDER BY created_at DESC
                 LIMIT ?1 OFFSET ?2",
            )
            .map_err(|e| SttError::Repository {
                reason: format!("list prepare failed: {e}"),
            })?;

        let rows = stmt
            .query_map(rusqlite::params![limit, offset], |row| {
                Ok(TranscriptRow {
                    id: row.get(0)?,
                    full_text: row.get(1)?,
                    language: row.get(2)?,
                    source: row.get(3)?,
                    audio_duration_ms: row.get(4)?,
                    processing_time_ms: row.get(5)?,
                    model_name: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })
            .map_err(|e| SttError::Repository {
                reason: format!("list query failed: {e}"),
            })?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row.map_err(|e| SttError::Repository {
                reason: format!("list row read failed: {e}"),
            })?);
        }

        Ok(entries)
    }

    /// Returns a single transcription by ID, or `None` if not found.
    pub fn get(&self, id: &str) -> Result<Option<TranscriptRow>, SttError> {
        match self.conn.query_row(
            "SELECT id, full_text, language, source, audio_duration_ms, processing_time_ms, model_name, created_at
             FROM stt_transcriptions
             WHERE id = ?1",
            rusqlite::params![id],
            |row| {
                Ok(TranscriptRow {
                    id: row.get(0)?,
                    full_text: row.get(1)?,
                    language: row.get(2)?,
                    source: row.get(3)?,
                    audio_duration_ms: row.get(4)?,
                    processing_time_ms: row.get(5)?,
                    model_name: row.get(6)?,
                    created_at: row.get(7)?,
                })
            },
        ) {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(SttError::Repository {
                reason: format!("get failed: {e}"),
            }),
        }
    }

    /// Returns transcriptions whose `created_at` falls within `[from_iso8601, to_iso8601]`.
    ///
    /// Both bounds are inclusive ISO 8601 strings (e.g. `"2026-01-01T00:00:00.000Z"`).
    /// Results are ordered by `created_at DESC`, capped to `limit` rows.
    pub fn search_by_date_range(
        &self,
        from_iso8601: &str,
        to_iso8601: &str,
        limit: u32,
    ) -> Result<Vec<TranscriptRow>, SttError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, full_text, language, source, audio_duration_ms, processing_time_ms, model_name, created_at
                 FROM stt_transcriptions
                 WHERE created_at >= ?1 AND created_at <= ?2
                 ORDER BY created_at DESC
                 LIMIT ?3",
            )
            .map_err(|e| SttError::Repository {
                reason: format!("search_by_date_range prepare failed: {e}"),
            })?;

        let rows = stmt
            .query_map(rusqlite::params![from_iso8601, to_iso8601, limit], |row| {
                Ok(TranscriptRow {
                    id: row.get(0)?,
                    full_text: row.get(1)?,
                    language: row.get(2)?,
                    source: row.get(3)?,
                    audio_duration_ms: row.get(4)?,
                    processing_time_ms: row.get(5)?,
                    model_name: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })
            .map_err(|e| SttError::Repository {
                reason: format!("search_by_date_range query failed: {e}"),
            })?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row.map_err(|e| SttError::Repository {
                reason: format!("search_by_date_range row read failed: {e}"),
            })?);
        }

        Ok(entries)
    }

    /// Deletes a transcription by ID. No-op if the ID does not exist.
    pub fn delete(&self, id: &str) -> Result<(), SttError> {
        self.conn
            .execute(
                "DELETE FROM stt_transcriptions WHERE id = ?1",
                rusqlite::params![id],
            )
            .map_err(|e| SttError::Repository {
                reason: format!("delete failed: {e}"),
            })?;

        Ok(())
    }

    /// Reads the current schema version, returning 0 if the table does not exist.
    fn read_schema_version(conn: &Connection) -> Result<u32, SttError> {
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='_schema_version'",
                [],
                |row| row.get(0),
            )
            .map_err(|e| SttError::Repository {
                reason: format!("schema version check failed: {e}"),
            })?;

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
    fn apply_migrations(conn: &Connection, current_version: u32) -> Result<(), SttError> {
        if current_version < 1 {
            Self::migrate_to_v1(conn)?;
        }
        Ok(())
    }

    /// Migration to schema version 1: creates the transcriptions table and index.
    fn migrate_to_v1(conn: &Connection) -> Result<(), SttError> {
        conn.execute_batch(SCHEMA_V1)
            .map_err(|e| SttError::Repository {
                reason: format!("migration to v1 failed: {e}"),
            })?;

        let existing: i64 = conn
            .query_row("SELECT COUNT(*) FROM _schema_version", [], |row| row.get(0))
            .map_err(|e| SttError::Repository {
                reason: format!("schema version seed failed: {e}"),
            })?;

        if existing == 0 {
            conn.execute(
                "INSERT INTO _schema_version (version) VALUES (?1)",
                rusqlite::params![SCHEMA_VERSION],
            )
            .map_err(|e| SttError::Repository {
                reason: format!("schema version insert failed: {e}"),
            })?;
        } else {
            conn.execute(
                "UPDATE _schema_version SET version = ?1",
                rusqlite::params![SCHEMA_VERSION],
            )
            .map_err(|e| SttError::Repository {
                reason: format!("schema version update failed: {e}"),
            })?;
        }

        tracing::info!(version = SCHEMA_VERSION, "stt.schema.migrated");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TranscriptResult;
    use std::path::PathBuf;

    /// Opens an isolated in-memory SQLite repository for test isolation and performance.
    fn open_in_memory() -> SttRepository {
        SttRepository::open(Path::new(":memory:")).expect("in-memory db should open")
    }

    fn setup() -> (SttRepository, PathBuf) {
        let dir = std::env::temp_dir().join(format!("apollia_stt_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("test_{}.db", uuid::Uuid::new_v4()));
        let repo = SttRepository::open(&path).unwrap();
        (repo, path)
    }

    // GIVEN a database stamped by a newer binary
    // WHEN opening it
    // THEN the open is refused instead of silently accepted
    #[test]
    fn test_newer_database_is_refused() {
        let dir = std::env::temp_dir().join(format!("apollia_stt_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("newer_{}.db", uuid::Uuid::new_v4()));
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(SCHEMA_V1).unwrap();
            conn.execute(
                "INSERT INTO _schema_version (version) VALUES (?1)",
                rusqlite::params![SCHEMA_VERSION + 1],
            )
            .unwrap();
        }

        let err = SttRepository::open(&path).map(|_| ()).unwrap_err();

        assert!(matches!(
            err,
            SttError::NewerThanBinary {
                supported: SCHEMA_VERSION,
                ..
            }
        ));
    }

    fn sample_result() -> TranscriptResult {
        TranscriptResult {
            full_text: "Bonjour le monde".to_owned(),
            segments: vec![],
            language: Some("fr".to_owned()),
            audio_duration_ms: 2000,
            processing_time_ms: 350,
        }
    }

    // GIVEN a fresh database
    // WHEN open is called
    // THEN the schema is created and version is 1
    #[test]
    fn open_creates_schema() {
        let (repo, _path) = setup();
        let version = SttRepository::read_schema_version(&repo.conn).unwrap();
        assert_eq!(version, 1);
    }

    // GIVEN a repository with no transcriptions
    // WHEN insert is called with a valid source and result
    // THEN it returns an ID and the row is retrievable via get
    #[test]
    fn insert_and_get() {
        let (repo, _path) = setup();
        let result = sample_result();

        let id = repo
            .insert("hotkey", &result, Some("whisper-large-v3"))
            .unwrap();
        assert!(!id.is_empty());

        let row = repo.get(&id).unwrap().expect("row should exist");
        assert_eq!(row.full_text, "Bonjour le monde");
        assert_eq!(row.language.as_deref(), Some("fr"));
        assert_eq!(row.source, "hotkey");
        assert_eq!(row.audio_duration_ms, 2000);
        assert_eq!(row.processing_time_ms, 350);
        assert_eq!(row.model_name.as_deref(), Some("whisper-large-v3"));
    }

    // GIVEN multiple inserted transcriptions
    // WHEN list is called with limit and offset
    // THEN the results are paginated and ordered by created_at DESC
    #[test]
    fn list_with_pagination() {
        let (repo, _path) = setup();

        let mut ids = Vec::new();
        for i in 0..5 {
            let mut result = sample_result();
            result.full_text = format!("Transcription {i}");
            let id = repo.insert("file", &result, None).unwrap();
            ids.push(id);
        }

        let page1 = repo.list(2, 0).unwrap();
        assert_eq!(page1.len(), 2);

        let page2 = repo.list(2, 2).unwrap();
        assert_eq!(page2.len(), 2);

        let page3 = repo.list(2, 4).unwrap();
        assert_eq!(page3.len(), 1);

        let all = repo.list(100, 0).unwrap();
        assert_eq!(all.len(), 5);
    }

    // GIVEN an inserted transcription
    // WHEN delete is called with its ID
    // THEN get returns None for that ID
    #[test]
    fn delete_removes_row() {
        let (repo, _path) = setup();
        let result = sample_result();

        let id = repo.insert("api", &result, None).unwrap();
        assert!(repo.get(&id).unwrap().is_some());

        repo.delete(&id).unwrap();
        assert!(repo.get(&id).unwrap().is_none());
    }

    // GIVEN no transcription with a given ID
    // WHEN delete is called with that ID
    // THEN it succeeds silently
    #[test]
    fn delete_nonexistent_is_silent() {
        let (repo, _path) = setup();
        let result = repo.delete("nonexistent-id");
        assert!(result.is_ok());
    }

    // GIVEN no transcription with a given ID
    // WHEN get is called with that ID
    // THEN it returns None
    #[test]
    fn get_nonexistent_returns_none() {
        let (repo, _path) = setup();
        let row = repo.get("nonexistent-id").unwrap();
        assert!(row.is_none());
    }

    // GIVEN an empty in-memory repository
    // WHEN list is called
    // THEN an empty vector is returned without error
    #[test]
    fn test_repository_list_empty_returns_empty_vec() {
        // GIVEN
        let repo = open_in_memory();

        // WHEN
        let rows = repo.list(100, 0).expect("list on empty db should succeed");

        // THEN
        assert!(
            rows.is_empty(),
            "expected empty list, got {} rows",
            rows.len()
        );
    }

    // GIVEN 3 transcriptions inserted into an in-memory repository
    // WHEN search_by_date_range spans the current epoch (2026 → 2099)
    // THEN all 3 rows are returned
    #[test]
    fn test_repository_search_by_date_range_all_rows() {
        // GIVEN
        let repo = open_in_memory();
        let result = sample_result();
        for _ in 0..3 {
            repo.insert("hotkey", &result, None)
                .expect("insert should succeed");
        }

        // WHEN
        let rows = repo
            .search_by_date_range("2026-01-01T00:00:00.000Z", "2099-12-31T23:59:59.999Z", 100)
            .expect("search should succeed");

        // THEN
        assert_eq!(rows.len(), 3);
    }

    // GIVEN transcriptions inserted in the current epoch
    // WHEN search_by_date_range uses a past range ending in 2020
    // THEN the result is empty
    #[test]
    fn test_repository_search_empty_result() {
        // GIVEN
        let repo = open_in_memory();
        let result = sample_result();
        repo.insert("file", &result, None)
            .expect("insert should succeed");

        // WHEN
        let rows = repo
            .search_by_date_range("2000-01-01T00:00:00.000Z", "2020-12-31T23:59:59.999Z", 100)
            .expect("search with empty result should succeed");

        // THEN
        assert!(rows.is_empty());
    }

    // GIVEN 10 transcriptions inserted in an in-memory repository
    // WHEN list is called with limit 5 and then offset 5
    // THEN each page returns 5 distinct rows covering all 10 entries
    #[test]
    fn test_repository_pagination() {
        // GIVEN
        let repo = open_in_memory();
        let mut result = sample_result();
        for i in 0..10 {
            result.full_text = format!("Transcription {i}");
            repo.insert("api", &result, None)
                .expect("insert should succeed");
        }

        // WHEN
        let page1 = repo.list(5, 0).expect("page 1 should succeed");
        let page2 = repo.list(5, 5).expect("page 2 should succeed");

        // THEN
        assert_eq!(page1.len(), 5);
        assert_eq!(page2.len(), 5);

        let mut all_ids: Vec<_> = page1
            .iter()
            .chain(page2.iter())
            .map(|r| r.id.clone())
            .collect();
        all_ids.sort();
        all_ids.dedup();
        assert_eq!(
            all_ids.len(),
            10,
            "all 10 IDs must be distinct across pages"
        );
    }

    // GIVEN an in-memory repository with all 3 valid source types inserted
    // WHEN each row is retrieved by ID
    // THEN the source field is stored and returned correctly
    #[test]
    fn test_repository_all_source_types_accepted() {
        // GIVEN
        let repo = open_in_memory();
        let result = sample_result();

        // WHEN / THEN
        for source in &["hotkey", "file", "api"] {
            let id = repo
                .insert(source, &result, None)
                .expect("insert should succeed");
            let row = repo
                .get(&id)
                .expect("get should succeed")
                .expect("row should exist");
            assert_eq!(&row.source, source, "source mismatch for '{source}'");
        }
    }
}
