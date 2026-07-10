use std::collections::HashMap;

use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

use apollia_core::token_budget::TokenBudget;
use apollia_core::{
    AIPInput, AIPTask, AgentId, ProcessState, RunId, RunOptions, RuntimeEvent, TaskId, TaskStatus,
};

use crate::coordinator::{ExecutionBackend, ExecutionCoordinator};
use crate::eventbus::EventBusSender;
use crate::registry::AgentRegistryHandle;

/// Task submission errors.
#[derive(Debug, thiserror::Error)]
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
    GetOutput {
        task_id: TaskId,
        reply: oneshot::Sender<Option<String>>,
    },
    /// Cancel a running task.
    Cancel {
        task_id: TaskId,
        reply: oneshot::Sender<Option<TaskStatus>>,
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
    /// Latest session budget snapshot, updated on each TokenBudgetUpdated event.
    latest_budget: Option<TokenBudget>,
}

impl<B: ExecutionBackend> TaskRouter<B> {
    /// Main actor loop.
    ///
    /// Processes mpsc messages AND EventBus events (TaskCompleted/TaskFailed).
    async fn run(mut self) {
        use apollia_core::RuntimeEvent;
        info!("TaskRouter demarre");
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
                        RouterMessage::GetOutput { task_id, reply } => {
                            let output = self.task_outputs.get(&task_id).cloned();
                            let _ = reply.send(output);
                        }
                        RouterMessage::Cancel { task_id, reply } => {
                            let result = self.handle_cancel(&task_id);
                            let _ = reply.send(result);
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
                            info!(agent_id = %agent_id, "Coordinator enregistre");
                            self.coordinators.insert(agent_id, coordinator);
                        }
                        RouterMessage::UnregisterCoordinator { agent_id } => {
                            info!(agent_id = %agent_id, "Coordinator retire");
                            self.coordinators.remove(&agent_id);
                        }
                        RouterMessage::GetBudget { reply } => {
                            let _ = reply.send(self.latest_budget.clone());
                        }
                        RouterMessage::Shutdown => {
                            info!("TaskRouter arret demande");
                            break;
                        }
                    }
                }
                event = self.event_rx.recv() => {
                    match event {
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
        info!("TaskRouter arrete");
    }

    /// Handle a task submission.
    ///
    /// 1. Check the agent state via AgentRegistryHandle
    /// 2. Generate a TaskId (UUID v4)
    /// 3. Build the AIPTask
    /// 4. Dispatch to the coordinator
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
                warn!(agent_id = %agent_id, "task submitted to degraded agent");
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
        let task = AIPTask {
            task_id: task_id.to_string(),
            context_id: format!("ctx-{}", agent_id),
            skill_id,
            input,
            history: vec![],
            timeout_seconds: None,
            delegation_chain,
            run_options,
            run_id: Some(RunId::new()),
            ..AIPTask::default()
        };

        // 4. Dispatch to the coordinator
        let coordinator = self
            .coordinators
            .get(&agent_id)
            .ok_or_else(|| SubmitError::NoCoordinator(agent_id.clone()))?;

        coordinator
            .submit_task(task)
            .map_err(|_| SubmitError::ConcurrencyLimit(agent_id.clone()))?;

        // 5. Record the status and the task -> agent association
        self.task_statuses
            .insert(task_id.clone(), TaskStatus::Working);
        self.task_agents.insert(task_id.clone(), agent_id.clone());

        info!(task_id = %task_id, agent_id = %agent_id, "Task dispatched");
        Ok(task_id)
    }

    /// Handle a task cancellation.
    ///
    /// Returns the new status if the task exists, None otherwise.
    /// Only tasks in `Submitted` or `Working` state can be canceled.
    fn handle_cancel(&mut self, task_id: &TaskId) -> Option<TaskStatus> {
        let status = self.task_statuses.get_mut(task_id)?;
        match status {
            TaskStatus::Submitted | TaskStatus::Working | TaskStatus::InputRequired => {
                *status = TaskStatus::Canceled;
                info!(task_id = %task_id, "Task canceled");
                Some(TaskStatus::Canceled)
            }
            _ => Some(status.clone()),
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
        let event_rx = event_bus.subscribe();
        let router = TaskRouter {
            rx,
            registry,
            event_bus,
            event_rx,
            coordinators: HashMap::new(),
            task_statuses: HashMap::new(),
            task_agents: HashMap::new(),
            task_outputs: HashMap::new(),
            latest_budget: None,
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

    /// Submit a root task with per-run control options (plan-gate / autonomy).
    ///
    /// Used by the REST submit handler to forward CLI flags (`--plan`,
    /// `--autonomy`) to the per-task engine. Equivalent to [`Self::submit`] with
    /// no targeted skill and an empty delegation chain, plus the options.
    pub async fn submit_with_options(
        &self,
        agent_id: &str,
        input: AIPInput,
        run_options: RunOptions,
    ) -> Result<TaskId, SubmitError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RouterMessage::Submit {
                agent_id: AgentId::from(agent_id),
                input,
                skill_id: None,
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
    /// Returns the new status if the task exists, None otherwise.
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
    use tokio::sync::broadcast;

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
        loop {
            match event_rx.try_recv() {
                Ok(_) => {}
                Err(_) => break,
            }
        }

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

    /// Mock backend with a configurable delay, used to simulate a late completion.
    struct DelayedMockBackend {
        delay_ms: u64,
    }

    impl ExecutionBackend for DelayedMockBackend {
        fn execute(
            &self,
            task: AIPTask,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>>
        {
            let delay_ms = self.delay_ms;
            Box::pin(async move {
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
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
        // GIVEN an active agent with a backend that completes with a delay (100ms)
        let (event_tx, mut event_rx) = broadcast::channel::<RuntimeEvent>(64);
        let registry = AgentRegistry::spawn(event_tx.clone());
        let router = TaskRouterHandle::spawn(registry.clone(), event_tx.clone(), 256);

        let agent_id =
            register_agent_in_state(&registry, "agent-cancel-race", ProcessState::Active).await;
        let coordinator = ExecutionCoordinator::new(
            agent_id.clone(),
            1,
            event_tx,
            DelayedMockBackend { delay_ms: 100 },
        );
        router
            .register_coordinator(agent_id.clone(), coordinator)
            .await
            .expect("register coordinator failed");

        // WHEN submitting a task then cancelling it immediately
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

        // AND wait deterministically for the backend's late TaskCompleted event
        // (proves the completion actually fired, instead of guessing a delay).
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
                    panic!("backend never emitted its late TaskCompleted event");
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
}
