//! The task row of an agent run outside the coordinators.
//!
//! Such a run is recorded at its start with its agent, its input whole and
//! its run, and at its end with its status, its output and its duration.

use apollia_core::{truncate_with_marker, ObservabilityConfig};
use rusqlite::params;

use crate::task_repository::{open_conn, TaskRepoError, TaskRepository};

/// The start of an agent run, as recorded.
#[derive(Debug, Clone)]
pub struct AgentRunStart<'a> {
    /// The task.
    pub task_id: &'a str,
    /// The agent that runs it.
    pub agent: &'a str,
    /// The run the audit journal records it under.
    pub run_id: &'a str,
    /// The input as given, kept whole.
    pub input: &'a serde_json::Value,
    /// The request the agent reads, kept cut at the observability limit.
    pub message: &'a str,
}

/// The end of an agent run, as recorded.
#[derive(Debug, Clone)]
pub struct AgentRunEnd<'a> {
    /// The task.
    pub task_id: &'a str,
    /// Its terminal status: `completed`, `failed` or `canceled`.
    pub status: &'a str,
    /// The answer, or the failure.
    pub output: &'a str,
    /// How long the run took.
    pub duration_ms: i64,
}

impl TaskRepository {
    /// Records the start of an agent run: its agent, its input whole, the
    /// request cut at `config.max_input_bytes`, its run, and the `working`
    /// status.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`] or [`TaskRepoError::Json`].
    pub async fn record_agent_run_start(
        &self,
        start: AgentRunStart<'_>,
        config: &ObservabilityConfig,
    ) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = start.task_id.to_string();
        let agent = start.agent.to_string();
        let run_id = start.run_id.to_string();
        let input_json = serde_json::to_string(start.input)?;
        let (input_text, truncated) = truncate_with_marker(start.message, config.max_input_bytes);

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = open_conn(&path)?;
            conn.execute(
                "INSERT INTO tasks \
                     (task_id, agent_name, status, run_id, input_json, input_text, input_truncated) \
                 VALUES (?1, ?2, 'working', ?3, ?4, ?5, ?6) \
                 ON CONFLICT(task_id) DO UPDATE SET \
                     agent_name      = excluded.agent_name, \
                     status          = excluded.status, \
                     run_id          = excluded.run_id, \
                     input_json      = excluded.input_json, \
                     input_text      = excluded.input_text, \
                     input_truncated = excluded.input_truncated, \
                     updated_at      = CURRENT_TIMESTAMP",
                params![
                    &task_id,
                    &agent,
                    &run_id,
                    &input_json,
                    &input_text,
                    truncated as i32
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }

    /// Records the end of an agent run: its terminal status, its output cut
    /// at `config.max_output_bytes`, and its duration.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`].
    pub async fn record_agent_run_end(
        &self,
        end: AgentRunEnd<'_>,
        config: &ObservabilityConfig,
    ) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = end.task_id.to_string();
        let status = end.status.to_string();
        let duration_ms = end.duration_ms;
        let (output_text, truncated) = truncate_with_marker(end.output, config.max_output_bytes);

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = open_conn(&path)?;
            conn.execute(
                "UPDATE tasks SET \
                     status           = ?2, \
                     output_text      = ?3, \
                     output_truncated = ?4, \
                     duration_ms      = ?5, \
                     updated_at       = CURRENT_TIMESTAMP \
                 WHERE task_id = ?1",
                params![
                    &task_id,
                    &status,
                    &output_text,
                    truncated as i32,
                    duration_ms
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn repo(dir: &tempfile::TempDir) -> TaskRepository {
        TaskRepository::open(&dir.path().join("hitl.db"))
            .await
            .unwrap()
    }

    // GIVEN an input larger than the observability limit
    // WHEN the start of a run is recorded
    // THEN the input reads back whole, the request is cut, and the row names
    //      the agent, the run and the working status
    #[tokio::test]
    async fn test_record_agent_run_start_keeps_the_input_whole() {
        // GIVEN
        let dir = tempfile::tempdir().unwrap();
        let repo = repo(&dir).await;
        let config = ObservabilityConfig {
            max_input_bytes: 16,
            ..ObservabilityConfig::default()
        };
        let message = "Reconcile the ledger of March, then report every gap.";
        let input = serde_json::json!({"parts": [{"type": "text", "text": message}]});

        // WHEN
        repo.record_agent_run_start(
            AgentRunStart {
                task_id: "t-1",
                agent: "ledger-clerk",
                run_id: "run-1",
                input: &input,
                message,
            },
            &config,
        )
        .await
        .unwrap();

        // THEN
        let conn = rusqlite::Connection::open(dir.path().join("hitl.db")).unwrap();
        let whole: String = conn
            .query_row(
                "SELECT input_json FROM tasks WHERE task_id = 't-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&whole).unwrap(),
            input
        );
        let (agent, status, run_id, truncated): (String, String, String, i64) = conn
            .query_row(
                "SELECT agent_name, status, run_id, input_truncated FROM tasks WHERE task_id = 't-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            (agent.as_str(), status.as_str(), run_id.as_str(), truncated),
            ("ledger-clerk", "working", "run-1", 1)
        );
    }

    // GIVEN a recorded run start
    // WHEN its end is recorded as completed with an output
    // THEN the row carries the status, the output and the duration
    #[tokio::test]
    async fn test_record_agent_run_end_writes_status_and_output() {
        // GIVEN
        let dir = tempfile::tempdir().unwrap();
        let repo = repo(&dir).await;
        let config = ObservabilityConfig::default();
        let input = serde_json::json!({"parts": [{"type": "text", "text": "hi"}]});
        repo.record_agent_run_start(
            AgentRunStart {
                task_id: "t-2",
                agent: "ledger-clerk",
                run_id: "run-2",
                input: &input,
                message: "hi",
            },
            &config,
        )
        .await
        .unwrap();

        // WHEN
        repo.record_agent_run_end(
            AgentRunEnd {
                task_id: "t-2",
                status: "completed",
                output: "Two gaps found.",
                duration_ms: 42,
            },
            &config,
        )
        .await
        .unwrap();

        // THEN
        let conn = rusqlite::Connection::open(dir.path().join("hitl.db")).unwrap();
        let (status, output, duration): (String, String, i64) = conn
            .query_row(
                "SELECT status, output_text, duration_ms FROM tasks WHERE task_id = 't-2'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            (status.as_str(), output.as_str(), duration),
            ("completed", "Two gaps found.", 42)
        );
    }
}
