//! SQLite repository for HITL task persistence.
//!
//! Provides [`TaskRepository`], which stores and restores HITL data
//! (prompt, context, human response) in a local SQLite database.
//!
//! All public methods are `async` and delegate to blocking SQLite operations
//! via `tokio::task::spawn_blocking`, the same pattern as
//! [`crate::audit::AuditTrail`].
//!
//! The `005_hitl_tables.sql` migration is applied idempotently when
//! [`TaskRepository::open`] is called.

mod hitl;
mod lifecycle;

use std::path::{Path, PathBuf};

use rusqlite::params;

// ─────────────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────────────

/// Errors specific to [`TaskRepository`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TaskRepoError {
    /// No task found for the given `task_id`.
    #[error("task not found: {0}")]
    NotFound(String),

    /// Underlying SQLite error.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] apollia_core::schema::SchemaError),

    /// JSON deserialization error.
    #[error("JSON deserialisation error: {0}")]
    Json(#[from] serde_json::Error),

    /// Infrastructure error (spawn_blocking join error, etc.).
    #[error("internal error: {0}")]
    Internal(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// Return types
// ─────────────────────────────────────────────────────────────────────────────

/// Task observability detail returned by [`TaskRepository::get_task_detail`].
#[derive(Debug, Clone)]
pub struct TaskDetail {
    /// Task input text (possibly truncated).
    pub input_text: Option<String>,
    /// Task output text (possibly truncated).
    pub output_text: Option<String>,
    /// Execution duration in milliseconds.
    pub duration_ms: Option<i64>,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
}

/// Opens a SQLite connection and applies the HITL migration idempotently.
///
/// Called by every [`TaskRepository`] method to guarantee schema integrity even
/// if the database was deleted and recreated after the runtime started. The
/// migration uses `CREATE TABLE IF NOT EXISTS`, so it is a no-op on a valid
/// database.
///
/// If the main file does not exist, any leftover WAL/SHM files are removed
/// before opening, to avoid an orphan WAL recovery that would leave the
/// database without tables.
pub(crate) fn open_conn(path: &std::path::Path) -> Result<rusqlite::Connection, TaskRepoError> {
    if !path.exists() {
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }
    let conn = rusqlite::Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    crate::hitl_schema::open_hitl_schema(&conn)?;
    Ok(conn)
}

// ─────────────────────────────────────────────────────────────────────────────
// Repository
// ─────────────────────────────────────────────────────────────────────────────

/// SQLite repository for HITL task persistence.
///
/// Wraps the SQLite database path. Each method opens a dedicated connection
/// inside `spawn_blocking`, which keeps it compatible with the Tokio runtime
/// without sharing a connection across threads.
///
/// The `005_hitl_tables.sql` migration is applied in [`open`](Self::open).
#[derive(Clone)]
pub struct TaskRepository {
    db_path: PathBuf,
}

impl TaskRepository {
    /// Opens (or creates) the SQLite database and applies the HITL migration.
    ///
    /// The migration is idempotent (`CREATE TABLE IF NOT EXISTS`), so it is
    /// safe to replay on an existing database. Enables WAL mode for
    /// read/write concurrency.
    ///
    /// # Errors
    ///
    /// Returns an error if the SQLite file cannot be opened or if the migration
    /// fails (incompatible schema, permissions, etc.).
    pub async fn open(db_path: &Path) -> Result<Self, TaskRepoError> {
        let path = db_path.to_path_buf();

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            open_conn(&path)?;
            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))??;

        Ok(Self {
            db_path: db_path.to_path_buf(),
        })
    }

    // ─── HITL timing methods ───────────────────────────────────────────

    /// Returns a task's SQLite status, or `None` if absent from the `tasks` table.
    ///
    /// Used by the `ResumeHandler` to verify a task is in `input_required`
    /// status before processing the resume.
    ///
    /// # Errors
    ///
    /// Returns [`TaskRepoError::Sqlite`] on a SQLite error.
    pub async fn get_task_status(&self, task_id: &str) -> Result<Option<String>, TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();

        tokio::task::spawn_blocking(move || -> Result<Option<String>, TaskRepoError> {
            let conn = open_conn(&path)?;
            let result = conn.query_row(
                "SELECT status FROM tasks WHERE task_id = ?1",
                params![&task_id],
                |row| row.get::<_, String>(0),
            );
            match result {
                Ok(status) => Ok(Some(status)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(TaskRepoError::Sqlite(e)),
            }
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }

    /// Returns a task's observability detail: input, output, duration, creation date.
    ///
    /// Returns `None` if the task does not exist in the database (a recent task
    /// not yet persisted, or a task submitted before the observability migration).
    ///
    /// # Errors
    ///
    /// Returns [`TaskRepoError::Sqlite`] on a SQLite error.
    pub async fn get_task_detail(
        &self,
        task_id: &str,
    ) -> Result<Option<TaskDetail>, TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();

        tokio::task::spawn_blocking(move || -> Result<Option<TaskDetail>, TaskRepoError> {
            let conn = open_conn(&path)?;
            let result = conn.query_row(
                "SELECT input_text, output_text, duration_ms, created_at \
                 FROM tasks WHERE task_id = ?1",
                params![&task_id],
                |row| {
                    Ok(TaskDetail {
                        input_text: row.get::<_, Option<String>>(0)?,
                        output_text: row.get::<_, Option<String>>(1)?,
                        duration_ms: row.get::<_, Option<i64>>(2)?,
                        created_at: apollia_core::utils::sqlite_to_rfc3339(
                            &row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                        ),
                    })
                },
            );
            match result {
                Ok(detail) => Ok(Some(detail)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(TaskRepoError::Sqlite(e)),
            }
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }

    // ─── Observability methods ───────────────────────────────────────────

    /// Returns the recent tasks persisted in SQLite.
    ///
    /// Sorted by `created_at DESC`, limited to `limit` entries. The status is
    /// derived from `transitions_json` (last transition). Used to display task
    /// history after a runtime restart (when the in-memory `TaskRouter` is
    /// empty).
    ///
    /// # Errors
    ///
    /// Returns [`TaskRepoError::Sqlite`] on a SQLite error.
    pub async fn list_recent_tasks(
        &self,
        limit: usize,
    ) -> Result<Vec<PersistedTaskSummary>, TaskRepoError> {
        let path = self.db_path.clone();

        tokio::task::spawn_blocking(
            move || -> Result<Vec<PersistedTaskSummary>, TaskRepoError> {
                let conn = open_conn(&path)?;

                let mut stmt = conn.prepare(
                    "SELECT task_id, agent_name, input_text, output_text, \
                        duration_ms, transitions_json, created_at \
                 FROM tasks \
                 ORDER BY created_at DESC \
                 LIMIT ?1",
                )?;

                let rows = stmt
                    .query_map(params![limit as i64], |row| {
                        let task_id: String = row.get(0)?;
                        let agent_name: String = row.get(1)?;
                        let input_text: Option<String> = row.get(2)?;
                        let output_text: Option<String> = row.get(3)?;
                        let duration_ms: Option<i64> = row.get(4)?;
                        let transitions_json: Option<String> = row.get(5)?;
                        let created_at: String = apollia_core::utils::sqlite_to_rfc3339(
                            &row.get::<_, Option<String>>(6)?.unwrap_or_default(),
                        );

                        let status = derive_status(&transitions_json, duration_ms);
                        let input_preview = input_text
                            .as_deref()
                            .unwrap_or("")
                            .chars()
                            .take(120)
                            .collect::<String>();

                        Ok(PersistedTaskSummary {
                            task_id,
                            agent_name,
                            status,
                            input_preview,
                            output_text,
                            duration_ms,
                            created_at,
                        })
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;

                Ok(rows)
            },
        )
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Derives a task's final status from `transitions_json`.
///
/// Reads the last `{"status": "<status>", "ts": "<timestamp>"}` entry and
/// returns the status. Returns `"completed"` when the duration is set but no
/// transition is available (the case for older tasks).
fn derive_status(transitions_json: &Option<String>, duration_ms: Option<i64>) -> String {
    if let Some(status) = last_transition_status(transitions_json) {
        return status;
    }
    // Fallback: if we have duration, it was completed; otherwise unknown
    if duration_ms.is_some() {
        "completed".to_string()
    } else {
        "unknown".to_string()
    }
}

/// Extracts the status of the last transition in `transitions_json`, or `None`
/// if the JSON is absent, empty, invalid, or has no usable `status` field.
fn last_transition_status(transitions_json: &Option<String>) -> Option<String> {
    let json = transitions_json.as_ref()?;
    if json.is_empty() {
        return None;
    }
    let transitions = serde_json::from_str::<Vec<serde_json::Value>>(json).ok()?;
    let last = transitions.last()?;
    let status = last.get("status").and_then(|s| s.as_str())?;
    Some(status.to_string())
}

/// Summary of a persisted task, read from SQLite.
///
/// Used by `list_recent_tasks` to provide task history after a runtime restart
/// (finished tasks are no longer in memory).
#[derive(Debug, Clone)]
pub struct PersistedTaskSummary {
    /// Unique task identifier.
    pub task_id: String,
    /// Agent name.
    pub agent_name: String,
    /// Status derived from `transitions_json` (last transition).
    pub status: String,
    /// Preview of the input text (truncated to 120 chars).
    pub input_preview: String,
    /// Output text.
    pub output_text: Option<String>,
    /// Execution duration in milliseconds.
    pub duration_ms: Option<i64>,
    /// ISO 8601 creation date.
    pub created_at: String,
}

/// Information about a pending approval, read from SQLite.
#[derive(Debug, Clone)]
pub struct ApprovalInfo {
    /// Agent name (from the manifest).
    pub agent_name: String,
    /// Prompt displayed to the user.
    pub prompt: String,
    /// JSON context serialized by the agent.
    pub context: serde_json::Value,
    /// ISO 8601 suspension timestamp.
    pub suspended_at: String,
}

/// Row of a resolved approval, read from `task_approvals`.
#[derive(Debug, Clone)]
pub struct ResolvedApprovalRow {
    /// Task identifier.
    pub task_id: String,
    /// Agent name.
    pub agent_name: String,
    /// `true` if approved, `false` if rejected.
    pub approved: bool,
    /// Rejection reason (if applicable).
    pub reason: Option<String>,
    /// ISO 8601 suspension timestamp.
    pub suspended_at: Option<String>,
    /// ISO 8601 response timestamp.
    pub responded_at: Option<String>,
    /// Wait duration in milliseconds.
    pub wait_duration_ms: Option<i64>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use apollia_core::{InputResponseData, ObservabilityConfig};

    /// Opens a `TaskRepository` on a unique temporary file.
    async fn open_test_repo() -> (TaskRepository, PathBuf) {
        let path = std::env::temp_dir().join(format!("apollia_hitl_{}.db", uuid::Uuid::new_v4()));
        let repo = TaskRepository::open(&path).await.expect("open failed");
        (repo, path)
    }

    // ─── Task observability tests ─────────────────────────────────────

    #[tokio::test]
    async fn test_run_id_round_trip() {
        // GIVEN a repository
        let (repo, _path) = open_test_repo().await;

        // WHEN a run_id is recorded for a task
        repo.set_run_id("t-run-1", "run-abc-123")
            .await
            .expect("set_run_id failed");

        // THEN it reads back, and an unknown task returns None
        assert_eq!(
            repo.get_run_id("t-run-1").await.expect("get_run_id failed"),
            Some("run-abc-123".to_string())
        );
        assert_eq!(
            repo.get_run_id("t-missing")
                .await
                .expect("get_run_id failed"),
            None
        );
    }

    // Input persisted at submission (not truncated)

    #[tokio::test]
    async fn test_story126_ac1_input_persisted() {
        // GIVEN a TaskRepository plus a task_id created via save_input
        let (repo, db_path) = open_test_repo().await;
        let config = ObservabilityConfig::default();

        // WHEN save_input with a short text
        repo.save_input("t-126-1", "hello world", &config)
            .await
            .expect("save_input failed");

        // THEN input_text == "hello world", input_truncated == 0
        let (text, truncated): (String, i32) = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).expect("open db");
            conn.query_row(
                "SELECT input_text, input_truncated FROM tasks WHERE task_id = ?1",
                params!["t-126-1"],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?)),
            )
            .expect("query failed")
        })
        .await
        .expect("join failed");

        assert_eq!(text, "hello world");
        assert_eq!(truncated, 0);
    }

    // Input truncated when larger than the limit

    #[tokio::test]
    async fn test_story126_ac2_input_truncated_at_limit() {
        // GIVEN config with max_input_bytes = 100
        let (repo, db_path) = open_test_repo().await;
        let config = ObservabilityConfig {
            max_input_bytes: 100,
            ..ObservabilityConfig::default()
        };
        let big_text = "x".repeat(500);

        // WHEN save_input with a 500-byte text
        repo.save_input("t-126-2", &big_text, &config)
            .await
            .expect("save_input failed");

        // THEN input_truncated == 1, input_text contains the marker
        let (text, truncated): (String, i32) = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).expect("open db");
            conn.query_row(
                "SELECT input_text, input_truncated FROM tasks WHERE task_id = ?1",
                params!["t-126-2"],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?)),
            )
            .expect("query failed")
        })
        .await
        .expect("join failed");

        assert_eq!(truncated, 1);
        assert!(text.ends_with("bytes total]"), "got: {text}");
        assert!(text.contains("500"), "marker should mention 500 bytes");
    }

    // Output persisted at completion

    #[tokio::test]
    async fn test_story126_ac3_output_persisted() {
        // GIVEN an existing task
        let (repo, db_path) = open_test_repo().await;
        let config = ObservabilityConfig::default();
        repo.save_input("t-126-3", "input", &config)
            .await
            .expect("save_input failed");

        // WHEN save_output with a short text
        repo.save_output("t-126-3", "result output", &config)
            .await
            .expect("save_output failed");

        // THEN output_text == "result output", output_truncated == 0
        let (text, truncated): (String, i32) = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).expect("open db");
            conn.query_row(
                "SELECT output_text, output_truncated FROM tasks WHERE task_id = ?1",
                params!["t-126-3"],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?)),
            )
            .expect("query failed")
        })
        .await
        .expect("join failed");

        assert_eq!(text, "result output");
        assert_eq!(truncated, 0);
    }

    // Output truncated when larger than the limit

    #[tokio::test]
    async fn test_story126_ac3_output_truncated_at_limit() {
        // GIVEN config with max_output_bytes = 100
        let (repo, db_path) = open_test_repo().await;
        let config = ObservabilityConfig {
            max_output_bytes: 100,
            ..ObservabilityConfig::default()
        };
        repo.save_input("t-126-3b", "input", &config)
            .await
            .expect("save_input failed");
        let big_output = "y".repeat(500);

        // WHEN save_output with a 500-byte text
        repo.save_output("t-126-3b", &big_output, &config)
            .await
            .expect("save_output failed");

        // THEN output_truncated == 1
        let truncated: i32 = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).expect("open db");
            conn.query_row(
                "SELECT output_truncated FROM tasks WHERE task_id = ?1",
                params!["t-126-3b"],
                |row| row.get::<_, i32>(0),
            )
            .expect("query failed")
        })
        .await
        .expect("join failed");

        assert_eq!(truncated, 1);
    }

    // Transitions ordered chronologically

    #[tokio::test]
    async fn test_story126_ac4_transitions_ordered() {
        // GIVEN an existing task
        let (repo, db_path) = open_test_repo().await;
        let config = ObservabilityConfig::default();
        repo.save_input("t-126-4", "input", &config)
            .await
            .expect("save_input failed");

        // WHEN 3 transitions are appended in order
        repo.append_transition("t-126-4", "submitted", "2026-03-13T10:00:00Z")
            .await
            .expect("append 1 failed");
        repo.append_transition("t-126-4", "running", "2026-03-13T10:00:01Z")
            .await
            .expect("append 2 failed");
        repo.append_transition("t-126-4", "completed", "2026-03-13T10:00:02Z")
            .await
            .expect("append 3 failed");

        // THEN transitions_json contains 3 elements in order
        let json_str: String = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).expect("open db");
            conn.query_row(
                "SELECT transitions_json FROM tasks WHERE task_id = ?1",
                params!["t-126-4"],
                |row| row.get::<_, String>(0),
            )
            .expect("query failed")
        })
        .await
        .expect("join failed");

        let transitions: Vec<serde_json::Value> =
            serde_json::from_str(&json_str).expect("parse json");
        assert_eq!(transitions.len(), 3);
        assert_eq!(transitions[0]["status"], "submitted");
        assert_eq!(transitions[1]["status"], "running");
        assert_eq!(transitions[2]["status"], "completed");
        assert_eq!(transitions[0]["ts"], "2026-03-13T10:00:00Z");
        assert_eq!(transitions[2]["ts"], "2026-03-13T10:00:02Z");
    }

    // Duration measured

    #[tokio::test]
    async fn test_story126_ac5_duration_recorded() {
        // GIVEN an existing task
        let (repo, db_path) = open_test_repo().await;
        let config = ObservabilityConfig::default();
        repo.save_input("t-126-5", "input", &config)
            .await
            .expect("save_input failed");

        // WHEN set_duration(250ms)
        repo.set_duration("t-126-5", 250)
            .await
            .expect("set_duration failed");

        // THEN duration_ms == 250
        let duration: i64 = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).expect("open db");
            conn.query_row(
                "SELECT duration_ms FROM tasks WHERE task_id = ?1",
                params!["t-126-5"],
                |row| row.get::<_, i64>(0),
            )
            .expect("query failed")
        })
        .await
        .expect("join failed");

        assert_eq!(duration, 250);
    }

    // Observability columns present after migration

    #[tokio::test]
    async fn test_story126_migration_columns_present() {
        // GIVEN a fresh database
        let (_, db_path) = open_test_repo().await;

        // WHEN inspecting the columns
        let cols: Vec<String> = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).expect("open");
            let mut stmt = conn
                .prepare("SELECT name FROM pragma_table_info('tasks')")
                .expect("prepare");
            stmt.query_map([], |row| row.get::<_, String>(0))
                .expect("query")
                .map(|r| r.expect("get"))
                .collect()
        })
        .await
        .expect("join");

        // THEN the 6 observability columns are present
        for expected in &[
            "input_text",
            "input_truncated",
            "output_text",
            "output_truncated",
            "duration_ms",
            "transitions_json",
        ] {
            assert!(
                cols.contains(&expected.to_string()),
                "missing column: {expected} ; columns found = {cols:?}"
            );
        }
    }

    // ─── Existing HITL tests ─────────────────────────────────────────

    // Migration applied at startup: HITL columns present in tasks

    #[tokio::test]
    async fn test_migration_005_colonnes_existantes() {
        // GIVEN a fresh temporary database
        let (_, db_path) = open_test_repo().await;

        // WHEN inspecting the columns via PRAGMA table_info
        let cols: Vec<String> = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            let mut stmt = conn
                .prepare("SELECT name FROM pragma_table_info('tasks')")
                .unwrap();
            stmt.query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        })
        .await
        .unwrap();

        // THEN all the HITL columns are present
        for expected in &[
            "input_required_prompt",
            "input_required_context",
            "input_required_at",
            "input_response_approved",
            "input_response_reason",
            "input_response_at",
        ] {
            assert!(
                cols.contains(&expected.to_string()),
                "missing column: {expected} ; columns found = {cols:?}"
            );
        }

        // AND the task_approvals table is created
        let tables: Vec<String> = tokio::task::spawn_blocking(|| {
            let conn = rusqlite::Connection::open_in_memory().unwrap();
            crate::hitl_schema::open_hitl_schema(&conn).unwrap();
            let mut stmt = conn
                .prepare(
                    "SELECT name FROM sqlite_master \
                     WHERE type='table' AND name='task_approvals'",
                )
                .unwrap();
            stmt.query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        })
        .await
        .unwrap();

        assert!(
            !tables.is_empty(),
            "the task_approvals table must exist after the migration"
        );
    }

    // save_input_response() persists the response and inserts into task_approvals

    #[tokio::test]
    async fn test_save_input_response_persiste() {
        // GIVEN a task created in input_required status
        let (repo, db_path) = open_test_repo().await;
        let task_id = "t-hitl-002";
        let context = serde_json::json!({"montant": 12_500});

        repo.save_input_required(task_id, None, "Confirmer l'envoi ?", &context)
            .await
            .expect("save_input_required failed");

        // WHEN save_input_response() is called with approved=true
        let response = InputResponseData {
            approved: true,
            reason: None,
            context: context.clone(),
            responded_at: "2026-03-09T10:00:00Z".into(),
        };
        repo.save_input_response(task_id, &response)
            .await
            .expect("save_input_response failed");

        // THEN the tasks row is updated with the correct values
        let db_path_a = db_path.clone();
        let (approved, at, status): (i32, String, String) =
            tokio::task::spawn_blocking(move || {
                let conn = rusqlite::Connection::open(&db_path_a).unwrap();
                conn.query_row(
                    "SELECT input_response_approved, input_response_at, status \
                     FROM tasks WHERE task_id = ?1",
                    params![task_id],
                    |row| {
                        Ok((
                            row.get::<_, i32>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .unwrap()
            })
            .await
            .unwrap();

        assert_eq!(approved, 1, "input_response_approved must be 1 (true)");
        assert_eq!(at, "2026-03-09T10:00:00Z");
        assert_eq!(status, "working");

        // AND task_approvals contains exactly one row for this task_id
        let count: i64 = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM task_approvals WHERE task_id = ?1",
                params![task_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
        })
        .await
        .unwrap();

        assert_eq!(count, 1, "task_approvals must hold exactly one row");
    }

    // rebuild_for_resume() rebuilds the AIPTask with is_resumed=true

    #[tokio::test]
    async fn test_rebuild_for_resume_is_resumed_true() {
        // GIVEN a task with a persisted response (approved=true)
        let (repo, _db_path) = open_test_repo().await;
        let task_id = "t-resume-003";
        let context = serde_json::json!({"devis": 42});

        repo.save_input_required(task_id, None, "Confirmer ?", &context)
            .await
            .unwrap();

        let response = InputResponseData {
            approved: true,
            reason: None,
            context: context.clone(),
            responded_at: "2026-03-09T11:00:00Z".into(),
        };
        repo.save_input_response(task_id, &response).await.unwrap();

        // WHEN rebuild_for_resume() is called
        let task = repo.rebuild_for_resume(task_id).await.unwrap();

        // THEN is_resumed=true, input_response.approved=true, context == original
        assert!(task.is_resumed, "is_resumed must be true");
        assert_eq!(task.task_id, task_id);

        let ir = task
            .input_response
            .expect("input_response must be Some after the rebuild");
        assert!(ir.approved, "approved must be true");
        assert_eq!(ir.context, context, "context must match the original");
        assert!(
            !ir.responded_at.is_empty(),
            "responded_at must not be empty"
        );
    }

    // find_input_required_older_than() returns the expired tasks

    #[tokio::test]
    async fn test_delete_task_removes_record() {
        // GIVEN a persisted task with an approval row
        let (repo, _db_path) = open_test_repo().await;
        let task_id = "t-delete-001";
        repo.save_input_required(task_id, None, "confirm?", &serde_json::json!({}))
            .await
            .expect("save_input_required failed");

        // WHEN the task is hard-deleted
        let removed = repo.delete_task(task_id).await.expect("delete_task failed");

        // THEN it reports a removal and the record is gone
        assert!(removed);
        let status = repo
            .get_task_status(task_id)
            .await
            .expect("get_task_status failed");
        assert!(status.is_none(), "task record should be gone after delete");
    }

    #[tokio::test]
    async fn test_delete_task_absent_returns_false() {
        // GIVEN an empty repository
        let (repo, _db_path) = open_test_repo().await;

        // WHEN deleting a task that was never persisted
        let removed = repo
            .delete_task("t-nonexistent")
            .await
            .expect("delete_task failed");

        // THEN nothing was removed
        assert!(!removed);
    }

    #[tokio::test]
    async fn test_find_expired_input_required() {
        // GIVEN a task input_required for 25h (set directly in the database)
        let (repo, db_path) = open_test_repo().await;
        let task_id = "t-expired-004";

        repo.save_input_required(task_id, None, "check", &serde_json::json!({}))
            .await
            .unwrap();

        // Direct manipulation: push input_required_at back by 25h
        tokio::task::spawn_blocking({
            let path = db_path.clone();
            move || {
                let conn = rusqlite::Connection::open(&path).unwrap();
                conn.execute(
                    "UPDATE tasks \
                     SET input_required_at = datetime('now', '-25 hours') \
                     WHERE task_id = ?1",
                    params![task_id],
                )
                .unwrap();
            }
        })
        .await
        .unwrap();

        // WHEN find_input_required_older_than(24h)
        let expired = repo
            .find_input_required_older_than(Duration::from_secs(24 * 3600))
            .await
            .unwrap();

        // THEN the task_id is present in the expired list
        assert!(
            expired.contains(&task_id.to_string()),
            "task_id must be in the expired list ; got={expired:?}"
        );
    }

    // Recent task absent from the expired list

    #[tokio::test]
    async fn test_find_recent_not_expired() {
        // GIVEN a task input_required created just now (~0s)
        let (repo, _db_path) = open_test_repo().await;
        let task_id = "t-recent-005";

        repo.save_input_required(task_id, None, "check", &serde_json::json!({}))
            .await
            .unwrap();

        // WHEN find_input_required_older_than(24h)
        let expired = repo
            .find_input_required_older_than(Duration::from_secs(24 * 3600))
            .await
            .unwrap();

        // THEN the task_id is NOT in the list
        assert!(
            !expired.contains(&task_id.to_string()),
            "a recent task must NOT be in the expired list ; got={expired:?}"
        );
    }

    // ─── HITL timing tests ───────────────────────────────────────────

    // suspended_at recorded at suspension

    #[tokio::test]
    async fn test_story131_ac1_suspended_at_recorded() {
        // GIVEN a TaskRepository with a pending approval
        let (repo, db_path) = open_test_repo().await;
        let task_id = "t-131-1";
        repo.save_input_required(task_id, None, "Confirmer ?", &serde_json::json!({}))
            .await
            .expect("save_input_required failed");

        // WHEN save_suspended_at is called
        repo.save_suspended_at(task_id, None, "2026-03-13T14:30:00.000Z")
            .await
            .expect("save_suspended_at failed");

        // THEN suspended_at is set in task_approvals
        let suspended: String = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).expect("open db");
            conn.query_row(
                "SELECT suspended_at FROM task_approvals WHERE task_id = ?1",
                params!["t-131-1"],
                |row| row.get::<_, String>(0),
            )
            .expect("query failed")
        })
        .await
        .expect("join failed");

        assert_eq!(suspended, "2026-03-13T14:30:00.000Z");
    }

    // responded_at recorded on response (via save_input_response)

    #[tokio::test]
    async fn test_story131_ac2_responded_at_recorded() {
        // GIVEN an approval with suspended_at set
        let (repo, db_path) = open_test_repo().await;
        let task_id = "t-131-2";
        repo.save_input_required(task_id, None, "Budget OK ?", &serde_json::json!({}))
            .await
            .expect("save_input_required failed");
        repo.save_suspended_at(task_id, None, "2026-03-13T14:30:00.000Z")
            .await
            .expect("save_suspended_at failed");

        // WHEN save_input_response is called
        let response = InputResponseData {
            approved: true,
            reason: None,
            context: serde_json::json!({}),
            responded_at: "2026-03-13T14:35:00.000Z".into(),
        };
        repo.save_input_response(task_id, &response)
            .await
            .expect("save_input_response failed");

        // THEN responded_at is set in task_approvals
        let responded: String = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).expect("open db");
            conn.query_row(
                "SELECT responded_at FROM task_approvals WHERE task_id = ?1",
                params!["t-131-2"],
                |row| row.get::<_, String>(0),
            )
            .expect("query failed")
        })
        .await
        .expect("join failed");

        assert_eq!(responded, "2026-03-13T14:35:00.000Z");
    }

    // wait_duration_ms computed automatically (5 min = 300000ms)

    #[tokio::test]
    async fn test_story131_ac3_wait_duration_calculated() {
        // GIVEN suspended_at = 14:30:00, responded_at = 14:35:00
        let (repo, db_path) = open_test_repo().await;
        let task_id = "t-131-3";
        repo.save_input_required(task_id, None, "Valider ?", &serde_json::json!({}))
            .await
            .expect("save_input_required failed");
        repo.save_suspended_at(task_id, None, "2026-03-13T14:30:00.000Z")
            .await
            .expect("save_suspended_at failed");

        // WHEN save_input_response is called 5 min later
        let response = InputResponseData {
            approved: true,
            reason: None,
            context: serde_json::json!({}),
            responded_at: "2026-03-13T14:35:00.000Z".into(),
        };
        repo.save_input_response(task_id, &response)
            .await
            .expect("save_input_response failed");

        // THEN wait_duration_ms is about 300000 (5 min in ms, +/-1000 tolerance for julianday rounding)
        let wait_ms: i64 = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).expect("open db");
            conn.query_row(
                "SELECT wait_duration_ms FROM task_approvals WHERE task_id = ?1",
                params!["t-131-3"],
                |row| row.get::<_, i64>(0),
            )
            .expect("query failed")
        })
        .await
        .expect("join failed");

        assert!(
            (299_000..=301_000).contains(&wait_ms),
            "wait_duration_ms should be ~300000 (5 min), got {wait_ms}"
        );
    }

    // Pending index created

    #[tokio::test]
    async fn test_story131_ac4_pending_index_exists() {
        // GIVEN a fresh database
        let (_, db_path) = open_test_repo().await;

        // WHEN checking the indexes
        let has_index: bool = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).expect("open");
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master \
                     WHERE type = 'index' AND name = 'idx_task_approvals_pending'",
                    [],
                    |row| row.get(0),
                )
                .expect("query failed");
            count > 0
        })
        .await
        .expect("join failed");

        // THEN the index exists
        assert!(has_index, "idx_task_approvals_pending must exist");
    }

    // ─── list_resolved_approvals tests ────────────────────────────────

    #[tokio::test]
    async fn test_story141_list_resolved_approvals() {
        // GIVEN a repo with a resolved approval
        let (repo, _db_path) = open_test_repo().await;
        let task_id = "t-141-resolved";
        let context = serde_json::json!({"montant": 5000});

        repo.save_input_required(task_id, None, "Approve the payment?", &context)
            .await
            .expect("save_input_required failed");

        repo.save_suspended_at(task_id, None, "2099-01-01T10:00:00.000Z")
            .await
            .expect("save_suspended_at failed");

        let response = InputResponseData {
            approved: true,
            reason: None,
            context,
            responded_at: "2099-01-01T10:05:00.000Z".to_string(),
        };
        repo.save_input_response(task_id, &response)
            .await
            .expect("save_input_response failed");

        // WHEN list_resolved_approvals is called
        let rows = repo
            .list_resolved_approvals(20, 30)
            .await
            .expect("list_resolved_approvals failed");

        // THEN one resolved approval is returned
        assert_eq!(rows.len(), 1, "expected 1 resolved approval");
        assert_eq!(rows[0].task_id, task_id);
        assert!(rows[0].approved);
        assert!(rows[0].reason.is_none());
    }

    #[tokio::test]
    async fn test_story141_list_resolved_excludes_pending() {
        // GIVEN a repo with an approval still pending (no response)
        let (repo, _db_path) = open_test_repo().await;
        let task_id = "t-141-pending";

        repo.save_input_required(task_id, None, "Pending", &serde_json::json!({}))
            .await
            .expect("save_input_required failed");

        repo.save_suspended_at(task_id, None, "2026-03-13T10:00:00.000Z")
            .await
            .expect("save_suspended_at failed");

        // WHEN list_resolved_approvals is called
        let rows = repo
            .list_resolved_approvals(20, 30)
            .await
            .expect("list_resolved_approvals failed");

        // THEN no rows returned (pending is excluded)
        assert!(
            rows.is_empty(),
            "pending approval should not appear in resolved list"
        );
    }
}
