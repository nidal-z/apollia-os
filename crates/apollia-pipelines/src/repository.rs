//! SQLite repository for pipeline runs and step runs.
//!
//! [`PipelineRepository`] is a synchronous wrapper around `rusqlite`. It must
//! be called from `tokio::task::spawn_blocking` when used from async code,
//! following the same pattern as `TaskRepository` (Sprint 11, STORY-094).
//!
//! The `006_pipeline_tables.sql` migration is embedded at compile time and
//! applied idempotently on every call to [`PipelineRepository::migrate`].

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use thiserror::Error;

use crate::types::{
    PipelineId, PipelineRun, PipelineStatus, RunId, StepId, StepRun, StepRunStatus,
};

/// SQL migration embedded at compile time — applied idempotently on open.
const MIGRATION_SQL: &str = include_str!("../../apollia-tools/migrations/006_pipeline_tables.sql");

// ── Private row-tuple aliases (avoid clippy::type_complexity) ─────────────────

/// Columns selected by `find_running_runs`: run_id, pipeline_id, trigger_id,
/// trigger_payload, started_at.
type RunningRunRow = (String, String, Option<String>, Option<String>, String);

/// Columns selected by `find_runs_by_pipeline`: run_id, trigger_id, status,
/// failed_step, trigger_payload, started_at, ended_at.
type PipelineRunRow = (
    String,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
);

/// Columns selected by `load_step_runs`: step_id, task_id, status, output,
/// error, started_at, ended_at.
type StepRunRow = (
    String,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

// ── Errors ────────────────────────────────────────────────────────────────────

/// Errors returned by [`PipelineRepository`] operations.
#[derive(Debug, Error)]
pub enum PipelineRepositoryError {
    /// Underlying SQLite error.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// No pipeline run found for the given `run_id`.
    #[error("run not found: {0}")]
    RunNotFound(String),

    /// A step with the same `(run_id, step_id)` pair already exists.
    #[error("step already exists: ({0}, {1})")]
    StepAlreadyExists(String, String),

    /// JSON serialization or deserialization failed.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

// ── Repository ────────────────────────────────────────────────────────────────

/// Synchronous SQLite repository for pipeline runs and step runs.
///
/// Holds an open `rusqlite::Connection`. All methods are blocking; call them
/// inside `tokio::task::spawn_blocking` from async contexts.
///
/// The database is created (if absent) and migrated automatically by
/// [`open`](Self::open) and [`open_in_memory`](Self::open_in_memory).
pub struct PipelineRepository {
    conn: Connection,
}

impl PipelineRepository {
    /// Opens (or creates) a pipeline database at the given filesystem path.
    ///
    /// Enables WAL journal mode and applies migration `006_pipeline_tables.sql`
    /// idempotently before returning. Intended for production use at
    /// `~/.apollia/pipelines.db`.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineRepositoryError::Sqlite`] if the file cannot be
    /// opened or the migration fails.
    pub fn open(path: &str) -> Result<Self, PipelineRepositoryError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(MIGRATION_SQL)?;
        Ok(Self { conn })
    }

    /// Opens an in-memory database for unit testing.
    ///
    /// The migration is applied automatically so the database is immediately
    /// usable without a separate [`migrate`](Self::migrate) call.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineRepositoryError::Sqlite`] if the in-memory database
    /// cannot be initialised.
    pub fn open_in_memory() -> Result<Self, PipelineRepositoryError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(MIGRATION_SQL)?;
        Ok(Self { conn })
    }

    /// Applies the `006_pipeline_tables.sql` migration idempotently.
    ///
    /// All DDL statements use `IF NOT EXISTS`, so repeated calls are safe.
    /// This method is useful when the caller controls the migration lifecycle
    /// separately from the constructor.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineRepositoryError::Sqlite`] if the migration SQL fails.
    pub fn migrate(&mut self) -> Result<(), PipelineRepositoryError> {
        self.conn.execute_batch(MIGRATION_SQL)?;
        Ok(())
    }

    // ── Write operations ──────────────────────────────────────────────────────

    /// Inserts a new pipeline run record with its initial status.
    ///
    /// The `run.status` is serialised to one of the SQL strings
    /// `running | waiting_approval | completed | failed`.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineRepositoryError::Sqlite`] on constraint violations or
    /// other SQLite errors.
    pub fn insert_run(&mut self, run: &PipelineRun) -> Result<(), PipelineRepositoryError> {
        let status_str = pipeline_status_to_str(&run.status);
        let started_at = run.started_at.to_rfc3339();
        self.conn.execute(
            "INSERT INTO pipeline_runs \
             (run_id, pipeline_id, trigger_id, status, trigger_payload, started_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                run.run_id.0,
                run.pipeline_id.0,
                run.trigger_id,
                status_str,
                run.trigger_payload,
                started_at,
            ],
        )?;
        Ok(())
    }

    /// Inserts a step run record linked to an existing pipeline run.
    ///
    /// # Errors
    ///
    /// - [`PipelineRepositoryError::StepAlreadyExists`] if the
    ///   `(run_id, step_id)` pair already exists (AC-6).
    /// - [`PipelineRepositoryError::Sqlite`] for other database errors.
    pub fn insert_step(
        &mut self,
        run_id: &RunId,
        step: &StepRun,
        agent_name: &str,
    ) -> Result<(), PipelineRepositoryError> {
        let status_str = step_status_to_str(&step.status);
        let result = self.conn.execute(
            "INSERT INTO pipeline_step_runs \
             (run_id, step_id, task_id, agent_name, status, output, error, \
              started_at, ended_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                run_id.0,
                step.step_id.0,
                step.task_id,
                agent_name,
                status_str,
                step.output,
                step.error,
                step.started_at.map(|t| t.to_rfc3339()),
                step.ended_at.map(|t| t.to_rfc3339()),
            ],
        );
        match result {
            Err(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Err(PipelineRepositoryError::StepAlreadyExists(
                    run_id.0.clone(),
                    step.step_id.0.clone(),
                ))
            }
            Err(e) => Err(PipelineRepositoryError::Sqlite(e)),
            Ok(_) => Ok(()),
        }
    }

    /// Updates the status, output, error, and task_id of a step run.
    ///
    /// Also records timestamps automatically:
    /// - `started_at = CURRENT_TIMESTAMP` when `status` is `Running`.
    /// - `ended_at = CURRENT_TIMESTAMP` when `status` is a terminal variant
    ///   (`Completed`, `Failed`, `Skipped`, `FallbackActive`).
    ///
    /// # Errors
    ///
    /// Returns [`PipelineRepositoryError::Sqlite`] on database errors.
    pub fn update_step(
        &mut self,
        run_id: &RunId,
        step_id: &StepId,
        status: &StepRunStatus,
        output: Option<&str>,
        error: Option<&str>,
        task_id: Option<&str>,
    ) -> Result<(), PipelineRepositoryError> {
        let status_str = step_status_to_str(status);
        let is_starting = matches!(status, StepRunStatus::Running);
        let is_terminal = matches!(
            status,
            StepRunStatus::Completed
                | StepRunStatus::Failed
                | StepRunStatus::Skipped
                | StepRunStatus::FallbackActive
        );
        if is_starting {
            self.conn.execute(
                "UPDATE pipeline_step_runs \
                 SET status = ?3, task_id = ?4, output = ?5, error = ?6, \
                     started_at = CURRENT_TIMESTAMP \
                 WHERE run_id = ?1 AND step_id = ?2",
                params![run_id.0, step_id.0, status_str, task_id, output, error],
            )?;
        } else if is_terminal {
            self.conn.execute(
                "UPDATE pipeline_step_runs \
                 SET status = ?3, task_id = ?4, output = ?5, error = ?6, \
                     ended_at = CURRENT_TIMESTAMP \
                 WHERE run_id = ?1 AND step_id = ?2",
                params![run_id.0, step_id.0, status_str, task_id, output, error],
            )?;
        } else {
            self.conn.execute(
                "UPDATE pipeline_step_runs \
                 SET status = ?3, task_id = ?4, output = ?5, error = ?6 \
                 WHERE run_id = ?1 AND step_id = ?2",
                params![run_id.0, step_id.0, status_str, task_id, output, error],
            )?;
        }
        Ok(())
    }

    /// Marks a run as `completed` and records `ended_at`.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineRepositoryError::Sqlite`] on database errors.
    pub fn complete_run(&mut self, run_id: &RunId) -> Result<(), PipelineRepositoryError> {
        self.conn.execute(
            "UPDATE pipeline_runs \
             SET status = 'completed', ended_at = CURRENT_TIMESTAMP \
             WHERE run_id = ?1",
            params![run_id.0],
        )?;
        Ok(())
    }

    /// Marks a run as `failed`, recording the responsible step and reason.
    ///
    /// Also sets `status = 'failed'` and `error = reason` on the failing step
    /// row in `pipeline_step_runs` so the reason can be retrieved by
    /// [`find_run`](Self::find_run) without extra parameters.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineRepositoryError::Sqlite`] on database errors.
    pub fn fail_run(
        &mut self,
        run_id: &RunId,
        failed_step: &StepId,
        reason: &str,
    ) -> Result<(), PipelineRepositoryError> {
        self.conn.execute(
            "UPDATE pipeline_runs \
             SET status = 'failed', failed_step = ?2, ended_at = CURRENT_TIMESTAMP \
             WHERE run_id = ?1",
            params![run_id.0, failed_step.0],
        )?;
        // Best-effort update on the step run — the step may not have been
        // inserted yet if the failure occurred before submission.
        self.conn.execute(
            "UPDATE pipeline_step_runs \
             SET status = 'failed', error = ?3, ended_at = CURRENT_TIMESTAMP \
             WHERE run_id = ?1 AND step_id = ?2",
            params![run_id.0, failed_step.0, reason],
        )?;
        Ok(())
    }

    /// Marks the run as `waiting_approval` (HITL suspend) and updates the
    /// suspending step to `waiting_approval` with its associated `task_id`.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineRepositoryError::Sqlite`] on database errors.
    pub fn suspend_run(
        &mut self,
        run_id: &RunId,
        step_id: &StepId,
        task_id: &str,
    ) -> Result<(), PipelineRepositoryError> {
        self.conn.execute(
            "UPDATE pipeline_runs SET status = 'waiting_approval' WHERE run_id = ?1",
            params![run_id.0],
        )?;
        self.conn.execute(
            "UPDATE pipeline_step_runs \
             SET status = 'waiting_approval', task_id = ?3 \
             WHERE run_id = ?1 AND step_id = ?2",
            params![run_id.0, step_id.0, task_id],
        )?;
        Ok(())
    }

    /// Restores a run to `running` status after a HITL approval.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineRepositoryError::Sqlite`] on database errors.
    pub fn resume_run(&mut self, run_id: &RunId) -> Result<(), PipelineRepositoryError> {
        self.conn.execute(
            "UPDATE pipeline_runs SET status = 'running' WHERE run_id = ?1",
            params![run_id.0],
        )?;
        Ok(())
    }

    // ── Read operations ───────────────────────────────────────────────────────

    /// Loads a pipeline run and all of its step runs by `run_id`.
    ///
    /// Returns `None` if no run with the given identifier exists.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineRepositoryError::Sqlite`] on database errors.
    pub fn find_run(&self, run_id: &RunId) -> Result<Option<PipelineRun>, PipelineRepositoryError> {
        let result = self.conn.query_row(
            "SELECT run_id, pipeline_id, trigger_id, status, failed_step, \
                    trigger_payload, started_at, ended_at \
             FROM pipeline_runs WHERE run_id = ?1",
            params![run_id.0],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            },
        );

        let (
            rid,
            pid,
            trigger_id,
            status_str,
            failed_step,
            trigger_payload,
            started_at_str,
            ended_at_str,
        ) = match result {
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(e) => return Err(PipelineRepositoryError::Sqlite(e)),
            Ok(row) => row,
        };

        let step_runs = self.load_step_runs(&RunId(rid.clone()), false)?;
        let status = str_to_pipeline_status(&status_str, &failed_step, &step_runs);
        let started_at = parse_timestamp(&started_at_str);
        let ended_at = ended_at_str.and_then(|s| parse_timestamp_opt(&s));

        Ok(Some(PipelineRun {
            run_id: RunId(rid),
            pipeline_id: PipelineId(pid),
            trigger_id,
            status,
            step_runs,
            trigger_payload,
            started_at,
            ended_at,
        }))
    }

    /// Returns all runs currently in `running` status.
    ///
    /// Steps that were in `running` status when the process crashed are
    /// automatically reset to `pending` so the `PipelineExecutor` can
    /// re-submit them on restart (STORY-115 resumption logic).
    ///
    /// # Errors
    ///
    /// Returns [`PipelineRepositoryError::Sqlite`] on database errors.
    pub fn find_running_runs(&self) -> Result<Vec<PipelineRun>, PipelineRepositoryError> {
        let mut stmt = self.conn.prepare(
            "SELECT run_id, pipeline_id, trigger_id, trigger_payload, started_at \
             FROM pipeline_runs WHERE status = 'running'",
        )?;

        let run_rows: Vec<RunningRunRow> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut runs = Vec::with_capacity(run_rows.len());
        for (rid, pid, trigger_id, trigger_payload, started_at_str) in run_rows {
            // Load steps and reset any 'running' ones back to 'pending'.
            let step_runs = self.load_step_runs(&RunId(rid.clone()), true)?;
            let started_at = parse_timestamp(&started_at_str);
            runs.push(PipelineRun {
                run_id: RunId(rid),
                pipeline_id: PipelineId(pid),
                trigger_id,
                status: PipelineStatus::Running,
                step_runs,
                trigger_payload,
                started_at,
                ended_at: None,
            });
        }
        Ok(runs)
    }

    /// Returns the run history for a pipeline ordered by `started_at` descending.
    ///
    /// At most `limit` rows are returned.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineRepositoryError::Sqlite`] on database errors.
    pub fn find_runs_by_pipeline(
        &self,
        pipeline_id: &PipelineId,
        limit: u32,
    ) -> Result<Vec<PipelineRun>, PipelineRepositoryError> {
        let mut stmt = self.conn.prepare(
            "SELECT run_id, trigger_id, status, failed_step, \
                    trigger_payload, started_at, ended_at \
             FROM pipeline_runs WHERE pipeline_id = ?1 \
             ORDER BY started_at DESC LIMIT ?2",
        )?;

        let run_rows: Vec<PipelineRunRow> = stmt
            .query_map(params![pipeline_id.0, limit], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut runs = Vec::with_capacity(run_rows.len());
        for (
            rid,
            trigger_id,
            status_str,
            failed_step,
            trigger_payload,
            started_at_str,
            ended_at_str,
        ) in run_rows
        {
            let step_runs = self.load_step_runs(&RunId(rid.clone()), false)?;
            let status = str_to_pipeline_status(&status_str, &failed_step, &step_runs);
            let started_at = parse_timestamp(&started_at_str);
            let ended_at = ended_at_str.and_then(|s| parse_timestamp_opt(&s));
            runs.push(PipelineRun {
                run_id: RunId(rid),
                pipeline_id: pipeline_id.clone(),
                trigger_id,
                status,
                step_runs,
                trigger_payload,
                started_at,
                ended_at,
            });
        }
        Ok(runs)
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Loads all step runs for the given `run_id` into a `HashMap`.
    ///
    /// If `reset_running_to_pending` is `true`, any step whose SQL status is
    /// `running` is returned with [`StepRunStatus::Pending`] instead — used
    /// by [`find_running_runs`](Self::find_running_runs) for restart recovery.
    fn load_step_runs(
        &self,
        run_id: &RunId,
        reset_running_to_pending: bool,
    ) -> Result<HashMap<StepId, StepRun>, PipelineRepositoryError> {
        let mut stmt = self.conn.prepare(
            "SELECT step_id, task_id, status, output, error, started_at, ended_at \
             FROM pipeline_step_runs WHERE run_id = ?1",
        )?;

        let rows: Vec<StepRunRow> = stmt
            .query_map(params![run_id.0], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut step_runs = HashMap::with_capacity(rows.len());
        for (sid, task_id, status_str, output, error, started_at_str, ended_at_str) in rows {
            let mut status = str_to_step_status(&status_str);
            if reset_running_to_pending && status == StepRunStatus::Running {
                status = StepRunStatus::Pending;
            }
            let started_at = started_at_str.and_then(|s| parse_timestamp_opt(&s));
            let ended_at = ended_at_str.and_then(|s| parse_timestamp_opt(&s));
            let step_id = StepId(sid);
            step_runs.insert(
                step_id.clone(),
                StepRun {
                    step_id,
                    task_id,
                    status,
                    output,
                    error,
                    started_at,
                    ended_at,
                },
            );
        }
        Ok(step_runs)
    }
}

// ── Conversion helpers ────────────────────────────────────────────────────────

/// Maps a [`PipelineStatus`] to its SQL string representation.
fn pipeline_status_to_str(status: &PipelineStatus) -> &'static str {
    match status {
        PipelineStatus::Running => "running",
        PipelineStatus::WaitingApproval { .. } => "waiting_approval",
        PipelineStatus::Completed => "completed",
        PipelineStatus::Failed { .. } => "failed",
    }
}

/// Reconstructs a [`PipelineStatus`] from a SQL status string.
///
/// For `waiting_approval`, the step and task IDs are resolved from the
/// already-loaded `step_runs` map by finding the step whose status is
/// [`StepRunStatus::WaitingApproval`].
///
/// For `failed`, the failure reason is resolved from the `error` column of the
/// failed step identified by `failed_step`.
fn str_to_pipeline_status(
    s: &str,
    failed_step: &Option<String>,
    step_runs: &HashMap<StepId, StepRun>,
) -> PipelineStatus {
    match s {
        "completed" => PipelineStatus::Completed,
        "failed" => {
            let step_id = StepId(failed_step.clone().unwrap_or_default());
            let reason = step_runs
                .get(&step_id)
                .and_then(|sr| sr.error.clone())
                .unwrap_or_default();
            PipelineStatus::Failed { step_id, reason }
        }
        "waiting_approval" => {
            // Locate the step that is currently suspended for approval.
            match step_runs
                .values()
                .find(|sr| sr.status == StepRunStatus::WaitingApproval)
            {
                Some(sr) => PipelineStatus::WaitingApproval {
                    step_id: sr.step_id.clone(),
                    task_id: sr.task_id.clone().unwrap_or_default(),
                },
                // Defensive fallback — should not occur in normal operation.
                None => PipelineStatus::Running,
            }
        }
        _ => PipelineStatus::Running,
    }
}

/// Maps a [`StepRunStatus`] to its SQL string representation.
fn step_status_to_str(status: &StepRunStatus) -> &'static str {
    match status {
        StepRunStatus::Pending => "pending",
        StepRunStatus::Running => "running",
        StepRunStatus::WaitingApproval => "waiting_approval",
        StepRunStatus::Completed => "completed",
        StepRunStatus::Failed => "failed",
        StepRunStatus::Skipped => "skipped",
        StepRunStatus::FallbackActive => "fallback_active",
    }
}

/// Parses a SQL status string back to [`StepRunStatus`].
///
/// Unknown strings fall back to [`StepRunStatus::Pending`] rather than
/// panicking, which preserves forward compatibility.
fn str_to_step_status(s: &str) -> StepRunStatus {
    match s {
        "running" => StepRunStatus::Running,
        "waiting_approval" => StepRunStatus::WaitingApproval,
        "completed" => StepRunStatus::Completed,
        "failed" => StepRunStatus::Failed,
        "skipped" => StepRunStatus::Skipped,
        "fallback_active" => StepRunStatus::FallbackActive,
        _ => StepRunStatus::Pending,
    }
}

/// Parses a timestamp string into a `DateTime<Utc>`.
///
/// Tries RFC 3339 first, then SQLite's default `YYYY-MM-DD HH:MM:SS` format.
/// Falls back to `Utc::now()` if both fail — guarantees a valid value without
/// panicking.
fn parse_timestamp(s: &str) -> DateTime<Utc> {
    parse_timestamp_opt(s).unwrap_or_else(Utc::now)
}

/// Parses a timestamp string into a `DateTime<Utc>`, returning `None` on failure.
///
/// Tries RFC 3339 first (used when this crate writes timestamps), then
/// SQLite's default `YYYY-MM-DD HH:MM:SS` format (used by `CURRENT_TIMESTAMP`
/// in `DEFAULT` clauses or `SET col = CURRENT_TIMESTAMP` statements).
fn parse_timestamp_opt(s: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|ndt| ndt.and_utc())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use chrono::Utc;
    use std::collections::HashMap;

    /// Builds a minimal `PipelineRun` for test fixtures.
    fn make_run(run_id: &str, pipeline_id: &str) -> PipelineRun {
        PipelineRun {
            run_id: RunId(run_id.into()),
            pipeline_id: PipelineId(pipeline_id.into()),
            trigger_id: None,
            status: PipelineStatus::Running,
            step_runs: HashMap::new(),
            trigger_payload: Some("test.pdf".into()),
            started_at: Utc::now(),
            ended_at: None,
        }
    }

    /// Builds a minimal `StepRun` for test fixtures.
    fn make_step(step_id: &str) -> StepRun {
        StepRun {
            step_id: StepId(step_id.into()),
            task_id: None,
            status: StepRunStatus::Pending,
            output: None,
            error: None,
            started_at: None,
            ended_at: None,
        }
    }

    // AC-1 — insert_run followed by find_run returns the same run

    #[test]
    fn test_ac1_insert_and_find_run() {
        // GIVEN
        let mut repo = PipelineRepository::open_in_memory().unwrap();
        repo.migrate().unwrap();
        let run = make_run("r-0001", "pipeline-test");

        // WHEN
        repo.insert_run(&run).unwrap();
        let found = repo.find_run(&run.run_id).unwrap();

        // THEN
        let found = found.expect("run must be present after insert");
        assert_eq!(found.run_id, run.run_id);
        assert_eq!(found.pipeline_id, run.pipeline_id);
        assert_eq!(found.status, PipelineStatus::Running);
        assert_eq!(found.trigger_payload.as_deref(), Some("test.pdf"));
    }

    // AC-2 — update_step with Completed sets output and status correctly

    #[test]
    fn test_ac2_update_step_completed() {
        // GIVEN
        let mut repo = PipelineRepository::open_in_memory().unwrap();
        repo.migrate().unwrap();
        let run = make_run("r-0002", "pipeline-test");
        repo.insert_run(&run).unwrap();
        repo.insert_step(&run.run_id, &make_step("ocr"), "ocr-agent")
            .unwrap();

        // WHEN
        repo.update_step(
            &run.run_id,
            &StepId("ocr".into()),
            &StepRunStatus::Completed,
            Some("texte extrait"),
            None,
            Some("t-0001"),
        )
        .unwrap();

        // THEN
        let found = repo.find_run(&run.run_id).unwrap().unwrap();
        let ocr = found
            .step_runs
            .get(&StepId("ocr".into()))
            .expect("ocr step must exist");
        assert_eq!(ocr.status, StepRunStatus::Completed);
        assert_eq!(ocr.output.as_deref(), Some("texte extrait"));
        assert_eq!(ocr.task_id.as_deref(), Some("t-0001"));
    }

    // AC-3 — complete_run sets status=Completed and ended_at

    #[test]
    fn test_ac3_complete_run() {
        // GIVEN
        let mut repo = PipelineRepository::open_in_memory().unwrap();
        repo.migrate().unwrap();
        let run = make_run("r-0003", "pipeline-test");
        repo.insert_run(&run).unwrap();

        // WHEN
        repo.complete_run(&run.run_id).unwrap();

        // THEN
        let found = repo.find_run(&run.run_id).unwrap().unwrap();
        assert_eq!(found.status, PipelineStatus::Completed);
        assert!(
            found.ended_at.is_some(),
            "ended_at must be set after completion"
        );
    }

    // AC-4 — fail_run sets status=Failed with the correct step_id

    #[test]
    fn test_ac4_fail_run() {
        // GIVEN
        let mut repo = PipelineRepository::open_in_memory().unwrap();
        repo.migrate().unwrap();
        let run = make_run("r-0004", "pipeline-test");
        repo.insert_run(&run).unwrap();
        repo.insert_step(&run.run_id, &make_step("ocr"), "ocr-agent")
            .unwrap();

        // WHEN
        repo.fail_run(&run.run_id, &StepId("ocr".into()), "timeout agent")
            .unwrap();

        // THEN
        let found = repo.find_run(&run.run_id).unwrap().unwrap();
        match found.status {
            PipelineStatus::Failed { step_id, reason } => {
                assert_eq!(step_id, StepId("ocr".into()));
                assert_eq!(reason, "timeout agent");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    // AC-5 — find_running_runs returns only runs in 'running' status

    #[test]
    fn test_ac5_find_running_runs_excludes_completed() {
        // GIVEN
        let mut repo = PipelineRepository::open_in_memory().unwrap();
        repo.migrate().unwrap();
        repo.insert_run(&make_run("r-running-1", "p1")).unwrap();
        repo.insert_run(&make_run("r-running-2", "p1")).unwrap();
        let completed = make_run("r-done", "p1");
        repo.insert_run(&completed).unwrap();
        repo.complete_run(&completed.run_id).unwrap();

        // WHEN
        let running = repo.find_running_runs().unwrap();

        // THEN
        assert_eq!(running.len(), 2, "only 2 running runs should be returned");
        assert!(
            running.iter().all(|r| r.status == PipelineStatus::Running),
            "all returned runs must have status=Running"
        );
    }

    // AC-6 — inserting a step twice returns StepAlreadyExists

    #[test]
    fn test_ac6_step_already_exists() {
        // GIVEN
        let mut repo = PipelineRepository::open_in_memory().unwrap();
        repo.migrate().unwrap();
        let run = make_run("r-0006", "pipeline-test");
        repo.insert_run(&run).unwrap();
        repo.insert_step(&run.run_id, &make_step("ocr"), "ocr-agent")
            .unwrap();

        // WHEN — inserting the same step a second time
        let result = repo.insert_step(&run.run_id, &make_step("ocr"), "ocr-agent");

        // THEN
        assert!(
            matches!(result, Err(PipelineRepositoryError::StepAlreadyExists(..))),
            "expected StepAlreadyExists, got {result:?}"
        );
    }

    // Additional — find_running_runs resets 'running' steps to 'pending'

    #[test]
    fn test_running_steps_reset_to_pending_on_restart() {
        // GIVEN — a run with a step that was 'running' when the process crashed
        let mut repo = PipelineRepository::open_in_memory().unwrap();
        repo.migrate().unwrap();
        let run = make_run("r-restart", "p1");
        repo.insert_run(&run).unwrap();
        repo.insert_step(&run.run_id, &make_step("ocr"), "ocr-agent")
            .unwrap();
        repo.update_step(
            &run.run_id,
            &StepId("ocr".into()),
            &StepRunStatus::Running,
            None,
            None,
            Some("t-crash"),
        )
        .unwrap();

        // WHEN
        let running = repo.find_running_runs().unwrap();

        // THEN — the step is returned as Pending, not Running
        let step = running[0]
            .step_runs
            .get(&StepId("ocr".into()))
            .expect("ocr step must be present");
        assert_eq!(
            step.status,
            StepRunStatus::Pending,
            "running steps must be reset to pending on restart"
        );
    }

    // Additional — suspend_run + resume_run round-trip

    #[test]
    fn test_suspend_and_resume_run() {
        // GIVEN
        let mut repo = PipelineRepository::open_in_memory().unwrap();
        repo.migrate().unwrap();
        let run = make_run("r-hitl", "p1");
        repo.insert_run(&run).unwrap();
        repo.insert_step(&run.run_id, &make_step("validation"), "val-agent")
            .unwrap();

        // WHEN — suspend
        repo.suspend_run(&run.run_id, &StepId("validation".into()), "t-approval-001")
            .unwrap();
        let suspended = repo.find_run(&run.run_id).unwrap().unwrap();
        assert!(
            matches!(suspended.status, PipelineStatus::WaitingApproval { .. }),
            "status must be WaitingApproval after suspend"
        );

        // WHEN — resume
        repo.resume_run(&run.run_id).unwrap();
        let resumed = repo.find_run(&run.run_id).unwrap().unwrap();

        // THEN
        assert_eq!(
            resumed.status,
            PipelineStatus::Running,
            "status must be Running after resume"
        );
    }

    // Additional — find_runs_by_pipeline respects limit and pipeline_id

    #[test]
    fn test_find_runs_by_pipeline() {
        // GIVEN
        let mut repo = PipelineRepository::open_in_memory().unwrap();
        repo.migrate().unwrap();
        repo.insert_run(&make_run("r-p1-1", "pipeline-a")).unwrap();
        repo.insert_run(&make_run("r-p1-2", "pipeline-a")).unwrap();
        repo.insert_run(&make_run("r-p2-1", "pipeline-b")).unwrap();

        // WHEN
        let runs_a = repo
            .find_runs_by_pipeline(&PipelineId("pipeline-a".into()), 10)
            .unwrap();
        let runs_b = repo
            .find_runs_by_pipeline(&PipelineId("pipeline-b".into()), 10)
            .unwrap();
        let runs_limited = repo
            .find_runs_by_pipeline(&PipelineId("pipeline-a".into()), 1)
            .unwrap();

        // THEN
        assert_eq!(runs_a.len(), 2, "pipeline-a must have 2 runs");
        assert_eq!(runs_b.len(), 1, "pipeline-b must have 1 run");
        assert_eq!(runs_limited.len(), 1, "limit=1 must return only 1 run");
    }
}
