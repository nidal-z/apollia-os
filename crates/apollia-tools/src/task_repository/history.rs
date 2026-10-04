//! Removing the history of agents and workflows from `hitl.db`.
//!
//! The history of an agent is its tasks (with their approvals) and its
//! batches (with their items); the history of a workflow is its runs (with
//! their steps). Removed in one transaction, when the user uninstalls them
//! and chooses not to keep it.

use rusqlite::params;

use crate::task_repository::{TaskRepoError, TaskRepository};

/// What [`TaskRepository::remove_history`] removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HistoryRemoved {
    /// Tasks removed.
    pub tasks: u64,
    /// Batches removed.
    pub batches: u64,
    /// Workflow runs removed.
    pub workflow_runs: u64,
}

impl TaskRepository {
    /// Removes the tasks and batches of each of `agents`, and the runs of
    /// each of `workflows` (`<package>/<workflow>`), with the rows that hang
    /// off them, in one transaction.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`].
    pub async fn remove_history(
        &self,
        agents: Vec<String>,
        workflows: Vec<String>,
    ) -> Result<HistoryRemoved, TaskRepoError> {
        self.blocking(move |conn| {
            let tx = conn.transaction()?;
            let mut removed = HistoryRemoved::default();
            for agent in &agents {
                tx.execute(
                    "DELETE FROM task_approvals WHERE task_id IN \
                     (SELECT task_id FROM tasks WHERE agent_name = ?1)",
                    params![agent],
                )?;
                tx.execute(
                    "DELETE FROM batch_items WHERE batch_id IN \
                     (SELECT batch_id FROM batches WHERE agent_name = ?1)",
                    params![agent],
                )?;
                removed.batches +=
                    count(tx.execute("DELETE FROM batches WHERE agent_name = ?1", params![agent])?);
                removed.tasks +=
                    count(tx.execute("DELETE FROM tasks WHERE agent_name = ?1", params![agent])?);
            }
            for workflow in &workflows {
                tx.execute(
                    "DELETE FROM workflow_steps WHERE run_id IN \
                     (SELECT run_id FROM workflow_runs WHERE workflow_id = ?1)",
                    params![workflow],
                )?;
                removed.workflow_runs += count(tx.execute(
                    "DELETE FROM workflow_runs WHERE workflow_id = ?1",
                    params![workflow],
                )?);
            }
            tx.commit()?;
            Ok(removed)
        })
        .await
    }
}

fn count(rows: usize) -> u64 {
    u64::try_from(rows).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests;
