//! Running an agent on one request, outside any conversation.
//!
//! This is the path a task, a batch, a trigger or a workflow step takes to
//! make an agent work: the agent as resolved, one message, the agent executor.
//! The task is recorded in `hitl.db` with its agent, its input whole and its
//! output; the task router follows it, so it is listed, read and cancelled
//! like any other task; and its end is published on the bus, as
//! [`RuntimeEvent::TaskCompleted`] on success or failure and
//! [`RuntimeEvent::TaskCanceled`] once a cancellation has stopped it.

use std::sync::Arc;
use std::time::Instant;

use apollia_core::{AIPInput, AIPPart, AgentId, ObservabilityConfig, RunId, RuntimeEvent, TaskId};
use apollia_tools::{AgentRunEnd, AgentRunStart, ResolvedAgent, TaskRepository};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::chat::{
    AgentRunSpec, AgentTaskRun, ChatAgentResponse, ChatError, ChatSessionManagerHandle,
};
use crate::coordinator::ExecutionBackend;
use crate::eventbus::EventBusSender;
use crate::router::{SubmitError, TaskRouterHandle, TrackedRun};

/// Why an agent run could not start.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AgentTaskError {
    /// The agent is disabled.
    #[error("agent '{0}' is disabled")]
    Disabled(String),
    /// The task could not be recorded.
    #[error("cannot record the task: {0}")]
    Store(#[from] apollia_tools::TaskRepoError),
    /// The task router is gone.
    #[error(transparent)]
    Router(#[from] SubmitError),
    /// The executor could not start the run.
    #[error(transparent)]
    Executor(#[from] ChatError),
}

/// What an agent run outside a conversation goes through.
pub struct AgentTaskLauncher<B: ExecutionBackend> {
    /// Follows the task, and cancels it.
    pub router: TaskRouterHandle<B>,
    /// Runs the agent executor.
    pub chat: ChatSessionManagerHandle,
    /// Records the task; `None` leaves it unrecorded (tests, minimal runtimes).
    pub repository: Option<Arc<TaskRepository>>,
    /// Where the start and the end of the run are published.
    pub event_bus: EventBusSender,
    /// Limits applied to the recorded request and output.
    pub observability: ObservabilityConfig,
}

/// The request an agent reads from a task input: its text parts, then any
/// structured data part as JSON, one after the other.
pub fn message_of(input: &AIPInput) -> String {
    input
        .parts
        .iter()
        .filter_map(|part| match part {
            AIPPart::Text(text) => Some(text.text.clone()),
            AIPPart::Data(data) => Some(data.data.to_string()),
            AIPPart::File(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

impl<B: ExecutionBackend> AgentTaskLauncher<B> {
    /// Run `agent` on `message`, `input` being the input as given.
    ///
    /// Returns the task once the run has started; the run goes on in the
    /// background.
    ///
    /// # Errors
    ///
    /// [`AgentTaskError::Disabled`] for a disabled agent; otherwise the store,
    /// router or executor failure that kept the run from starting. A run that
    /// fails once started is a failed task, not an error here.
    pub async fn launch(
        &self,
        agent: &ResolvedAgent,
        input: &serde_json::Value,
        message: String,
    ) -> Result<TaskId, AgentTaskError> {
        if !agent.enabled {
            return Err(AgentTaskError::Disabled(agent.id.clone()));
        }
        let task_id = TaskId::new_v4();
        let run_id = RunId::new();
        let agent_id = AgentId::from(agent.id.as_str());
        let cancel = CancellationToken::new();

        if let Some(repo) = &self.repository {
            repo.record_agent_run_start(
                AgentRunStart {
                    task_id: task_id.as_str(),
                    agent: &agent.id,
                    run_id: run_id.as_str(),
                    input,
                    message: &message,
                },
                &self.observability,
            )
            .await?;
        }
        self.router
            .track(TrackedRun {
                task_id: task_id.clone(),
                agent_id: agent_id.clone(),
                run_id: run_id.clone(),
                cancel: cancel.clone(),
            })
            .await?;
        let _ = self.event_bus.send(RuntimeEvent::TaskStarted {
            agent_id: agent_id.clone(),
            task_id: task_id.clone(),
        });

        let finish = RunFinish {
            router: self.router.clone(),
            repository: self.repository.clone(),
            event_bus: self.event_bus.clone(),
            observability: self.observability.clone(),
            task_id: task_id.clone(),
            agent_id,
            cancel: cancel.clone(),
            started: Instant::now(),
        };
        let (done_tx, done_rx) = oneshot::channel();
        let started = self
            .chat
            .run_agent_task(AgentTaskRun {
                task_id: task_id.to_string(),
                run_id,
                spec: AgentRunSpec::from_resolved(agent),
                access: apollia_tools::AgentAccess::of(agent),
                message,
                cancel,
                done: done_tx,
            })
            .await;
        if let Err(e) = started {
            finish
                .settle(Err(ChatError::InternalError(e.to_string())))
                .await;
            return Err(e.into());
        }
        tokio::spawn(async move {
            let outcome = done_rx
                .await
                .unwrap_or_else(|_| Err(ChatError::InternalError("the run was dropped".into())));
            finish.settle(outcome).await;
        });
        Ok(task_id)
    }
}

/// What the end of a run is reported to.
struct RunFinish<B: ExecutionBackend> {
    router: TaskRouterHandle<B>,
    repository: Option<Arc<TaskRepository>>,
    event_bus: EventBusSender,
    observability: ObservabilityConfig,
    task_id: TaskId,
    agent_id: AgentId,
    cancel: CancellationToken,
    started: Instant,
}

impl<B: ExecutionBackend> RunFinish<B> {
    /// Record the end of the run, publish it, and tell the router it ended.
    ///
    /// A cancelled run is published by the router, once it learns the run has
    /// stopped; a completed or failed one is published here.
    async fn settle(self, outcome: Result<ChatAgentResponse, ChatError>) {
        let (status, output) = match &outcome {
            Ok(response) if response.paused || self.cancel.is_cancelled() => {
                ("canceled", response.content.clone())
            }
            Ok(response) => ("completed", response.content.clone()),
            Err(e) => ("failed", e.to_string()),
        };
        if let Some(repo) = &self.repository {
            let duration_ms = i64::try_from(self.started.elapsed().as_millis()).unwrap_or(i64::MAX);
            if let Err(e) = repo
                .record_agent_run_end(
                    AgentRunEnd {
                        task_id: self.task_id.as_str(),
                        status,
                        output: &output,
                        duration_ms,
                    },
                    &self.observability,
                )
                .await
            {
                tracing::event!(
                    tracing::Level::WARN,
                    task_id = %self.task_id,
                    error = %e,
                    "agent.task.record_end.failed"
                );
            }
        }
        if status != "canceled" {
            let _ = self.event_bus.send(RuntimeEvent::TaskCompleted {
                agent_id: self.agent_id.clone(),
                task_id: self.task_id.clone(),
                success: status == "completed",
                output: Some(output),
            });
        }
        tracing::event!(
            tracing::Level::INFO,
            task_id = %self.task_id,
            agent_id = %self.agent_id,
            status,
            "agent.task.finished"
        );
        let _ = self.router.run_ended(&self.task_id).await;
    }
}

#[cfg(test)]
mod tests;
