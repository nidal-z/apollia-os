use std::collections::HashMap;

use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

use apollia_core::token_budget::TokenBudget;
use apollia_core::{
    AIPInput, AIPTask, AgentId, ProcessState, RunId, RunOptions, RuntimeEvent, TaskId, TaskStatus,
};

use crate::coordinator::{CancellationToken, ExecutionBackend, ExecutionCoordinator};
use crate::eventbus::EventBusSender;
use crate::registry::AgentRegistryHandle;

mod external;
pub use external::TrackedRun;

/// Task submission errors.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SubmitError {
    /// The agent is still initializing.
    #[error("agent '{0}' not ready (still initializing)")]
    AgentNotReady(AgentId),

    /// The agent is stopping or stopped.
    #[error("agent '{0}' unavailable (stopping or stopped)")]
    AgentUnavailable(AgentId),

    /// The agent does not exist in the registry.
    #[error("agent '{0}' not found")]
    AgentNotFound(AgentId),

    /// The agent's coordinator reached its concurrency limit.
    #[error("concurrency limit reached for agent '{0}'")]
    ConcurrencyLimit(AgentId),

    /// No coordinator registered for this agent.
    #[error("no coordinator registered for agent '{0}'")]
    NoCoordinator(AgentId),

    /// The TaskRouter actor is dead.
    #[error("router actor is dead")]
    ActorDead,
}

/// Internal messages of the TaskRouter actor.
enum RouterMessage<B: ExecutionBackend> {
    /// Submit a task for an agent.
    Submit {
        agent_id: AgentId,
        input: AIPInput,
        /// A2A skill invoked, populated only during an A2A delegation.
        /// `None` for root executions, triggers, or direct invocations
        /// without a targeted skill. Propagated through to `AIPTask.skill_id`.
        skill_id: Option<String>,
        delegation_chain: Vec<AgentId>,
        /// Per-run control options (plan-gate / autonomy overrides).
        run_options: RunOptions,
        reply: oneshot::Sender<Result<TaskId, SubmitError>>,
    },
    /// Get the status of a task.
    GetStatus {
        task_id: TaskId,
        reply: oneshot::Sender<Option<TaskStatus>>,
    },
    /// Get the output text of a finished task.
    /// The run a submitted task journals under.
    ///
    /// A task and its run are two identifiers, minted separately at submission:
    /// the chained audit journal is keyed by the run, not by the task, so a
    /// caller holding only a `task_id` cannot reach the journal of the work it
    /// started. This is how it asks.
    GetRunId {
        task_id: TaskId,
        reply: oneshot::Sender<Option<RunId>>,
    },
    GetOutput {
        task_id: TaskId,
        reply: oneshot::Sender<Option<String>>,
    },
    /// Cancel a running task.
    ///
    /// Answered once the execution has stopped, not when it was asked to.
    Cancel {
        task_id: TaskId,
        reply: oneshot::Sender<Option<TaskStatus>>,
    },
    /// The execution of a task has ended, whatever the outcome. Sent by the
    /// watcher that holds the task's `JoinHandle`.
    ExecutionEnded { task_id: TaskId },
    /// Follow a task run outside the coordinators, as one submitted here.
    Track {
        run: external::TrackedRun,
        reply: oneshot::Sender<()>,
    },
    /// Return the IDs of active tasks (Working or Submitted).
    GetActiveTasks { reply: oneshot::Sender<Vec<TaskId>> },
    /// Return all known tasks with their agent_id and status.
    GetAllTasks {
        reply: oneshot::Sender<Vec<(TaskId, AgentId, TaskStatus)>>,
    },
    /// Register an ExecutionCoordinator for an agent.
    RegisterCoordinator {
        agent_id: AgentId,
        coordinator: ExecutionCoordinator<B>,
    },
    /// Remove an agent's coordinator (agent stopping).
    UnregisterCoordinator { agent_id: AgentId },
    /// Get the latest session budget snapshot.
    GetBudget {
        reply: oneshot::Sender<Option<TokenBudget>>,
    },
    /// Stop the actor.
    Shutdown,
}

/// TaskRouter actor, the centralized entry point for task submissions.
///
/// Manages dispatch of tasks to the ExecutionCoordinators of active agents.
/// Maintains the agent_id -> coordinator and task_id -> status maps.
struct TaskRouter<B: ExecutionBackend> {
    rx: mpsc::Receiver<RouterMessage<B>>,
    registry: AgentRegistryHandle,
    event_bus: EventBusSender,
    /// Subscription to the EventBus for receiving TaskCompleted/TaskFailed events.
    event_rx: tokio::sync::broadcast::Receiver<apollia_core::RuntimeEvent>,
    coordinators: HashMap<AgentId, ExecutionCoordinator<B>>,
    task_statuses: HashMap<TaskId, TaskStatus>,
    /// Maps each task to the agent that runs it (for GET /api/v1/tasks list).
    task_agents: HashMap<TaskId, AgentId>,
    /// Output text stored when TaskCompleted is received (for GET /api/v1/tasks/:id).
    task_outputs: HashMap<TaskId, String>,
    /// The run each task journals under, recorded at submission.
    ///
    /// Same lifetime as `task_statuses` and `task_agents`: filled when the task
    /// is dispatched, kept for the router's life.
    task_runs: HashMap<TaskId, RunId>,
    /// Latest session budget snapshot, updated on each TokenBudgetUpdated event.
    latest_budget: Option<TokenBudget>,
    /// The cancellation token of every execution still running, removed when
    /// its `JoinHandle` resolves.
    task_cancels: HashMap<TaskId, CancellationToken>,
    /// Cancel requests waiting for their execution to stop before they answer.
    pending_cancels: HashMap<TaskId, Vec<oneshot::Sender<Option<TaskStatus>>>>,
    /// Weak sender to this actor's own mailbox, for the `JoinHandle` watchers.
    /// Weak so that the watchers never keep the actor alive on their own.
    self_tx: mpsc::WeakSender<RouterMessage<B>>,
}

impl<B: ExecutionBackend> TaskRouter<B> {
    /// Main actor loop.
    ///
    /// Processes mpsc messages AND EventBus events (TaskCompleted/TaskFailed).
    async fn run(mut self) {
        use apollia_core::RuntimeEvent;
        info!("router.started");
        loop {
            tokio::select! {
                msg = self.rx.recv() => {
                    let Some(msg) = msg else { break };
                    match msg {
                        RouterMessage::Submit { agent_id, input, skill_id, delegation_chain, run_options, reply } => {
                            let result = self.handle_submit(agent_id, input, skill_id, delegation_chain, run_options).await;
                            let _ = reply.send(result);
                        }
                        RouterMessage::GetStatus { task_id, reply } => {
                            let status = self.task_statuses.get(&task_id).cloned();
                            let _ = reply.send(status);
                        }
                        RouterMessage::GetRunId { task_id, reply } => {
                            let _ = reply.send(self.task_runs.get(&task_id).cloned());
                        }
                        RouterMessage::GetOutput { task_id, reply } => {
                            let output = self.task_outputs.get(&task_id).cloned();
                            let _ = reply.send(output);
                        }
                        RouterMessage::Cancel { task_id, reply } => {
                            self.handle_cancel(task_id, reply);
                        }
                        RouterMessage::ExecutionEnded { task_id } => {
                            self.handle_execution_ended(task_id);
                        }
                        RouterMessage::Track { run, reply } => {
                            self.handle_track(run);
                            let _ = reply.send(());
                        }
                        RouterMessage::GetActiveTasks { reply } => {
                            let active: Vec<TaskId> = self
                                .task_statuses
                                .iter()
                                .filter(|(_, s)| {
                                    matches!(s, TaskStatus::Working | TaskStatus::Submitted)
                                })
                                .map(|(id, _)| id.clone())
                                .collect();
                            let _ = reply.send(active);
                        }
                        RouterMessage::GetAllTasks { reply } => {
                            let all: Vec<(TaskId, AgentId, TaskStatus)> = self
                                .task_statuses
                                .iter()
                                .map(|(id, status)| {
                                    let agent_id = self
                                        .task_agents
                                        .get(id)
                                        .cloned()
                                        .unwrap_or_else(|| AgentId::from("unknown"));
                                    (id.clone(), agent_id, status.clone())
                                })
                                .collect();
                            let _ = reply.send(all);
                        }
                        RouterMessage::RegisterCoordinator { agent_id, coordinator } => {
                            info!(agent_id = %agent_id, "router.coordinator.registered");
                            self.coordinators.insert(agent_id, coordinator);
                        }
                        RouterMessage::UnregisterCoordinator { agent_id } => {
                            info!(agent_id = %agent_id, "router.coordinator.removed");
                            self.coordinators.remove(&agent_id);
                        }
                        RouterMessage::GetBudget { reply } => {
                            let _ = reply.send(self.latest_budget.clone());
                        }
                        RouterMessage::Shutdown => {
                            info!("router.stop.requested");
                            break;
                        }
                    }
                }
                event = self.event_rx.recv() => {
                    match event {
                        // A pause and its resume move the listed status, so
                        // `GET /api/v1/tasks?status=input_required` finds the
                        // task that is waiting. Only a live task moves: a
                        // terminal status is never reopened by a late event.
                        Ok(RuntimeEvent::TaskInputRequired { task_id, .. }) => {
                            if let Some(status) = self.task_statuses.get_mut(&task_id) {
                                if matches!(*status, TaskStatus::Working | TaskStatus::Submitted) {
                                    *status = TaskStatus::InputRequired;
                                }
                            }
                        }
                        Ok(RuntimeEvent::TaskResumed { task_id, .. }) => {
                            if let Some(status) = self.task_statuses.get_mut(&task_id) {
                                if *status == TaskStatus::InputRequired {
                                    *status = TaskStatus::Working;
                                }
                            }
                        }
                        Ok(RuntimeEvent::TaskCompleted { task_id, success, output, .. }) => {
                            if let Some(status) = self.task_statuses.get_mut(&task_id) {
                                // Do not overwrite an already-set terminal status (Canceled, Completed, Failed).
                                // A late TaskCompleted event from the backend must not erase an
                                // explicit user cancellation.
                                if !matches!(*status, TaskStatus::Canceled | TaskStatus::Completed | TaskStatus::Failed) {
                                    *status = if success {
                                        TaskStatus::Completed
                                    } else {
                                        TaskStatus::Failed
                                    };
                                }
                            }
                            if let Some(text) = output {
                                self.task_outputs.insert(task_id, text);
                            }
                        }
                        Ok(RuntimeEvent::TokenBudgetUpdated {
                            session_cost_usd,
                            total_input_tokens,
                            total_output_tokens,
                            total_cache_read_tokens,
                            ..
                        }) => {
                            self.latest_budget = Some(TokenBudget {
                                input_tokens: total_input_tokens,
                                output_tokens: total_output_tokens,
                                cache_read_tokens: total_cache_read_tokens,
                                cost_usd: session_cost_usd,
                                ..Default::default()
                            });
                        }
                        _ => {}
                    }
                }
            }
        }
        info!("router.stopped");
    }

    /// Handle a task submission.
    ///
    /// 1. Check the agent state via AgentRegistryHandle
    /// 2. Generate a TaskId (UUID v4)
    /// 3. Build the AIPTask
    /// 4. Dispatch to the coordinator
    // Submit path threads several independent request fields; a struct would
    // just move the argument list.
    // REASON: flattened fields of one submit message, handled in one place.
    #[allow(clippy::too_many_arguments)]
    async fn handle_submit(
        &mut self,
        agent_id: AgentId,
        input: AIPInput,
        skill_id: Option<String>,
        delegation_chain: Vec<AgentId>,
        run_options: RunOptions,
    ) -> Result<TaskId, SubmitError> {
        // 1. Check the agent in the registry (by UUID then by manifest name)
        let resolved_id = if self
            .registry
            .get_agent(agent_id.as_str())
            .await
            .map_err(|_| SubmitError::ActorDead)?
            .is_some()
        {
            agent_id.clone()
        } else {
            // Attempt resolution by manifest name
            self.registry
                .find_by_name(agent_id.as_str())
                .await
                .map_err(|_| SubmitError::ActorDead)?
                .ok_or_else(|| SubmitError::AgentNotFound(agent_id.clone()))?
        };

        let agent_entry = self
            .registry
            .get_agent(resolved_id.as_str())
            .await
            .map_err(|_| SubmitError::ActorDead)?
            .ok_or_else(|| SubmitError::AgentNotFound(agent_id.clone()))?;

        let agent_id = resolved_id;

        // 2. Check the ProcessState
        match agent_entry.process_state {
            ProcessState::Initializing => {
                return Err(SubmitError::AgentNotReady(agent_id));
            }
            ProcessState::Stopping | ProcessState::Stopped => {
                return Err(SubmitError::AgentUnavailable(agent_id));
            }
            ProcessState::Degraded => {
                warn!(agent_id = %agent_id, "task.submitted.degraded_agent");
                let _ = self.event_bus.send(RuntimeEvent::AgentDegraded {
                    agent_id: agent_id.clone(),
                    reason: "task submitted to degraded agent".into(),
                });
            }
            ProcessState::Active => {
                // OK, normal dispatch
            }
        }

        // 3. Generate TaskId + build AIPTask.
        // Every submission gets a stable run_id so its tool/LLM events are
        // journaled (the audit journal only appends events carrying a run_id),
        // making the run verifiable via `audit verify`.
        let task_id = TaskId::new_v4();
        let run_id = RunId::new();
        let task = AIPTask {
            task_id: task_id.to_string(),
            context_id: format!("ctx-{}", agent_id),
            skill_id,
            input,
            history: vec![],
            timeout_seconds: None,
            delegation_chain,
            run_options,
            run_id: Some(run_id.clone()),
            ..AIPTask::default()
        };

        // 4. Dispatch to the coordinator
        let coordinator = self
            .coordinators
            .get(&agent_id)
            .ok_or_else(|| SubmitError::NoCoordinator(agent_id.clone()))?;

        let submitted = coordinator
            .submit_task(task)
            .map_err(|_| SubmitError::ConcurrencyLimit(agent_id.clone()))?;
        self.task_cancels.insert(task_id.clone(), submitted.cancel);
        self.watch_execution(task_id.clone(), submitted.handle);

        // 5. Record the status and the task -> agent association
        self.task_statuses
            .insert(task_id.clone(), TaskStatus::Working);
        self.task_agents.insert(task_id.clone(), agent_id.clone());
        // Recorded before the reply goes out, so a caller that asks for the run
        // right after `submit` returns always finds it.
        self.task_runs.insert(task_id.clone(), run_id);

        info!(task_id = %task_id, agent_id = %agent_id, "task.dispatched");
        Ok(task_id)
    }

    /// Handle a task cancellation.
    ///
    /// A task whose execution is still running has its token cancelled and
    /// keeps its status (`Working`) until that execution has stopped; the
    /// reply is sent then, by [`Self::handle_execution_ended`]. A live status
    /// with no execution left (a task paused for input, whose run already
    /// returned) is cancelled at once. A terminal status is answered as is,
    /// and an unknown task with `None`.
    fn handle_cancel(&mut self, task_id: TaskId, reply: oneshot::Sender<Option<TaskStatus>>) {
        let Some(status) = self.task_statuses.get_mut(&task_id) else {
            let _ = reply.send(None);
            return;
        };
        if !matches!(
            status,
            TaskStatus::Submitted | TaskStatus::Working | TaskStatus::InputRequired
        ) {
            let _ = reply.send(Some(status.clone()));
            return;
        }
        if let Some(token) = self.task_cancels.get(&task_id) {
            token.cancel();
            info!(task_id = %task_id, "task.cancel.requested");
            self.pending_cancels.entry(task_id).or_default().push(reply);
            return;
        }
        *status = TaskStatus::Canceled;
        info!(task_id = %task_id, "task.canceled");
        let _ = self.event_bus.send(RuntimeEvent::TaskCanceled {
            task_id: task_id.clone(),
        });
        let _ = reply.send(Some(TaskStatus::Canceled));
    }

    /// Hands the `JoinHandle` of a task to a watcher that reports back to
    /// this actor once the execution has ended.
    fn watch_execution(
        &self,
        task_id: TaskId,
        handle: tokio::task::JoinHandle<
            Result<apollia_core::AIPResult, crate::coordinator::CoordinatorError>,
        >,
    ) {
        let self_tx = self.self_tx.clone();
        tokio::spawn(async move {
            let _ = handle.await;
            if let Some(tx) = self_tx.upgrade() {
                let _ = tx.send(RouterMessage::ExecutionEnded { task_id }).await;
            }
        });
    }

    /// The execution of `task_id` has ended: drop its token and, when a
    /// cancellation was waiting for it, settle the status and answer.
    fn handle_execution_ended(&mut self, task_id: TaskId) {
        self.task_cancels.remove(&task_id);
        let Some(replies) = self.pending_cancels.remove(&task_id) else {
            return;
        };
        let status = match self.task_statuses.get_mut(&task_id) {
            Some(status)
                if matches!(
                    status,
                    TaskStatus::Submitted | TaskStatus::Working | TaskStatus::InputRequired
                ) =>
            {
                *status = TaskStatus::Canceled;
                info!(task_id = %task_id, "task.canceled");
                let _ = self.event_bus.send(RuntimeEvent::TaskCanceled {
                    task_id: task_id.clone(),
                });
                Some(TaskStatus::Canceled)
            }
            // The execution finished on its own before the cancellation
            // reached it: the outcome it reported is the true one.
            other => other.cloned(),
        };
        for reply in replies {
            let _ = reply.send(status.clone());
        }
    }
}

/// Cloneable handle to interact with the TaskRouter actor.
///
/// Thread-safe: Clone + Send + Sync.
/// Clone is implemented manually because `mpsc::Sender` is Clone
/// independently of the bounds on B.
pub struct TaskRouterHandle<B: ExecutionBackend> {
    tx: mpsc::Sender<RouterMessage<B>>,
}

impl<B: ExecutionBackend> Clone for TaskRouterHandle<B> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}

impl<B: ExecutionBackend> TaskRouterHandle<B> {
    /// Spawn the TaskRouter actor and return a Handle.
    ///
    /// # Arguments
    /// - `registry`: handle to the AgentRegistry for checking states
    /// - `event_bus`: channel for emitting runtime events
    /// - `buffer_size`: mpsc channel size (recommended default: 256)
    pub fn spawn(
        registry: AgentRegistryHandle,
        event_bus: EventBusSender,
        buffer_size: usize,
    ) -> Self {
        let (tx, rx) = mpsc::channel(buffer_size);
        let self_tx = tx.downgrade();
        let event_rx = event_bus.subscribe();
        let router = TaskRouter {
            rx,
            registry,
            event_bus,
            event_rx,
            coordinators: HashMap::new(),
            task_statuses: HashMap::new(),
            task_agents: HashMap::new(),
            task_runs: HashMap::new(),
            task_outputs: HashMap::new(),
            latest_budget: None,
            task_cancels: HashMap::new(),
            pending_cancels: HashMap::new(),
            self_tx,
        };
        tokio::spawn(router.run());
        Self { tx }
    }

    /// Submit a task for an agent.
    ///
    /// Returns the generated TaskId on success. To submit as part of an A2A
    /// delegation (with a targeted `skill_id`), use [`Self::submit_with_chain`].
    pub async fn submit(&self, agent_id: &str, input: AIPInput) -> Result<TaskId, SubmitError> {
        self.submit_with_chain(agent_id, input, None, Vec::new())
            .await
    }

    /// Submit a root task with per-run control options (plan-gate / autonomy),
    /// optionally aimed at one skill.
    ///
    /// Used by the REST submit handler to forward CLI flags (`--plan`,
    /// `--autonomy`) to the per-task engine. `skill_id` reaches
    /// `AIPTask.skill_id`, so the SDK dispatches to that skill and a pause
    /// records which skill paused.
    pub async fn submit_skill_with_options(
        &self,
        agent_id: &str,
        input: AIPInput,
        skill_id: Option<String>,
        run_options: RunOptions,
    ) -> Result<TaskId, SubmitError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RouterMessage::Submit {
                agent_id: AgentId::from(agent_id),
                input,
                skill_id,
                delegation_chain: Vec::new(),
                run_options,
                reply: reply_tx,
            })
            .await
            .map_err(|_| SubmitError::ActorDead)?;
        reply_rx.await.map_err(|_| SubmitError::ActorDead)?
    }

    /// Submit a task with an explicit A2A delegation chain and an optional
    /// targeted `skill_id`.
    ///
    /// Used by [`crate::a2a::delegate_inner`] after chain validation by
    /// [`crate::a2a::validate_chain`]. For root submissions (CLI, triggers,
    /// REST), use [`Self::submit`] which passes `None` and an empty chain.
    pub async fn submit_with_chain(
        &self,
        agent_id: &str,
        input: AIPInput,
        skill_id: Option<String>,
        delegation_chain: Vec<AgentId>,
    ) -> Result<TaskId, SubmitError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RouterMessage::Submit {
                agent_id: AgentId::from(agent_id),
                input,
                skill_id,
                delegation_chain,
                run_options: RunOptions::default(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| SubmitError::ActorDead)?;
        reply_rx.await.map_err(|_| SubmitError::ActorDead)?
    }

    /// Get the status of a task.
    pub async fn get_status(&self, task_id: &str) -> Result<Option<TaskStatus>, SubmitError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RouterMessage::GetStatus {
                task_id: TaskId::from(task_id),
                reply: reply_tx,
            })
            .await
            .map_err(|_| SubmitError::ActorDead)?;
        reply_rx.await.map_err(|_| SubmitError::ActorDead)
    }

    /// The run a submitted task journals under, when the router knows the task.
    ///
    /// `GET /api/v1/audit/journal/{run_id}` is keyed by this, not by the task
    /// id: the two are separate identifiers minted together at submission.
    pub async fn get_run_id(&self, task_id: &str) -> Result<Option<RunId>, SubmitError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RouterMessage::GetRunId {
                task_id: TaskId::from(task_id),
                reply: reply_tx,
            })
            .await
            .map_err(|_| SubmitError::ActorDead)?;
        reply_rx.await.map_err(|_| SubmitError::ActorDead)
    }

    /// Get the output text of a finished task, if available.
    pub async fn get_output(&self, task_id: &str) -> Result<Option<String>, SubmitError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RouterMessage::GetOutput {
                task_id: TaskId::from(task_id),
                reply: reply_tx,
            })
            .await
            .map_err(|_| SubmitError::ActorDead)?;
        reply_rx.await.map_err(|_| SubmitError::ActorDead)
    }

    /// Register a coordinator for an agent.
    pub async fn register_coordinator(
        &self,
        agent_id: AgentId,
        coordinator: ExecutionCoordinator<B>,
    ) -> Result<(), SubmitError> {
        self.tx
            .send(RouterMessage::RegisterCoordinator {
                agent_id,
                coordinator,
            })
            .await
            .map_err(|_| SubmitError::ActorDead)
    }

    /// Remove an agent's coordinator.
    pub async fn unregister_coordinator(&self, agent_id: &AgentId) -> Result<(), SubmitError> {
        self.tx
            .send(RouterMessage::UnregisterCoordinator {
                agent_id: agent_id.clone(),
            })
            .await
            .map_err(|_| SubmitError::ActorDead)
    }

    /// Return the latest LLM session budget snapshot, if available.
    pub async fn get_budget(&self) -> Result<Option<TokenBudget>, SubmitError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RouterMessage::GetBudget { reply: reply_tx })
            .await
            .map_err(|_| SubmitError::ActorDead)?;
        reply_rx.await.map_err(|_| SubmitError::ActorDead)
    }

    /// Cancel a running task.
    ///
    /// Returns the new status if the task exists, None otherwise. For a task
    /// whose execution is running, the answer comes once that execution has
    /// stopped, within [`crate::coordinator::CANCEL_GRACE`] of the request for
    /// a backend that does not stop by itself.
    pub async fn cancel(&self, task_id: &str) -> Result<Option<TaskStatus>, SubmitError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RouterMessage::Cancel {
                task_id: TaskId::from(task_id),
                reply: reply_tx,
            })
            .await
            .map_err(|_| SubmitError::ActorDead)?;
        reply_rx.await.map_err(|_| SubmitError::ActorDead)
    }

    /// Return the IDs of active tasks (Working or Submitted).
    pub async fn active_tasks(&self) -> Result<Vec<TaskId>, SubmitError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RouterMessage::GetActiveTasks { reply: reply_tx })
            .await
            .map_err(|_| SubmitError::ActorDead)?;
        reply_rx.await.map_err(|_| SubmitError::ActorDead)
    }

    /// Return all known tasks with their agent_id and status.
    ///
    /// Used by `GET /api/v1/tasks` to list recent tasks.
    pub async fn all_tasks(&self) -> Result<Vec<(TaskId, AgentId, TaskStatus)>, SubmitError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RouterMessage::GetAllTasks { reply: reply_tx })
            .await
            .map_err(|_| SubmitError::ActorDead)?;
        reply_rx.await.map_err(|_| SubmitError::ActorDead)
    }

    /// Request the actor to shut down.
    pub fn shutdown(&self) {
        let _ = self.tx.try_send(RouterMessage::Shutdown);
    }
}

// TaskSubmitter impl

use std::future::Future;
use std::pin::Pin;

/// `TaskSubmitter` trait implementation for `TaskRouterHandle<B>`.
///
/// Lets the `TriggerEngine` submit tasks to the `TaskRouter` without a direct
/// dependency on `apollia-runtime`.
///
/// `pending_count` always returns 0 for now; per-agent counting is not yet
/// exposed in the `TaskRouter`'s public API. `OnBusyPolicy::Drop` could be
/// improved with a `GetPendingCountForAgent` message.
impl<B> apollia_triggers::TaskSubmitter for TaskRouterHandle<B>
where
    B: ExecutionBackend + Clone + Send + Sync + 'static,
{
    fn submit<'a>(
        &'a self,
        agent: &'a str,
        input: AIPInput,
    ) -> Pin<Box<dyn Future<Output = Result<TaskId, String>> + Send + 'a>> {
        Box::pin(async move {
            TaskRouterHandle::submit(self, agent, input)
                .await
                .map_err(|e| e.to_string())
        })
    }

    fn pending_count<'a>(
        &'a self,
        _agent: &'a str,
    ) -> Pin<Box<dyn Future<Output = usize> + Send + 'a>> {
        // Per-agent pending count not yet exposed, returns 0.
        Box::pin(async move { 0 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{AIPResult, AgentManifest};
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::sync::{broadcast, watch};

    use crate::coordinator::ExecutionBackend;
    use crate::registry::AgentRegistry;

    /// Mock backend that always accepts tasks.
    struct MockBackend {
        should_fail: AtomicBool,
    }

    impl MockBackend {
        fn success() -> Self {
            Self {
                should_fail: AtomicBool::new(false),
            }
        }

        fn failing() -> Self {
            Self {
                should_fail: AtomicBool::new(true),
            }
        }
    }

    impl ExecutionBackend for MockBackend {
        fn execute(
            &self,
            task: AIPTask,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>>
        {
            let fail = self.should_fail.load(Ordering::SeqCst);
            Box::pin(async move {
                if fail {
                    Err("mock failure".to_string())
                } else {
                    Ok(AIPResult {
                        task_id: task.task_id,
                        status: TaskStatus::Completed,
                        output: vec![],
                        error: None,
                        artifacts: vec![],
                        input_required_data: None,
                    })
                }
            })
        }
    }

    fn test_manifest(name: &str) -> AgentManifest {
        AgentManifest {
            autonomy_level: None,
            format_version: 1,
            name: name.to_string(),
            version: "0.1.0".to_string(),
            description: String::new(),
            tools_required: vec![],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
            execution_mode: "auto".to_string(),
            supports_mailbox: false,
            mailbox_allowlist: None,
            system_prompt: None,
            tools_requiring_approval: vec![],
            llm_backend: None,
            model: None,
            packages: vec![],
            memory_config: None,
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            user_memory_write: false,
            datasources: vec![],
            templates: vec![],
            secrets: vec![],
            check_commands: vec![],
        }
    }

    /// Helper to create a full test environment.
    /// Returns (router, registry, event_rx).
    async fn setup_test_env() -> (
        TaskRouterHandle<MockBackend>,
        AgentRegistryHandle,
        broadcast::Receiver<RuntimeEvent>,
    ) {
        let (event_tx, event_rx) = broadcast::channel(64);
        let registry = AgentRegistry::spawn(event_tx.clone());
        let router = TaskRouterHandle::spawn(registry.clone(), event_tx, 256);
        (router, registry, event_rx)
    }

    /// Helper to register an agent and put it in the desired state.
    /// Returns the generated AgentId.
    async fn register_agent_in_state(
        registry: &AgentRegistryHandle,
        name: &str,
        target_state: ProcessState,
    ) -> AgentId {
        let agent_id = registry
            .register(test_manifest(name))
            .await
            .expect("register failed");

        // Transitions to the target state
        match target_state {
            ProcessState::Initializing => {}
            ProcessState::Active => {
                registry
                    .update_state(agent_id.as_str(), ProcessState::Active)
                    .await
                    .expect("transition to Active failed");
            }
            ProcessState::Degraded => {
                registry
                    .update_state(agent_id.as_str(), ProcessState::Active)
                    .await
                    .expect("transition to Active failed");
                registry
                    .update_state(agent_id.as_str(), ProcessState::Degraded)
                    .await
                    .expect("transition to Degraded failed");
            }
            ProcessState::Stopping => {
                registry
                    .update_state(agent_id.as_str(), ProcessState::Active)
                    .await
                    .expect("transition to Active failed");
                registry
                    .update_state(agent_id.as_str(), ProcessState::Stopping)
                    .await
                    .expect("transition to Stopping failed");
            }
            ProcessState::Stopped => {
                registry
                    .update_state(agent_id.as_str(), ProcessState::Active)
                    .await
                    .expect("transition to Active failed");
                registry
                    .update_state(agent_id.as_str(), ProcessState::Stopping)
                    .await
                    .expect("transition to Stopping failed");
                registry
                    .update_state(agent_id.as_str(), ProcessState::Stopped)
                    .await
                    .expect("transition to Stopped failed");
            }
        }

        agent_id
    }

    #[tokio::test]
    async fn test_submit_to_active_agent_returns_task_id() {
        // GIVEN an agent registered in Active state with a coordinator
        let (router, registry, _rx) = setup_test_env().await;
        let agent_id =
            register_agent_in_state(&registry, "agent-active", ProcessState::Active).await;
        let (event_tx, _) = broadcast::channel(64);
        let coordinator =
            ExecutionCoordinator::new(agent_id.clone(), 1, event_tx, MockBackend::success());
        router
            .register_coordinator(agent_id.clone(), coordinator)
            .await
            .expect("register coordinator failed");

        // WHEN submitting a task via router.submit()
        let result = router.submit(agent_id.as_str(), AIPInput::default()).await;

        // THEN a TaskId is returned (non-empty, UUID format)
        assert!(result.is_ok(), "submit should succeed, got: {result:?}");
        let task_id = result.expect("already checked");
        assert!(!task_id.as_str().is_empty());
        assert!(
            uuid::Uuid::parse_str(task_id.as_str()).is_ok(),
            "task_id should be a valid UUID"
        );
    }

    /// Backend that records the run id stamped on the task it receives.
    ///
    /// That run id is the one every tool and LLM event of the task carries, so
    /// it is the key the chained journal stores the task under.
    struct RunCapturingBackend {
        seen: std::sync::Arc<std::sync::Mutex<Option<RunId>>>,
    }

    impl ExecutionBackend for RunCapturingBackend {
        fn execute(
            &self,
            task: AIPTask,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>>
        {
            if let Ok(mut slot) = self.seen.lock() {
                *slot = task.run_id.clone();
            }
            Box::pin(async move { Ok(AIPResult::completed("ok")) })
        }
    }

    #[tokio::test]
    async fn test_get_run_id_answers_the_run_the_task_journals_under() {
        // GIVEN an active agent whose backend records the run id of the task it runs
        let (event_tx, _) = broadcast::channel(64);
        let registry = AgentRegistry::spawn(event_tx.clone());
        let router: TaskRouterHandle<RunCapturingBackend> =
            TaskRouterHandle::spawn(registry.clone(), event_tx.clone(), 256);
        let agent_id = register_agent_in_state(&registry, "agent-run", ProcessState::Active).await;
        let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
        let coordinator = ExecutionCoordinator::new(
            agent_id.clone(),
            1,
            event_tx,
            RunCapturingBackend { seen: seen.clone() },
        );
        router
            .register_coordinator(agent_id.clone(), coordinator)
            .await
            .expect("register coordinator failed");

        // WHEN a task is submitted and its run id is asked for straight away
        let task_id = router
            .submit(agent_id.as_str(), AIPInput::default())
            .await
            .expect("submit");
        let run_id = router
            .get_run_id(task_id.as_str())
            .await
            .expect("router alive")
            .expect("the router records the run before it replies");

        // THEN the run id is not the task id: the two are separate identifiers,
        // which is why a caller holding the task id alone could not reach the
        // journal
        assert_ne!(run_id.to_string(), task_id.to_string());

        // AND it is the very run id stamped on the task the backend executed,
        // the one its events carry and the journal is keyed by
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let stamped = loop {
            let current = seen.lock().ok().and_then(|slot| slot.clone());
            if let Some(stamped) = current {
                break stamped;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the backend never ran"
            );
            tokio::task::yield_now().await;
        };
        assert_eq!(run_id, stamped);
    }

    #[tokio::test]
    async fn test_get_run_id_of_an_unknown_task_is_none() {
        // GIVEN a router that never saw the task
        let (router, _registry, _rx) = setup_test_env().await;

        // WHEN its run id is asked for
        let run_id = router
            .get_run_id("no-such-task")
            .await
            .expect("router alive");

        // THEN nothing is invented
        assert!(run_id.is_none());
    }

    #[tokio::test]
    async fn test_a_paused_task_is_listed_as_input_required_until_resumed() {
        // GIVEN an active agent and a task submitted to it
        let (event_tx, _) = broadcast::channel(64);
        let registry = AgentRegistry::spawn(event_tx.clone());
        let router: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry.clone(), event_tx.clone(), 256);
        let agent_id =
            register_agent_in_state(&registry, "agent-pause", ProcessState::Active).await;
        // The coordinator reports on a bus the router does not listen to, so its
        // completion never reaches the router and the task stays Working.
        let (idle_tx, _) = broadcast::channel(64);
        let coordinator =
            ExecutionCoordinator::new(agent_id.clone(), 1, idle_tx, MockBackend::success());
        router
            .register_coordinator(agent_id.clone(), coordinator)
            .await
            .expect("register coordinator failed");
        let task_id = router
            .submit(agent_id.as_str(), AIPInput::default())
            .await
            .expect("submit");

        let status_of = |router: TaskRouterHandle<MockBackend>, id: TaskId| async move {
            router
                .all_tasks()
                .await
                .expect("router alive")
                .into_iter()
                .find(|(t, _, _)| *t == id)
                .map(|(_, _, s)| s)
        };

        // WHEN the engine announces a pause
        event_tx
            .send(RuntimeEvent::TaskInputRequired {
                task_id: task_id.clone(),
                prompt: "?".into(),
                step_id: None,
            })
            .expect("send");
        // THEN the router lists the task as waiting on a human
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while status_of(router.clone(), task_id.clone()).await != Some(TaskStatus::InputRequired) {
            assert!(
                std::time::Instant::now() < deadline,
                "never listed as input_required"
            );
            tokio::task::yield_now().await;
        }

        // WHEN it is resumed
        event_tx
            .send(RuntimeEvent::TaskResumed {
                task_id: task_id.clone(),
                approved: true,
            })
            .expect("send");
        // THEN it is working again
        while status_of(router.clone(), task_id.clone()).await != Some(TaskStatus::Working) {
            assert!(
                std::time::Instant::now() < deadline,
                "never back to working"
            );
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test]
    async fn test_submit_to_initializing_agent_rejected() {
        // GIVEN an agent registered in Initializing state
        let (router, registry, _rx) = setup_test_env().await;
        let agent_id =
            register_agent_in_state(&registry, "agent-init", ProcessState::Initializing).await;

        // WHEN submitting a task
        let result = router.submit(agent_id.as_str(), AIPInput::default()).await;

        // THEN returns SubmitError::AgentNotReady
        assert!(matches!(
            result.expect_err("should fail"),
            SubmitError::AgentNotReady(_)
        ));
    }

    #[tokio::test]
    async fn test_submit_to_stopped_agent_rejected() {
        // GIVEN an agent registered in Stopped state
        let (router, registry, _rx) = setup_test_env().await;
        let agent_id =
            register_agent_in_state(&registry, "agent-stopped", ProcessState::Stopped).await;

        // WHEN submitting a task
        let result = router.submit(agent_id.as_str(), AIPInput::default()).await;

        // THEN returns SubmitError::AgentUnavailable
        assert!(matches!(
            result.expect_err("should fail"),
            SubmitError::AgentUnavailable(_)
        ));
    }

    #[tokio::test]
    async fn test_submit_to_degraded_agent_dispatches_with_warning() {
        // GIVEN an agent registered in Degraded state with a coordinator
        let (event_tx, mut event_rx) = broadcast::channel(64);
        let registry = AgentRegistry::spawn(event_tx.clone());
        let router = TaskRouterHandle::spawn(registry.clone(), event_tx.clone(), 256);

        let agent_id =
            register_agent_in_state(&registry, "agent-degraded", ProcessState::Degraded).await;

        let coordinator =
            ExecutionCoordinator::new(agent_id.clone(), 1, event_tx, MockBackend::success());
        router
            .register_coordinator(agent_id.clone(), coordinator)
            .await
            .expect("register coordinator failed");

        // Drain events from agent registration/state transitions
        while event_rx.try_recv().is_ok() {}

        // WHEN submitting a task
        let result = router.submit(agent_id.as_str(), AIPInput::default()).await;

        // THEN the task is dispatched (TaskId returned)
        assert!(result.is_ok(), "submit should succeed for degraded agent");

        // AND a RuntimeEvent::AgentDegraded is emitted on the EventBus. The
        // event is sent synchronously inside handle_submit before submit()
        // returns, so it is already queued; await it deterministically instead
        // of sleeping for propagation.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut found_degraded = false;
        while !found_degraded {
            tokio::select! {
                ev = event_rx.recv() => match ev {
                    Ok(RuntimeEvent::AgentDegraded { reason, .. }) => {
                        assert!(reason.contains("degraded"));
                        found_degraded = true;
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                _ = tokio::time::sleep_until(deadline) => break,
            }
        }
        assert!(found_degraded, "should have received AgentDegraded event");
    }

    #[tokio::test]
    async fn test_submit_to_unknown_agent_not_found() {
        // GIVEN no agent registered with the id "unknown-agent"
        let (router, _registry, _rx) = setup_test_env().await;

        // WHEN submitting a task for "unknown-agent"
        let result = router.submit("unknown-agent", AIPInput::default()).await;

        // THEN returns SubmitError::AgentNotFound
        assert!(matches!(
            result.expect_err("should fail"),
            SubmitError::AgentNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_get_status_returns_task_status() {
        // GIVEN a task submitted successfully (known task_id)
        let (router, registry, _rx) = setup_test_env().await;
        let agent_id =
            register_agent_in_state(&registry, "agent-status", ProcessState::Active).await;
        let (event_tx, _) = broadcast::channel(64);
        let coordinator =
            ExecutionCoordinator::new(agent_id.clone(), 1, event_tx, MockBackend::success());
        router
            .register_coordinator(agent_id.clone(), coordinator)
            .await
            .expect("register coordinator failed");

        let task_id = router
            .submit(agent_id.as_str(), AIPInput::default())
            .await
            .expect("submit failed");

        // WHEN calling get_status(task_id)
        let status = router
            .get_status(task_id.as_str())
            .await
            .expect("get_status failed");

        // THEN returns Some(TaskStatus::Working)
        assert_eq!(status, Some(TaskStatus::Working));
    }

    #[tokio::test]
    async fn test_router_is_actor_handle_clone_send_sync() {
        // GIVEN a TaskRouterHandle
        // THEN the handle is Send + Sync (checked at compile time)
        // WHEN the handle is asked to cross a thread and to give a second owner
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TaskRouterHandle<MockBackend>>();

        // AND the handle can be cloned
        let (router, _registry, _rx) = setup_test_env().await;
        let _cloned = router.clone();
    }

    #[tokio::test]
    async fn test_task_status_transitions_to_completed_via_eventbus() {
        // GIVEN an active agent with a MockBackend coordinator (instant completion)
        // The coordinator uses the SAME event_tx as the router so that
        // TaskCompleted is received by the TaskRouter's event_rx.
        let (event_tx, _event_rx) = broadcast::channel::<RuntimeEvent>(64);
        let registry = AgentRegistry::spawn(event_tx.clone());
        let router = TaskRouterHandle::spawn(registry.clone(), event_tx.clone(), 256);

        let agent_id =
            register_agent_in_state(&registry, "agent-lifecycle-ok", ProcessState::Active).await;
        let coordinator =
            ExecutionCoordinator::new(agent_id.clone(), 1, event_tx, MockBackend::success());
        router
            .register_coordinator(agent_id.clone(), coordinator)
            .await
            .expect("register coordinator failed");

        // WHEN submitting a task
        let task_id = router
            .submit(agent_id.as_str(), AIPInput::default())
            .await
            .expect("submit failed");

        // THEN get_status() returns Completed after EventBus propagation
        let final_status = tokio::time::timeout(std::time::Duration::from_millis(500), async {
            loop {
                let s = router.get_status(task_id.as_str()).await.unwrap();
                if !matches!(s, Some(TaskStatus::Working) | Some(TaskStatus::Submitted)) {
                    return s;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("timeout waiting for task completion");

        assert_eq!(final_status, Some(TaskStatus::Completed));
    }

    #[tokio::test]
    async fn test_submit_by_manifest_name_dispatches_task() {
        // GIVEN an agent registered with manifest.name = "hello-agent" in Active state
        let (router, registry, _rx) = setup_test_env().await;
        let agent_id =
            register_agent_in_state(&registry, "hello-agent", ProcessState::Active).await;

        // AND a coordinator registered for this agent (by UUID)
        let (event_tx, _) = broadcast::channel(64);
        let coordinator =
            ExecutionCoordinator::new(agent_id.clone(), 1, event_tx, MockBackend::success());
        router
            .register_coordinator(agent_id.clone(), coordinator)
            .await
            .expect("register coordinator failed");

        // WHEN router.submit("hello-agent", input) is called (by name, not by UUID)
        let result = router.submit("hello-agent", AIPInput::default()).await;

        // THEN a valid TaskId is returned (not AgentNotFound)
        assert!(
            result.is_ok(),
            "submit by manifest name should succeed, got: {result:?}"
        );
        let task_id = result.unwrap();
        assert!(
            uuid::Uuid::parse_str(task_id.as_str()).is_ok(),
            "task_id should be a valid UUID"
        );
    }

    /// Mock backend that completes only once the test releases its gate, used to
    /// simulate a late completion that lands after a cancel, with no wall-clock
    /// delay to race against.
    struct GatedMockBackend {
        gate: watch::Receiver<bool>,
    }

    impl ExecutionBackend for GatedMockBackend {
        fn execute(
            &self,
            task: AIPTask,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>>
        {
            let mut gate = self.gate.clone();
            Box::pin(async move {
                let _ = gate.wait_for(|released| *released).await;
                Ok(AIPResult {
                    task_id: task.task_id,
                    status: TaskStatus::Completed,
                    output: vec![],
                    error: None,
                    artifacts: vec![],
                    input_required_data: None,
                })
            })
        }
    }

    #[tokio::test]
    async fn test_canceled_status_not_overwritten_by_late_task_completed_event() {
        // GIVEN an active agent with a gated backend: it stays in-flight until the
        // test releases it, so the completion is guaranteed to land after cancel
        let (event_tx, mut event_rx) = broadcast::channel::<RuntimeEvent>(64);
        let registry = AgentRegistry::spawn(event_tx.clone());
        let router = TaskRouterHandle::spawn(registry.clone(), event_tx.clone(), 256);

        let agent_id =
            register_agent_in_state(&registry, "agent-cancel-race", ProcessState::Active).await;
        let (_gate_tx, gate_rx) = watch::channel(false);
        let coordinator = ExecutionCoordinator::new(
            agent_id.clone(),
            1,
            event_tx.clone(),
            GatedMockBackend { gate: gate_rx },
        );
        router
            .register_coordinator(agent_id.clone(), coordinator)
            .await
            .expect("register coordinator failed");

        // WHEN submitting a task then cancelling it. The backend is parked on the
        // gate, so the cancel deterministically wins the race.
        let task_id = router
            .submit(agent_id.as_str(), AIPInput::default())
            .await
            .expect("submit failed");

        let cancel_result = router
            .cancel(task_id.as_str())
            .await
            .expect("cancel failed");
        assert_eq!(
            cancel_result,
            Some(TaskStatus::Canceled),
            "cancel should return Canceled"
        );

        // AND a completion for the same task lands afterwards. The cancelled
        // execution itself no longer reports one (it is stopped, not left to
        // finish), so the late event is published the way any other source on
        // the bus would; wait for it to prove it actually went out.
        event_tx
            .send(RuntimeEvent::TaskCompleted {
                agent_id: agent_id.clone(),
                task_id: task_id.clone(),
                success: true,
                output: None,
            })
            .expect("publish the late completion");
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            tokio::select! {
                ev = event_rx.recv() => {
                    if let Ok(RuntimeEvent::TaskCompleted { task_id: tid, .. }) = ev {
                        if tid.as_str() == task_id.as_str() {
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep_until(deadline) => {
                    panic!("the late TaskCompleted event never went out");
                }
            }
        }

        // THEN the status stays Canceled: the router received the same broadcast,
        // so give its select loop repeated opportunities to process it via
        // command round-trips. The terminal-status guard must hold on every one.
        for _ in 0..50 {
            let status = router
                .get_status(task_id.as_str())
                .await
                .expect("get_status failed");
            assert_eq!(
                status,
                Some(TaskStatus::Canceled),
                "Canceled status must not be overwritten by a late TaskCompleted event"
            );
        }
    }

    #[tokio::test]
    async fn test_task_status_transitions_to_failed_via_eventbus() {
        // GIVEN an active agent with a FailingMockBackend coordinator
        let (event_tx, _event_rx) = broadcast::channel::<RuntimeEvent>(64);
        let registry = AgentRegistry::spawn(event_tx.clone());
        let router = TaskRouterHandle::spawn(registry.clone(), event_tx.clone(), 256);

        let agent_id =
            register_agent_in_state(&registry, "agent-lifecycle-fail", ProcessState::Active).await;
        let coordinator =
            ExecutionCoordinator::new(agent_id.clone(), 1, event_tx, MockBackend::failing());
        router
            .register_coordinator(agent_id.clone(), coordinator)
            .await
            .expect("register coordinator failed");

        // WHEN submitting a task
        let task_id = router
            .submit(agent_id.as_str(), AIPInput::default())
            .await
            .expect("submit failed");

        // THEN get_status() returns Failed after EventBus propagation
        let final_status = tokio::time::timeout(std::time::Duration::from_millis(500), async {
            loop {
                let s = router.get_status(task_id.as_str()).await.unwrap();
                if !matches!(s, Some(TaskStatus::Working) | Some(TaskStatus::Submitted)) {
                    return s;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("timeout waiting for task failure");

        assert_eq!(final_status, Some(TaskStatus::Failed));
    }

    // Real cancellation: the execution stops, and the status says so only then.

    /// Sets its flag when dropped, so a test can tell an abandoned future
    /// from one still running.
    struct DropFlag(std::sync::Arc<AtomicBool>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    /// Backend whose execution never ends and ignores the cancellation token.
    struct ForeverBackend {
        dropped: std::sync::Arc<AtomicBool>,
    }

    impl ExecutionBackend for ForeverBackend {
        fn execute(
            &self,
            _task: AIPTask,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>>
        {
            let flag = DropFlag(std::sync::Arc::clone(&self.dropped));
            Box::pin(async move {
                let _flag = flag;
                std::future::pending::<Result<AIPResult, String>>().await
            })
        }
    }

    /// Backend that honours the token: once cancelled it reports that it is
    /// stopping, then returns only when the test releases `release`.
    struct StoppingBackend {
        stopping: std::sync::Arc<tokio::sync::Notify>,
        release: watch::Receiver<bool>,
    }

    impl ExecutionBackend for StoppingBackend {
        fn execute(
            &self,
            task: AIPTask,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>>
        {
            self.execute_cancellable(task, CancellationToken::new())
        }

        fn execute_cancellable(
            &self,
            _task: AIPTask,
            cancel: CancellationToken,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>>
        {
            let stopping = std::sync::Arc::clone(&self.stopping);
            let mut release = self.release.clone();
            Box::pin(async move {
                cancel.cancelled().await;
                stopping.notify_one();
                let _ = release.wait_for(|released| *released).await;
                Err("stopped".to_string())
            })
        }
    }

    /// Tool executor standing for an HTTP or MCP call that never answers.
    struct UnansweredToolExecutor {
        started: std::sync::Arc<tokio::sync::Notify>,
        abandoned: std::sync::Arc<AtomicBool>,
    }

    impl apollia_tools::ToolExecutor for UnansweredToolExecutor {
        fn name(&self) -> &str {
            "http_get"
        }

        fn execute(
            &self,
            _input: serde_json::Value,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<serde_json::Value, apollia_tools::ToolExecutionError>,
                    > + Send
                    + '_,
            >,
        > {
            let flag = DropFlag(std::sync::Arc::clone(&self.abandoned));
            let started = std::sync::Arc::clone(&self.started);
            Box::pin(async move {
                let _flag = flag;
                started.notify_one();
                std::future::pending().await
            })
        }
    }

    /// Backend whose execution is one tool call through the dispatcher.
    struct ToolCallingBackend {
        dispatcher: std::sync::Arc<apollia_tools::ToolDispatcher>,
    }

    impl ExecutionBackend for ToolCallingBackend {
        fn execute(
            &self,
            task: AIPTask,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>>
        {
            let dispatcher = std::sync::Arc::clone(&self.dispatcher);
            Box::pin(async move {
                dispatcher
                    .dispatch(
                        "http_get",
                        serde_json::json!({"url": "https://example.org"}),
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(AIPResult {
                    task_id: task.task_id,
                    status: TaskStatus::Completed,
                    output: vec![],
                    error: None,
                    artifacts: vec![],
                    input_required_data: None,
                })
            })
        }
    }

    /// Spawns a router with one active agent served by `backend`.
    async fn router_with_backend<B: ExecutionBackend>(
        name: &str,
        backend: B,
    ) -> (
        TaskRouterHandle<B>,
        AgentId,
        broadcast::Receiver<RuntimeEvent>,
    ) {
        let (event_tx, event_rx) = broadcast::channel::<RuntimeEvent>(64);
        let registry = AgentRegistry::spawn(event_tx.clone());
        let router = TaskRouterHandle::spawn(registry.clone(), event_tx.clone(), 256);
        let agent_id = register_agent_in_state(&registry, name, ProcessState::Active).await;
        let coordinator = ExecutionCoordinator::new(agent_id.clone(), 1, event_tx, backend);
        router
            .register_coordinator(agent_id.clone(), coordinator)
            .await
            .expect("register coordinator failed");
        (router, agent_id, event_rx)
    }

    #[tokio::test]
    async fn cancel_stops_an_execution_that_would_wait_forever() {
        // GIVEN a task whose execution never ends and does not even look at
        // its cancellation token
        let dropped = std::sync::Arc::new(AtomicBool::new(false));
        let backend = ForeverBackend {
            dropped: std::sync::Arc::clone(&dropped),
        };
        let (router, agent_id, _events) = router_with_backend("agent-forever", backend).await;
        let task_id = router
            .submit(agent_id.as_str(), AIPInput::default())
            .await
            .expect("submit failed");

        // WHEN the task is cancelled
        let asked = std::time::Instant::now();
        let status = router
            .cancel(task_id.as_str())
            .await
            .expect("cancel failed");
        let took = asked.elapsed();

        // THEN the execution has stopped, its future dropped, within a second
        assert_eq!(status, Some(TaskStatus::Canceled));
        assert!(
            dropped.load(Ordering::SeqCst),
            "the execution must be stopped, not only relabelled"
        );
        assert!(took < std::time::Duration::from_secs(1), "took {took:?}");
        // AND its concurrency slot is free again: a new task is accepted
        router
            .submit(agent_id.as_str(), AIPInput::default())
            .await
            .expect("the agent must accept a task once the cancelled one stopped");
    }

    #[tokio::test]
    async fn status_reads_working_while_the_execution_stops_then_canceled() {
        // GIVEN a task whose backend honours the token but takes its time to
        // stop, under the test's control
        let stopping = std::sync::Arc::new(tokio::sync::Notify::new());
        let (release_tx, release_rx) = watch::channel(false);
        let backend = StoppingBackend {
            stopping: std::sync::Arc::clone(&stopping),
            release: release_rx,
        };
        let (router, agent_id, mut events) = router_with_backend("agent-stopping", backend).await;
        let task_id = router
            .submit(agent_id.as_str(), AIPInput::default())
            .await
            .expect("submit failed");

        // WHEN the task is cancelled and its execution has begun to stop
        let canceller = router.clone();
        let cancel_id = task_id.clone();
        let cancel = tokio::spawn(async move { canceller.cancel(cancel_id.as_str()).await });
        stopping.notified().await;

        // THEN the status still reads Working while the execution stops
        let during = router.get_status(task_id.as_str()).await.expect("status");
        assert_eq!(during, Some(TaskStatus::Working));
        assert!(!cancel.is_finished(), "cancel must wait for the stop");

        // AND once the execution has ended, the status is Canceled
        release_tx.send(true).expect("release the backend");
        let answered = cancel.await.expect("join").expect("cancel failed");
        assert_eq!(answered, Some(TaskStatus::Canceled));
        let after = router.get_status(task_id.as_str()).await.expect("status");
        assert_eq!(after, Some(TaskStatus::Canceled));
        // AND the bus has heard TaskCanceled, never a completion. The router
        // publishes it before answering, so it is already buffered here.
        let mut heard_canceled = false;
        loop {
            match events.try_recv() {
                Ok(RuntimeEvent::TaskCanceled { task_id: id }) if id == task_id => {
                    heard_canceled = true;
                }
                Ok(RuntimeEvent::TaskCompleted { task_id: id, .. }) if id == task_id => {
                    panic!("a cancelled task reported a completion");
                }
                Ok(_) | Err(broadcast::error::TryRecvError::Lagged(_)) => {}
                Err(_) => break,
            }
        }
        assert!(heard_canceled, "TaskCanceled was not published");
    }

    #[tokio::test]
    async fn cancel_abandons_a_tool_call_that_never_answers() {
        // GIVEN a task blocked inside a tool call (HTTP or MCP) that never
        // answers
        let started = std::sync::Arc::new(tokio::sync::Notify::new());
        let abandoned = std::sync::Arc::new(AtomicBool::new(false));
        let executor = UnansweredToolExecutor {
            started: std::sync::Arc::clone(&started),
            abandoned: std::sync::Arc::clone(&abandoned),
        };
        let backend = ToolCallingBackend {
            dispatcher: std::sync::Arc::new(apollia_tools::ToolDispatcher::new(vec![Box::new(
                executor,
            )])),
        };
        let (router, agent_id, _events) = router_with_backend("agent-tool", backend).await;
        let task_id = router
            .submit(agent_id.as_str(), AIPInput::default())
            .await
            .expect("submit failed");
        started.notified().await;

        // WHEN the task is cancelled
        let status = router
            .cancel(task_id.as_str())
            .await
            .expect("cancel failed");

        // THEN the in-flight call has been abandoned, not left pending
        assert_eq!(status, Some(TaskStatus::Canceled));
        assert!(
            abandoned.load(Ordering::SeqCst),
            "the tool call future must be dropped on cancel"
        );
    }
}
