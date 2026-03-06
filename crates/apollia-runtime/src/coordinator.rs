use std::sync::Arc;

use apollia_core::{AIPResult, AIPTask, AgentId, RuntimeEvent, TaskId};
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

            // Emettre TaskCompleted (succes ou echec)
            let is_success = result.is_ok();
            let _ = event_bus.send(RuntimeEvent::TaskCompleted {
                agent_id,
                task_id,
                success: is_success,
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
            matches!(&completed, RuntimeEvent::TaskCompleted { agent_id, task_id, success }
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
}
