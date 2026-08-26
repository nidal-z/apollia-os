//! The task lifecycle half of the repository.
//!
//! Split out of `task_repository.rs`: opening the store and reading a task
//! stay in the parent, the writes that move a task through its run (run id,
//! agent, output, transitions, duration, cancellation, deletion) live here.

use apollia_core::{truncate_with_marker, ObservabilityConfig};
use rusqlite::params;

use crate::task_repository::{open_conn, TaskRepoError, TaskRepository};

impl TaskRepository {
    /// Records the `run_id` this task belongs to.
    ///
    /// Called by the coordinator at submission so a `task_id` can later be
    /// resolved to its `run_id` (the key the audit journal is indexed by).
    ///
    /// # Errors
    ///
    /// Returns [`TaskRepoError::Sqlite`] on a SQLite error.
    pub async fn set_run_id(&self, task_id: &str, run_id: &str) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();
        let run_id = run_id.to_string();

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = open_conn(&path)?;
            conn.execute(
                "INSERT INTO tasks (task_id, run_id) VALUES (?1, ?2) \
                 ON CONFLICT(task_id) DO UPDATE SET \
                     run_id     = excluded.run_id, \
                     updated_at = CURRENT_TIMESTAMP",
                params![&task_id, &run_id],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))??;

        Ok(())
    }
    /// Returns the `run_id` recorded for `task_id`, if any.
    ///
    /// # Errors
    ///
    /// Returns [`TaskRepoError::Sqlite`] on a SQLite error.
    pub async fn get_run_id(&self, task_id: &str) -> Result<Option<String>, TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();

        tokio::task::spawn_blocking(move || -> Result<Option<String>, TaskRepoError> {
            let conn = open_conn(&path)?;
            let result = conn.query_row(
                "SELECT run_id FROM tasks WHERE task_id = ?1",
                params![&task_id],
                |row| row.get::<_, Option<String>>(0),
            );
            match result {
                Ok(v) => Ok(v),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(TaskRepoError::Sqlite(e)),
            }
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }
    /// Updates the agent name for a task.
    ///
    /// Called by the coordinator just after `save_input` to set the
    /// `agent_name` field (not available at the initial `INSERT`).
    ///
    /// # Errors
    ///
    /// Returns [`TaskRepoError::Sqlite`] on a SQLite error.
    pub async fn set_agent_name(
        &self,
        task_id: &str,
        agent_name: &str,
    ) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();
        let agent_name = agent_name.to_string();

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = open_conn(&path)?;
            conn.execute(
                "UPDATE tasks SET agent_name = ?2, updated_at = CURRENT_TIMESTAMP \
                 WHERE task_id = ?1",
                params![&task_id, &agent_name],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))??;

        Ok(())
    }
    /// Persists a task's output text, truncating if necessary.
    ///
    /// Uses [`truncate_with_marker`] to cut the output if its size exceeds
    /// `config.max_output_bytes`. The `output_truncated` column is set to 1 if
    /// the text was truncated, 0 otherwise.
    ///
    /// # Errors
    ///
    /// - [`TaskRepoError::Sqlite`] on a SQLite error
    pub async fn save_output(
        &self,
        task_id: &str,
        text: &str,
        config: &ObservabilityConfig,
    ) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();
        let (truncated_text, was_truncated) = truncate_with_marker(text, config.max_output_bytes);

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = open_conn(&path)?;
            conn.execute(
                "UPDATE tasks SET \
                     output_text      = ?2, \
                     output_truncated = ?3, \
                     updated_at       = CURRENT_TIMESTAMP \
                 WHERE task_id = ?1",
                params![&task_id, &truncated_text, was_truncated as i32],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))??;

        Ok(())
    }
    /// Appends a state transition to the `transitions_json` JSON array.
    ///
    /// Reads the existing JSON (or `[]` if absent), pushes
    /// `{"status": "<status>", "ts": "<timestamp>"}`, and rewrites it.
    /// Transitions are ordered chronologically by insertion order.
    ///
    /// # Errors
    ///
    /// - [`TaskRepoError::Sqlite`] on a SQLite error
    /// - [`TaskRepoError::Json`] if the existing JSON is invalid
    pub async fn append_transition(
        &self,
        task_id: &str,
        status: &str,
        timestamp: &str,
    ) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();
        let status = status.to_string();
        let timestamp = timestamp.to_string();

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = open_conn(&path)?;

            let existing: Option<String> = conn
                .query_row(
                    "SELECT transitions_json FROM tasks WHERE task_id = ?1",
                    params![&task_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .unwrap_or(None);

            let mut transitions: Vec<serde_json::Value> = match existing {
                Some(ref json) if !json.is_empty() => serde_json::from_str(json)?,
                _ => Vec::new(),
            };

            transitions.push(serde_json::json!({
                "status": status,
                "ts": timestamp,
            }));

            let new_json = serde_json::to_string(&transitions)?;

            conn.execute(
                "UPDATE tasks SET \
                     transitions_json = ?2, \
                     updated_at       = CURRENT_TIMESTAMP \
                 WHERE task_id = ?1",
                params![&task_id, &new_json],
            )?;

            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }
    /// Records the execution duration in milliseconds.
    ///
    /// Called by the coordinator when the task completes.
    ///
    /// # Errors
    ///
    /// Returns [`TaskRepoError::Sqlite`] on a SQLite error.
    pub async fn set_duration(&self, task_id: &str, duration_ms: i64) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = open_conn(&path)?;
            conn.execute(
                "UPDATE tasks SET \
                     duration_ms = ?2, \
                     updated_at  = CURRENT_TIMESTAMP \
                 WHERE task_id = ?1",
                params![&task_id, duration_ms],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))??;

        Ok(())
    }
    /// Cancels a task by setting its status to `cancelled` in the database.
    ///
    /// Persists `reason` in the `input_response_reason` column for traceability.
    /// Called by the `TimeoutWatcher` when an `input_required` suspension
    /// expires.
    ///
    /// # Errors
    ///
    /// Returns [`TaskRepoError::Sqlite`] on a SQLite error.
    pub async fn cancel_task(&self, task_id: &str, reason: &str) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();
        let reason = reason.to_string();

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = open_conn(&path)?;
            conn.execute(
                "UPDATE tasks SET \
                     status                = 'cancelled', \
                     input_response_reason = ?2, \
                     updated_at            = CURRENT_TIMESTAMP \
                 WHERE task_id = ?1",
                params![&task_id, &reason],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))??;

        Ok(())
    }
    /// Hard-deletes a task and its approval rows from the persisted store.
    ///
    /// Removes the `tasks` row plus every `task_approvals` row for `task_id`.
    /// Distinct from [`cancel_task`](Self::cancel_task), which keeps the record
    /// and only transitions its status to `cancelled`. Returns `true` when a
    /// task row existed and was removed, `false` when none matched.
    ///
    /// # Errors
    ///
    /// Returns [`TaskRepoError::Sqlite`] on a SQLite error.
    pub async fn delete_task(&self, task_id: &str) -> Result<bool, TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();

        tokio::task::spawn_blocking(move || -> Result<bool, TaskRepoError> {
            let conn = open_conn(&path)?;
            conn.execute(
                "DELETE FROM task_approvals WHERE task_id = ?1",
                params![&task_id],
            )?;
            let removed =
                conn.execute("DELETE FROM tasks WHERE task_id = ?1", params![&task_id])?;
            Ok(removed > 0)
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }
}
