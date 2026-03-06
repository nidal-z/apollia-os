use std::collections::HashMap;

use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

use apollia_core::{AIPInput, AIPTask, AgentId, ProcessState, RuntimeEvent, TaskId, TaskStatus};

use crate::coordinator::{ExecutionBackend, ExecutionCoordinator};
use crate::eventbus::EventBusSender;
use crate::registry::AgentRegistryHandle;

/// Erreurs de soumission de tache.
#[derive(Debug, thiserror::Error)]
pub enum SubmitError {
    /// L'agent est encore en initialisation.
    #[error("agent '{0}' not ready (still initializing)")]
    AgentNotReady(AgentId),

    /// L'agent est en arret ou arrete.
    #[error("agent '{0}' unavailable (stopping or stopped)")]
    AgentUnavailable(AgentId),

    /// L'agent n'existe pas dans le registre.
    #[error("agent '{0}' not found")]
    AgentNotFound(AgentId),

    /// Le coordinateur de l'agent a atteint sa limite de concurrence.
    #[error("concurrency limit reached for agent '{0}'")]
    ConcurrencyLimit(AgentId),

    /// Pas de coordinateur enregistre pour cet agent.
    #[error("no coordinator registered for agent '{0}'")]
    NoCoordinator(AgentId),

    /// L'acteur TaskRouter est mort.
    #[error("router actor is dead")]
    ActorDead,
}

/// Messages internes du TaskRouter acteur.
enum RouterMessage<B: ExecutionBackend> {
    /// Soumettre une tache pour un agent.
    Submit {
        agent_id: AgentId,
        input: AIPInput,
        reply: oneshot::Sender<Result<TaskId, SubmitError>>,
    },
    /// Obtenir le statut d'une tache.
    GetStatus {
        task_id: TaskId,
        reply: oneshot::Sender<Option<TaskStatus>>,
    },
    /// Enregistrer un ExecutionCoordinator pour un agent.
    RegisterCoordinator {
        agent_id: AgentId,
        coordinator: ExecutionCoordinator<B>,
    },
    /// Retirer le coordinateur d'un agent (agent stopping).
    UnregisterCoordinator { agent_id: AgentId },
    /// Arreter l'acteur.
    Shutdown,
}

/// Acteur TaskRouter — point d'entree centralise pour les soumissions de taches.
///
/// Gere le dispatch des taches vers les ExecutionCoordinator des agents actifs.
/// Maintient la table de correspondance agent_id -> coordinator et task_id -> status.
struct TaskRouter<B: ExecutionBackend> {
    rx: mpsc::Receiver<RouterMessage<B>>,
    registry: AgentRegistryHandle,
    event_bus: EventBusSender,
    coordinators: HashMap<AgentId, ExecutionCoordinator<B>>,
    task_statuses: HashMap<TaskId, TaskStatus>,
}

impl<B: ExecutionBackend> TaskRouter<B> {
    /// Boucle principale de l'acteur.
    ///
    /// Traite les messages jusqu'a reception de Shutdown.
    async fn run(mut self) {
        info!("TaskRouter demarre");
        while let Some(msg) = self.rx.recv().await {
            match msg {
                RouterMessage::Submit {
                    agent_id,
                    input,
                    reply,
                } => {
                    let result = self.handle_submit(agent_id, input).await;
                    let _ = reply.send(result);
                }
                RouterMessage::GetStatus { task_id, reply } => {
                    let status = self.task_statuses.get(&task_id).cloned();
                    let _ = reply.send(status);
                }
                RouterMessage::RegisterCoordinator {
                    agent_id,
                    coordinator,
                } => {
                    info!(agent_id = %agent_id, "Coordinator enregistre");
                    self.coordinators.insert(agent_id, coordinator);
                }
                RouterMessage::UnregisterCoordinator { agent_id } => {
                    info!(agent_id = %agent_id, "Coordinator retire");
                    self.coordinators.remove(&agent_id);
                }
                RouterMessage::Shutdown => {
                    info!("TaskRouter arret demande");
                    break;
                }
            }
        }
        info!("TaskRouter arrete");
    }

    /// Gere la soumission d'une tache.
    ///
    /// 1. Verifie l'etat de l'agent via AgentRegistryHandle
    /// 2. Genere un TaskId (UUID v4)
    /// 3. Construit l'AIPTask
    /// 4. Dispatche vers le coordinateur
    async fn handle_submit(
        &mut self,
        agent_id: AgentId,
        input: AIPInput,
    ) -> Result<TaskId, SubmitError> {
        // 1. Verifier l'agent dans le registre
        let agent_entry = self
            .registry
            .get_agent(&agent_id)
            .await
            .map_err(|_| SubmitError::ActorDead)?
            .ok_or_else(|| SubmitError::AgentNotFound(agent_id.clone()))?;

        // 2. Verifier le ProcessState
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
                // OK, dispatch normal
            }
        }

        // 3. Generer TaskId + construire AIPTask
        let task_id: TaskId = uuid::Uuid::new_v4().to_string();
        let task = AIPTask {
            task_id: task_id.clone(),
            context_id: format!("ctx-{}", agent_id),
            input,
            history: vec![],
            timeout_seconds: None,
        };

        // 4. Dispatcher vers le coordinateur
        let coordinator = self
            .coordinators
            .get(&agent_id)
            .ok_or_else(|| SubmitError::NoCoordinator(agent_id.clone()))?;

        coordinator
            .submit_task(task)
            .map_err(|_| SubmitError::ConcurrencyLimit(agent_id.clone()))?;

        // 5. Enregistrer le statut
        self.task_statuses
            .insert(task_id.clone(), TaskStatus::Working);

        info!(task_id = %task_id, agent_id = %agent_id, "Task dispatched");
        Ok(task_id)
    }
}

/// Handle clonable pour interagir avec le TaskRouter acteur.
///
/// Thread-safe : Clone + Send + Sync.
/// Clone est implemente manuellement car `mpsc::Sender` est Clone
/// independamment des bounds sur B.
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
    /// Spawne le TaskRouter acteur et retourne un Handle.
    ///
    /// # Arguments
    /// - `registry` : handle vers l'AgentRegistry pour verifier les etats
    /// - `event_bus` : canal d'emission des evenements runtime
    /// - `buffer_size` : taille du canal mpsc (defaut recommande: 256)
    pub fn spawn(
        registry: AgentRegistryHandle,
        event_bus: EventBusSender,
        buffer_size: usize,
    ) -> Self {
        let (tx, rx) = mpsc::channel(buffer_size);
        let router = TaskRouter {
            rx,
            registry,
            event_bus,
            coordinators: HashMap::new(),
            task_statuses: HashMap::new(),
        };
        tokio::spawn(router.run());
        Self { tx }
    }

    /// Soumet une tache pour un agent.
    ///
    /// Retourne le TaskId genere en cas de succes.
    pub async fn submit(
        &self,
        agent_id: &str,
        input: AIPInput,
    ) -> Result<TaskId, SubmitError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RouterMessage::Submit {
                agent_id: agent_id.to_string(),
                input,
                reply: reply_tx,
            })
            .await
            .map_err(|_| SubmitError::ActorDead)?;
        reply_rx.await.map_err(|_| SubmitError::ActorDead)?
    }

    /// Obtient le statut d'une tache.
    pub async fn get_status(
        &self,
        task_id: &str,
    ) -> Result<Option<TaskStatus>, SubmitError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RouterMessage::GetStatus {
                task_id: task_id.to_string(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| SubmitError::ActorDead)?;
        reply_rx.await.map_err(|_| SubmitError::ActorDead)
    }

    /// Enregistre un coordinateur pour un agent.
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

    /// Retire le coordinateur d'un agent.
    pub async fn unregister_coordinator(
        &self,
        agent_id: &AgentId,
    ) -> Result<(), SubmitError> {
        self.tx
            .send(RouterMessage::UnregisterCoordinator {
                agent_id: agent_id.clone(),
            })
            .await
            .map_err(|_| SubmitError::ActorDead)
    }

    /// Demande l'arret de l'acteur.
    pub fn shutdown(&self) {
        let _ = self.tx.try_send(RouterMessage::Shutdown);
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

    /// Backend mock qui accepte toujours les taches.
    struct MockBackend {
        should_fail: AtomicBool,
    }

    impl MockBackend {
        fn success() -> Self {
            Self {
                should_fail: AtomicBool::new(false),
            }
        }
    }

    impl ExecutionBackend for MockBackend {
        fn execute(
            &self,
            task: AIPTask,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>,
        > {
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
        }
    }

    /// Helper pour creer un environnement de test complet.
    /// Retourne (router, registry, event_rx).
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

    /// Helper pour enregistrer un agent et le mettre dans l'etat voulu.
    /// Retourne l'AgentId genere.
    async fn register_agent_in_state(
        registry: &AgentRegistryHandle,
        name: &str,
        target_state: ProcessState,
    ) -> AgentId {
        let agent_id = registry.register(test_manifest(name)).await.expect("register failed");

        // Transitions vers l'etat cible
        match target_state {
            ProcessState::Initializing => {}
            ProcessState::Active => {
                registry
                    .update_state(&agent_id, ProcessState::Active)
                    .await
                    .expect("transition to Active failed");
            }
            ProcessState::Degraded => {
                registry
                    .update_state(&agent_id, ProcessState::Active)
                    .await
                    .expect("transition to Active failed");
                registry
                    .update_state(&agent_id, ProcessState::Degraded)
                    .await
                    .expect("transition to Degraded failed");
            }
            ProcessState::Stopping => {
                registry
                    .update_state(&agent_id, ProcessState::Active)
                    .await
                    .expect("transition to Active failed");
                registry
                    .update_state(&agent_id, ProcessState::Stopping)
                    .await
                    .expect("transition to Stopping failed");
            }
            ProcessState::Stopped => {
                registry
                    .update_state(&agent_id, ProcessState::Active)
                    .await
                    .expect("transition to Active failed");
                registry
                    .update_state(&agent_id, ProcessState::Stopping)
                    .await
                    .expect("transition to Stopping failed");
                registry
                    .update_state(&agent_id, ProcessState::Stopped)
                    .await
                    .expect("transition to Stopped failed");
            }
        }

        agent_id
    }

    #[tokio::test]
    async fn test_submit_to_active_agent_returns_task_id() {
        // GIVEN un agent enregistre en etat Active avec un coordinateur
        let (router, registry, _rx) = setup_test_env().await;
        let agent_id = register_agent_in_state(&registry, "agent-active", ProcessState::Active).await;
        let (event_tx, _) = broadcast::channel(64);
        let coordinator = ExecutionCoordinator::new(agent_id.clone(), 1, event_tx, MockBackend::success());
        router
            .register_coordinator(agent_id.clone(), coordinator)
            .await
            .expect("register coordinator failed");

        // WHEN on soumet une tache via router.submit()
        let result = router.submit(&agent_id, AIPInput::default()).await;

        // THEN un TaskId est retourne (non vide, format UUID)
        assert!(result.is_ok(), "submit should succeed, got: {result:?}");
        let task_id = result.expect("already checked");
        assert!(!task_id.is_empty());
        assert!(uuid::Uuid::parse_str(&task_id).is_ok(), "task_id should be a valid UUID");
    }

    #[tokio::test]
    async fn test_submit_to_initializing_agent_rejected() {
        // GIVEN un agent enregistre en etat Initializing
        let (router, registry, _rx) = setup_test_env().await;
        let agent_id =
            register_agent_in_state(&registry, "agent-init", ProcessState::Initializing).await;

        // WHEN on soumet une tache
        let result = router.submit(&agent_id, AIPInput::default()).await;

        // THEN retourne SubmitError::AgentNotReady
        assert!(matches!(
            result.expect_err("should fail"),
            SubmitError::AgentNotReady(_)
        ));
    }

    #[tokio::test]
    async fn test_submit_to_stopped_agent_rejected() {
        // GIVEN un agent enregistre en etat Stopped
        let (router, registry, _rx) = setup_test_env().await;
        let agent_id =
            register_agent_in_state(&registry, "agent-stopped", ProcessState::Stopped).await;

        // WHEN on soumet une tache
        let result = router.submit(&agent_id, AIPInput::default()).await;

        // THEN retourne SubmitError::AgentUnavailable
        assert!(matches!(
            result.expect_err("should fail"),
            SubmitError::AgentUnavailable(_)
        ));
    }

    #[tokio::test]
    async fn test_submit_to_degraded_agent_dispatches_with_warning() {
        // GIVEN un agent enregistre en etat Degraded avec un coordinateur
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

        // WHEN on soumet une tache
        let result = router.submit(&agent_id, AIPInput::default()).await;

        // THEN la tache est dispatche (TaskId retourne)
        assert!(result.is_ok(), "submit should succeed for degraded agent");

        // AND un RuntimeEvent::AgentDegraded est emis sur l'EventBus
        // Wait briefly for event propagation
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let mut found_degraded = false;
        loop {
            match event_rx.try_recv() {
                Ok(RuntimeEvent::AgentDegraded { reason, .. }) => {
                    assert!(reason.contains("degraded"));
                    found_degraded = true;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        assert!(found_degraded, "should have received AgentDegraded event");
    }

    #[tokio::test]
    async fn test_submit_to_unknown_agent_not_found() {
        // GIVEN aucun agent enregistre avec l'id "unknown-agent"
        let (router, _registry, _rx) = setup_test_env().await;

        // WHEN on soumet une tache pour "unknown-agent"
        let result = router.submit("unknown-agent", AIPInput::default()).await;

        // THEN retourne SubmitError::AgentNotFound
        assert!(matches!(
            result.expect_err("should fail"),
            SubmitError::AgentNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_get_status_returns_task_status() {
        // GIVEN une tache soumise avec succes (task_id connu)
        let (router, registry, _rx) = setup_test_env().await;
        let agent_id = register_agent_in_state(&registry, "agent-status", ProcessState::Active).await;
        let (event_tx, _) = broadcast::channel(64);
        let coordinator = ExecutionCoordinator::new(agent_id.clone(), 1, event_tx, MockBackend::success());
        router
            .register_coordinator(agent_id.clone(), coordinator)
            .await
            .expect("register coordinator failed");

        let task_id = router
            .submit(&agent_id, AIPInput::default())
            .await
            .expect("submit failed");

        // WHEN on appelle get_status(task_id)
        let status = router.get_status(&task_id).await.expect("get_status failed");

        // THEN retourne Some(TaskStatus::Working)
        assert_eq!(status, Some(TaskStatus::Working));
    }

    #[tokio::test]
    async fn test_router_is_actor_handle_clone_send_sync() {
        // GIVEN un TaskRouterHandle
        // THEN le handle est Send + Sync (verifie a la compilation)
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TaskRouterHandle<MockBackend>>();

        // AND on peut cloner le handle
        let (router, _registry, _rx) = setup_test_env().await;
        let _cloned = router.clone();
    }
}
