use std::collections::HashMap;
use std::time::Instant;

use tokio::sync::{mpsc, oneshot};
use tracing::info;

use apollia_core::{AgentId, AgentManifest, ProcessState, RuntimeEvent};

use crate::eventbus::EventBusSender;

/// Registry entry for a registered agent.
#[derive(Debug, Clone)]
pub struct AgentEntry {
    /// Unique identifier generated at registration (UUID v4).
    pub id: AgentId,
    /// Manifest declared by the agent at registration.
    pub manifest: AgentManifest,
    /// Current state of the agent process.
    pub process_state: ProcessState,
    /// Registration instant, used to compute uptime.
    pub registered_at: Instant,
}

/// Possible errors from registry operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AgentRegistryError {
    /// The requested agent does not exist in the registry.
    #[error("agent '{0}' not found in the registry")]
    NotFound(AgentId),
    /// The requested state transition violates the `ProcessState` state machine.
    #[error("invalid state transition from {from:?} to {to:?}")]
    InvalidTransition {
        from: ProcessState,
        to: ProcessState,
    },
    /// The channel to the actor is closed: the actor has stopped.
    #[error("L'acteur AgentRegistry est mort (canal fermé)")]
    ActorDead,
}

// Internal messages, private enum, never exposed publicly.
// AgentManifest is boxed in Register to avoid an oversized enum variant.
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

/// Internal registry actor, private state, never exposed directly.
///
/// All interaction goes through [`AgentRegistryHandle`].
/// Construction happens only via [`AgentRegistry::spawn`].
pub struct AgentRegistry {
    agents: HashMap<AgentId, AgentEntry>,
    /// Secondary index: manifest.name to AgentId for name lookup.
    name_index: HashMap<String, AgentId>,
    bus: EventBusSender,
}

impl AgentRegistry {
    /// Spawns the actor in a Tokio task and returns its public [`AgentRegistryHandle`].
    ///
    /// The mpsc channel has a capacity of 256 messages. The actor stops
    /// naturally when all handles are dropped (channel closed),
    /// or explicitly upon receiving [`RegistryMessage::Shutdown`].
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

/// Public handle to the `AgentRegistry` actor, clonable, thread-safe.
///
/// Obtained via [`AgentRegistry::spawn`]. Every clone shares the same
/// underlying actor. All methods are async and return
/// [`AgentRegistryError::ActorDead`] if the actor has stopped.
#[derive(Clone)]
pub struct AgentRegistryHandle {
    tx: mpsc::Sender<RegistryMessage>,
}

impl AgentRegistryHandle {
    /// Registers a new agent with its manifest.
    ///
    /// Returns the generated [`AgentId`] (UUID v4).
    /// The agent is created in state [`ProcessState::Initializing`] and
    /// [`RuntimeEvent::AgentRegistered`] is published on the EventBus.
    /// If an agent with the same name already exists (stop/restart cycle), the old
    /// entry is evicted before inserting the new registration.
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

    /// Removes an agent from the registry and publishes [`RuntimeEvent::AgentStopped`].
    ///
    /// Returns [`AgentRegistryError::NotFound`] if the agent does not exist.
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

    /// Updates an agent's [`ProcessState`].
    ///
    /// Returns [`AgentRegistryError::InvalidTransition`] if the transition
    /// is rejected by the `ProcessState` state machine.
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

    /// Returns the AgentId matching a manifest name, or `None`.
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

    /// Returns an agent's entry, or `None` if it is not registered.
    ///
    /// The absence of an agent is not an error.
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

    /// Returns all currently registered agents.
    pub async fn list_agents(&self) -> Result<Vec<AgentEntry>, AgentRegistryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RegistryMessage::ListAgents { reply: reply_tx })
            .await
            .map_err(|_| AgentRegistryError::ActorDead)?;
        reply_rx.await.map_err(|_| AgentRegistryError::ActorDead)
    }

    /// Returns all agents where `manifest.supports_a2a == true`.
    pub async fn list_a2a_agents(&self) -> Result<Vec<AgentEntry>, AgentRegistryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RegistryMessage::ListA2aAgents { reply: reply_tx })
            .await
            .map_err(|_| AgentRegistryError::ActorDead)?;
        reply_rx.await.map_err(|_| AgentRegistryError::ActorDead)
    }

    /// Requests the actor to stop (fire-and-forget).
    ///
    /// Messages already queued are processed before stopping.
    /// If the actor is already dead, the error is silently ignored.
    pub fn shutdown(&self) {
        let _ = self.tx.try_send(RegistryMessage::Shutdown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{AgentManifest, AgentSkill, ProcessState};
    use tokio::sync::broadcast;

    fn test_manifest(name: &str) -> AgentManifest {
        AgentManifest {
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

    fn a2a_manifest(name: &str, skill_ids: &[&str]) -> AgentManifest {
        let skills = skill_ids
            .iter()
            .map(|id| AgentSkill {
                id: id.to_string(),
                name: id.to_string(),
                description: String::new(),
                input_modes: vec!["text".to_string()],
                output_modes: vec!["text".to_string()],
                input_schema: None,
                examples: vec![],
            })
            .collect();
        AgentManifest {
            format_version: 1,
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

    // ── Tests ────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_register_emet_event() {
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
    async fn test_transition_valide() {
        // GIVEN
        let (bus_tx, mut bus_rx) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        let id = handle.register(test_manifest("agent-test")).await.unwrap();
        let _ = bus_rx.recv().await; // consume AgentRegistered

        // WHEN
        let result = handle.update_state(id.as_str(), ProcessState::Active).await;

        // THEN
        assert!(result.is_ok());
        let event = bus_rx.recv().await.unwrap();
        assert!(matches!(event, RuntimeEvent::AgentReady(eid) if eid == id));
    }

    #[tokio::test]
    async fn test_transition_invalide() {
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

        // WHEN, Stopped to Active is invalid
        let result = handle.update_state(id.as_str(), ProcessState::Active).await;

        // THEN
        assert!(matches!(
            result.unwrap_err(),
            AgentRegistryError::InvalidTransition { .. }
        ));
    }

    #[tokio::test]
    async fn test_agent_inexistant() {
        // GIVEN
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);

        // WHEN
        let result = handle.get_agent("inexistant").await;

        // THEN
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_unregister_emet_event() {
        // GIVEN
        let (bus_tx, mut bus_rx) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        let id = handle.register(test_manifest("agent-test")).await.unwrap();
        let _ = bus_rx.recv().await; // consume AgentRegistered

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

        // WHEN, two concurrent calls via two distinct handles
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
        // GIVEN a spawned registry that is asked to shut down
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        handle.shutdown();

        // WHEN registering after shutdown, retried until the actor has torn down
        // (deterministic: poll until the command channel is closed, instead of
        // guessing a fixed sleep). Any Ok means the actor was still draining
        // queued messages; once it drops its receiver, the send fails.
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(5);
        let err = loop {
            match handle.register(test_manifest("agent-test")).await {
                Err(e) => break e,
                Ok(_) => {
                    assert!(
                        tokio::time::Instant::now() < deadline,
                        "registry did not become dead after shutdown"
                    );
                    tokio::task::yield_now().await;
                }
            }
        };

        // THEN the terminal outcome is ActorDead
        assert!(matches!(err, AgentRegistryError::ActorDead));
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
        // GIVEN an agent registered with manifest.name = "hello-agent"
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        let registered_id = handle.register(test_manifest("hello-agent")).await.unwrap();

        // WHEN registry.find_by_name("hello-agent") is called
        let result = handle.find_by_name("hello-agent").await;

        // THEN Some(agent_uuid) is returned
        assert!(result.is_ok());
        let found = result.unwrap();
        assert_eq!(found, Some(registered_id));
    }

    #[tokio::test]
    async fn test_find_by_name_unknown_name_returns_none() {
        // GIVEN no agent with manifest.name = "fantome"
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);

        // WHEN registry.find_by_name("fantome") is called
        let result = handle.find_by_name("fantome").await;

        // THEN None is returned
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[tokio::test]
    async fn test_find_by_name_cleared_after_unregister() {
        // GIVEN a registered agent "hello-agent"
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        let agent_id = handle.register(test_manifest("hello-agent")).await.unwrap();

        // Verify the name is indexed before unregister
        let before = handle.find_by_name("hello-agent").await.unwrap();
        assert_eq!(before, Some(agent_id.clone()));

        // WHEN the agent is removed via unregister
        handle.unregister(agent_id.as_str()).await.unwrap();

        // THEN find_by_name("hello-agent") returns None
        let after = handle.find_by_name("hello-agent").await.unwrap();
        assert_eq!(after, None);
    }

    #[tokio::test]
    async fn test_list_a2a_agents_filters_correctly() {
        // GIVEN 3 agents: 2 with supports_a2a, 1 without
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

        // WHEN list_a2a_agents is called
        let a2a_agents = handle.list_a2a_agents().await.unwrap();

        // THEN only the 2 A2A agents are returned
        assert_eq!(a2a_agents.len(), 2);
        assert!(a2a_agents.iter().all(|e| e.manifest.supports_a2a));
    }

    #[tokio::test]
    async fn test_stop_then_restart_produces_fresh_entry() {
        // Regression: stopping then restarting an A2A agent must produce a fresh
        // AgentId and not leave stale entries in the registry.

        // GIVEN spec-assistant registered and active
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

        // WHEN the agent is stopped then restarted
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

        // THEN a fresh id is produced, and list_agents holds only one entry
        assert_ne!(id1, id2, "restart must produce a fresh AgentId");
        let agents = handle.list_agents().await.unwrap();
        assert_eq!(agents.len(), 1, "no stale entry should remain");
        assert_eq!(agents[0].id, id2);
    }
}
