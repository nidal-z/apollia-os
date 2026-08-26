//! The human-in-the-loop half of the task repository.
//!
//! Split out of `task_repository.rs`: opening the store and reading a task
//! stay in the parent, the rows that park a task on a human answer, resume it,
//! and report the approvals live here.

use std::time::Duration;

use apollia_core::{truncate_with_marker, AIPTask, InputResponseData, ObservabilityConfig};
use rusqlite::params;

use crate::task_repository::{
    open_conn, ApprovalInfo, ResolvedApprovalRow, TaskRepoError, TaskRepository,
};

impl TaskRepository {
    /// Persists an `input_required` suspension with its prompt and JSON context.
    ///
    /// Inserts or updates the `tasks` row with `status = 'input_required'` and
    /// `input_required_at = CURRENT_TIMESTAMP`. Called by ORIA before emitting
    /// `RuntimeEvent::TaskInputRequired` on the EventBus.
    ///
    /// # Errors
    ///
    /// - [`TaskRepoError::Json`] if `context` cannot be serialized
    /// - [`TaskRepoError::Sqlite`] on a SQLite error
    pub async fn save_input_required(
        &self,
        task_id: &str,
        step_id: Option<&str>,
        prompt: &str,
        context: &serde_json::Value,
    ) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();
        let step_id = step_id.map(|s| s.to_string());
        let prompt = prompt.to_string();
        let context_json = serde_json::to_string(context)?;

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = open_conn(&path)?;
            conn.execute(
                "INSERT INTO tasks \
                     (task_id, step_id, status, \
                      input_required_prompt, input_required_context, input_required_at) \
                 VALUES (?1, ?2, 'input_required', ?3, ?4, CURRENT_TIMESTAMP) \
                 ON CONFLICT(task_id) DO UPDATE SET \
                     step_id                = excluded.step_id, \
                     status                 = 'input_required', \
                     input_required_prompt  = excluded.input_required_prompt, \
                     input_required_context = excluded.input_required_context, \
                     input_required_at      = CURRENT_TIMESTAMP, \
                     updated_at             = CURRENT_TIMESTAMP",
                params![&task_id, &step_id, &prompt, &context_json],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))??;

        Ok(())
    }
    /// Records the suspension timestamp for an `input_required` event.
    ///
    /// Inserts a preliminary row into `task_approvals` with `suspended_at` set
    /// and `approved IS NULL`. The `prompt` and `context_json` are read from the
    /// `tasks` table (populated by [`save_input_required`]).
    ///
    /// Called by ORIA when emitting `AIPResult.input_required()`.
    ///
    /// # Errors
    ///
    /// - [`TaskRepoError::Sqlite`] on a SQLite error
    pub async fn save_suspended_at(
        &self,
        task_id: &str,
        step_id: Option<&str>,
        suspended_at: &str,
    ) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();
        let step_id = step_id.map(|s| s.to_string());
        let suspended_at = suspended_at.to_string();

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = open_conn(&path)?;
            conn.execute(
                "INSERT INTO task_approvals \
                     (task_id, step_id, prompt, context_json, suspended_at) \
                 SELECT ?1, ?2, \
                        COALESCE(input_required_prompt, ''), \
                        COALESCE(input_required_context, '{}'), \
                        ?3 \
                 FROM tasks WHERE task_id = ?1",
                params![&task_id, &step_id, &suspended_at],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))??;

        Ok(())
    }
    /// Persists the human response and inserts a row into `task_approvals`.
    ///
    /// Updates `input_response_approved`, `input_response_reason`,
    /// `input_response_at` and `status` in `tasks`, then inserts a row into
    /// `task_approvals` for the multi-approval history. Called by the
    /// `ResumeHandler` after the response is validated.
    ///
    /// # Errors
    ///
    /// - [`TaskRepoError::NotFound`] if `task_id` is absent from the `tasks` table
    /// - [`TaskRepoError::Json`] if the context cannot be serialized
    /// - [`TaskRepoError::Sqlite`] on a SQLite error
    pub async fn save_input_response(
        &self,
        task_id: &str,
        response: &InputResponseData,
    ) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();
        let approved = response.approved;
        let reason = response.reason.clone();
        let responded_at = response.responded_at.clone();
        let context_json = serde_json::to_string(&response.context)?;

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = open_conn(&path)?;

            // Fetch prompt and step_id for the task_approvals insert.
            let (prompt, step_id): (String, Option<String>) = conn
                .query_row(
                    "SELECT COALESCE(input_required_prompt, ''), step_id \
                     FROM tasks WHERE task_id = ?1",
                    params![&task_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
                )
                .map_err(|_| TaskRepoError::NotFound(task_id.clone()))?;

            // Update the tasks row with the human decision.
            conn.execute(
                "UPDATE tasks SET \
                     input_response_approved = ?2, \
                     input_response_reason   = ?3, \
                     input_response_at       = ?4, \
                     status                  = 'working', \
                     updated_at              = CURRENT_TIMESTAMP \
                 WHERE task_id = ?1",
                params![&task_id, approved as i32, &reason, &responded_at],
            )?;

            // Update the pending row created by save_suspended_at.
            // If no pending row exists (backward compat), fallback to INSERT.
            let updated = conn.execute(
                "UPDATE task_approvals \
                 SET approved = ?2, \
                     reason = ?3, \
                     responded_at = ?4, \
                     wait_duration_ms = CAST( \
                         (julianday(?4) - julianday(suspended_at)) * 86400000 AS INTEGER \
                     ) \
                 WHERE task_id = ?1 AND approved IS NULL",
                params![&task_id, approved as i32, &reason, &responded_at],
            )?;

            if updated == 0 {
                conn.execute(
                    "INSERT INTO task_approvals \
                         (task_id, step_id, prompt, context_json, approved, reason, responded_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        &task_id,
                        &step_id,
                        &prompt,
                        &context_json,
                        approved as i32,
                        &reason,
                        &responded_at,
                    ],
                )?;
            }

            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))??;

        Ok(())
    }
    /// Rebuilds an enriched [`AIPTask`] for resuming after `input_required`.
    ///
    /// Reads the `input_response_*` columns from `tasks` and builds an `AIPTask`
    /// with `is_resumed = true` and `input_response` populated with the human
    /// decision and the original JSON context. Called by the `ResumeHandler`
    /// before relaunching the agent through ORIA.
    ///
    /// # Errors
    ///
    /// - [`TaskRepoError::NotFound`] if `task_id` is absent from the `tasks` table
    /// - [`TaskRepoError::Json`] if the stored JSON context is invalid
    /// - [`TaskRepoError::Sqlite`] on a SQLite error
    pub async fn rebuild_for_resume(&self, task_id: &str) -> Result<AIPTask, TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();

        tokio::task::spawn_blocking(move || -> Result<AIPTask, TaskRepoError> {
            let conn = open_conn(&path)?;

            // Read the HITL columns needed for reconstruction.
            let (tid, approved_raw, reason, context_json_opt, responded_at_opt): (
                String,
                Option<i32>,
                Option<String>,
                Option<String>,
                Option<String>,
            ) = conn
                .query_row(
                    "SELECT task_id, \
                            input_response_approved, \
                            input_response_reason, \
                            input_required_context, \
                            input_response_at \
                     FROM tasks WHERE task_id = ?1",
                    params![&task_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<i32>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                        ))
                    },
                )
                .map_err(|_| TaskRepoError::NotFound(task_id.clone()))?;

            let input_response = match approved_raw {
                Some(raw) => {
                    let ctx_str = context_json_opt.as_deref().unwrap_or("{}");
                    let context: serde_json::Value = serde_json::from_str(ctx_str)?;
                    Some(InputResponseData {
                        approved: raw != 0,
                        reason,
                        context,
                        responded_at: responded_at_opt.unwrap_or_default(),
                    })
                }
                None => None,
            };

            Ok(AIPTask {
                task_id: tid,
                is_resumed: true,
                input_response,
                ..AIPTask::default()
            })
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }
    /// Persists a task's input text, truncating if necessary.
    ///
    /// Uses [`truncate_with_marker`] to cut the input if its size exceeds
    /// `config.max_input_bytes`. The `input_truncated` column is set to 1 if the
    /// text was truncated, 0 otherwise.
    ///
    /// Creates the `tasks` row via `INSERT ... ON CONFLICT DO UPDATE` so it can
    /// be called before or after `save_input_required`.
    ///
    /// # Errors
    ///
    /// Returns [`TaskRepoError::Sqlite`] on a SQLite error.
    pub async fn save_input(
        &self,
        task_id: &str,
        text: &str,
        config: &ObservabilityConfig,
    ) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();
        let (truncated_text, was_truncated) = truncate_with_marker(text, config.max_input_bytes);

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = open_conn(&path)?;
            conn.execute(
                "INSERT INTO tasks (task_id, input_text, input_truncated) \
                 VALUES (?1, ?2, ?3) \
                 ON CONFLICT(task_id) DO UPDATE SET \
                     input_text      = excluded.input_text, \
                     input_truncated = excluded.input_truncated, \
                     updated_at      = CURRENT_TIMESTAMP",
                params![&task_id, &truncated_text, was_truncated as i32],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))??;

        Ok(())
    }
    /// Returns the `task_id`s in `input_required` status for longer than `older_than`.
    ///
    /// Uses `strftime('%s', 'now') - strftime('%s', input_required_at)` to
    /// compute the elapsed seconds and compare them to the `older_than.as_secs()`
    /// threshold. Used by the `TimeoutWatcher` to cancel expired tasks.
    ///
    /// # Errors
    ///
    /// Returns [`TaskRepoError::Sqlite`] on a SQLite error.
    pub async fn find_input_required_older_than(
        &self,
        older_than: Duration,
    ) -> Result<Vec<String>, TaskRepoError> {
        let path = self.db_path.clone();
        let threshold_secs = older_than.as_secs() as i64;

        tokio::task::spawn_blocking(move || -> Result<Vec<String>, TaskRepoError> {
            let conn = open_conn(&path)?;

            let mut stmt = conn.prepare(
                "SELECT task_id FROM tasks \
                 WHERE status = 'input_required' \
                   AND input_required_at IS NOT NULL \
                   AND (strftime('%s', 'now') - strftime('%s', input_required_at)) > ?1",
            )?;

            let ids = stmt
                .query_map(params![threshold_secs], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            Ok(ids)
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }
    /// Returns the approval info for a task in `input_required` status.
    ///
    /// Reads the prompt, JSON context, and `suspended_at` from the `tasks` and
    /// `task_approvals` tables. Returns `None` if the task does not exist or is
    /// not in `input_required`.
    pub async fn get_approval_info(
        &self,
        task_id: &str,
    ) -> Result<Option<ApprovalInfo>, TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();

        tokio::task::spawn_blocking(move || -> Result<Option<ApprovalInfo>, TaskRepoError> {
            let conn = open_conn(&path)?;
            let mut stmt = conn.prepare(
                "SELECT t.agent_name, \
                        COALESCE(t.input_required_prompt, ''), \
                        COALESCE(t.input_required_context, '{}'), \
                        ta.suspended_at \
                 FROM tasks t \
                 LEFT JOIN task_approvals ta ON t.task_id = ta.task_id AND ta.approved IS NULL \
                 WHERE t.task_id = ?1 AND t.status = 'input_required' \
                 LIMIT 1",
            )?;

            let result = match stmt.query_row(params![task_id], |row| {
                let agent_name: String = row.get(0)?;
                let prompt: String = row.get(1)?;
                let context_str: String = row.get(2)?;
                let suspended_at: Option<String> = row.get(3)?;
                Ok((agent_name, prompt, context_str, suspended_at))
            }) {
                Ok(row) => Some(row),
                Err(rusqlite::Error::QueryReturnedNoRows) => None,
                Err(e) => return Err(TaskRepoError::Sqlite(e)),
            };

            match result {
                None => Ok(None),
                Some((agent_name, prompt, context_str, suspended_at)) => {
                    let context = serde_json::from_str(&context_str).unwrap_or_default();
                    Ok(Some(ApprovalInfo {
                        agent_name,
                        prompt,
                        context,
                        suspended_at: suspended_at.unwrap_or_default(),
                    }))
                }
            }
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }
    /// Lists resolved approvals (approved or rejected) from the last `days` days.
    ///
    /// Returns at most `limit` rows sorted by `responded_at` descending. Reads
    /// the `task_approvals` table joined to `tasks` to fetch the `agent_name`.
    ///
    /// # Errors
    ///
    /// - [`TaskRepoError::Sqlite`] on a SQLite error
    /// - [`TaskRepoError::Internal`] if the `spawn_blocking` fails
    pub async fn list_resolved_approvals(
        &self,
        limit: u32,
        days: u32,
    ) -> Result<Vec<ResolvedApprovalRow>, TaskRepoError> {
        let path = self.db_path.clone();

        tokio::task::spawn_blocking(move || -> Result<Vec<ResolvedApprovalRow>, TaskRepoError> {
            let conn = open_conn(&path)?;

            let mut stmt = conn.prepare(
                "SELECT ta.task_id, \
                        COALESCE(t.agent_name, ''), \
                        ta.approved, \
                        ta.reason, \
                        ta.suspended_at, \
                        ta.responded_at, \
                        ta.wait_duration_ms \
                 FROM task_approvals ta \
                 LEFT JOIN tasks t ON ta.task_id = t.task_id \
                 WHERE ta.approved IS NOT NULL \
                   AND ta.responded_at IS NOT NULL \
                   AND ta.responded_at >= datetime('now', ?1) \
                 ORDER BY ta.responded_at DESC \
                 LIMIT ?2",
            )?;

            let days_param = format!("-{days} days");
            let rows = stmt
                .query_map(params![&days_param, limit], |row| {
                    let approved_int: i32 = row.get(2)?;
                    Ok(ResolvedApprovalRow {
                        task_id: row.get(0)?,
                        agent_name: row.get(1)?,
                        approved: approved_int != 0,
                        reason: row.get(3)?,
                        suspended_at: row.get(4)?,
                        responded_at: row.get(5)?,
                        wait_duration_ms: row.get(6)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            Ok(rows)
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }
}
