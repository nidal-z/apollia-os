//! A2AInvoker — orchestrateur de haut niveau pour les invocations inter-agents par skill ID.
//!
//! Gère le cycle complet d'une invocation A2A :
//! résolution du skill (état `Active` requis), émission des événements runtime,
//! délégation au TaskRouter avec timeout, et construction du résultat structuré.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::info;

use apollia_core::{AIPResult, ProcessState, RuntimeEvent};

use crate::a2a::{make_delegate_fn, A2aDelegateFn};
use crate::coordinator::ExecutionBackend;
use crate::eventbus::EventBusSender;
use crate::registry::{AgentEntry, AgentRegistryHandle};
use crate::router::TaskRouterHandle;

/// Timeout par défaut des invocations A2A (120 secondes).
const DEFAULT_A2A_TIMEOUT: Duration = Duration::from_secs(120);

/// Configuration de contexte d'exécution pour un agent invoqué via A2A.
///
/// Produite par [`A2AInvoker::build_a2a_context`] et consommée par le runtime
/// lors de la construction du [`RuntimeContext`] PyO3 pour la tâche déléguée.
///
/// Encode le trust model A2A : l'agent invoqué lit la mémoire utilisateur globale
/// en lecture seule mais écrit exclusivement dans son propre namespace.
#[derive(Debug, Clone)]
pub struct RuntimeContextConfig {
    /// Si `true`, la mémoire utilisateur globale est accessible en lecture via
    /// `ctx.memory.recall()`. Les écritures restent confinées au namespace de l'agent.
    pub user_memory_read_only: bool,
}

/// Erreurs structurées retournées par [`A2AInvoker`].
///
/// Surface d'erreur orientée métier, distincte des erreurs de bas niveau
/// [`crate::a2a::A2aError`] qui couvrent la couche de délégation.
#[derive(Debug, thiserror::Error)]
pub enum A2AError {
    /// Aucun agent A2A disponible ne déclare le skill demandé.
    #[error("skill '{skill_id}' not found — available: {available:?}")]
    SkillNotFound {
        /// Identifiant du skill demandé.
        skill_id: String,
        /// Liste des skill IDs disponibles dans les agents A2A actifs ou dégradés.
        available: Vec<String>,
    },

    /// Un agent déclare le skill mais n'est pas en état `Active`.
    ///
    /// Seul l'état `Active` est accepté pour l'invocation (fail-fast, Principe #4).
    #[error("agent '{agent_name}' is not active (state: {state})")]
    AgentNotActive {
        /// Nom de l'agent cible.
        agent_name: String,
        /// État actuel de l'agent (ex: `"Degraded"`, `"Stopping"`).
        state: String,
    },

    /// L'invocation A2A a expiré avant que le Worker Agent ne réponde.
    #[error(
        "A2A invocation timed out after {timeout_secs}s (skill: {skill_id}, agent: {agent_name})"
    )]
    Timeout {
        /// Identifiant du skill invoqué.
        skill_id: String,
        /// Nom de l'agent cible.
        agent_name: String,
        /// Timeout configuré en secondes.
        timeout_secs: u64,
    },

    /// Le Worker Agent a retourné un résultat d'échec.
    #[error("agent '{agent_name}' execution failed: {message}")]
    ExecutionFailed {
        /// Nom de l'agent cible.
        agent_name: String,
        /// Raison de l'échec.
        message: String,
    },

    /// Erreur de communication avec le registry ou le router.
    #[error("A2A infrastructure error: {0}")]
    RegistryError(String),
}

/// Résultat d'une invocation A2A réussie.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2AInvocationResult {
    /// Résultat AIP retourné par le Worker Agent.
    pub result: AIPResult,
    /// Nom du Worker Agent qui a traité l'invocation.
    pub agent_name: String,
    /// Identifiant du skill invoqué.
    pub skill_id: String,
    /// Durée totale de l'invocation en millisecondes.
    pub duration_ms: u64,
}

/// Informations de découverte d'un skill A2A.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2ASkillInfo {
    /// Identifiant unique du skill (ex: `"read-excel"`).
    pub id: String,
    /// Nom humain du skill.
    pub name: String,
    /// Description de ce que fait le skill.
    pub description: String,
    /// Modes d'entrée supportés (ex: `["text", "data"]`).
    pub input_modes: Vec<String>,
    /// Modes de sortie supportés (ex: `["text", "file"]`).
    pub output_modes: Vec<String>,
}

/// Carte de découverte d'un agent A2A.
///
/// Retournée par [`A2AInvoker::discover`] et [`A2AInvoker::list_agent_cards`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2AAgentCard {
    /// Nom unique de l'agent.
    pub name: String,
    /// Version semver de l'agent.
    pub version: String,
    /// Description de l'agent.
    pub description: String,
    /// Skills déclarés par cet agent.
    pub skills: Vec<A2ASkillInfo>,
    /// Tags associés à cet agent.
    pub tags: Vec<String>,
}

/// Entrée dans la liste des skills disponibles.
///
/// Retournée par [`A2AInvoker::list_skills`] et utilisée par `ctx.a2a_list_skills()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillListing {
    /// Identifiant du skill.
    pub skill_id: String,
    /// Nom de l'agent qui fournit ce skill.
    pub agent_name: String,
    /// Nom humain du skill.
    pub skill_name: String,
    /// Description du skill.
    pub description: String,
}

/// Orchestrateur de haut niveau pour les invocations inter-agents par skill ID.
///
/// Orchestre le cycle complet d'une invocation A2A :
/// 1. Résolution du `skill_id` → agent (état `Active` requis, Principe #4)
/// 2. Émission de [`RuntimeEvent::A2AInvocationStarted`]
/// 3. Délégation via le TaskRouter avec timeout appliqué par le runtime (Principe #7)
/// 4. Émission de [`RuntimeEvent::A2AInvocationCompleted`]
/// 5. Construction du [`A2AInvocationResult`]
///
/// N'est pas un acteur Tokio — struct clonable avec des handles internes.
#[derive(Clone)]
pub struct A2AInvoker {
    registry: AgentRegistryHandle,
    delegate_fn: A2aDelegateFn,
    event_bus: EventBusSender,
}

impl A2AInvoker {
    /// Construit un `A2AInvoker` depuis les handles runtime.
    ///
    /// Générique sur `B: ExecutionBackend` — le résultat est non-générique grâce
    /// à l'erasure de type opérée par [`make_delegate_fn`].
    pub fn new<B>(
        registry: AgentRegistryHandle,
        router: TaskRouterHandle<B>,
        event_bus: EventBusSender,
    ) -> Self
    where
        B: ExecutionBackend + Clone + Send + Sync + 'static,
    {
        let delegate_fn = make_delegate_fn(registry.clone(), router, event_bus.clone());
        Self {
            registry,
            delegate_fn,
            event_bus,
        }
    }

    /// Invoque un Worker Agent par son `skill_id`.
    ///
    /// Résout le skill, valide que l'agent cible est en état `Active`,
    /// délègue l'exécution via le TaskRouter avec un timeout optionnel,
    /// et retourne un [`A2AInvocationResult`] enrichi.
    ///
    /// Émet [`RuntimeEvent::A2AInvocationStarted`] avant la délégation
    /// et [`RuntimeEvent::A2AInvocationCompleted`] après (succès ou échec inclus).
    ///
    /// # Errors
    ///
    /// - [`A2AError::SkillNotFound`] si aucun agent A2A disponible ne déclare le skill.
    /// - [`A2AError::AgentNotActive`] si l'agent cible n'est pas en état `Active`.
    /// - [`A2AError::Timeout`] si la durée d'exécution dépasse `timeout`.
    /// - [`A2AError::ExecutionFailed`] si le Worker Agent retourne un échec.
    /// - [`A2AError::RegistryError`] en cas d'erreur de communication avec le registry.
    pub async fn invoke(
        &self,
        skill_id: &str,
        input: serde_json::Value,
        caller: &str,
        timeout: Option<Duration>,
    ) -> Result<A2AInvocationResult, A2AError> {
        let timeout_dur = timeout.unwrap_or(DEFAULT_A2A_TIMEOUT);
        let timeout_secs = timeout_dur.as_secs();

        // 1. Récupérer tous les agents et construire le pool A2A disponible.
        let entries = self
            .registry
            .list_agents()
            .await
            .map_err(|e| A2AError::RegistryError(e.to_string()))?;

        let pool: Vec<&AgentEntry> = entries
            .iter()
            .filter(|e| {
                e.manifest.supports_a2a
                    && matches!(
                        e.process_state,
                        ProcessState::Active | ProcessState::Degraded
                    )
            })
            .collect();

        // 2. Trouver les agents du pool déclarant le skill demandé.
        let matching: Vec<&&AgentEntry> = pool
            .iter()
            .filter(|e| e.manifest.skills.iter().any(|s| s.id == skill_id))
            .collect();

        if matching.is_empty() {
            let mut available: Vec<String> = pool
                .iter()
                .flat_map(|e| e.manifest.skills.iter().map(|s| s.id.clone()))
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            available.sort();
            return Err(A2AError::SkillNotFound {
                skill_id: skill_id.to_string(),
                available,
            });
        }

        // 3. Parmi les candidats, exiger l'état Active (fail-fast, Principe #4).
        let target = matching
            .iter()
            .find(|e| e.process_state == ProcessState::Active)
            .ok_or_else(|| A2AError::AgentNotActive {
                agent_name: matching[0].manifest.name.clone(),
                state: format!("{:?}", matching[0].process_state),
            })?;

        let agent_name = target.manifest.name.clone();

        info!(
            skill_id = %skill_id,
            agent = %agent_name,
            caller = %caller,
            "A2A invocation starting"
        );

        // 4. Émettre A2AInvocationStarted (fire-and-forget).
        let _ = self.event_bus.send(RuntimeEvent::A2AInvocationStarted {
            caller: caller.to_string(),
            target: agent_name.clone(),
            skill_id: skill_id.to_string(),
        });

        let start = Instant::now();

        // 5. Déléguer via la fn type-erasée (submit → wait → timeout interne).
        let delegate_result = (self.delegate_fn)(skill_id.to_string(), input, timeout_secs).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        // 6. Émettre A2AInvocationCompleted (fire-and-forget).
        let status = if delegate_result.is_ok() {
            "completed"
        } else {
            "failed"
        };
        let _ = self.event_bus.send(RuntimeEvent::A2AInvocationCompleted {
            caller: caller.to_string(),
            target: agent_name.clone(),
            skill_id: skill_id.to_string(),
            status: status.to_string(),
            duration_ms,
        });

        // 7. Convertir le résultat de délégation.
        let delegate = delegate_result
            .map_err(|e| map_delegate_err(e, skill_id, &agent_name, timeout_secs))?;

        let aip_result = AIPResult::completed(&delegate.output);

        Ok(A2AInvocationResult {
            result: aip_result,
            agent_name: delegate.agent_name,
            skill_id: skill_id.to_string(),
            duration_ms,
        })
    }

    /// Découvre l'agent qui expose `skill_id` et retourne sa carte de découverte.
    ///
    /// Cherche dans les agents `supports_a2a = true` en état `Active` ou `Degraded`.
    /// Retourne `None` si aucun agent disponible ne déclare ce skill.
    pub async fn discover(&self, skill_id: &str) -> Result<Option<A2AAgentCard>, A2AError> {
        let entries = self
            .registry
            .list_agents()
            .await
            .map_err(|e| A2AError::RegistryError(e.to_string()))?;

        let card = entries
            .iter()
            .filter(|e| {
                e.manifest.supports_a2a
                    && matches!(
                        e.process_state,
                        ProcessState::Active | ProcessState::Degraded
                    )
                    && e.manifest.skills.iter().any(|s| s.id == skill_id)
            })
            .map(to_agent_card)
            .next();

        Ok(card)
    }

    /// Liste toutes les cartes de découverte des agents A2A disponibles.
    ///
    /// Inclut les agents en état `Active` ou `Degraded` avec `supports_a2a = true`.
    /// La liste est triée par nom d'agent.
    pub async fn list_agent_cards(&self) -> Result<Vec<A2AAgentCard>, A2AError> {
        let entries = self
            .registry
            .list_agents()
            .await
            .map_err(|e| A2AError::RegistryError(e.to_string()))?;

        let mut cards: Vec<A2AAgentCard> = entries
            .iter()
            .filter(|e| {
                e.manifest.supports_a2a
                    && matches!(
                        e.process_state,
                        ProcessState::Active | ProcessState::Degraded
                    )
            })
            .map(to_agent_card)
            .collect();

        cards.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(cards)
    }

    /// Liste tous les skills disponibles, toutes cartes A2A confondues.
    ///
    /// Retourne une liste plate de [`SkillListing`], triée par `skill_id`.
    pub async fn list_skills(&self) -> Result<Vec<SkillListing>, A2AError> {
        let cards = self.list_agent_cards().await?;

        let mut skills: Vec<SkillListing> = cards
            .iter()
            .flat_map(|card| {
                card.skills.iter().map(|s| SkillListing {
                    skill_id: s.id.clone(),
                    agent_name: card.name.clone(),
                    skill_name: s.name.clone(),
                    description: s.description.clone(),
                })
            })
            .collect();

        skills.sort_by(|a, b| a.skill_id.cmp(&b.skill_id));
        Ok(skills)
    }

    /// Construit la configuration de contexte d'exécution pour un agent invoqué via A2A.
    ///
    /// Retourne une [`RuntimeContextConfig`] avec `user_memory_read_only = true`,
    /// appliquant le trust model A2A : l'agent peut lire la mémoire utilisateur
    /// globale mais écrit uniquement dans son propre namespace.
    pub fn build_a2a_context(&self) -> RuntimeContextConfig {
        RuntimeContextConfig {
            user_memory_read_only: true,
        }
    }

    /// Constructeur de test — injecte une `A2aDelegateFn` personnalisée.
    #[cfg(test)]
    pub(crate) fn new_for_test(
        registry: AgentRegistryHandle,
        delegate_fn: A2aDelegateFn,
        event_bus: EventBusSender,
    ) -> Self {
        Self {
            registry,
            delegate_fn,
            event_bus,
        }
    }
}

/// Convertit une [`AgentEntry`] en [`A2AAgentCard`].
fn to_agent_card(entry: &AgentEntry) -> A2AAgentCard {
    let skills = entry
        .manifest
        .skills
        .iter()
        .map(|s| A2ASkillInfo {
            id: s.id.clone(),
            name: s.name.clone(),
            description: s.description.clone(),
            input_modes: s.input_modes.clone(),
            output_modes: s.output_modes.clone(),
        })
        .collect();

    A2AAgentCard {
        name: entry.manifest.name.clone(),
        version: entry.manifest.version.clone(),
        description: entry.manifest.description.clone(),
        skills,
        tags: entry.manifest.tags.clone(),
    }
}

/// Mappe une [`crate::a2a::A2aError`] vers une [`A2AError`] de haut niveau.
fn map_delegate_err(
    err: crate::a2a::A2aError,
    skill_id: &str,
    agent_name: &str,
    timeout_secs: u64,
) -> A2AError {
    match err {
        crate::a2a::A2aError::Timeout { .. } => A2AError::Timeout {
            skill_id: skill_id.to_string(),
            agent_name: agent_name.to_string(),
            timeout_secs,
        },
        crate::a2a::A2aError::WorkerFailed { reason } => A2AError::ExecutionFailed {
            agent_name: agent_name.to_string(),
            message: reason,
        },
        other => A2AError::RegistryError(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::a2a::A2aError as LowLevelA2aError;
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use apollia_core::{AgentId, AgentManifest, AgentSkill};
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::time::Instant;

    fn make_a2a_manifest(name: &str, skill_ids: &[&str]) -> AgentManifest {
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
            description: name.to_string(),
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
            tags: vec!["worker".to_string()],
            skills,
            execution_mode: "direct".to_string(),
            system_prompt: None,
            tools_requiring_approval: vec![],
            llm_backend: None,
            packages: vec![],
        }
    }

    fn make_never_called_delegate() -> A2aDelegateFn {
        Arc::new(
            |_skill_id: String, _input: serde_json::Value, _timeout: u64| {
                let fut: Pin<
                    Box<
                        dyn Future<Output = Result<crate::a2a::A2aDelegateResult, LowLevelA2aError>>
                            + Send,
                    >,
                > = Box::pin(async { Err(LowLevelA2aError::RouterDead) });
                fut
            },
        )
    }

    fn make_ok_delegate(output: &str) -> A2aDelegateFn {
        let output = output.to_string();
        Arc::new(
            move |_skill_id: String, _input: serde_json::Value, _timeout: u64| {
                let out = output.clone();
                let fut: Pin<
                    Box<
                        dyn Future<Output = Result<crate::a2a::A2aDelegateResult, LowLevelA2aError>>
                            + Send,
                    >,
                > = Box::pin(async move {
                    Ok(crate::a2a::A2aDelegateResult {
                        task_id: "task-test".to_string(),
                        agent_name: "excel-worker".to_string(),
                        output: out,
                    })
                });
                fut
            },
        )
    }

    fn make_timeout_delegate() -> A2aDelegateFn {
        Arc::new(
            |_skill_id: String, _input: serde_json::Value, _timeout: u64| {
                let fut: Pin<
                    Box<
                        dyn Future<Output = Result<crate::a2a::A2aDelegateResult, LowLevelA2aError>>
                            + Send,
                    >,
                > = Box::pin(async { Err(LowLevelA2aError::Timeout { timeout_secs: 1 }) });
                fut
            },
        )
    }

    // ── Pure function tests ────────────────────────────────────────────────────

    #[test]
    fn test_a2a_error_skill_not_found_message() {
        // GIVEN
        let err = A2AError::SkillNotFound {
            skill_id: "unknown".to_string(),
            available: vec!["read-excel".to_string(), "read-csv".to_string()],
        };
        // WHEN
        let msg = err.to_string();
        // THEN message contains skill_id and available list
        assert!(msg.contains("unknown"), "message: {msg}");
        assert!(msg.contains("read-excel"), "message: {msg}");
    }

    #[test]
    fn test_a2a_error_agent_not_active_message() {
        // GIVEN
        let err = A2AError::AgentNotActive {
            agent_name: "excel-worker".to_string(),
            state: "Degraded".to_string(),
        };
        // WHEN
        let msg = err.to_string();
        // THEN message contains agent name and state
        assert!(msg.contains("excel-worker"), "message: {msg}");
        assert!(msg.contains("Degraded"), "message: {msg}");
    }

    #[test]
    fn test_a2a_error_timeout_message() {
        // GIVEN
        let err = A2AError::Timeout {
            skill_id: "read-excel".to_string(),
            agent_name: "excel-worker".to_string(),
            timeout_secs: 30,
        };
        // WHEN / THEN timeout_secs appears in message
        assert!(err.to_string().contains("30"));
    }

    #[test]
    fn test_a2a_invocation_result_serializable() {
        // GIVEN
        let result = A2AInvocationResult {
            result: AIPResult::completed("data processed"),
            agent_name: "excel-worker".to_string(),
            skill_id: "read-excel".to_string(),
            duration_ms: 450,
        };
        // WHEN
        let json = serde_json::to_string(&result).expect("serialization failed");
        // THEN JSON round-trips correctly
        assert!(json.contains("excel-worker"));
        assert!(json.contains("read-excel"));
        assert!(json.contains("450"));
        let _: A2AInvocationResult = serde_json::from_str(&json).expect("deserialization failed");
    }

    #[test]
    fn test_skill_listing_serializable() {
        // GIVEN
        let listing = SkillListing {
            skill_id: "read-excel".to_string(),
            agent_name: "excel-worker".to_string(),
            skill_name: "Read Excel".to_string(),
            description: "Reads an Excel file".to_string(),
        };
        // WHEN / THEN serializes correctly
        let json = serde_json::to_string(&listing).expect("serialization failed");
        assert!(json.contains("read-excel"));
        assert!(json.contains("excel-worker"));
    }

    #[test]
    fn test_to_agent_card_maps_entry_correctly() {
        // GIVEN an AgentEntry with 2 skills
        let entry = crate::registry::AgentEntry {
            id: AgentId::new_v4(),
            manifest: make_a2a_manifest("excel-worker", &["read-excel", "edit-excel"]),
            process_state: ProcessState::Active,
            registered_at: Instant::now(),
        };
        // WHEN
        let card = to_agent_card(&entry);
        // THEN all fields are mapped
        assert_eq!(card.name, "excel-worker");
        assert_eq!(card.skills.len(), 2);
        assert!(card.skills.iter().any(|s| s.id == "read-excel"));
        assert!(card.skills.iter().any(|s| s.id == "edit-excel"));
        assert_eq!(card.tags, vec!["worker"]);
    }

    #[test]
    fn test_map_delegate_err_timeout_maps_correctly() {
        // GIVEN
        let low = LowLevelA2aError::Timeout { timeout_secs: 5 };
        // WHEN
        let mapped = map_delegate_err(low, "read-excel", "excel-worker", 5);
        // THEN
        assert!(matches!(
            mapped,
            A2AError::Timeout {
                timeout_secs: 5,
                ..
            }
        ));
    }

    #[test]
    fn test_map_delegate_err_worker_failed_maps_correctly() {
        // GIVEN
        let low = LowLevelA2aError::WorkerFailed {
            reason: "out of memory".to_string(),
        };
        // WHEN
        let mapped = map_delegate_err(low, "read-excel", "excel-worker", 120);
        // THEN
        match mapped {
            A2AError::ExecutionFailed { message, .. } => {
                assert!(message.contains("out of memory"), "message: {message}");
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn test_a2a_agent_card_round_trip() {
        // GIVEN
        let card = A2AAgentCard {
            name: "excel-worker".to_string(),
            version: "1.0.0".to_string(),
            description: "Handles Excel files".to_string(),
            skills: vec![A2ASkillInfo {
                id: "read-excel".to_string(),
                name: "Read Excel".to_string(),
                description: "Reads Excel data".to_string(),
                input_modes: vec!["text".to_string()],
                output_modes: vec!["data".to_string()],
            }],
            tags: vec!["excel".to_string()],
        };
        // WHEN
        let json = serde_json::to_string(&card).expect("serialization failed");
        let restored: A2AAgentCard = serde_json::from_str(&json).expect("deserialization failed");
        // THEN
        assert_eq!(restored.name, "excel-worker");
        assert_eq!(restored.skills.len(), 1);
        assert_eq!(restored.skills[0].id, "read-excel");
    }

    // ── Registry-based async tests ────────────────────────────────────────────

    #[tokio::test]
    async fn test_invoke_unknown_skill_returns_skill_not_found_with_available() {
        // GIVEN excel-worker Active avec "read-excel", invoke pour "unknown-skill"
        let (bus_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(bus_tx.clone());
        let agent_id = registry
            .register(make_a2a_manifest(
                "excel-worker",
                &["read-excel", "edit-excel"],
            ))
            .await
            .expect("register failed");
        registry
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .expect("update state failed");

        let invoker = A2AInvoker::new_for_test(registry, make_never_called_delegate(), bus_tx);

        // WHEN
        let result = invoker
            .invoke("unknown-skill", serde_json::json!({}), "director", None)
            .await;

        // THEN Err(SkillNotFound) avec available contenant "read-excel" et "edit-excel"
        match result.expect_err("expected error") {
            A2AError::SkillNotFound {
                skill_id,
                available,
            } => {
                assert_eq!(skill_id, "unknown-skill");
                assert!(
                    available.contains(&"read-excel".to_string()),
                    "available: {available:?}"
                );
                assert!(
                    available.contains(&"edit-excel".to_string()),
                    "available: {available:?}"
                );
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[tokio::test]
    async fn test_invoke_degraded_agent_returns_not_active() {
        // GIVEN excel-worker Degraded avec "read-excel"
        let (bus_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(bus_tx.clone());
        let agent_id = registry
            .register(make_a2a_manifest("excel-worker", &["read-excel"]))
            .await
            .expect("register failed");
        registry
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .expect("active transition failed");
        registry
            .update_state(agent_id.as_str(), ProcessState::Degraded)
            .await
            .expect("degraded transition failed");

        let invoker = A2AInvoker::new_for_test(registry, make_never_called_delegate(), bus_tx);

        // WHEN
        let result = invoker
            .invoke("read-excel", serde_json::json!({}), "director", None)
            .await;

        // THEN Err(AgentNotActive) avec state == "Degraded"
        match result.expect_err("expected error") {
            A2AError::AgentNotActive { agent_name, state } => {
                assert_eq!(agent_name, "excel-worker");
                assert!(state.contains("Degraded"), "state: {state}");
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[tokio::test]
    async fn test_invoke_active_agent_succeeds_and_emits_events() {
        // GIVEN excel-worker Active, delegate retourne Ok
        let (bus_tx, mut bus_rx) = EventBus::new();
        let registry = AgentRegistry::spawn(bus_tx.clone());
        let agent_id = registry
            .register(make_a2a_manifest("excel-worker", &["read-excel"]))
            .await
            .expect("register failed");
        registry
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .expect("update state failed");

        let invoker =
            A2AInvoker::new_for_test(registry, make_ok_delegate("colonnes: A, B, C"), bus_tx);

        // WHEN
        let result = invoker
            .invoke(
                "read-excel",
                serde_json::json!({"text": "Lis ventes.xlsx"}),
                "director",
                None,
            )
            .await
            .expect("invoke failed");

        // THEN résultat correct
        assert_eq!(result.skill_id, "read-excel");
        assert_eq!(result.agent_name, "excel-worker");
        assert!(
            result.duration_ms < 5000,
            "duration: {}ms",
            result.duration_ms
        );
        assert_eq!(result.result.status, apollia_core::TaskStatus::Completed);

        // THEN A2AInvocationStarted émis
        let mut found_started = false;
        let mut found_completed = false;
        loop {
            match bus_rx.try_recv() {
                Ok(RuntimeEvent::A2AInvocationStarted { skill_id, .. }) => {
                    assert_eq!(skill_id, "read-excel");
                    found_started = true;
                }
                Ok(RuntimeEvent::A2AInvocationCompleted {
                    skill_id, status, ..
                }) => {
                    assert_eq!(skill_id, "read-excel");
                    assert_eq!(status, "completed");
                    found_completed = true;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        assert!(found_started, "A2AInvocationStarted not emitted");
        assert!(found_completed, "A2AInvocationCompleted not emitted");
    }

    #[tokio::test]
    async fn test_invoke_timeout_returns_a2a_timeout_error() {
        // GIVEN excel-worker Active, delegate retourne Timeout
        let (bus_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(bus_tx.clone());
        let agent_id = registry
            .register(make_a2a_manifest("excel-worker", &["read-excel"]))
            .await
            .expect("register failed");
        registry
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .expect("update state failed");

        let invoker = A2AInvoker::new_for_test(registry, make_timeout_delegate(), bus_tx);

        // WHEN
        let result = invoker
            .invoke(
                "read-excel",
                serde_json::json!({}),
                "director",
                Some(Duration::from_secs(1)),
            )
            .await;

        // THEN Err(Timeout)
        assert!(
            matches!(result, Err(A2AError::Timeout { .. })),
            "expected Timeout, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_discover_returns_agent_card() {
        // GIVEN excel-worker enregistré et Active
        let (bus_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(bus_tx.clone());
        let agent_id = registry
            .register(make_a2a_manifest(
                "excel-worker",
                &["read-excel", "edit-excel"],
            ))
            .await
            .expect("register failed");
        registry
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .expect("update state failed");

        let invoker = A2AInvoker::new_for_test(registry, make_never_called_delegate(), bus_tx);

        // WHEN
        let card = invoker
            .discover("read-excel")
            .await
            .expect("discover failed")
            .expect("expected Some(card)");

        // THEN carte correcte
        assert_eq!(card.name, "excel-worker");
        assert_eq!(card.skills.len(), 2);
    }

    #[tokio::test]
    async fn test_discover_unknown_skill_returns_none() {
        // GIVEN registry vide
        let (bus_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(bus_tx.clone());
        let invoker = A2AInvoker::new_for_test(registry, make_never_called_delegate(), bus_tx);

        // WHEN
        let result = invoker.discover("unknown").await.expect("discover failed");

        // THEN None
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_agent_cards_returns_sorted_active_agents() {
        // GIVEN 2 agents A2A Active (zebra-worker avant alpha-worker dans l'ordre d'insertion)
        let (bus_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(bus_tx.clone());

        for (name, skills) in [
            ("zebra-worker", vec!["skill-z"]),
            ("alpha-worker", vec!["skill-a", "skill-b"]),
        ] {
            let id = registry
                .register(make_a2a_manifest(name, &skills))
                .await
                .expect("register failed");
            registry
                .update_state(id.as_str(), ProcessState::Active)
                .await
                .expect("update state failed");
        }

        let invoker = A2AInvoker::new_for_test(registry, make_never_called_delegate(), bus_tx);

        // WHEN
        let cards = invoker.list_agent_cards().await.expect("list failed");

        // THEN triées par nom, alpha-worker en premier
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].name, "alpha-worker");
        assert_eq!(cards[1].name, "zebra-worker");
    }

    #[tokio::test]
    async fn test_list_skills_aggregates_all_a2a_skills() {
        // GIVEN 2 agents A2A avec des skills distincts
        let (bus_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(bus_tx.clone());

        for (name, skills) in [
            ("excel-worker", vec!["read-excel", "edit-excel"]),
            ("csv-worker", vec!["read-csv"]),
        ] {
            let id = registry
                .register(make_a2a_manifest(name, &skills))
                .await
                .expect("register failed");
            registry
                .update_state(id.as_str(), ProcessState::Active)
                .await
                .expect("update state failed");
        }

        let invoker = A2AInvoker::new_for_test(registry, make_never_called_delegate(), bus_tx);

        // WHEN
        let skills = invoker.list_skills().await.expect("list failed");

        // THEN 3 skills triés par skill_id
        assert_eq!(skills.len(), 3);
        assert_eq!(skills[0].skill_id, "edit-excel");
        assert_eq!(skills[1].skill_id, "read-csv");
        assert_eq!(skills[2].skill_id, "read-excel");
    }
}
