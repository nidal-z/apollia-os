//! Tasks the router follows without running them.
//!
//! A task that runs outside the coordinators, such as an agent run by the
//! agent executor, is tracked here like a submitted one: it is listed, its
//! status and output are read, and a cancellation cancels its token, then
//! answers once its run reports that it has ended.

use super::*;

/// A task run elsewhere, to follow from its start.
pub struct TrackedRun {
    /// The task.
    pub task_id: TaskId,
    /// The agent that runs it.
    pub agent_id: AgentId,
    /// The run it journals under.
    pub run_id: RunId,
    /// Cancelled when the task is.
    pub cancel: CancellationToken,
}

impl<B: ExecutionBackend> TaskRouter<B> {
    /// Record `run` as working, with its token, agent and run.
    pub(super) fn handle_track(&mut self, run: TrackedRun) {
        let TrackedRun {
            task_id,
            agent_id,
            run_id,
            cancel,
        } = run;
        self.task_cancels.insert(task_id.clone(), cancel);
        self.task_statuses
            .insert(task_id.clone(), TaskStatus::Working);
        self.task_agents.insert(task_id.clone(), agent_id.clone());
        self.task_runs.insert(task_id.clone(), run_id);
        info!(task_id = %task_id, agent_id = %agent_id, "task.tracked");
    }
}

impl<B: ExecutionBackend> TaskRouterHandle<B> {
    /// Follow `run`, a task that runs outside the coordinators. Its outcome
    /// is read from the `TaskCompleted` event its runner emits, and its end
    /// is reported through [`Self::run_ended`].
    ///
    /// # Errors
    ///
    /// [`SubmitError::ActorDead`] when the router is gone.
    pub async fn track(&self, run: TrackedRun) -> Result<(), SubmitError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RouterMessage::Track {
                run,
                reply: reply_tx,
            })
            .await
            .map_err(|_| SubmitError::ActorDead)?;
        reply_rx.await.map_err(|_| SubmitError::ActorDead)
    }

    /// The run of a tracked task has ended, whatever its outcome. A
    /// cancellation waiting on it is answered.
    ///
    /// # Errors
    ///
    /// [`SubmitError::ActorDead`] when the router is gone.
    pub async fn run_ended(&self, task_id: &TaskId) -> Result<(), SubmitError> {
        self.tx
            .send(RouterMessage::ExecutionEnded {
                task_id: task_id.clone(),
            })
            .await
            .map_err(|_| SubmitError::ActorDead)
    }
}
