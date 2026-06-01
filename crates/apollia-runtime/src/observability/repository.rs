//! Read repository for `runtime_events.db`.
//!
//! Writes go exclusively through [`super::persistor::EventPersistorHandle`].
//! This module provides the paginated queries consumed by the
//! `GET /api/v1/tasks/{id}/trace` API and the purge routine.

use std::path::{Path, PathBuf};

use serde::Serialize;
use thiserror::Error;

/// In-memory representation of a `runtime_events` row.
///
/// Serializable to JSON for the API. The `payload_json` field stays opaque at
/// this level; each `kind` defines its own schema on the UI / consumer side.
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeEventRecord {
    /// UUID v7, lexicographically orderable.
    pub event_id: String,
    /// Task this event belongs to.
    pub task_id: String,
    /// Emitting agent.
    pub agent_id: String,
    /// Self-FK for nesting (tool_call_completed under tool_call_started, A2A).
    pub parent_event_id: Option<String>,
    /// ID shared across an A2A chain (NULL for root executions).
    pub correlation_id: Option<String>,
    /// ReAct turn (NULL outside the loop).
    pub step_num: Option<i64>,
    /// Discriminant, see `EventKind` on the UI side.
    pub kind: String,
    /// Payload typed by kind, raw JSON.
    pub payload_json: String,
    /// ISO 8601 RFC 3339, milliseconds included.
    pub ts: String,
    /// Unix seconds, used by the purge.
    pub created_at_unix: i64,
}

/// Read errors.
#[derive(Debug, Error)]
pub enum RepositoryError {
    /// Failed to open the SQLite file.
    #[error("failed to open runtime_events database at {path}: {source}")]
    OpenFailed {
        /// Attempted path.
        path: PathBuf,
        /// SQLite cause.
        source: rusqlite::Error,
    },
    /// SQLite error while executing a query.
    #[error("query failed: {0}")]
    Query(#[from] rusqlite::Error),
}

/// Synchronous repository (blocking SQLite calls are expected to run inside
/// `tokio::task::spawn_blocking` on the handler side).
pub struct RuntimeEventsRepository {
    conn: rusqlite::Connection,
}

impl RuntimeEventsRepository {
    /// Opens the database as logically read-only (no writes through this handle).
    pub fn open(db_path: &Path) -> Result<Self, RepositoryError> {
        let conn =
            rusqlite::Connection::open(db_path).map_err(|source| RepositoryError::OpenFailed {
                path: db_path.to_path_buf(),
                source,
            })?;
        // WAL mode on the reader side too: allows concurrent reads without
        // being blocked by the persistor.
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        Ok(Self { conn })
    }

    /// Lists a task's events in chronological order (UUIDv7, so lexical order
    /// equals creation order).
    ///
    /// `since`: if present, returns only events *strictly* after this
    /// `event_id`. Enables cursor pagination without `OFFSET`.
    /// `limit`: upper bound on the number of rows returned.
    pub fn list_for_task(
        &self,
        task_id: &str,
        since: Option<&str>,
        limit: usize,
    ) -> Result<Vec<RuntimeEventRecord>, RepositoryError> {
        let limit_i = limit as i64;
        let mut stmt = match since {
            Some(_) => self.conn.prepare(
                "SELECT event_id, task_id, agent_id, parent_event_id, correlation_id, \
                 step_num, kind, payload_json, ts, created_at_unix \
                 FROM runtime_events \
                 WHERE task_id = ?1 AND event_id > ?2 \
                 ORDER BY event_id ASC \
                 LIMIT ?3",
            )?,
            None => self.conn.prepare(
                "SELECT event_id, task_id, agent_id, parent_event_id, correlation_id, \
                 step_num, kind, payload_json, ts, created_at_unix \
                 FROM runtime_events \
                 WHERE task_id = ?1 \
                 ORDER BY event_id ASC \
                 LIMIT ?2",
            )?,
        };

        let map_row = |row: &rusqlite::Row<'_>| {
            Ok(RuntimeEventRecord {
                event_id: row.get(0)?,
                task_id: row.get(1)?,
                agent_id: row.get(2)?,
                parent_event_id: row.get(3)?,
                correlation_id: row.get(4)?,
                step_num: row.get(5)?,
                kind: row.get(6)?,
                payload_json: row.get(7)?,
                ts: row.get(8)?,
                created_at_unix: row.get(9)?,
            })
        };

        let rows: Vec<RuntimeEventRecord> = match since {
            Some(s) => stmt
                .query_map(rusqlite::params![task_id, s, limit_i], map_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?,
            None => stmt
                .query_map(rusqlite::params![task_id, limit_i], map_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?,
        };

        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::persistor::EventPersistorHandle;
    use tempfile::tempdir;

    #[tokio::test]
    async fn list_for_task_returns_chronological() {
        // GIVEN a persistor with three events for the same task, written in
        // the right order (UUID v7 generated by chrono::Utc::now would be
        // ordered, but we'll forge stable UUIDv7-ish ids manually).
        let dir = tempdir().expect("tempdir");
        let db = dir.path().join("rt.db");
        let handle = EventPersistorHandle::open(&db).await.expect("open");

        let now_unix = chrono::Utc::now().timestamp();
        let now_iso = chrono::Utc::now().to_rfc3339();

        for (i, kind) in [(0, "agent_log"), (1, "agent_log"), (2, "agent_log")] {
            handle.append(RuntimeEventRecord {
                event_id: format!("01900000-0000-7000-8000-00000000000{i}"),
                task_id: "T1".into(),
                agent_id: "A1".into(),
                parent_event_id: None,
                correlation_id: None,
                step_num: None,
                kind: kind.into(),
                payload_json: format!("{{\"i\":{i}}}"),
                ts: now_iso.clone(),
                created_at_unix: now_unix,
            });
        }

        handle.shutdown().await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // WHEN we query the repository
        let repo = RuntimeEventsRepository::open(&db).expect("open repo");
        let rows = repo.list_for_task("T1", None, 10).expect("list");

        // THEN all three rows are returned, in insertion order
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].payload_json, "{\"i\":0}");
        assert_eq!(rows[2].payload_json, "{\"i\":2}");
    }

    #[tokio::test]
    async fn list_for_task_supports_cursor_pagination() {
        // GIVEN a persistor with three events
        let dir = tempdir().expect("tempdir");
        let db = dir.path().join("rt.db");
        let handle = EventPersistorHandle::open(&db).await.expect("open");

        let now_unix = chrono::Utc::now().timestamp();
        let now_iso = chrono::Utc::now().to_rfc3339();
        for i in 0..3 {
            handle.append(RuntimeEventRecord {
                event_id: format!("01900000-0000-7000-8000-00000000000{i}"),
                task_id: "T1".into(),
                agent_id: "A1".into(),
                parent_event_id: None,
                correlation_id: None,
                step_num: None,
                kind: "agent_log".into(),
                payload_json: "{}".into(),
                ts: now_iso.clone(),
                created_at_unix: now_unix,
            });
        }
        handle.shutdown().await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let repo = RuntimeEventsRepository::open(&db).expect("open repo");

        // WHEN we paginate after the first row
        let cursor = "01900000-0000-7000-8000-000000000000";
        let rows = repo.list_for_task("T1", Some(cursor), 10).expect("list");

        // THEN only events 1 and 2 are returned (strictly after the cursor)
        assert_eq!(rows.len(), 2);
        assert!(rows[0].event_id.ends_with("000000000001"));
        assert!(rows[1].event_id.ends_with("000000000002"));
    }
}
