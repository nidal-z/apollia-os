//! The rows of a workflow run and of its steps, in `hitl.db`.
//!
//! A run is written at launch with one row per declared step, every step
//! `pending`. Each step then moves to `running` when the workflow opens it,
//! and to `completed` or `failed` when it closes. When the run ends, the step
//! still running takes the end of the run (`canceled` for a stopped run,
//! `failed` otherwise) and every step never opened becomes `not_started`, in
//! the same transaction as the run's own end.

use rusqlite::{params, OptionalExtension};

use crate::task_repository::{TaskRepoError, TaskRepository};

/// The time format of the workflow tables: UTC, with milliseconds.
const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%fZ', 'now')";

/// A workflow run as launched.
#[derive(Debug, Clone)]
pub struct NewWorkflowRun {
    /// The run.
    pub run_id: String,
    /// The workflow, `<package>/<workflow>`.
    pub workflow_id: String,
    /// The run whose step started this one, when one did.
    pub parent_run_id: Option<String>,
    /// 1 for a run started on its own, one more per nested call.
    pub depth: u32,
    /// The inputs, as validated.
    pub inputs: serde_json::Value,
    /// The declared steps, in order.
    pub steps: Vec<String>,
}

/// A workflow run as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowRunRow {
    pub run_id: String,
    pub workflow_id: String,
    pub parent_run_id: Option<String>,
    pub depth: u32,
    pub inputs: serde_json::Value,
    /// `running`, `completed`, `failed` or `canceled`.
    pub status: String,
    /// What the workflow returned.
    pub output: Option<serde_json::Value>,
    /// Why it failed or stopped.
    pub error: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
}

/// How a step ended.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowStepEnd {
    /// `completed`, `failed` or `canceled`.
    pub status: &'static str,
    /// What the step produced, when the runtime saw it.
    pub output: Option<String>,
    /// Why it failed or stopped.
    pub error: Option<String>,
}

/// One step of a workflow run as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowStepRow {
    /// Its position among the declared steps, from 0.
    pub rank: u32,
    pub name: String,
    /// `pending`, `running`, `completed`, `failed`, `canceled` or
    /// `not_started`.
    pub status: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    /// What the step produced, when the runtime saw it.
    pub output: Option<String>,
    /// Why it failed or stopped.
    pub error: Option<String>,
}

const RUN_COLUMNS: &str = "run_id, workflow_id, parent_run_id, depth, inputs_json, status, \
     output_json, error, started_at, ended_at";

const STEP_COLUMNS: &str = "rank, name, status, started_at, ended_at, output, error";

fn json_column(idx: usize, raw: &str) -> Result<serde_json::Value, rusqlite::Error> {
    serde_json::from_str(raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn small(row: &rusqlite::Row<'_>, idx: usize) -> Result<u32, rusqlite::Error> {
    Ok(u32::try_from(row.get::<_, i64>(idx)?).unwrap_or(0))
}

fn run_row(row: &rusqlite::Row<'_>) -> Result<WorkflowRunRow, rusqlite::Error> {
    let inputs: String = row.get(4)?;
    let output: Option<String> = row.get(6)?;
    Ok(WorkflowRunRow {
        run_id: row.get(0)?,
        workflow_id: row.get(1)?,
        parent_run_id: row.get(2)?,
        depth: small(row, 3)?,
        inputs: json_column(4, &inputs)?,
        status: row.get(5)?,
        output: output.map(|o| json_column(6, &o)).transpose()?,
        error: row.get(7)?,
        started_at: row.get(8)?,
        ended_at: row.get(9)?,
    })
}

fn step_row(row: &rusqlite::Row<'_>) -> Result<WorkflowStepRow, rusqlite::Error> {
    Ok(WorkflowStepRow {
        rank: small(row, 0)?,
        name: row.get(1)?,
        status: row.get(2)?,
        started_at: row.get(3)?,
        ended_at: row.get(4)?,
        output: row.get(5)?,
        error: row.get(6)?,
    })
}

fn get_run(
    conn: &rusqlite::Connection,
    run_id: &str,
) -> Result<Option<WorkflowRunRow>, TaskRepoError> {
    Ok(conn
        .query_row(
            &format!("SELECT {RUN_COLUMNS} FROM workflow_runs WHERE run_id = ?1"),
            params![run_id],
            run_row,
        )
        .optional()?)
}

impl TaskRepository {
    /// Records a run, `running`, and its declared steps, `pending`, in one
    /// transaction.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`] or [`TaskRepoError::Json`].
    pub async fn create_workflow_run(
        &self,
        run: NewWorkflowRun,
    ) -> Result<WorkflowRunRow, TaskRepoError> {
        let inputs = serde_json::to_string(&run.inputs)?;
        self.blocking(move |conn| {
            let tx = conn.transaction()?;
            tx.execute(
                "INSERT INTO workflow_runs (run_id, workflow_id, parent_run_id, depth, inputs_json) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    &run.run_id,
                    &run.workflow_id,
                    &run.parent_run_id,
                    run.depth,
                    &inputs
                ],
            )?;
            {
                let mut insert = tx.prepare(
                    "INSERT INTO workflow_steps (run_id, rank, name) VALUES (?1, ?2, ?3)",
                )?;
                for (rank, name) in run.steps.iter().enumerate() {
                    insert.execute(params![
                        &run.run_id,
                        i64::try_from(rank).unwrap_or(i64::MAX),
                        name
                    ])?;
                }
            }
            let row = get_run(&tx, &run.run_id)?
                .ok_or_else(|| TaskRepoError::NotFound(run.run_id.clone()))?;
            tx.commit()?;
            Ok(row)
        })
        .await
    }

    /// Moves the first `pending` step named `name` to `running`. Returns
    /// `false` when the run has no such step left to open.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`].
    pub async fn start_workflow_step(
        &self,
        run_id: &str,
        name: &str,
    ) -> Result<bool, TaskRepoError> {
        let (run_id, name) = (run_id.to_string(), name.to_string());
        self.blocking(move |conn| {
            let changed = conn.execute(
                &format!(
                    "UPDATE workflow_steps SET status = 'running', started_at = {NOW} \
                     WHERE run_id = ?1 AND rank = (SELECT MIN(rank) FROM workflow_steps \
                         WHERE run_id = ?1 AND name = ?2 AND status = 'pending')"
                ),
                params![&run_id, &name],
            )?;
            Ok(changed > 0)
        })
        .await
    }

    /// Ends the `running` step named `name` as `end` says.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`].
    pub async fn finish_workflow_step(
        &self,
        run_id: &str,
        name: &str,
        end: WorkflowStepEnd,
    ) -> Result<(), TaskRepoError> {
        let (run_id, name) = (run_id.to_string(), name.to_string());
        self.blocking(move |conn| {
            conn.execute(
                &format!(
                    "UPDATE workflow_steps SET status = ?3, ended_at = {NOW}, \
                         output = ?4, error = ?5 \
                     WHERE run_id = ?1 AND name = ?2 AND status = 'running'"
                ),
                params![&run_id, &name, end.status, &end.output, &end.error],
            )?;
            Ok(())
        })
        .await
    }

    /// Ends a run in `status`, `completed`, `failed` or `canceled`. The step
    /// still running takes `canceled` when the run was stopped and `failed`
    /// otherwise; every step never opened becomes `not_started`.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`] or [`TaskRepoError::Json`].
    pub async fn end_workflow_run(
        &self,
        run_id: &str,
        status: &str,
        output: Option<serde_json::Value>,
        error: Option<String>,
    ) -> Result<WorkflowRunRow, TaskRepoError> {
        let output = output.map(|o| serde_json::to_string(&o)).transpose()?;
        let (run_id, status) = (run_id.to_string(), status.to_string());
        self.blocking(move |conn| {
            let tx = conn.transaction()?;
            let open_step = if status == "canceled" {
                "canceled"
            } else {
                "failed"
            };
            tx.execute(
                &format!(
                    "UPDATE workflow_steps SET status = ?2, ended_at = {NOW}, \
                         error = COALESCE(error, ?3) \
                     WHERE run_id = ?1 AND status = 'running'"
                ),
                params![&run_id, open_step, &error],
            )?;
            tx.execute(
                "UPDATE workflow_steps SET status = 'not_started' \
                 WHERE run_id = ?1 AND status = 'pending'",
                params![&run_id],
            )?;
            tx.execute(
                &format!(
                    "UPDATE workflow_runs SET status = ?2, output_json = ?3, error = ?4, \
                         ended_at = {NOW} \
                     WHERE run_id = ?1"
                ),
                params![&run_id, &status, &output, &error],
            )?;
            let row =
                get_run(&tx, &run_id)?.ok_or_else(|| TaskRepoError::NotFound(run_id.clone()))?;
            tx.commit()?;
            Ok(row)
        })
        .await
    }

    /// Fails every run still `running`: a process that stopped left them,
    /// and a workflow does not resume. Returns how many were failed.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`].
    pub async fn fail_interrupted_workflow_runs(&self) -> Result<u64, TaskRepoError> {
        self.blocking(move |conn| {
            let tx = conn.transaction()?;
            let reason = "the runtime stopped during the run";
            tx.execute(
                &format!(
                    "UPDATE workflow_steps SET status = 'failed', ended_at = {NOW}, \
                         error = ?1 \
                     WHERE status = 'running' AND run_id IN \
                         (SELECT run_id FROM workflow_runs WHERE status = 'running')"
                ),
                params![reason],
            )?;
            tx.execute(
                "UPDATE workflow_steps SET status = 'not_started' \
                 WHERE status = 'pending' AND run_id IN \
                     (SELECT run_id FROM workflow_runs WHERE status = 'running')",
                [],
            )?;
            let failed = tx.execute(
                &format!(
                    "UPDATE workflow_runs SET status = 'failed', error = ?1, ended_at = {NOW} \
                     WHERE status = 'running'"
                ),
                params![reason],
            )?;
            tx.commit()?;
            Ok(u64::try_from(failed).unwrap_or(0))
        })
        .await
    }

    /// One run, or `None`.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`] or [`TaskRepoError::Json`].
    pub async fn workflow_run(
        &self,
        run_id: &str,
    ) -> Result<Option<WorkflowRunRow>, TaskRepoError> {
        let run_id = run_id.to_string();
        self.blocking(move |conn| get_run(conn, &run_id)).await
    }

    /// The steps of a run, in declared order.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`].
    pub async fn workflow_steps(
        &self,
        run_id: &str,
    ) -> Result<Vec<WorkflowStepRow>, TaskRepoError> {
        let run_id = run_id.to_string();
        self.blocking(move |conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {STEP_COLUMNS} FROM workflow_steps WHERE run_id = ?1 ORDER BY rank"
            ))?;
            let rows = stmt
                .query_map(params![&run_id], step_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
    }

    /// The runs of one workflow, newest first, at most `limit`.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`] or [`TaskRepoError::Json`].
    pub async fn workflow_runs(
        &self,
        workflow_id: &str,
        limit: u32,
    ) -> Result<Vec<WorkflowRunRow>, TaskRepoError> {
        let workflow_id = workflow_id.to_string();
        self.blocking(move |conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {RUN_COLUMNS} FROM workflow_runs WHERE workflow_id = ?1 \
                 ORDER BY started_at DESC, rowid DESC LIMIT ?2"
            ))?;
            let rows = stmt
                .query_map(params![&workflow_id, limit], run_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
    }
}

#[cfg(test)]
mod tests;
