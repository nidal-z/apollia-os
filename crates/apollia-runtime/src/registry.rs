use std::collections::HashMap;

use tokio::sync::{mpsc, oneshot};
use tracing::info;

use apollia_core::{AgentId, AgentManifest, ProcessState, RuntimeEvent};

use crate::eventbus::EventBusSender;

/// Entrée dans le registry pour un agent enregistré.
#[derive(Debug, Clone)]
pub struct AgentEntry {
    /// Identifiant unique généré à l'enregistrement (UUID v4).
    pub id: AgentId,
    /// Manifest déclaré par l'agent à l'enregistrement.
    pub manifest: AgentManifest,
    /// État courant du processus agent.
    pub process_state: ProcessState,
}

/// Erreurs possibles des opérations sur le registry.
#[derive(Debug, thiserror::Error)]
pub enum AgentRegistryError {
    /// L'agent demandé n'existe pas dans le registry.
    #[error("Agent '{0}' introuvable dans le registry")]
    NotFound(AgentId),
    /// La transition d'état demandée viole la machine d'état de `ProcessState`.
    #[error("Transition d'état invalide : {from:?} → {to:?}")]
    InvalidTransition {
        from: ProcessState,
        to: ProcessState,
    },
    /// Le canal vers l'acteur est fermé — l'acteur s'est arrêté.
    #[error("L'acteur AgentRegistry est mort (canal fermé)")]
    ActorDead,
}

// Messages internes — enum privé, jamais exposé publiquement.
// AgentManifest est boxé dans Register pour éviter une variante de taille disproportionnée.
// dead_code autorisé : ces types seront utilisés par le Supervisor (stories futures).
#[allow(dead_code)]
enum RegistryMessage {
    Register {
        manifest: Box<AgentManifest>,
        reply: oneshot::Sender<Result<AgentId, AgentRegistryError>>,
    },
    Unregister {
        id: AgentId,
        reply: oneshot::Sender<Result<(), AgentRegistryError>>,
    },
    UpdateState {
        id: AgentId,
        state: ProcessState,
        reply: oneshot::Sender<Result<(), AgentRegistryError>>,
    },
    GetAgent {
        id: AgentId,
        reply: oneshot::Sender<Option<AgentEntry>>,
    },
    ListAgents {
        reply: oneshot::Sender<Vec<AgentEntry>>,
    },
    Shutdown,
}

/// Acteur interne du registry — état privé, jamais exposé directement.
///
/// Toute interaction passe par [`AgentRegistryHandle`].
/// La construction se fait uniquement via [`AgentRegistry::spawn`], accessible
/// depuis les autres modules du crate (`pub(crate)`).
/// dead_code autorisé : sera utilisé par le Supervisor (stories futures).
#[allow(dead_code)]
pub struct AgentRegistry {
    agents: HashMap<AgentId, AgentEntry>,
    bus: EventBusSender,
}

#[allow(dead_code)]
impl AgentRegistry {
    /// Spawn l'acteur dans un Tokio task et retourne son [`AgentRegistryHandle`] public.
    ///
    /// Le canal mpsc a une capacité de 256 messages. L'acteur s'arrête
    /// naturellement quand tous les handles sont droppés (canal fermé),
    /// ou explicitement sur réception de [`RegistryMessage::Shutdown`].
    pub fn spawn(bus: EventBusSender) -> AgentRegistryHandle {
        let (tx, rx) = mpsc::channel(256);
        let registry = Self {
            agents: HashMap::new(),
            bus,
        };
        tokio::spawn(registry.run(rx));
        AgentRegistryHandle { tx }
    }

    async fn run(mut self, mut rx: mpsc::Receiver<RegistryMessage>) {
        info!("AgentRegistry démarré");
        while let Some(msg) = rx.recv().await {
            match msg {
                RegistryMessage::Register { manifest, reply } => {
                    let result = self.handle_register(*manifest);
                    let _ = reply.send(result);
                }
                RegistryMessage::Unregister { id, reply } => {
                    let result = self.handle_unregister(&id);
                    let _ = reply.send(result);
                }
                RegistryMessage::UpdateState { id, state, reply } => {
                    let result = self.handle_update_state(&id, state);
                    let _ = reply.send(result);
                }
                RegistryMessage::GetAgent { id, reply } => {
                    let entry = self.agents.get(&id).cloned();
                    let _ = reply.send(entry);
                }
                RegistryMessage::ListAgents { reply } => {
                    let list = self.agents.values().cloned().collect();
                    let _ = reply.send(list);
                }
                RegistryMessage::Shutdown => {
                    info!("AgentRegistry arrêt demandé");
                    break;
                }
            }
        }
        info!("AgentRegistry arrêté");
    }

    fn handle_register(&mut self, manifest: AgentManifest) -> Result<AgentId, AgentRegistryError> {
        let id = uuid::Uuid::new_v4().to_string();
        let entry = AgentEntry {
            id: id.clone(),
            manifest,
            process_state: ProcessState::Initializing,
        };
        self.agents.insert(id.clone(), entry);
        let _ = self.bus.send(RuntimeEvent::AgentRegistered(id.clone()));
        info!(agent_id = %id, "Agent enregistré");
        Ok(id)
    }

    fn handle_unregister(&mut self, id: &str) -> Result<(), AgentRegistryError> {
        if self.agents.remove(id).is_none() {
            return Err(AgentRegistryError::NotFound(id.to_string()));
        }
        let _ = self.bus.send(RuntimeEvent::AgentStopped(id.to_string()));
        info!(agent_id = %id, "Agent désenregistré");
        Ok(())
    }

    fn handle_update_state(
        &mut self,
        id: &str,
        new_state: ProcessState,
    ) -> Result<(), AgentRegistryError> {
        let entry = self
            .agents
            .get_mut(id)
            .ok_or_else(|| AgentRegistryError::NotFound(id.to_string()))?;

        if !entry.process_state.can_transition_to(&new_state) {
            return Err(AgentRegistryError::InvalidTransition {
                from: entry.process_state.clone(),
                to: new_state,
            });
        }

        let prev = entry.process_state.clone();
        entry.process_state = new_state.clone();
        info!(agent_id = %id, from = ?prev, to = ?new_state, "Transition ProcessState");

        let event = match &new_state {
            ProcessState::Active => RuntimeEvent::AgentReady(id.to_string()),
            ProcessState::Degraded => RuntimeEvent::AgentDegraded {
                agent_id: id.to_string(),
                reason: "transition manuelle".to_string(),
            },
            ProcessState::Stopped => RuntimeEvent::AgentStopped(id.to_string()),
            _ => return Ok(()),
        };
        let _ = self.bus.send(event);
        Ok(())
    }
}

/// Handle public vers l'acteur `AgentRegistry` — clonable, thread-safe.
///
/// Obtenu via [`AgentRegistry::spawn`]. Chaque clone partage le même acteur
/// sous-jacent. Toutes les méthodes sont async et retournent
/// [`AgentRegistryError::ActorDead`] si l'acteur s'est arrêté.
#[derive(Clone)]
pub struct AgentRegistryHandle {
    tx: mpsc::Sender<RegistryMessage>,
}

impl AgentRegistryHandle {
    /// Enregistre un nouvel agent avec son manifest.
    ///
    /// Retourne l'[`AgentId`] généré (UUID v4).
    /// L'agent est créé en état [`ProcessState::Initializing`] et
    /// [`RuntimeEvent::AgentRegistered`] est publié sur l'EventBus.
    pub async fn register(&self, manifest: AgentManifest) -> Result<AgentId, AgentRegistryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RegistryMessage::Register {
                manifest: Box::new(manifest),
                reply: reply_tx,
            })
            .await
            .map_err(|_| AgentRegistryError::ActorDead)?;
        reply_rx.await.map_err(|_| AgentRegistryError::ActorDead)?
    }

    /// Retire un agent du registry et publie [`RuntimeEvent::AgentStopped`].
    ///
    /// Retourne [`AgentRegistryError::NotFound`] si l'agent n'existe pas.
    pub async fn unregister(&self, id: &str) -> Result<(), AgentRegistryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RegistryMessage::Unregister {
                id: id.to_string(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| AgentRegistryError::ActorDead)?;
        reply_rx.await.map_err(|_| AgentRegistryError::ActorDead)?
    }

    /// Met à jour l'état [`ProcessState`] d'un agent.
    ///
    /// Retourne [`AgentRegistryError::InvalidTransition`] si la transition
    /// est refusée par la machine d'état de `ProcessState`.
    pub async fn update_state(
        &self,
        id: &str,
        state: ProcessState,
    ) -> Result<(), AgentRegistryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RegistryMessage::UpdateState {
                id: id.to_string(),
                state,
                reply: reply_tx,
            })
            .await
            .map_err(|_| AgentRegistryError::ActorDead)?;
        reply_rx.await.map_err(|_| AgentRegistryError::ActorDead)?
    }

    /// Retourne l'entrée d'un agent ou `None` s'il n'est pas enregistré.
    ///
    /// L'absence d'un agent n'est pas une erreur.
    pub async fn get_agent(&self, id: &str) -> Result<Option<AgentEntry>, AgentRegistryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RegistryMessage::GetAgent {
                id: id.to_string(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| AgentRegistryError::ActorDead)?;
        reply_rx.await.map_err(|_| AgentRegistryError::ActorDead)
    }

    /// Retourne tous les agents actuellement enregistrés.
    pub async fn list_agents(&self) -> Result<Vec<AgentEntry>, AgentRegistryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RegistryMessage::ListAgents { reply: reply_tx })
            .await
            .map_err(|_| AgentRegistryError::ActorDead)?;
        reply_rx.await.map_err(|_| AgentRegistryError::ActorDead)
    }

    /// Demande l'arrêt de l'acteur (fire-and-forget).
    ///
    /// Les messages déjà en file sont traités avant l'arrêt.
    /// Si l'acteur est déjà mort, l'erreur est silencieusement ignorée.
    pub fn shutdown(&self) {
        let _ = self.tx.try_send(RegistryMessage::Shutdown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{AgentManifest, ProcessState};
    use tokio::sync::broadcast;

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

    #[tokio::test]
    async fn test_ac1_register_emet_event() {
        // GIVEN
        let (bus_tx, mut bus_rx) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);

        // WHEN
        let result = handle.register(test_manifest("agent-test")).await;

        // THEN
        assert!(result.is_ok());
        let id = result.unwrap();
        assert!(!id.is_empty());
        let event = bus_rx.recv().await.unwrap();
        assert!(matches!(event, RuntimeEvent::AgentRegistered(eid) if eid == id));
    }

    #[tokio::test]
    async fn test_ac2_transition_valide() {
        // GIVEN
        let (bus_tx, mut bus_rx) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        let id = handle.register(test_manifest("agent-test")).await.unwrap();
        let _ = bus_rx.recv().await; // consommer AgentRegistered

        // WHEN
        let result = handle.update_state(&id, ProcessState::Active).await;

        // THEN
        assert!(result.is_ok());
        let event = bus_rx.recv().await.unwrap();
        assert!(matches!(event, RuntimeEvent::AgentReady(eid) if eid == id));
    }

    #[tokio::test]
    async fn test_ac3_transition_invalide() {
        // GIVEN
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        let id = handle.register(test_manifest("agent-test")).await.unwrap();
        handle
            .update_state(&id, ProcessState::Stopping)
            .await
            .unwrap();
        handle
            .update_state(&id, ProcessState::Stopped)
            .await
            .unwrap();

        // WHEN — Stopped → Active est invalide
        let result = handle.update_state(&id, ProcessState::Active).await;

        // THEN
        assert!(matches!(
            result.unwrap_err(),
            AgentRegistryError::InvalidTransition { .. }
        ));
    }

    #[tokio::test]
    async fn test_ac4_agent_inexistant() {
        // GIVEN
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);

        // WHEN
        let result = handle.get_agent("inexistant").await;

        // THEN
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_ac5_unregister_emet_event() {
        // GIVEN
        let (bus_tx, mut bus_rx) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        let id = handle.register(test_manifest("agent-test")).await.unwrap();
        let _ = bus_rx.recv().await; // consommer AgentRegistered

        // WHEN
        let result = handle.unregister(&id).await;

        // THEN
        assert!(result.is_ok());
        let event = bus_rx.recv().await.unwrap();
        assert!(matches!(event, RuntimeEvent::AgentStopped(eid) if eid == id));
    }

    #[tokio::test]
    async fn test_handle_clone_concurrent() {
        // GIVEN
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        let handle2 = handle.clone();

        // WHEN — deux appels concurrent via deux handles distincts
        let (r1, r2) = tokio::join!(
            handle.register(test_manifest("agent-a")),
            handle2.register(test_manifest("agent-b")),
        );

        // THEN
        assert!(r1.is_ok());
        assert!(r2.is_ok());
        assert_ne!(r1.unwrap(), r2.unwrap());
    }

    #[tokio::test]
    async fn test_actor_dead_apres_shutdown() {
        // GIVEN
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        handle.shutdown();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // WHEN
        let result = handle.register(test_manifest("agent-test")).await;

        // THEN
        assert!(matches!(result.unwrap_err(), AgentRegistryError::ActorDead));
    }

    #[tokio::test]
    async fn test_list_agents() {
        // GIVEN
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        handle.register(test_manifest("agent-a")).await.unwrap();
        handle.register(test_manifest("agent-b")).await.unwrap();

        // WHEN
        let agents = handle.list_agents().await.unwrap();

        // THEN
        assert_eq!(agents.len(), 2);
    }

    #[tokio::test]
    async fn test_unregister_agent_inexistant_retourne_not_found() {
        // GIVEN
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);

        // WHEN
        let result = handle.unregister("inexistant").await;

        // THEN
        assert!(matches!(
            result.unwrap_err(),
            AgentRegistryError::NotFound(_)
        ));
    }
}
