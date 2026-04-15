use std::collections::HashMap;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use tracing::info;

use apollia_core::{AgentId, AgentManifest, AgentSkill, ProcessState, RuntimeEvent};

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

/// Erreurs de l'index de skills A2A.
///
/// Retournées lors des opérations sur le [`SkillIndex`] intégré à l'[`AgentRegistry`].
#[derive(Debug, thiserror::Error)]
pub enum SkillIndexError {
    /// Un autre agent déclare déjà ce skill_id.
    #[error("skill '{skill_id}' already registered by agent '{existing_agent}'")]
    SkillConflict {
        /// Identifiant du skill en conflit.
        skill_id: String,
        /// Nom de l'agent qui détient déjà ce skill.
        existing_agent: String,
        /// Nom de l'agent tentant de s'enregistrer.
        new_agent: String,
    },
    /// Aucun agent n'est indexé pour ce skill_id.
    #[error("no agent found for skill '{skill_id}'")]
    SkillNotFound {
        /// Identifiant du skill demandé.
        skill_id: String,
    },
}

/// Entrée de l'index de skills, retournée par les API de découverte A2A.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEntry {
    /// Identifiant du skill.
    pub skill_id: String,
    /// Nom de l'agent qui fournit ce skill.
    pub agent_name: String,
}

/// Index inversé `skill_id → (agent_name, AgentId)` pour le routing A2A.
///
/// Composant interne de l'[`AgentRegistry`] — jamais exposé directement.
/// Alimenté automatiquement lors des `register` / `unregister` pour les agents
/// dont `manifest.supports_a2a == true`.
#[derive(Debug, Default)]
struct SkillIndex {
    /// skill_id → (agent_name, agent_id)
    index: HashMap<String, (String, AgentId)>,
}

impl SkillIndex {
    /// Enregistre les skills d'un agent dans l'index.
    ///
    /// Vérifie d'abord tous les conflits (lecture seule), puis insère en masse.
    /// Retourne [`SkillIndexError::SkillConflict`] au premier conflit détecté,
    /// sans modifier l'index.
    fn register_agent(
        &mut self,
        agent_name: &str,
        agent_id: AgentId,
        skills: &[AgentSkill],
    ) -> Result<(), SkillIndexError> {
        // Phase 1 — vérification des conflits sans mutation.
        for skill in skills {
            if let Some((existing_agent, _)) = self.index.get(&skill.id) {
                return Err(SkillIndexError::SkillConflict {
                    skill_id: skill.id.clone(),
                    existing_agent: existing_agent.clone(),
                    new_agent: agent_name.to_string(),
                });
            }
        }
        // Phase 2 — insertion en masse (aucun conflit).
        for skill in skills {
            self.index
                .insert(skill.id.clone(), (agent_name.to_string(), agent_id.clone()));
        }
        Ok(())
    }

    /// Supprime tous les skills d'un agent de l'index.
    fn unregister_agent(&mut self, agent_name: &str) {
        self.index.retain(|_, (name, _)| name != agent_name);
    }

    /// Résout un `skill_id` vers l'`(agent_name, AgentId)` correspondant.
    ///
    /// Retourne [`SkillIndexError::SkillNotFound`] si aucun agent n'est indexé
    /// pour ce skill.
    fn resolve(&self, skill_id: &str) -> Result<(String, AgentId), SkillIndexError> {
        self.index
            .get(skill_id)
            .cloned()
            .ok_or_else(|| SkillIndexError::SkillNotFound {
                skill_id: skill_id.to_string(),
            })
    }

    /// Retourne toutes les entrées de l'index sous forme de [`SkillEntry`].
    fn list_skills(&self) -> Vec<SkillEntry> {
        self.index
            .iter()
            .map(|(skill_id, (agent_name, _))| SkillEntry {
                skill_id: skill_id.clone(),
                agent_name: agent_name.clone(),
            })
            .collect()
    }
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
    /// Erreur de l'index de skills A2A (conflit ou skill introuvable).
    #[error("skill index: {0}")]
    SkillIndex(#[from] SkillIndexError),
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
    ResolveSkill {
        skill_id: String,
        reply: oneshot::Sender<Result<AgentEntry, SkillIndexError>>,
    },
    ListSkills {
        reply: oneshot::Sender<Vec<SkillEntry>>,
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
    /// Index inversé skill_id → (agent_name, AgentId) pour le routing A2A.
    skill_index: SkillIndex,
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
            skill_index: SkillIndex::default(),
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
                RegistryMessage::ResolveSkill { skill_id, reply } => {
                    let result = self.handle_resolve_skill(&skill_id);
                    let _ = reply.send(result);
                }
                RegistryMessage::ListSkills { reply } => {
                    let list = self.skill_index.list_skills();
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
        let id = AgentId::new_v4();

        // Vérifier les conflits de skills avant toute mutation — fail-fast sans rollback.
        if manifest.supports_a2a && !manifest.skills.is_empty() {
            self.skill_index
                .register_agent(&manifest.name, id.clone(), &manifest.skills)?;
        }

        self.name_index.insert(manifest.name.clone(), id.clone());
        let agent_name = manifest.name.clone();
        let supports_a2a = manifest.supports_a2a;
        let skill_ids: Vec<String> = manifest.skills.iter().map(|s| s.id.clone()).collect();

        let entry = AgentEntry {
            id: id.clone(),
            manifest,
            process_state: ProcessState::Initializing,
            registered_at: Instant::now(),
        };
        self.agents.insert(id.clone(), entry);
        let _ = self.bus.send(RuntimeEvent::AgentRegistered(id.clone()));
        info!(agent_id = %id, "Agent enregistré");

        if supports_a2a && !skill_ids.is_empty() {
            info!(agent = %agent_name, skills = ?skill_ids, "a2a skills registered");
        }

        Ok(id)
    }

    fn handle_unregister(&mut self, id: &AgentId) -> Result<(), AgentRegistryError> {
        let entry = self
            .agents
            .remove(id)
            .ok_or_else(|| AgentRegistryError::NotFound(id.clone()))?;
        self.name_index.remove(&entry.manifest.name);
        if entry.manifest.supports_a2a {
            self.skill_index.unregister_agent(&entry.manifest.name);
        }
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

    fn handle_resolve_skill(&self, skill_id: &str) -> Result<AgentEntry, SkillIndexError> {
        let (_, agent_id) = self.skill_index.resolve(skill_id)?;
        self.agents
            .get(&agent_id)
            .cloned()
            .ok_or_else(|| SkillIndexError::SkillNotFound {
                skill_id: skill_id.to_string(),
            })
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
    /// Si `manifest.supports_a2a == true`, les skills déclarés sont indexés.
    /// Retourne [`AgentRegistryError::SkillIndex`] en cas de conflit de skill_id.
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

    /// Résout un `skill_id` vers l'[`AgentEntry`] de l'agent qui le fournit.
    ///
    /// Utilise l'index inversé interne — O(1), sans parcourir tous les agents.
    /// Retourne [`AgentRegistryError::SkillIndex`] si le skill est introuvable.
    pub async fn resolve_skill(&self, skill_id: &str) -> Result<AgentEntry, AgentRegistryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RegistryMessage::ResolveSkill {
                skill_id: skill_id.to_string(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| AgentRegistryError::ActorDead)?;
        reply_rx
            .await
            .map_err(|_| AgentRegistryError::ActorDead)?
            .map_err(AgentRegistryError::SkillIndex)
    }

    /// Retourne toutes les entrées de l'index de skills A2A.
    pub async fn list_skills(&self) -> Result<Vec<SkillEntry>, AgentRegistryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RegistryMessage::ListSkills { reply: reply_tx })
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
        }
    }

    // ── Tests unitaires SkillIndex ──────────────────────────────────────────────

    mod skill_index_tests {
        use super::*;

        #[test]
        fn test_register_agent_populates_index() {
            // GIVEN un SkillIndex vide
            let mut index = SkillIndex::default();
            let id = AgentId::new_v4();
            let skills = vec![
                AgentSkill {
                    id: "read-excel".to_string(),
                    name: "Read Excel".to_string(),
                    description: String::new(),
                    input_modes: vec![],
                    output_modes: vec![],
                },
                AgentSkill {
                    id: "edit-excel".to_string(),
                    name: "Edit Excel".to_string(),
                    description: String::new(),
                    input_modes: vec![],
                    output_modes: vec![],
                },
            ];

            // WHEN register_agent est appelé
            let result = index.register_agent("excel-worker", id.clone(), &skills);

            // THEN l'index est populé
            assert!(result.is_ok());
            let (name, resolved_id) = index.resolve("read-excel").unwrap();
            assert_eq!(name, "excel-worker");
            assert_eq!(resolved_id, id);
            let (name2, _) = index.resolve("edit-excel").unwrap();
            assert_eq!(name2, "excel-worker");
        }

        #[test]
        fn test_skill_conflict_returns_error() {
            // GIVEN "excel-worker" enregistré avec "read-excel"
            let mut index = SkillIndex::default();
            let id1 = AgentId::new_v4();
            let id2 = AgentId::new_v4();
            let skill = |id: &str| AgentSkill {
                id: id.to_string(),
                name: id.to_string(),
                description: String::new(),
                input_modes: vec![],
                output_modes: vec![],
            };
            index
                .register_agent("excel-worker", id1, &[skill("read-excel")])
                .unwrap();

            // WHEN "other-agent" tente de s'enregistrer avec le même skill
            let result = index.register_agent("other-agent", id2, &[skill("read-excel")]);

            // THEN SkillConflict est retourné
            assert!(result.is_err());
            match result.unwrap_err() {
                SkillIndexError::SkillConflict {
                    skill_id,
                    existing_agent,
                    new_agent,
                } => {
                    assert_eq!(skill_id, "read-excel");
                    assert_eq!(existing_agent, "excel-worker");
                    assert_eq!(new_agent, "other-agent");
                }
                other => panic!("unexpected error: {other}"),
            }
        }

        #[test]
        fn test_unregister_cleans_all_skills() {
            // GIVEN "excel-worker" avec 3 skills
            let mut index = SkillIndex::default();
            let id = AgentId::new_v4();
            let skill = |s: &str| AgentSkill {
                id: s.to_string(),
                name: s.to_string(),
                description: String::new(),
                input_modes: vec![],
                output_modes: vec![],
            };
            index
                .register_agent(
                    "excel-worker",
                    id,
                    &[
                        skill("read-excel"),
                        skill("edit-excel"),
                        skill("analyze-excel"),
                    ],
                )
                .unwrap();

            // WHEN unregister_agent est appelé
            index.unregister_agent("excel-worker");

            // THEN tous les skills sont supprimés
            assert!(index.resolve("read-excel").is_err());
            assert!(index.resolve("edit-excel").is_err());
            assert!(index.resolve("analyze-excel").is_err());
            assert!(index.list_skills().is_empty());
        }

        #[test]
        fn test_resolve_unknown_skill_returns_not_found() {
            // GIVEN SkillIndex vide
            let index = SkillIndex::default();

            // WHEN resolve est appelé sur un skill inexistant
            let result = index.resolve("unknown-skill");

            // THEN SkillNotFound est retourné
            assert!(matches!(
                result.unwrap_err(),
                SkillIndexError::SkillNotFound { skill_id } if skill_id == "unknown-skill"
            ));
        }

        #[test]
        fn test_list_skills_returns_all_entries() {
            // GIVEN 2 agents avec 3 skills chacun
            let mut index = SkillIndex::default();
            let skill = |s: &str| AgentSkill {
                id: s.to_string(),
                name: s.to_string(),
                description: String::new(),
                input_modes: vec![],
                output_modes: vec![],
            };
            index
                .register_agent(
                    "excel-worker",
                    AgentId::new_v4(),
                    &[
                        skill("read-excel"),
                        skill("edit-excel"),
                        skill("analyze-excel"),
                    ],
                )
                .unwrap();
            index
                .register_agent(
                    "csv-data-worker",
                    AgentId::new_v4(),
                    &[skill("read-csv"), skill("edit-csv"), skill("analyze-csv")],
                )
                .unwrap();

            // WHEN list_skills est appelé
            let entries = index.list_skills();

            // THEN 6 entrées sont retournées
            assert_eq!(entries.len(), 6);
        }
    }

    // ── Tests existants (non-régression) ───────────────────────────────────────

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

    // ── Tests skill index intégration (via handle) ─────────────────────────────

    #[tokio::test]
    async fn test_register_a2a_agent_populates_skill_index() {
        // GIVEN un registry spawné et un manifest A2A avec 2 skills
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);

        // WHEN excel-worker est enregistré avec supports_a2a = true
        let id = handle
            .register(a2a_manifest(
                "excel-worker",
                &["read-excel", "analyze-excel"],
            ))
            .await
            .unwrap();

        // THEN resolve_skill retourne l'entrée correcte pour chaque skill
        let entry = handle.resolve_skill("read-excel").await.unwrap();
        assert_eq!(entry.manifest.name, "excel-worker");
        assert_eq!(entry.id, id);

        let entry2 = handle.resolve_skill("analyze-excel").await.unwrap();
        assert_eq!(entry2.manifest.name, "excel-worker");

        let skills = handle.list_skills().await.unwrap();
        assert_eq!(skills.len(), 2);
    }

    #[tokio::test]
    async fn test_skill_conflict_rejects_second_agent() {
        // GIVEN excel-worker enregistré avec "read-excel"
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        handle
            .register(a2a_manifest("excel-worker", &["read-excel"]))
            .await
            .unwrap();

        // WHEN other-agent tente de s'enregistrer avec le même skill
        let result = handle
            .register(a2a_manifest("other-agent", &["read-excel"]))
            .await;

        // THEN l'enregistrement est rejeté et other-agent n'est pas dans le registry
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            AgentRegistryError::SkillIndex(SkillIndexError::SkillConflict { .. })
        ));
        let agents = handle.list_agents().await.unwrap();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].manifest.name, "excel-worker");
    }

    #[tokio::test]
    async fn test_unregister_clears_skill_index() {
        // GIVEN excel-worker enregistré avec 3 skills
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        let id = handle
            .register(a2a_manifest(
                "excel-worker",
                &["read-excel", "edit-excel", "analyze-excel"],
            ))
            .await
            .unwrap();

        // WHEN unregister est appelé
        handle.unregister(id.as_str()).await.unwrap();

        // THEN tous les skills sont retirés de l'index
        assert!(handle.resolve_skill("read-excel").await.is_err());
        assert!(handle.resolve_skill("edit-excel").await.is_err());
        assert!(handle.resolve_skill("analyze-excel").await.is_err());
        let skills = handle.list_skills().await.unwrap();
        assert!(skills.is_empty());
    }

    #[tokio::test]
    async fn test_non_a2a_agent_does_not_populate_skill_index() {
        // GIVEN un agent avec supports_a2a = false
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        handle
            .register(test_manifest("standard-agent"))
            .await
            .unwrap();

        // THEN l'index reste vide et l'agent est bien enregistré
        let skills = handle.list_skills().await.unwrap();
        assert!(skills.is_empty());
        let agents = handle.list_agents().await.unwrap();
        assert_eq!(agents.len(), 1);
    }

    #[tokio::test]
    async fn test_resolve_skill_unknown_returns_error() {
        // GIVEN un registry vide
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);

        // WHEN resolve_skill est appelé sur un skill inexistant
        let result = handle.resolve_skill("unknown-skill").await;

        // THEN une erreur SkillIndex(SkillNotFound) est retournée
        assert!(matches!(
            result.unwrap_err(),
            AgentRegistryError::SkillIndex(SkillIndexError::SkillNotFound { .. })
        ));
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
    async fn test_list_skills_aggregates_multiple_agents() {
        // GIVEN excel-worker (3 skills) et csv-data-worker (3 skills)
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        handle
            .register(a2a_manifest(
                "excel-worker",
                &["read-excel", "edit-excel", "analyze-excel"],
            ))
            .await
            .unwrap();
        handle
            .register(a2a_manifest(
                "csv-data-worker",
                &["read-csv", "edit-csv", "analyze-csv"],
            ))
            .await
            .unwrap();
        handle
            .register(test_manifest("standard-agent"))
            .await
            .unwrap();

        // WHEN list_skills est appelé
        let skills = handle.list_skills().await.unwrap();

        // THEN 6 entrées (l'agent standard n'alimente pas l'index)
        assert_eq!(skills.len(), 6);
    }
}
