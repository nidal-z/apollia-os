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

/// Why an agent could not be resolved.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AgentResolveError {
    /// `agents.db` could not be opened or read.
    #[error("cannot read agents.db: {0}")]
    Agents(String),
    /// The model store could not be read.
    #[error("cannot read the model store: {0}")]
    Models(String),
}

/// The agent `name` as resolved, when it is stored in `agents.db` under
/// `data_dir` and runs no code, with its model read from `models`. `None` for
/// an agent that runs code and for an unknown name.
///
/// # Errors
///
/// [`AgentResolveError`] when `agents.db` or the model store cannot be read.
pub fn resolve_without_code(
    data_dir: &std::path::Path,
    models: Option<&apollia_core::LlmBackendRepository>,
    name: &str,
) -> Result<Option<ResolvedAgent>, AgentResolveError> {
    let path = apollia_core::paths::DataFile::Agents.path(data_dir);
    if !path.exists() {
        return Ok(None);
    }
    let repo = apollia_tools::AgentRepository::open(&path)
        .map_err(|e| AgentResolveError::Agents(e.to_string()))?;
    let runs_code = repo
        .get(name)
        .map_err(|e| AgentResolveError::Agents(e.to_string()))?
        .is_some();
    if runs_code {
        return Ok(None);
    }
    let model = match models {
        Some(models) => models
            .agent_model(name)
            .map_err(|e| AgentResolveError::Models(e.to_string()))?,
        None => None,
    };
    repo.resolved(name, model, crate::agents::global_default_autonomy())
        .map_err(|e| AgentResolveError::Agents(e.to_string()))
}

/// The enabled agent without code `agent`, resolved now from the stores
/// under `data_dir`, so that a run reads the overrides and the model in force
/// when it starts. The stores are opened for the occasion, as the CLI opens
/// them: no lock is shared. `runner` names what refuses an agent that runs
/// code or is unknown (`a batch`, `a workflow step`).
///
/// # Errors
///
/// A message for a store that cannot be read, a disabled agent, or an agent
/// that is not a stored agent without code.
pub async fn resolve_runnable(
    data_dir: std::path::PathBuf,
    agent: &str,
    runner: &str,
) -> Result<ResolvedAgent, String> {
    let name = agent.to_string();
    let resolved = tokio::task::spawn_blocking(move || {
        let system = apollia_core::paths::DataFile::System.path(&data_dir);
        let models = if system.exists() {
            Some(apollia_core::LlmBackendRepository::open(&system).map_err(|e| e.to_string())?)
        } else {
            None
        };
        resolve_without_code(&data_dir, models.as_ref(), &name).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    match resolved {
        Some(agent) if agent.enabled => Ok(agent),
        Some(agent) => Err(format!("agent '{}' is disabled", agent.id)),
        None => Err(format!(
            "agent '{agent}' is not a stored agent without code, which is what {runner} runs"
        )),
    }
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
        self.start(agent, input, message, Watch::default()).await
    }

    /// Run `agent` on `message` as [`Self::launch`] does, and also return
    /// where the end of the run is reported once it is recorded.
    ///
    /// # Errors
    ///
    /// As [`Self::launch`].
    pub async fn launch_watched(
        &self,
        agent: &ResolvedAgent,
        input: &serde_json::Value,
        message: String,
    ) -> Result<(TaskId, oneshot::Receiver<AgentRunOutcome>), AgentTaskError> {
        let (tx, rx) = oneshot::channel();
        let task_id = self
            .start(
                agent,
                input,
                message,
                Watch {
                    watcher: Some(tx),
                    nesting: None,
                },
            )
            .await?;
        Ok((task_id, rx))
    }

    /// Run `agent` on `message` on behalf of a caller that is itself running,
    /// a workflow step or an agent granted this one: the run stops when `nesting.parent` is cancelled and
    /// spends from `nesting.budget`. Returns as [`Self::launch_watched`].
    ///
    /// # Errors
    ///
    /// As [`Self::launch`].
    pub(crate) async fn launch_nested(
        &self,
        agent: &ResolvedAgent,
        input: &serde_json::Value,
        message: String,
        nesting: Nesting,
    ) -> Result<(TaskId, oneshot::Receiver<AgentRunOutcome>), AgentTaskError> {
        let (tx, rx) = oneshot::channel();
        let task_id = self
            .start(
                agent,
                input,
                message,
                Watch {
                    watcher: Some(tx),
                    nesting: Some(nesting),
                },
            )
            .await?;
        Ok((task_id, rx))
    }

    async fn start(
        &self,
        agent: &ResolvedAgent,
        input: &serde_json::Value,
        message: String,
        watch: Watch,
    ) -> Result<TaskId, AgentTaskError> {
        let Watch { watcher, nesting } = watch;
        if !agent.enabled {
            return Err(AgentTaskError::Disabled(agent.id.clone()));
        }
        let task_id = TaskId::new_v4();
        let run_id = RunId::new();
        let agent_id = AgentId::from(agent.id.as_str());
        let (cancel, budget, depth) = match nesting {
            Some(nesting) => (
                nesting.parent.child_token(),
                Some(nesting.budget),
                nesting.depth,
            ),
            None => (CancellationToken::new(), None, 1),
        };

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
            watcher,
        };
        let (done_tx, done_rx) = oneshot::channel();
        let started = self
            .chat
            .run_agent_task(AgentTaskRun {
                task_id: task_id.to_string(),
                run_id,
                spec: AgentRunSpec::from_resolved(agent),
                message,
                cancel,
                done: done_tx,
                budget,
                depth,
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

/// Who hears the end of a run, and what it inherits from its caller.
#[derive(Default)]
struct Watch {
    watcher: Option<oneshot::Sender<AgentRunOutcome>>,
    nesting: Option<Nesting>,
}

/// What a run started by a running caller inherits from it.
pub struct Nesting {
    /// The cancellation token of the caller; the run stops with it.
    pub parent: CancellationToken,
    /// The budget of the caller, which the run spends from.
    pub budget: Arc<apollia_oria::budget::StepBudget>,
    /// The level of the run in the nesting, its caller's plus one; a run
    /// started on its own is level 1.
    pub depth: u32,
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
    watcher: Option<oneshot::Sender<AgentRunOutcome>>,
}

/// How a run ended, as recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunOutcome {
    /// `completed`, `failed` or `canceled`.
    pub status: &'static str,
    /// The answer, or the failure.
    pub output: String,
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
                output: Some(output.clone()),
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
        if let Some(watcher) = self.watcher {
            let _ = watcher.send(AgentRunOutcome { status, output });
        }
    }
}

#[cfg(test)]
mod tests;
