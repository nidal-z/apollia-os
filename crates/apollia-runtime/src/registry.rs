use std::collections::HashMap;
use std::time::Instant;

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
    /// Instant de l'enregistrement, pour calculer l'uptime.
    pub registered_at: Instant,
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
    FindByName {
        name: String,
        reply: oneshot::Sender<Option<AgentId>>,
    },
    ListAgents {
        reply: oneshot::Sender<Vec<AgentEntry>>,
    },
    ListA2aAgents {
        reply: oneshot::Sender<Vec<AgentEntry>>,
    },
    Shutdown,
}

/// Acteur interne du registry — état privé, jamais exposé directement.
///
/// Toute interaction passe par [`AgentRegistryHandle`].
/// La construction se fait uniquement via [`AgentRegistry::spawn`].
pub struct AgentRegistry {
    agents: HashMap<AgentId, AgentEntry>,
    /// Index secondaire : manifest.name → AgentId pour lookup par nom.
    name_index: HashMap<String, AgentId>,
    bus: EventBusSender,
}

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
            name_index: HashMap::new(),
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
                RegistryMessage::FindByName { name, reply } => {
                    let id = self.name_index.get(&name).cloned();
                    let _ = reply.send(id);
                }
                RegistryMessage::ListAgents { reply } => {
                    let list = self.agents.values().cloned().collect();
                    let _ = reply.send(list);
                }
                RegistryMessage::ListA2aAgents { reply } => {
                    let list = self
                        .agents
                        .values()
                        .filter(|e| e.manifest.supports_a2a)
                        .cloned()
                        .collect();
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
        // If the same agent name is already registered (e.g. stop/restart without unregister),
        // evict the old entry so list_agents() never returns stale duplicates.
        if let Some(old_id) = self.name_index.remove(&manifest.name) {
            self.agents.remove(&old_id);
        }

        let id = AgentId::new_v4();

        self.name_index.insert(manifest.name.clone(), id.clone());
        let entry = AgentEntry {
            id: id.clone(),
            manifest,
            process_state: ProcessState::Initializing,
            registered_at: Instant::now(),
        };
        self.agents.insert(id.clone(), entry);
        let _ = self.bus.send(RuntimeEvent::AgentRegistered(id.clone()));
        info!(agent_id = %id, "Agent enregistré");
        Ok(id)
    }

    fn handle_unregister(&mut self, id: &AgentId) -> Result<(), AgentRegistryError> {
        let entry = self
            .agents
            .remove(id)
            .ok_or_else(|| AgentRegistryError::NotFound(id.clone()))?;
        self.name_index.remove(&entry.manifest.name);
        let _ = self.bus.send(RuntimeEvent::AgentStopped(id.clone()));
        info!(agent_id = %id, "Agent désenregistré");
        Ok(())
    }

    fn handle_update_state(
        &mut self,
        id: &AgentId,
        new_state: ProcessState,
    ) -> Result<(), AgentRegistryError> {
        let entry = self
            .agents
            .get_mut(id)
            .ok_or_else(|| AgentRegistryError::NotFound(id.clone()))?;

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
            ProcessState::Active => RuntimeEvent::AgentReady(id.clone()),
            ProcessState::Degraded => RuntimeEvent::AgentDegraded {
                agent_id: id.clone(),
                reason: "transition manuelle".to_string(),
            },
            ProcessState::Stopping => RuntimeEvent::AgentStopping(id.clone()),
            ProcessState::Stopped => RuntimeEvent::AgentStopped(id.clone()),
            ProcessState::Initializing => return Ok(()),
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
    /// Si un agent du même nom existe déjà (cycle stop/restart), l'ancienne entrée
    /// est évincée avant l'insertion du nouvel enregistrement.
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
                id: AgentId::from(id),
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
                id: AgentId::from(id),
                state,
                reply: reply_tx,
            })
            .await
            .map_err(|_| AgentRegistryError::ActorDead)?;
        reply_rx.await.map_err(|_| AgentRegistryError::ActorDead)?
    }

    /// Retourne l'AgentId correspondant à un nom de manifest, ou `None`.
    pub async fn find_by_name(&self, name: &str) -> Result<Option<AgentId>, AgentRegistryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RegistryMessage::FindByName {
                name: name.to_string(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| AgentRegistryError::ActorDead)?;
        reply_rx.await.map_err(|_| AgentRegistryError::ActorDead)
    }

    /// Retourne l'entrée d'un agent ou `None` s'il n'est pas enregistré.
    ///
    /// L'absence d'un agent n'est pas une erreur.
    pub async fn get_agent(&self, id: &str) -> Result<Option<AgentEntry>, AgentRegistryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RegistryMessage::GetAgent {
                id: AgentId::from(id),
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

    /// Retourne tous les agents dont `manifest.supports_a2a == true`.
    pub async fn list_a2a_agents(&self) -> Result<Vec<AgentEntry>, AgentRegistryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RegistryMessage::ListA2aAgents { reply: reply_tx })
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
    #[allow(unused_imports)]
    use apollia_core::AgentId;
    use apollia_core::{AgentManifest, AgentSkill, ProcessState};
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
            execution_mode: "auto".to_string(),
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
        }
    }

    fn a2a_manifest(name: &str, skill_ids: &[&str]) -> AgentManifest {
        let skills = skill_ids
            .iter()
            .map(|id| AgentSkill {
                id: id.to_string(),
                name: id.to_string(),
                description: String::new(),
                input_modes: vec!["text".to_string()],
                output_modes: vec!["text".to_string()],
            })
            .collect();
        AgentManifest {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            description: String::new(),
            tools_required: vec![],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: true,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec![],
            skills,
            execution_mode: "auto".to_string(),
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
        }
    }

    // ── Tests ────────────────────────────────────────────────────────────────────

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
        assert!(!id.as_str().is_empty());
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
        let result = handle.update_state(id.as_str(), ProcessState::Active).await;

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
            .update_state(id.as_str(), ProcessState::Stopping)
            .await
            .unwrap();
        handle
            .update_state(id.as_str(), ProcessState::Stopped)
            .await
            .unwrap();

        // WHEN — Stopped → Active est invalide
        let result = handle.update_state(id.as_str(), ProcessState::Active).await;

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
        let result = handle.unregister(id.as_str()).await;

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

    #[tokio::test]
    async fn test_find_by_name_returns_uuid_after_register() {
        // GIVEN un agent enregistre avec manifest.name = "hello-agent"
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        let registered_id = handle.register(test_manifest("hello-agent")).await.unwrap();

        // WHEN registry.find_by_name("hello-agent") est appele
        let result = handle.find_by_name("hello-agent").await;

        // THEN Some(agent_uuid) est retourne
        assert!(result.is_ok());
        let found = result.unwrap();
        assert_eq!(found, Some(registered_id));
    }

    #[tokio::test]
    async fn test_find_by_name_unknown_name_returns_none() {
        // GIVEN aucun agent avec manifest.name = "fantome"
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);

        // WHEN registry.find_by_name("fantome") est appele
        let result = handle.find_by_name("fantome").await;

        // THEN None est retourne
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[tokio::test]
    async fn test_find_by_name_cleared_after_unregister() {
        // GIVEN un agent "hello-agent" enregistre
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        let agent_id = handle.register(test_manifest("hello-agent")).await.unwrap();

        // Verifie que le nom est indexe avant unregister
        let before = handle.find_by_name("hello-agent").await.unwrap();
        assert_eq!(before, Some(agent_id.clone()));

        // WHEN l'agent est retire via unregister
        handle.unregister(agent_id.as_str()).await.unwrap();

        // THEN find_by_name("hello-agent") retourne None
        let after = handle.find_by_name("hello-agent").await.unwrap();
        assert_eq!(after, None);
    }

    #[tokio::test]
    async fn test_list_a2a_agents_filters_correctly() {
        // GIVEN 3 agents : 2 avec supports_a2a, 1 sans
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        handle
            .register(a2a_manifest("excel-worker", &["read-excel"]))
            .await
            .unwrap();
        handle
            .register(a2a_manifest("csv-data-worker", &["read-csv"]))
            .await
            .unwrap();
        handle
            .register(test_manifest("standard-agent"))
            .await
            .unwrap();

        // WHEN list_a2a_agents est appelé
        let a2a_agents = handle.list_a2a_agents().await.unwrap();

        // THEN seuls les 2 agents A2A sont retournés
        assert_eq!(a2a_agents.len(), 2);
        assert!(a2a_agents.iter().all(|e| e.manifest.supports_a2a));
    }

    #[tokio::test]
    async fn test_stop_then_restart_produces_fresh_entry() {
        // Regression: stopping then restarting an A2A agent must produce a fresh
        // AgentId and not leave stale entries in the registry.

        // GIVEN spec-assistant enregistré et actif
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        let id1 = handle
            .register(a2a_manifest(
                "spec-assistant",
                &["create-spec", "refine-spec"],
            ))
            .await
            .unwrap();
        handle
            .update_state(id1.as_str(), ProcessState::Active)
            .await
            .unwrap();

        // WHEN l'agent est arrêté puis redémarré
        handle
            .update_state(id1.as_str(), ProcessState::Stopping)
            .await
            .unwrap();
        handle
            .update_state(id1.as_str(), ProcessState::Stopped)
            .await
            .unwrap();
        let id2 = handle
            .register(a2a_manifest(
                "spec-assistant",
                &["create-spec", "refine-spec"],
            ))
            .await
            .expect("re-registration should succeed");

        // THEN un nouvel id est produit, et list_agents ne contient qu'une entrée
        assert_ne!(id1, id2, "restart must produce a fresh AgentId");
        let agents = handle.list_agents().await.unwrap();
        assert_eq!(agents.len(), 1, "no stale entry should remain");
        assert_eq!(agents[0].id, id2);
    }
}
