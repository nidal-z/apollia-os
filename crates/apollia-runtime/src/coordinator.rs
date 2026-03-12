use std::sync::Arc;

use apollia_core::{AIPPart, AIPResult, AIPTask, AgentId, RuntimeEvent, TaskId, TaskStatus};
use tokio::sync::Semaphore;

use crate::eventbus::EventBusSender;

/// Trait abstraisant le backend d'execution (ORIA ou mock pour les tests).
///
/// Suit le meme pattern que `ToolExecutor` (ADR-015) et `AgentRunner` (ADR-016) :
/// un trait injectable pour decoupler le coordinateur du moteur d'execution concret.
pub trait ExecutionBackend: Send + Sync + 'static {
    /// Execute une tache et retourne le resultat.
    fn execute(
        &self,
        task: AIPTask,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>>;
}

/// Type-erased execution backend for dynamic dispatch.
///
/// Wraps `Arc<dyn ExecutionBackend + Send + Sync>` and implements `Clone`
/// cheaply (atomic reference count increment). Used in production so that
/// different agents can each have a different concrete backend while sharing
/// the same generic `TaskRouter<DynBackend>`.
#[derive(Clone)]
pub struct DynBackend(pub Arc<dyn ExecutionBackend + Send + Sync + 'static>);

impl DynBackend {
    /// Wraps any `ExecutionBackend` implementation in a `DynBackend`.
    pub fn new<B: ExecutionBackend + Send + Sync + 'static>(backend: B) -> Self {
        Self(Arc::new(backend))
    }
}

impl ExecutionBackend for DynBackend {
    fn execute(
        &self,
        task: AIPTask,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>>
    {
        self.0.execute(task)
    }
}

/// Erreurs du coordinateur d'execution.
#[derive(Debug, thiserror::Error)]
pub enum CoordinatorError {
    /// Limite de concurrence atteinte pour cet agent.
    #[error("concurrency limit reached for agent '{0}' (max_concurrent_tasks)")]
    ConcurrencyLimitReached(AgentId),

    /// Echec de l'execution de la tache.
    #[error("execution failed: {0}")]
    ExecutionFailed(String),
}

/// Coordinateur d'execution pour un agent actif.
///
/// Gere la concurrence des taches via un semaphore Tokio.
/// Un coordinateur est cree par agent actif et possede son propre semaphore.
pub struct ExecutionCoordinator<B: ExecutionBackend> {
    agent_id: AgentId,
    concurrency: Arc<Semaphore>,
    event_bus: EventBusSender,
    backend: Arc<B>,
}

impl<B: ExecutionBackend> ExecutionCoordinator<B> {
    /// Cree un nouveau coordinateur pour l'agent donne.
    ///
    /// # Arguments
    /// - `agent_id` : identifiant de l'agent
    /// - `max_concurrent` : nombre maximal de taches en parallele (defaut: 1)
    /// - `event_bus` : canal d'emission des evenements runtime
    /// - `backend` : backend d'execution (ORIA ou mock)
    pub fn new(
        agent_id: AgentId,
        max_concurrent: u32,
        event_bus: EventBusSender,
        backend: B,
    ) -> Self {
        Self {
            agent_id,
            concurrency: Arc::new(Semaphore::new(max_concurrent as usize)),
            event_bus,
            backend: Arc::new(backend),
        }
    }

    /// Soumet une tache pour execution.
    ///
    /// Tente d'acquerir un permit sur le semaphore :
    /// - Si obtenu : spawne une tache Tokio, emet `TaskStarted`, retourne `JoinHandle`
    /// - Si semaphore plein : retourne `ConcurrencyLimitReached`
    ///
    /// Le permit est libere automatiquement quand la tache spawnee termine
    /// (via drop du `OwnedSemaphorePermit` dans la closure).
    pub fn submit_task(
        &self,
        task: AIPTask,
    ) -> Result<tokio::task::JoinHandle<Result<AIPResult, CoordinatorError>>, CoordinatorError>
    {
        let permit = Arc::clone(&self.concurrency)
            .try_acquire_owned()
            .map_err(|_| CoordinatorError::ConcurrencyLimitReached(self.agent_id.clone()))?;

        let agent_id = self.agent_id.clone();
        let event_bus = self.event_bus.clone();
        let task_id = TaskId::from(task.task_id.clone());
        let backend = Arc::clone(&self.backend);

        let handle = tokio::spawn(async move {
            // Le permit est move dans la closure — libere au drop
            let _permit = permit;

            // Emettre TaskStarted
            let _ = event_bus.send(RuntimeEvent::TaskStarted {
                agent_id: agent_id.clone(),
                task_id: task_id.clone(),
            });

            // Executer via le backend
            let result = backend.execute(task).await;

            // Emettre TaskCompleted (succes ou echec) avec l'output textuel si disponible.
            //
            // `is_success` must reflect the Python-level status, not just whether the
            // Rust call succeeded. An `Ok(AIPResult { status: Failed, .. })` means the
            // agent explicitly reported failure and must propagate as success=false so
            // that the TaskRouter transitions the task to `TaskStatus::Failed`.
            let is_success = match &result {
                Ok(aip_result) => aip_result.status != TaskStatus::Failed,
                Err(e) => {
                    tracing::error!(
                        agent_id = %agent_id,
                        task_id  = %task_id,
                        error    = %e,
                        "task execution failed"
                    );
                    false
                }
            };
            let output = result.as_ref().ok().map(aip_result_to_text);
            let _ = event_bus.send(RuntimeEvent::TaskCompleted {
                agent_id,
                task_id,
                success: is_success,
                output,
            });

            result.map_err(CoordinatorError::ExecutionFailed)
        });

        Ok(handle)
    }

    /// Retourne le nombre de permits disponibles (taches pouvant etre acceptees).
    pub fn available_permits(&self) -> usize {
        self.concurrency.available_permits()
    }
}

/// Concatenates the text parts of an [`AIPResult`] into a single `String`.
///
/// Only [`AIPPart::Text`] parts contribute; file and data parts are ignored.
/// Returns an empty string when the result contains no text parts.
fn aip_result_to_text(result: &AIPResult) -> String {
    result
        .output
        .iter()
        .filter_map(|part| {
            if let AIPPart::Text(tp) = part {
                Some(tp.text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{AIPInput, TaskStatus};
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::sync::broadcast;

    /// Backend mock qui retourne un resultat configurable.
    struct MockBackend {
        should_fail: AtomicBool,
        /// Duree d'attente simulee avant de retourner le resultat.
        delay: std::time::Duration,
    }

    impl MockBackend {
        fn success() -> Self {
            Self {
                should_fail: AtomicBool::new(false),
                delay: std::time::Duration::ZERO,
            }
        }

        fn success_with_delay(delay: std::time::Duration) -> Self {
            Self {
                should_fail: AtomicBool::new(false),
                delay,
            }
        }

        fn failing() -> Self {
            Self {
                should_fail: AtomicBool::new(true),
                delay: std::time::Duration::ZERO,
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
            let delay = self.delay;
            Box::pin(async move {
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
                if fail {
                    Err("mock execution failure".to_string())
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

    fn make_task(id: &str) -> AIPTask {
        AIPTask {
            task_id: id.to_string(),
            context_id: "ctx-test".to_string(),
            input: AIPInput::default(),
            history: vec![],
            timeout_seconds: None,
            ..AIPTask::default()
        }
    }

    #[tokio::test]
    async fn test_submit_task_returns_join_handle() {
        // GIVEN un coordinator avec max_concurrent=1
        let (tx, _rx) = broadcast::channel::<RuntimeEvent>(64);
        let coord = ExecutionCoordinator::new("agent-1".into(), 1, tx, MockBackend::success());

        // WHEN on soumet une tache
        let handle = coord.submit_task(make_task("task-1"));

        // THEN submit_task retourne Ok(JoinHandle)
        assert!(handle.is_ok());
        let result = handle.unwrap().await.expect("join failed");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().task_id, "task-1");
    }

    #[tokio::test]
    async fn test_concurrency_limit_sequential() {
        // GIVEN un coordinator avec max_concurrent=1
        let (tx, _rx) = broadcast::channel::<RuntimeEvent>(64);
        let coord = ExecutionCoordinator::new(
            "agent-1".into(),
            1,
            tx,
            MockBackend::success_with_delay(std::time::Duration::from_millis(200)),
        );

        // AND une tache deja en cours d'execution
        let _handle1 = coord
            .submit_task(make_task("task-1"))
            .expect("first submit should succeed");

        // WHEN on soumet une deuxieme tache
        let result = coord.submit_task(make_task("task-2"));

        // THEN retourne ConcurrencyLimitReached
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(&err, CoordinatorError::ConcurrencyLimitReached(id) if id == "agent-1"),
            "expected ConcurrencyLimitReached, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_concurrency_limit_parallel() {
        // GIVEN un coordinator avec max_concurrent=3
        let (tx, _rx) = broadcast::channel::<RuntimeEvent>(64);
        let coord = ExecutionCoordinator::new(
            "agent-1".into(),
            3,
            tx,
            MockBackend::success_with_delay(std::time::Duration::from_millis(200)),
        );

        // WHEN on soumet 3 taches simultanement
        let h1 = coord.submit_task(make_task("task-1"));
        let h2 = coord.submit_task(make_task("task-2"));
        let h3 = coord.submit_task(make_task("task-3"));

        // THEN toutes sont acceptees
        assert!(h1.is_ok());
        assert!(h2.is_ok());
        assert!(h3.is_ok());

        // AND une 4eme retourne ConcurrencyLimitReached
        let h4 = coord.submit_task(make_task("task-4"));
        assert!(h4.is_err());
        assert!(matches!(
            h4.unwrap_err(),
            CoordinatorError::ConcurrencyLimitReached(_)
        ));
    }

    #[tokio::test]
    async fn test_task_started_event_emitted() {
        // GIVEN un coordinator et un receiver sur l'EventBus
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(64);
        let coord = ExecutionCoordinator::new("agent-1".into(), 1, tx, MockBackend::success());

        // WHEN on soumet une tache
        let handle = coord
            .submit_task(make_task("task-42"))
            .expect("submit should succeed");
        handle
            .await
            .expect("join failed")
            .expect("execution failed");

        // THEN un RuntimeEvent::TaskStarted est recu avec le bon agent_id et task_id
        let event = rx.recv().await.expect("should receive TaskStarted");
        assert!(
            matches!(&event, RuntimeEvent::TaskStarted { agent_id, task_id }
                if agent_id == "agent-1" && task_id == "task-42"),
            "expected TaskStarted, got: {event:?}"
        );
    }

    #[tokio::test]
    async fn test_task_completed_event_emitted() {
        // GIVEN un coordinator et un receiver sur l'EventBus
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(64);
        let coord = ExecutionCoordinator::new("agent-1".into(), 1, tx, MockBackend::success());

        // WHEN une tache termine (succes)
        let handle = coord
            .submit_task(make_task("task-99"))
            .expect("submit should succeed");
        handle
            .await
            .expect("join failed")
            .expect("execution failed");

        // THEN un RuntimeEvent::TaskCompleted est recu avec success=true
        // (skip TaskStarted first)
        let _started = rx.recv().await.expect("should receive TaskStarted");
        let completed = rx.recv().await.expect("should receive TaskCompleted");
        assert!(
            matches!(&completed, RuntimeEvent::TaskCompleted { agent_id, task_id, success, .. }
                if agent_id == "agent-1" && task_id == "task-99" && *success),
            "expected TaskCompleted with success=true, got: {completed:?}"
        );
    }

    #[tokio::test]
    async fn test_permit_released_on_failure() {
        // GIVEN un coordinator avec max_concurrent=1
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(64);
        let coord = ExecutionCoordinator::new("agent-1".into(), 1, tx, MockBackend::failing());

        // AND une tache qui echoue
        let handle = coord
            .submit_task(make_task("task-fail"))
            .expect("submit should succeed");
        let result = handle.await.expect("join failed");
        assert!(result.is_err(), "execution should have failed");

        // THEN available_permits() == 1 (permit libere)
        assert_eq!(coord.available_permits(), 1);

        // AND on peut soumettre une nouvelle tache
        let handle2 = coord.submit_task(make_task("task-retry"));
        assert!(handle2.is_ok(), "should be able to submit after failure");

        // AND TaskCompleted with success=false was emitted
        let _started = rx.recv().await.expect("should receive TaskStarted");
        let completed = rx.recv().await.expect("should receive TaskCompleted");
        assert!(
            matches!(&completed, RuntimeEvent::TaskCompleted { success, .. } if !success),
            "expected TaskCompleted with success=false, got: {completed:?}"
        );
    }

    /// When the backend returns `Ok(AIPResult { status: Failed })` the coordinator
    /// must emit `TaskCompleted { success: false }`.  Without this, a Python agent
    /// that returns `{"status": "failed"}` would silently appear as "completed".
    #[tokio::test]
    async fn test_python_level_failure_is_success_false() {
        // GIVEN a backend that returns Ok(AIPResult { status: Failed })
        struct AgentLevelFailBackend;
        impl ExecutionBackend for AgentLevelFailBackend {
            fn execute(
                &self,
                task: AIPTask,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>,
            > {
                Box::pin(async move {
                    Ok(AIPResult {
                        task_id: task.task_id,
                        status: TaskStatus::Failed,
                        output: vec![],
                        error: Some(apollia_core::AIPError {
                            code: "MISSING_INPUT".into(),
                            message: "no text part".into(),
                            details: None,
                        }),
                        artifacts: vec![],
                        input_required_data: None,
                    })
                })
            }
        }

        let (tx, mut rx) = broadcast::channel(16);
        let coord = ExecutionCoordinator::new("agent-42".into(), 1, tx, AgentLevelFailBackend);

        // WHEN
        let handle = coord
            .submit_task(make_task("task-agent-fail"))
            .expect("submit should succeed");
        handle.await.expect("join should succeed");

        // THEN TaskCompleted carries success=false
        let _started = rx.recv().await.expect("TaskStarted");
        let completed = rx.recv().await.expect("TaskCompleted");
        assert!(
            matches!(&completed, RuntimeEvent::TaskCompleted { success, .. } if !success),
            "agent-level Failed must propagate as success=false, got: {completed:?}"
        );
    }
}
