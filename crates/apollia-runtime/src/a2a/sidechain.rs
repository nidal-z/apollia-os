//! Sidechain logging: structured tracing of A2A delegations in SQLite.
//!
//! [`SidechainRepository`] persists each delegation in the `task_sidechains` table.
//! [`SidechainLogger`] is its best-effort async wrapper: any logging error is
//! traced via `tracing::warn` and never blocks the delegation.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};
use serde::Serialize;
use tracing::warn;

use apollia_core::TaskId;

/// SQL migration applied when the database is opened.
const MIGRATION_SQL: &str = include_str!("../../migrations/002_task_sidechains.sql");

/// Current schema version of the sidechain store (a single step).
const SCHEMA_VERSION: u32 = 1;

/// The ordered migration list applied through
/// [`apollia_core::schema::open_versioned`].
const MIGRATIONS: [apollia_core::schema::Migration; SCHEMA_VERSION as usize] = [migrate_v1];

fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(MIGRATION_SQL)
}

// ─────────────────────────────────────────────────────────────────────────────
// Error
// ─────────────────────────────────────────────────────────────────────────────

/// Errors returned by the sidechain SQLite operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SidechainError {
    /// Underlying SQLite error.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// The store could not be opened at a supported schema version.
    #[error("sidechain schema error: {0}")]
    Schema(#[from] apollia_core::schema::SchemaError),
}

// ─────────────────────────────────────────────────────────────────────────────
// Row type
// ─────────────────────────────────────────────────────────────────────────────

/// Row returned by [`SidechainRepository::list_by_parent`].
#[derive(Debug, Serialize, Clone, utoipa::ToSchema)]
pub struct SidechainRow {
    /// Sequential delegation number for this parent (1-based).
    pub sidechain_n: i64,
    /// Target agent name (or the skill_id used for resolution).
    pub agent_name: String,
    /// Current status: `"running"`, `"completed"`, or `"failed"`.
    pub status: String,
    /// First 500 characters of the input.
    pub input_summary: Option<String>,
    /// First 500 characters of the output or the error message.
    pub output_summary: Option<String>,
    /// ISO 8601 timestamp when the delegation started.
    pub started_at: Option<String>,
    /// ISO 8601 timestamp when the delegation finished (`None` if still running).
    pub completed_at: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Repository (synchronous, rusqlite)
// ─────────────────────────────────────────────────────────────────────────────

/// Synchronous SQLite repository for A2A delegations.
///
/// All methods are synchronous and must be called from a blocking thread
/// (via `tokio::task::spawn_blocking` in an async context).
pub struct SidechainRepository {
    conn: Connection,
}

impl SidechainRepository {
    /// Opens or creates the SQLite database at the given path and applies the migration.
    pub fn open(path: &Path) -> Result<Self, SidechainError> {
        let conn = Connection::open(path)?;
        apollia_core::schema::open_versioned(
            &conn,
            apollia_core::paths::DataFile::Sidechains.file_name(),
            SCHEMA_VERSION,
            &MIGRATIONS,
        )?;
        Ok(Self { conn })
    }

    /// Creates an in-memory database for tests.
    pub fn new_in_memory() -> Result<Self, SidechainError> {
        let conn = Connection::open_in_memory()?;
        apollia_core::schema::open_versioned(
            &conn,
            apollia_core::paths::DataFile::Sidechains.file_name(),
            SCHEMA_VERSION,
            &MIGRATIONS,
        )?;
        Ok(Self { conn })
    }

    /// Records the start of a delegation. Returns `sidechain_n` (1-based).
    ///
    /// `sidechain_n` is computed application-side as `COUNT(*) + 1` for this
    /// `parent_task_id`.
    pub fn log_start(
        &self,
        parent_task_id: &str,
        agent_name: &str,
        input_summary: &str,
    ) -> Result<i64, SidechainError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM task_sidechains WHERE parent_task_id = ?1",
            params![parent_task_id],
            |row| row.get(0),
        )?;
        let sidechain_n = count + 1;
        self.conn.execute(
            "INSERT INTO task_sidechains \
             (parent_task_id, sidechain_n, agent_name, input_summary, status) \
             VALUES (?1, ?2, ?3, ?4, 'running')",
            params![parent_task_id, sidechain_n, agent_name, input_summary],
        )?;
        Ok(sidechain_n)
    }

    /// Updates a finished delegation with its final status and output summary.
    pub fn log_complete(
        &self,
        parent_task_id: &str,
        sidechain_n: i64,
        output_summary: &str,
        status: &str,
    ) -> Result<(), SidechainError> {
        self.conn.execute(
            "UPDATE task_sidechains \
             SET status = ?1, output_summary = ?2, completed_at = CURRENT_TIMESTAMP \
             WHERE parent_task_id = ?3 AND sidechain_n = ?4",
            params![status, output_summary, parent_task_id, sidechain_n],
        )?;
        Ok(())
    }

    /// Returns all delegations for a `parent_task_id`, ordered by `sidechain_n`.
    pub fn list_by_parent(
        &self,
        parent_task_id: &str,
    ) -> Result<Vec<SidechainRow>, SidechainError> {
        let mut stmt = self.conn.prepare(
            "SELECT sidechain_n, agent_name, status, input_summary, output_summary, \
             started_at, completed_at \
             FROM task_sidechains \
             WHERE parent_task_id = ?1 \
             ORDER BY sidechain_n ASC",
        )?;
        let rows = stmt
            .query_map(params![parent_task_id], |row| {
                Ok(SidechainRow {
                    sidechain_n: row.get(0)?,
                    agent_name: row.get(1)?,
                    status: row.get(2)?,
                    input_summary: row.get(3)?,
                    output_summary: row.get(4)?,
                    started_at: row.get(5)?,
                    completed_at: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Logger (async, best-effort)
// ─────────────────────────────────────────────────────────────────────────────

/// Best-effort async wrapper around [`SidechainRepository`].
///
/// All operations use `spawn_blocking`. Errors are logged via `tracing::warn`
/// and never propagated: the A2A delegation continues even if logging fails.
#[derive(Clone)]
pub struct SidechainLogger {
    repository: Arc<Mutex<SidechainRepository>>,
}

impl SidechainLogger {
    /// Builds a `SidechainLogger` from a shared repository.
    pub fn new(repository: Arc<Mutex<SidechainRepository>>) -> Self {
        Self { repository }
    }

    /// Builds an in-memory `SidechainLogger` for tests.
    pub fn new_in_memory() -> Result<Self, SidechainError> {
        let repo = SidechainRepository::new_in_memory()?;
        Ok(Self {
            repository: Arc::new(Mutex::new(repo)),
        })
    }

    /// Records the start of a delegation. Returns `sidechain_n`, or `0` on error.
    ///
    /// `sidechain_n == 0` signals a logging failure and is ignored by [`complete`].
    pub async fn start(
        &self,
        parent_task_id: &TaskId,
        agent_name: &str,
        input: &serde_json::Value,
    ) -> i64 {
        let input_summary: String = input.to_string().chars().take(500).collect();
        let parent_id = parent_task_id.to_string();
        let agent = agent_name.to_string();
        let repo = self.repository.clone();

        match tokio::task::spawn_blocking(move || {
            repo.lock()
                .map_err(|e| rusqlite::Error::InvalidParameterName(format!("mutex poisoned: {e}")))
                .map_err(SidechainError::from)
                .and_then(|guard| guard.log_start(&parent_id, &agent, &input_summary))
        })
        .await
        {
            Ok(Ok(n)) => n,
            Ok(Err(e)) => {
                warn!(error = %e, "sidechain start failed - delegation continues untracked");
                0
            }
            Err(e) => {
                warn!(error = %e, "sidechain start task panicked - delegation continues untracked");
                0
            }
        }
    }

    /// Updates a finished delegation. Best-effort: errors are logged, not propagated.
    ///
    /// `sidechain_n == 0` indicates that [`start`] failed; the call is silently ignored.
    pub async fn complete(
        &self,
        parent_task_id: &TaskId,
        sidechain_n: i64,
        output_summary: &str,
        status: &str,
    ) {
        if sidechain_n == 0 {
            return;
        }
        let parent_id = parent_task_id.to_string();
        let output: String = output_summary.chars().take(500).collect();
        let status = status.to_string();
        let repo = self.repository.clone();

        let result = tokio::task::spawn_blocking(move || {
            repo.lock()
                .map_err(|e| rusqlite::Error::InvalidParameterName(format!("mutex poisoned: {e}")))
                .map_err(SidechainError::from)
                .and_then(|guard| guard.log_complete(&parent_id, sidechain_n, &output, &status))
        })
        .await;

        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => warn!(error = %e, "sidechain complete failed"),
            Err(e) => warn!(error = %e, "sidechain complete task panicked"),
        }
    }

    /// Returns all delegations for a `parent_task_id`, ordered by `sidechain_n`.
    pub async fn list_by_parent(
        &self,
        parent_task_id: &str,
    ) -> Result<Vec<SidechainRow>, SidechainError> {
        let parent_id = parent_task_id.to_string();
        let repo = self.repository.clone();

        tokio::task::spawn_blocking(move || {
            repo.lock()
                .map_err(|e| rusqlite::Error::InvalidParameterName(format!("mutex poisoned: {e}")))
                .map_err(SidechainError::from)
                .and_then(|guard| guard.list_by_parent(&parent_id))
        })
        .await
        .map_err(|e| SidechainError::Sqlite(rusqlite::Error::InvalidParameterName(e.to_string())))?
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::TaskId;

    #[tokio::test]
    async fn test_two_delegations_get_sequential_sidechain_n() {
        // GIVEN an in-memory logger and a parent task
        let logger = SidechainLogger::new_in_memory().unwrap();
        let parent = TaskId::new_v4();

        // WHEN two delegations are recorded
        let n1 = logger
            .start(&parent, "agent-a", &serde_json::json!({"task": "hello"}))
            .await;
        let n2 = logger
            .start(&parent, "agent-b", &serde_json::json!({"task": "world"}))
            .await;

        // THEN the sequential numbers are 1 and 2
        assert_eq!(n1, 1);
        assert_eq!(n2, 2);
    }

    #[tokio::test]
    async fn test_completed_delegation_updates_status() {
        // GIVEN a running delegation
        let logger = SidechainLogger::new_in_memory().unwrap();
        let parent = TaskId::new_v4();
        let n = logger
            .start(&parent, "agent-a", &serde_json::json!({}))
            .await;
        assert_eq!(n, 1);

        // WHEN the delegation completes successfully
        logger
            .complete(&parent, n, r#"{"result":"ok"}"#, "completed")
            .await;

        // THEN the row has status = "completed"
        let rows = logger.list_by_parent(parent.as_ref()).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "completed");
    }

    #[tokio::test]
    async fn test_failed_delegation_sets_status_failed() {
        // GIVEN a running delegation
        let logger = SidechainLogger::new_in_memory().unwrap();
        let parent = TaskId::new_v4();
        let n = logger
            .start(&parent, "agent-a", &serde_json::json!({}))
            .await;

        // WHEN the delegation fails
        logger
            .complete(&parent, n, "agent not found", "failed")
            .await;

        // THEN the row has status = "failed"
        let rows = logger.list_by_parent(parent.as_ref()).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "failed");
    }

    #[tokio::test]
    async fn test_complete_with_zero_sidechain_n_is_noop() {
        // GIVEN a sidechain_n == 0 (start() failed)
        let logger = SidechainLogger::new_in_memory().unwrap();
        let parent = TaskId::new_v4();

        // WHEN complete() is called with sidechain_n = 0
        logger.complete(&parent, 0, "output", "completed").await;

        // THEN no row is inserted
        let rows = logger.list_by_parent(parent.as_ref()).await.unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn test_list_by_parent_returns_empty_for_unknown_task() {
        // GIVEN a logger with no delegations
        let logger = SidechainLogger::new_in_memory().unwrap();

        // WHEN listing for an unknown parent
        let rows = logger.list_by_parent("unknown-task-id").await.unwrap();

        // THEN the list is empty
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn test_input_summary_is_truncated_at_500_chars() {
        // GIVEN a very long input
        let logger = SidechainLogger::new_in_memory().unwrap();
        let parent = TaskId::new_v4();
        let long_value: String = "x".repeat(1000);
        let input = serde_json::json!({"data": long_value});

        // WHEN the delegation is recorded
        let n = logger.start(&parent, "agent-a", &input).await;
        assert_eq!(n, 1);

        // THEN input_summary is truncated to 500 chars
        let rows = logger.list_by_parent(parent.as_ref()).await.unwrap();
        assert_eq!(rows.len(), 1);
        if let Some(summary) = &rows[0].input_summary {
            assert!(summary.chars().count() <= 500);
        }
    }

    /// sidechains.db as the first shipped binary wrote it, with one row.
    const SIDECHAINS_V1_FIXTURE: &str =
        include_str!("../../tests/fixtures/schemas/sidechains_v1.sql");

    #[test]
    fn test_open_legacy_v1_database_keeps_rows_and_stamps_version() {
        // GIVEN a sidechains.db written before the versioned layer (schema v1,
        // user_version 0, one completed delegation)
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("sidechains.db");
        let seed = Connection::open(&path).expect("open raw");
        seed.execute_batch(SIDECHAINS_V1_FIXTURE).expect("seed v1");
        drop(seed);

        // WHEN opening it through the repository (versioned migration)
        let repo = SidechainRepository::open(&path).expect("open migrated");

        // THEN the legacy row survives and the file is stamped
        let rows = repo.list_by_parent("parent-1").expect("list");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].agent_name, "agent-b");
        let version: i64 = repo
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .expect("user_version");
        assert_eq!(version, i64::from(SCHEMA_VERSION));
    }

    #[test]
    fn test_open_newer_database_is_refused() {
        // GIVEN a sidechains.db stamped one version above this binary
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("sidechains.db");
        let seed = Connection::open(&path).expect("open raw");
        seed.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .expect("stamp");
        drop(seed);

        // WHEN opening it through the repository
        let result = SidechainRepository::open(&path);

        // THEN the open is refused instead of misreading the newer schema
        assert!(matches!(result, Err(SidechainError::Schema(_))));
    }
}
