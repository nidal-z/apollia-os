//! Reversible filesystem mutation journal.
//!
//! Tokio mpsc actor per session. Writes a JSONL entry before each native
//! filesystem mutation; the mutation only proceeds once the entry is durably
//! persisted (fsync). Enforces non-bypassable guardrails.
//!
//! One [`JournalWriter`] actor is spawned per chat session. Its clonable
//! [`JournalWriterHandle`] is injected into [`FileWrite`](crate::tools::file_write::FileWrite)
//! and [`FileEdit`](crate::tools::file_edit::FileEdit) at construction time.
//!
//! Layout on disk: `<root>/<session_id>/YYYY-MM-DD.jsonl`

use std::path::{Path, PathBuf};

use tokio::io::AsyncWriteExt as _;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, warn};

// ────────────────────────────────────────────────────────────────────────────
// Errors
// ────────────────────────────────────────────────────────────────────────────

/// Errors produced by the journal actor and [`rollback_session`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum JournalError {
    /// Underlying I/O failure while reading or writing a journal file.
    #[error("journal I/O failure: {0}")]
    Io(#[from] std::io::Error),

    /// The journal actor has already stopped; its channel is closed.
    #[error("journal actor stopped")]
    ActorStopped,

    /// No session directory found for the given session identifier.
    #[error("session '{0}' not found")]
    SessionNotFound(String),

    /// A JSONL line could not be deserialized.
    #[error("journal entry corrupt: {0}")]
    Corrupt(String),
}

// ────────────────────────────────────────────────────────────────────────────
// JournalEntry
// ────────────────────────────────────────────────────────────────────────────

/// A single reversible filesystem mutation, serialized as one JSONL line.
///
/// Each variant captures enough state to reconstruct the pre-mutation
/// condition, enabling [`rollback_session`] to replay inverses.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum JournalEntry {
    /// A file was created or overwritten.
    Write {
        /// Absolute path of the mutated file.
        path: PathBuf,
        /// Content before the write, `None` if the file did not previously exist.
        previous_content: Option<Vec<u8>>,
        /// Unix permission bits before the write (`None` on non-unix or new file).
        previous_mode: Option<u32>,
        /// Modification time as Unix epoch seconds before the write.
        previous_mtime: Option<i64>,
    },
    /// A file was deleted.
    Delete {
        /// Absolute path of the deleted file.
        path: PathBuf,
        /// Full content of the file before deletion.
        previous_content: Vec<u8>,
        /// Unix permission bits of the file before deletion.
        previous_mode: u32,
    },
    /// A directory was created.
    Mkdir {
        /// Absolute path of the created directory.
        path: PathBuf,
    },
    /// A directory was removed.
    Rmdir {
        /// Absolute path of the removed directory.
        path: PathBuf,
    },
    /// File permissions were changed.
    Chmod {
        /// Absolute path of the file whose permissions changed.
        path: PathBuf,
        /// Permission bits before the chmod.
        previous_mode: u32,
    },
}

// ────────────────────────────────────────────────────────────────────────────
// Internal actor message
// ────────────────────────────────────────────────────────────────────────────

enum Message {
    Record {
        entry: JournalEntry,
        reply: oneshot::Sender<Result<(), JournalError>>,
    },
    Shutdown,
}

// ────────────────────────────────────────────────────────────────────────────
// JournalWriterHandle
// ────────────────────────────────────────────────────────────────────────────

/// Clonable front-end to a per-session [`JournalWriter`] actor.
///
/// `record` blocks until the entry is fsync'd to disk. The corresponding
/// filesystem mutation **must** only proceed once `record` returns `Ok(())`.
#[derive(Clone, Debug)]
pub struct JournalWriterHandle {
    tx: mpsc::Sender<Message>,
    /// The session identifier this handle belongs to.
    pub session_id: String,
}

impl JournalWriterHandle {
    /// Persist a journal entry durably before the associated mutation.
    ///
    /// Returns `Err` if the actor has stopped or the write failed. The
    /// caller **must abort** the pending mutation on error.
    pub async fn record(&self, entry: JournalEntry) -> Result<(), JournalError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Message::Record {
                entry,
                reply: reply_tx,
            })
            .await
            .map_err(|_| JournalError::ActorStopped)?;
        reply_rx.await.map_err(|_| JournalError::ActorStopped)?
    }

    /// Signal the actor to drain its queue and terminate.
    ///
    /// Pending `record` calls already in flight complete normally.
    /// Subsequent calls to `record` return [`JournalError::ActorStopped`].
    pub async fn shutdown(&self) {
        let _ = self.tx.send(Message::Shutdown).await;
    }
}

// ────────────────────────────────────────────────────────────────────────────
// JournalWriter actor
// ────────────────────────────────────────────────────────────────────────────

/// Tokio actor that serializes journal entries for one session.
///
/// Entries are appended to `<root>/<session_id>/YYYY-MM-DD.jsonl` and
/// fsync'd before the actor replies to the caller. Spawned via
/// [`JournalWriter::spawn`]; interacted with via [`JournalWriterHandle`].
pub struct JournalWriter {
    session_id: String,
    root: PathBuf,
}

impl JournalWriter {
    /// Spawn the actor background task and return a clonable handle.
    ///
    /// Oldest session directories under `root` are purged (by `mtime`) before
    /// the new session directory is created when the total count would exceed
    /// `max_sessions`.
    pub fn spawn(session_id: String, root: PathBuf, max_sessions: usize) -> JournalWriterHandle {
        let (tx, rx) = mpsc::channel::<Message>(64);
        let writer = JournalWriter {
            session_id: session_id.clone(),
            root: root.clone(),
        };
        tokio::spawn(async move {
            if let Err(e) = purge_oldest_sessions(&root, max_sessions).await {
                warn!(
                    root = %root.display(),
                    error = %e,
                    "journal.retention.purge.failed"
                );
            }
            writer.run(rx).await;
        });
        JournalWriterHandle { tx, session_id }
    }

    async fn run(self, mut rx: mpsc::Receiver<Message>) {
        while let Some(msg) = rx.recv().await {
            match msg {
                Message::Record { entry, reply } => {
                    let result = self.append_entry(&entry).await;
                    if let Err(ref e) = result {
                        error!(
                            session_id = %self.session_id,
                            error = %e,
                            detail = "the caller must abort the mutation",
                            "journal.write.failed"
                        );
                    }
                    let _ = reply.send(result);
                }
                Message::Shutdown => break,
            }
        }
    }

    async fn append_entry(&self, entry: &JournalEntry) -> Result<(), JournalError> {
        let session_dir = self.root.join(&self.session_id);
        tokio::fs::create_dir_all(&session_dir).await?;

        let today = current_date_str();
        let log_path = session_dir.join(format!("{today}.jsonl"));

        let line =
            serde_json::to_string(entry).map_err(|e| std::io::Error::other(e.to_string()))?;

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .await?;

        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;
        // fsync: the entry must be durable before the actor replies and the
        // caller proceeds with the mutation.
        file.sync_all().await?;

        Ok(())
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Date helpers (no external crate dependency)
// ────────────────────────────────────────────────────────────────────────────

/// Return today's date as `YYYY-MM-DD` using the proleptic Gregorian calendar.
///
/// Uses Howard Hinnant's `civil_from_days` algorithm (public domain).
fn current_date_str() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    epoch_days_to_date_str(secs / 86400)
}

/// Convert days since Unix epoch (1970-01-01) to a `YYYY-MM-DD` string.
///
/// Uses Howard Hinnant's `civil_from_days` algorithm.
fn epoch_days_to_date_str(days: u64) -> String {
    let z = days as i64 + 719_468_i64;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

// ────────────────────────────────────────────────────────────────────────────
// Retention
// ────────────────────────────────────────────────────────────────────────────

/// Purge the oldest session directories under `root` so that at most
/// `max_sessions - 1` remain before the caller creates a new one.
async fn purge_oldest_sessions(root: &Path, max_sessions: usize) -> Result<(), JournalError> {
    if max_sessions == 0 {
        return Ok(());
    }

    let mut sessions: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    let mut dir = match tokio::fs::read_dir(root).await {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(JournalError::Io(e)),
    };

    while let Some(entry) = dir.next_entry().await? {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Ok(meta) = entry.metadata().await {
            if let Ok(modified) = meta.modified() {
                sessions.push((modified, path));
            }
        }
    }

    // Oldest-first
    sessions.sort_by_key(|(t, _)| *t);

    let keep = max_sessions.saturating_sub(1);
    let to_remove = sessions.len().saturating_sub(keep);

    for (_, path) in sessions.into_iter().take(to_remove) {
        if let Err(e) = tokio::fs::remove_dir_all(&path).await {
            warn!(
                path = %path.display(),
                error = %e,
                "journal.session.remove.failed"
            );
        } else {
            info!(path = %path.display(), "journal.session.purged");
        }
    }

    Ok(())
}

// ────────────────────────────────────────────────────────────────────────────
// rollback_session
// ────────────────────────────────────────────────────────────────────────────

/// Read all journal entries for `session_id` and replay their inverses.
///
/// Entries are applied in reverse chronological order (last mutation first).
///
/// When `dry_run` is `true`, the filesystem is not modified; the function
/// returns the list of entries that *would* be reverted. When `false`, each
/// inverse is applied and the session directory is renamed to
/// `<session_id>.rolled_back` to prevent double-rollback.
///
/// # Errors
///
/// Returns [`JournalError::SessionNotFound`] if no directory exists for
/// `session_id`. Returns [`JournalError::Io`] on any filesystem failure
/// during replay; the remaining entries are **not** applied in that case.
pub async fn rollback_session(
    root: &Path,
    session_id: &str,
    dry_run: bool,
) -> Result<Vec<JournalEntry>, JournalError> {
    let session_dir = root.join(session_id);

    if !session_dir.exists() {
        return Err(JournalError::SessionNotFound(session_id.to_string()));
    }

    let entries = read_session_entries(&session_dir).await?;

    if dry_run {
        return Ok(entries);
    }

    // Apply inverses in reverse order (last mutation first)
    for entry in entries.iter().rev() {
        apply_inverse(entry).await?;
    }

    // Archive to prevent double-rollback
    let archived = root.join(format!("{session_id}.rolled_back"));
    tokio::fs::rename(&session_dir, &archived).await?;

    info!(
        session_id = %session_id,
        ops = entries.len(),
        "journal.session.rolled_back"
    );

    Ok(entries)
}

/// List all session identifiers in the journal root, sorted by modification
/// time (newest first).
///
/// Excludes `.rolled_back` archives.
pub async fn list_sessions(root: &Path) -> Result<Vec<String>, JournalError> {
    let mut sessions: Vec<(std::time::SystemTime, String)> = Vec::new();

    let mut dir = match tokio::fs::read_dir(root).await {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(JournalError::Io(e)),
    };

    while let Some(entry) = dir.next_entry().await? {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        // Skip rolled_back archives
        if name.ends_with(".rolled_back") {
            continue;
        }
        if let Ok(meta) = entry.metadata().await {
            if let Ok(modified) = meta.modified() {
                sessions.push((modified, name));
            }
        }
    }

    // Newest-first
    sessions.sort_by_key(|b| std::cmp::Reverse(b.0));
    Ok(sessions.into_iter().map(|(_, n)| n).collect())
}

/// Read and deserialize all entries from a session directory in chronological
/// order (file names sorted lexicographically, lines in append order).
async fn read_session_entries(session_dir: &Path) -> Result<Vec<JournalEntry>, JournalError> {
    let mut file_names: Vec<String> = Vec::new();
    let mut dir = tokio::fs::read_dir(session_dir).await?;
    while let Some(e) = dir.next_entry().await? {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.ends_with(".jsonl") {
            file_names.push(name);
        }
    }
    file_names.sort();

    let mut entries = Vec::new();
    for name in &file_names {
        let path = session_dir.join(name);
        let content = tokio::fs::read_to_string(&path).await?;
        for (line_num, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let entry: JournalEntry = serde_json::from_str(line).map_err(|e| {
                JournalError::Corrupt(format!("{}:{}: {e}", path.display(), line_num + 1))
            })?;
            entries.push(entry);
        }
    }

    Ok(entries)
}

/// Apply the inverse of one journal entry to the live filesystem.
async fn apply_inverse(entry: &JournalEntry) -> Result<(), JournalError> {
    match entry {
        JournalEntry::Write {
            path,
            previous_content,
            previous_mode,
            ..
        } => match previous_content {
            Some(content) => {
                // File existed before, restore previous content
                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                tokio::fs::write(path, content).await?;
                restore_mode(path, *previous_mode).await?;
            }
            None => {
                // File did not exist before, remove it
                if path.exists() {
                    tokio::fs::remove_file(path).await?;
                }
            }
        },

        JournalEntry::Delete {
            path,
            previous_content,
            previous_mode,
        } => {
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(path, previous_content).await?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                let perms = std::fs::Permissions::from_mode(*previous_mode);
                tokio::fs::set_permissions(path, perms).await?;
            }
            #[cfg(not(unix))]
            let _ = previous_mode;
        }

        JournalEntry::Mkdir { path } => {
            // Inverse of mkdir: remove the directory (only if empty)
            if path.exists() {
                tokio::fs::remove_dir(path).await?;
            }
        }

        JournalEntry::Rmdir { path } => {
            // Inverse of rmdir: recreate the directory
            tokio::fs::create_dir_all(path).await?;
        }

        JournalEntry::Chmod {
            path,
            previous_mode,
        } => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                let perms = std::fs::Permissions::from_mode(*previous_mode);
                tokio::fs::set_permissions(path, perms).await?;
            }
            #[cfg(not(unix))]
            let _ = (path, previous_mode);
        }
    }

    Ok(())
}

/// Restore Unix permission bits on `path` if a mode is provided.
///
/// No-op on non-unix platforms.
async fn restore_mode(path: &Path, mode: Option<u32>) -> Result<(), JournalError> {
    #[cfg(unix)]
    if let Some(m) = mode {
        use std::os::unix::fs::PermissionsExt as _;
        let perms = std::fs::Permissions::from_mode(m);
        tokio::fs::set_permissions(path, perms).await?;
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
    Ok(())
}

// ────────────────────────────────────────────────────────────────────────────
// Platform helpers
// ────────────────────────────────────────────────────────────────────────────

/// Extract Unix permission bits from metadata. Returns `None` on non-unix.
#[cfg(unix)]
pub fn mode_from_metadata(meta: &std::fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::MetadataExt as _;
    Some(meta.mode())
}

/// Extract Unix permission bits from metadata. Returns `None` on non-unix.
#[cfg(not(unix))]
pub fn mode_from_metadata(_meta: &std::fs::Metadata) -> Option<u32> {
    None
}

/// Extract modification time as Unix epoch seconds from metadata.
pub fn mtime_from_metadata(meta: &std::fs::Metadata) -> Option<i64> {
    meta.modified()
        .ok()
        .and_then(|t: std::time::SystemTime| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d: std::time::Duration| d.as_secs() as i64)
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // Helper: spawn a writer, write one entry, await actor processing.
    async fn spawn_and_record(
        root: &Path,
        session_id: &str,
        entry: JournalEntry,
    ) -> JournalWriterHandle {
        let handle = JournalWriter::spawn(session_id.to_string(), root.to_path_buf(), 50);
        handle.record(entry).await.expect("record must succeed");
        handle
    }

    // ── epoch_days_to_date_str ───────────────────────────────────────────────

    #[test]
    fn date_str_epoch_zero_is_1970_01_01() {
        // GIVEN days = 0
        // WHEN
        let s = epoch_days_to_date_str(0);
        // THEN
        assert_eq!(s, "1970-01-01");
    }

    #[test]
    fn date_str_known_date_2024_01_15() {
        // GIVEN 2024-01-15 = 19737 days since epoch
        // WHEN
        let s = epoch_days_to_date_str(19737);
        // THEN
        assert_eq!(s, "2024-01-15");
    }

    // record_write creates a JSONL entry ─────────────────────────────────────

    #[tokio::test]
    async fn test_record_write_creates_entry() {
        // GIVEN a JournalWriter for session "sess-ac1"
        let dir = TempDir::new().expect("tmpdir");
        let entry = JournalEntry::Write {
            path: PathBuf::from("/tmp/test.txt"),
            previous_content: Some(b"old".to_vec()),
            previous_mode: Some(0o644),
            previous_mtime: Some(0),
        };

        // WHEN record is called
        let handle = spawn_and_record(dir.path(), "sess-ac1", entry).await;
        handle.shutdown().await;

        // THEN a *.jsonl file exists inside sess-ac1/
        let session_dir = dir.path().join("sess-ac1");
        let entries = read_session_entries(&session_dir).await.expect("read ok");
        assert_eq!(entries.len(), 1, "one entry expected");
        match &entries[0] {
            JournalEntry::Write {
                path,
                previous_content,
                ..
            } => {
                assert_eq!(path, &PathBuf::from("/tmp/test.txt"));
                assert_eq!(previous_content.as_deref(), Some(b"old".as_ref()));
            }
            other => panic!("unexpected entry: {other:?}"),
        }
    }

    // rollback restores previous content ──────────────────────────────────────

    #[tokio::test]
    async fn test_rollback_restores_previous_content() {
        // GIVEN a file at known_path with "old" content, then overwritten with "new"
        let dir = TempDir::new().expect("tmpdir");
        let known_path = dir.path().join("test.txt");
        tokio::fs::write(&known_path, b"new")
            .await
            .expect("write new");

        let journal_root = dir.path().join("journal");
        let entry = JournalEntry::Write {
            path: known_path.clone(),
            previous_content: Some(b"old".to_vec()),
            previous_mode: None,
            previous_mtime: None,
        };
        let handle = spawn_and_record(&journal_root, "sess-ac2", entry).await;
        handle.shutdown().await;

        // WHEN rollback is applied
        rollback_session(&journal_root, "sess-ac2", false)
            .await
            .expect("rollback ok");

        // THEN the file is restored to "old"
        let restored = tokio::fs::read(&known_path).await.expect("read restored");
        assert_eq!(restored, b"old");

        // AND the session dir is archived
        assert!(!journal_root.join("sess-ac2").exists());
        assert!(journal_root.join("sess-ac2.rolled_back").exists());
    }

    // dry-run does not mutate filesystem or archive session ────────────────────

    #[tokio::test]
    async fn test_rollback_dry_run_no_mutation() {
        // GIVEN a file with "new" content and a journal recording previous "old"
        let dir = TempDir::new().expect("tmpdir");
        let file_path = dir.path().join("test.txt");
        tokio::fs::write(&file_path, b"new").await.expect("write");

        let journal_root = dir.path().join("journal");
        let entry = JournalEntry::Write {
            path: file_path.clone(),
            previous_content: Some(b"old".to_vec()),
            previous_mode: None,
            previous_mtime: None,
        };
        let handle = spawn_and_record(&journal_root, "sess-ac3", entry).await;
        handle.shutdown().await;

        // WHEN dry-run rollback
        let ops = rollback_session(&journal_root, "sess-ac3", true)
            .await
            .expect("dry-run ok");

        // THEN the file still contains "new"
        let content = tokio::fs::read(&file_path).await.expect("read");
        assert_eq!(content, b"new");

        // AND the journal is intact (not archived)
        assert!(journal_root.join("sess-ac3").exists());
        assert!(!journal_root.join("sess-ac3.rolled_back").exists());

        // AND the ops list describes the write entry
        assert_eq!(ops.len(), 1);
    }

    // journal I/O failure aborts mutation ──────────────────────────────────────
    // Tested in file_write.rs integration tests (requires chmod which is
    // platform-specific; a unit-level test would be environment-dependent).

    // retention purges oldest session ──────────────────────────────────────────

    #[tokio::test]
    async fn test_retention_purges_oldest_session() {
        // GIVEN 51 existing session directories
        let dir = TempDir::new().expect("tmpdir");
        let journal_root = dir.path().join("journal");
        tokio::fs::create_dir_all(&journal_root)
            .await
            .expect("mkdir");

        for i in 0..51usize {
            let session_dir = journal_root.join(format!("sess-old-{i:02}"));
            tokio::fs::create_dir_all(&session_dir)
                .await
                .expect("mkdir");
            // Touch with a staggered mtime using a marker file
            let marker = session_dir.join("marker");
            tokio::fs::write(&marker, format!("{i}"))
                .await
                .expect("write");
            // Small sleep to differentiate mtimes
            tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
        }

        // WHEN a new session is spawned with max_sessions=50
        let handle = JournalWriter::spawn("sess-new".to_string(), journal_root.clone(), 50);
        // Give the actor time to purge before recording
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        handle
            .record(JournalEntry::Mkdir {
                path: PathBuf::from("/tmp/x"),
            })
            .await
            .expect("record");
        handle.shutdown().await;

        // THEN at most 50 sessions exist (including the new one)
        let mut count = 0usize;
        let mut dir_iter = tokio::fs::read_dir(&journal_root).await.expect("read_dir");
        while let Some(e) = dir_iter.next_entry().await.expect("entry") {
            if e.path().is_dir() {
                count += 1;
            }
        }
        assert!(count <= 50, "expected ≤ 50 sessions, found {count}");
    }

    // delete entry roundtrip preserves mode ────────────────────────────────────

    #[tokio::test]
    async fn test_delete_entry_roundtrip_preserves_mode() {
        // GIVEN a file with known content
        let dir = TempDir::new().expect("tmpdir");
        let file_path = dir.path().join("precious.txt");
        tokio::fs::write(&file_path, b"precious data")
            .await
            .expect("write");

        let journal_root = dir.path().join("journal");
        let entry = JournalEntry::Delete {
            path: file_path.clone(),
            previous_content: b"precious data".to_vec(),
            previous_mode: 0o644,
        };

        // Journal the delete entry
        let handle = spawn_and_record(&journal_root, "sess-ac6", entry).await;
        handle.shutdown().await;

        // Simulate the actual deletion
        tokio::fs::remove_file(&file_path).await.expect("delete");
        assert!(!file_path.exists());

        // WHEN rollback is applied
        rollback_session(&journal_root, "sess-ac6", false)
            .await
            .expect("rollback ok");

        // THEN the file is recreated with the correct content
        let content = tokio::fs::read(&file_path).await.expect("read");
        assert_eq!(content, b"precious data");

        // AND the mode is restored (unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            let meta = std::fs::metadata(&file_path).expect("meta");
            assert_eq!(meta.mode() & 0o777, 0o644);
        }
    }

    // ── session_not_found ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_rollback_session_not_found_returns_error() {
        // GIVEN a valid journal root
        let dir = TempDir::new().expect("tmpdir");
        let journal_root = dir.path().join("journal");
        tokio::fs::create_dir_all(&journal_root)
            .await
            .expect("mkdir");

        // WHEN rolling back a non-existent session
        let result = rollback_session(&journal_root, "ghost", false).await;

        // THEN SessionNotFound error
        assert!(
            matches!(result, Err(JournalError::SessionNotFound(_))),
            "expected SessionNotFound"
        );
    }

    // ── multiple entries reversed in order ───────────────────────────────────

    #[tokio::test]
    async fn test_rollback_multiple_entries_reversed() {
        // GIVEN two files: a.txt="v1", b.txt="v1"
        let dir = TempDir::new().expect("tmpdir");
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");

        let journal_root = dir.path().join("journal");
        let handle = JournalWriter::spawn("sess-multi".to_string(), journal_root.clone(), 50);

        // Journal write of a.txt then b.txt
        handle
            .record(JournalEntry::Write {
                path: a.clone(),
                previous_content: Some(b"v1-a".to_vec()),
                previous_mode: None,
                previous_mtime: None,
            })
            .await
            .expect("record a");
        handle
            .record(JournalEntry::Write {
                path: b.clone(),
                previous_content: Some(b"v1-b".to_vec()),
                previous_mode: None,
                previous_mtime: None,
            })
            .await
            .expect("record b");
        handle.shutdown().await;

        // Simulate mutations
        tokio::fs::write(&a, b"v2-a").await.expect("write a");
        tokio::fs::write(&b, b"v2-b").await.expect("write b");

        // WHEN rollback
        rollback_session(&journal_root, "sess-multi", false)
            .await
            .expect("rollback");

        // THEN both files restored to v1
        assert_eq!(tokio::fs::read(&a).await.expect("read a"), b"v1-a");
        assert_eq!(tokio::fs::read(&b).await.expect("read b"), b"v1-b");
    }
}
