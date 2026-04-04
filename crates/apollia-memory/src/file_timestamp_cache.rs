//! Cache des timestamps de fichiers lus par les agents.
//!
//! Persiste le `mtime` de chaque fichier lu par un agent dans une base SQLite
//! dédiée. À chaque accès ultérieur, le `mtime` courant est comparé à la valeur
//! stockée. Si les deux diffèrent, un [`RuntimeEvent::FileModifiedSinceRead`] est
//! émis afin qu'ORIA puisse invalider les entrées de plan cache dépendantes de ce
//! fichier (Principe #4 — Fail fast).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use apollia_core::{EventBusSender, RuntimeEvent};
use rusqlite::Connection;
use thiserror::Error;

/// Errors produced by [`FileTimestampCache`] operations.
#[derive(Debug, Error)]
pub enum FileTimestampCacheError {
    /// The database could not be opened or initialised.
    #[error("failed to open file timestamp cache: {0}")]
    OpenFailed(String),

    /// An SQLite operation failed.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// The filesystem `mtime` for a path could not be read.
    #[error("failed to read mtime for '{path}': {cause}")]
    MtimeReadFailed {
        /// Path whose mtime could not be read.
        path: String,
        /// Underlying I/O error description.
        cause: String,
    },
}

/// A single cached entry recording the last observed `mtime` of a file.
#[derive(Debug, Clone)]
pub struct FileTimestampEntry {
    /// Absolute path of the tracked file.
    pub path: PathBuf,
    /// Last observed modification timestamp in milliseconds since Unix epoch.
    pub mtime_ms: i64,
    /// Timestamp of the last recorded read in milliseconds since Unix epoch.
    pub last_read_at: i64,
}

/// Cache of file read timestamps for detecting stale plan conditions.
///
/// Each call to [`record_read`] compares the file's current `mtime` to the
/// previously stored value. On the first access the entry is created and no
/// event is emitted. On subsequent accesses, if the `mtime` has changed a
/// [`RuntimeEvent::FileModifiedSinceRead`] event is broadcast so that ORIA
/// can invalidate any plan cache entries that depended on the now-stale file.
///
/// The cache stores data in a standalone SQLite database (separate from the
/// agent's memory namespace databases). Pass `Path::new(":memory:")` to use
/// an in-memory database — intended for testing only.
///
/// [`record_read`]: FileTimestampCache::record_read
pub struct FileTimestampCache {
    db: Connection,
    event_tx: EventBusSender,
}

impl FileTimestampCache {
    /// Opens (or creates) the file timestamp cache at `db_path`.
    ///
    /// Initialises the `file_timestamps` table and its index on first use.
    /// Pass `Path::new(":memory:")` for an ephemeral in-memory database.
    ///
    /// # Errors
    ///
    /// Returns [`FileTimestampCacheError::OpenFailed`] if the SQLite database
    /// cannot be opened or the schema cannot be applied.
    pub fn new(db_path: &Path, event_tx: EventBusSender) -> Result<Self, FileTimestampCacheError> {
        let db = Connection::open(db_path)
            .map_err(|e| FileTimestampCacheError::OpenFailed(e.to_string()))?;

        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS file_timestamps (
                 path         TEXT PRIMARY KEY,
                 mtime_ms     INTEGER NOT NULL,
                 last_read_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_file_timestamps_mtime
                 ON file_timestamps(mtime_ms);",
        )
        .map_err(|e| FileTimestampCacheError::OpenFailed(e.to_string()))?;

        tracing::debug!(path = %db_path.display(), "file timestamp cache opened");

        Ok(Self { db, event_tx })
    }

    /// Records a file read and emits [`RuntimeEvent::FileModifiedSinceRead`]
    /// when the file has changed since the last recorded access.
    ///
    /// - **First access** — entry created, no event emitted.
    /// - **Unchanged file** — `last_read_at` updated, no event emitted.
    /// - **Modified file** — event broadcast, then entry updated.
    ///
    /// # Errors
    ///
    /// Returns an error if the filesystem `mtime` cannot be read or if the
    /// SQLite operation fails. The [`FileRead`] integration calls this with
    /// `.ok()` so errors are logged but never propagated to the agent.
    pub async fn record_read(&mut self, path: &Path) -> Result<(), FileTimestampCacheError> {
        let new_mtime_ms = Self::get_mtime_ms(path).await?;
        let now_ms = now_unix_ms();

        if let Some(entry) = self.query_entry(path)? {
            if entry.mtime_ms != new_mtime_ms {
                tracing::info!(
                    path = %path.display(),
                    old_mtime_ms = entry.mtime_ms,
                    new_mtime_ms = new_mtime_ms,
                    "file modified since last read"
                );
                self.event_tx
                    .send(RuntimeEvent::FileModifiedSinceRead {
                        path: path.to_path_buf(),
                        old_mtime_ms: entry.mtime_ms,
                        new_mtime_ms,
                    })
                    .ok();
            }
        }

        self.upsert_entry(path, new_mtime_ms, now_ms)?;
        Ok(())
    }

    /// Returns up to `limit` entries ordered by `mtime_ms` descending.
    ///
    /// # Errors
    ///
    /// Returns [`FileTimestampCacheError::Sqlite`] on database failure.
    pub fn list_entries(
        &self,
        limit: u32,
    ) -> Result<Vec<FileTimestampEntry>, FileTimestampCacheError> {
        let mut stmt = self.db.prepare(
            "SELECT path, mtime_ms, last_read_at
               FROM file_timestamps
              ORDER BY mtime_ms DESC
              LIMIT ?1",
        )?;

        let entries = stmt
            .query_map(rusqlite::params![limit], |row| {
                let path_str: String = row.get(0)?;
                Ok(FileTimestampEntry {
                    path: PathBuf::from(path_str),
                    mtime_ms: row.get(1)?,
                    last_read_at: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(entries)
    }

    /// Removes entries whose paths no longer exist on disk.
    ///
    /// Returns the number of entries deleted.
    ///
    /// # Errors
    ///
    /// Returns [`FileTimestampCacheError::Sqlite`] on database failure.
    pub async fn prune_deleted(&mut self) -> Result<usize, FileTimestampCacheError> {
        let paths: Vec<String> = {
            let mut stmt = self.db.prepare("SELECT path FROM file_timestamps")?;
            let collected = stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            collected
        };

        let mut removed = 0usize;
        for path_str in paths {
            if !Path::new(&path_str).exists() {
                self.db.execute(
                    "DELETE FROM file_timestamps WHERE path = ?1",
                    rusqlite::params![path_str],
                )?;
                removed += 1;
                tracing::debug!(path = %path_str, "pruned deleted file from timestamp cache");
            }
        }

        Ok(removed)
    }

    /// Reads the `mtime` of `path` and converts it to milliseconds since Unix epoch.
    async fn get_mtime_ms(path: &Path) -> Result<i64, FileTimestampCacheError> {
        let metadata = tokio::fs::metadata(path).await.map_err(|e| {
            FileTimestampCacheError::MtimeReadFailed {
                path: path.display().to_string(),
                cause: e.to_string(),
            }
        })?;

        let mtime = metadata
            .modified()
            .map_err(|e| FileTimestampCacheError::MtimeReadFailed {
                path: path.display().to_string(),
                cause: e.to_string(),
            })?;

        let mtime_ms = mtime
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        Ok(mtime_ms)
    }

    /// Queries the cache for an existing entry at `path`.
    fn query_entry(
        &self,
        path: &Path,
    ) -> Result<Option<FileTimestampEntry>, FileTimestampCacheError> {
        let path_str = path.to_string_lossy();

        match self.db.query_row(
            "SELECT path, mtime_ms, last_read_at
               FROM file_timestamps
              WHERE path = ?1",
            rusqlite::params![path_str.as_ref()],
            |row| {
                Ok(FileTimestampEntry {
                    path: PathBuf::from(row.get::<_, String>(0)?),
                    mtime_ms: row.get(1)?,
                    last_read_at: row.get(2)?,
                })
            },
        ) {
            Ok(entry) => Ok(Some(entry)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(FileTimestampCacheError::Sqlite(e)),
        }
    }

    /// Inserts or updates the cache entry for `path`.
    ///
    /// Uses a single UPSERT statement — no separate INSERT + UPDATE.
    fn upsert_entry(
        &self,
        path: &Path,
        mtime_ms: i64,
        now_ms: i64,
    ) -> Result<(), FileTimestampCacheError> {
        let path_str = path.to_string_lossy();

        self.db.execute(
            "INSERT INTO file_timestamps (path, mtime_ms, last_read_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET
                 mtime_ms     = excluded.mtime_ms,
                 last_read_at = excluded.last_read_at",
            rusqlite::params![path_str.as_ref(), mtime_ms, now_ms],
        )?;

        Ok(())
    }

    /// Exposes the underlying connection for test-only inspection.
    #[cfg(test)]
    fn conn(&self) -> &Connection {
        &self.db
    }
}

/// Returns the current time as milliseconds since Unix epoch.
fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;
    use tokio::sync::broadcast;

    fn make_cache() -> (FileTimestampCache, broadcast::Receiver<RuntimeEvent>) {
        let (tx, rx) = broadcast::channel(16);
        let cache = FileTimestampCache::new(Path::new(":memory:"), tx)
            .expect("in-memory cache should open");
        (cache, rx)
    }

    fn create_temp_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let path = dir.path().join(name);
        let mut f = std::fs::File::create(&path).expect("create temp file");
        f.write_all(content.as_bytes()).expect("write temp file");
        path
    }

    #[tokio::test]
    async fn file_timestamp_cache_records_first_read() {
        // GIVEN SQLite in-memory, a real temp file
        let dir = TempDir::new().expect("tmp dir");
        let path = create_temp_file(&dir, "test.txt", "hello");
        let (mut cache, mut rx) = make_cache();

        // WHEN record_read first access
        cache.record_read(&path).await.expect("record_read");

        // THEN entry created, no event emitted
        let entry = cache
            .query_entry(&path)
            .expect("query")
            .expect("entry present");
        assert_eq!(entry.path, path);
        assert!(entry.mtime_ms > 0);

        assert!(
            rx.try_recv().is_err(),
            "no event should be emitted on first read"
        );
    }

    #[tokio::test]
    async fn file_timestamp_cache_detects_modification() {
        // GIVEN a real temp file, first record_read already called
        let dir = TempDir::new().expect("tmp dir");
        let path = create_temp_file(&dir, "test.txt", "initial");
        let (mut cache, mut rx) = make_cache();

        cache.record_read(&path).await.expect("first record_read");

        // Force an older mtime in the DB so the comparison detects a change
        cache
            .conn()
            .execute(
                "UPDATE file_timestamps SET mtime_ms = 1 WHERE path = ?1",
                rusqlite::params![path.to_string_lossy().as_ref()],
            )
            .expect("update mtime");

        // WHEN file is read again (current mtime > 1)
        cache.record_read(&path).await.expect("second record_read");

        // THEN FileModifiedSinceRead emitted with old_mtime=1, new_mtime=current
        match rx.try_recv() {
            Ok(RuntimeEvent::FileModifiedSinceRead {
                path: evpath,
                old_mtime_ms,
                new_mtime_ms,
            }) => {
                assert_eq!(evpath, path);
                assert_eq!(old_mtime_ms, 1);
                assert!(new_mtime_ms > 1);
            }
            other => panic!("expected FileModifiedSinceRead, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn file_timestamp_cache_no_event_when_unmodified() {
        // GIVEN a real temp file, first record_read already called
        let dir = TempDir::new().expect("tmp dir");
        let path = create_temp_file(&dir, "test.txt", "hello");
        let (mut cache, mut rx) = make_cache();

        cache.record_read(&path).await.expect("first record_read");
        // Drain the channel (first read emits no event, but be safe)
        while rx.try_recv().is_ok() {}

        // WHEN record_read called again without touching the file
        cache.record_read(&path).await.expect("second record_read");

        // THEN no event emitted
        assert!(
            rx.try_recv().is_err(),
            "no event expected when file is unmodified"
        );
    }

    #[tokio::test]
    async fn file_timestamp_cache_prune_deleted() {
        // GIVEN an entry for a file that no longer exists
        let dir = TempDir::new().expect("tmp dir");
        let path = create_temp_file(&dir, "ephemeral.txt", "data");
        let (mut cache, _rx) = make_cache();

        cache.record_read(&path).await.expect("record_read");

        // Remove the file from disk
        std::fs::remove_file(&path).expect("remove file");

        // WHEN prune_deleted
        let removed = cache.prune_deleted().await.expect("prune_deleted");

        // THEN entry removed, returns 1
        assert_eq!(removed, 1);
        let count: i64 = cache
            .conn()
            .query_row("SELECT COUNT(*) FROM file_timestamps", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn file_timestamp_cache_list_entries_returns_all() {
        // GIVEN two files recorded
        let dir = TempDir::new().expect("tmp dir");
        let p1 = create_temp_file(&dir, "a.txt", "a");
        let p2 = create_temp_file(&dir, "b.txt", "b");
        let (mut cache, _rx) = make_cache();

        cache.record_read(&p1).await.expect("record p1");
        cache.record_read(&p2).await.expect("record p2");

        // WHEN list_entries(limit=10)
        let entries = cache.list_entries(10).expect("list");

        // THEN both entries returned
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn file_timestamp_cache_prune_keeps_existing_files() {
        // GIVEN two entries: one for an existing file, one for a deleted file
        let dir = TempDir::new().expect("tmp dir");
        let existing = create_temp_file(&dir, "existing.txt", "data");
        let deleted = create_temp_file(&dir, "deleted.txt", "data");
        let (mut cache, _rx) = make_cache();

        cache.record_read(&existing).await.expect("record existing");
        cache.record_read(&deleted).await.expect("record deleted");

        std::fs::remove_file(&deleted).expect("remove deleted");

        // WHEN prune_deleted
        let removed = cache.prune_deleted().await.expect("prune");

        // THEN only the deleted entry is removed
        assert_eq!(removed, 1);
        let remaining = cache.list_entries(10).expect("list");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].path, existing);
    }
}
