//! A2AInvoker — orchestrateur de haut niveau pour les invocations inter-agents par skill ID.
//!
//! Gère le cycle complet d'une invocation A2A :
//! résolution du skill (état `Active` requis), émission des événements runtime,
//! délégation au TaskRouter avec timeout, et construction du résultat structuré.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::info;

use apollia_core::{A2AConfig, AIPResult, ProcessState, RuntimeEvent};

use crate::a2a::{make_delegate_fn, A2aDelegateFn};
use crate::coordinator::ExecutionBackend;
use crate::eventbus::EventBusSender;
use crate::registry::{AgentEntry, AgentRegistryHandle};
use crate::router::TaskRouterHandle;

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

    /// La profondeur de récursivité A2A maximale est atteinte.
    ///
    /// Appliqué par le runtime avant résolution du skill (Principe #7).
    /// Protège contre les chaînes récursives infinies entre agents.
    #[error("a2a max depth {max_depth} exceeded (current: {current_depth}, caller: {caller}, skill: {skill_id})")]
    MaxDepthExceeded {
        /// Profondeur courante de l'invocation.
        current_depth: u32,
        /// Profondeur maximale configurée.
        max_depth: u32,
        /// Nom de l'agent initiateur.
        caller: String,
        /// Identifiant du skill demandé.
        skill_id: String,
    },

    /// Un agent tente de s'invoquer lui-même via un skill A2A.
    ///
    /// Appliqué après résolution du skill cible. Empêche les boucles
    /// directes où un agent expose un skill et l'invoque ensuite lui-même.
    #[error("agent '{agent_name}' cannot invoke itself via skill '{skill_id}'")]
    SelfInvocation {
        /// Nom de l'agent qui tente l'auto-invocation.
        agent_name: String,
        /// Identifiant du skill cible.
        skill_id: String,
    },

    /// Le timeout cumulé de la chaîne A2A est dépassé.
    ///
    /// Déclenché soit immédiatement si le `chain_deadline` est déjà expiré
    /// à l'entrée de `invoke()`, soit après la délégation si c'est le
    /// `chain_deadline` (et non l'`invocation_timeout`) qui a expiré en premier.
    #[error("a2a chain timeout exceeded (caller: {caller}, skill: {skill_id})")]
    ChainTimeoutExceeded {
        /// Nom de l'agent initiateur.
        caller: String,
        /// Identifiant du skill demandé.
        skill_id: String,
    },
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
/// 1. Application des garde-fous (profondeur, auto-invocation, timeout de chaîne)
/// 2. Résolution du `skill_id` → agent (état `Active` requis, Principe #4)
/// 3. Émission de [`RuntimeEvent::A2AInvocationStarted`]
/// 4. Délégation via le TaskRouter avec timeout effectif (Principe #7)
/// 5. Émission de [`RuntimeEvent::A2AInvocationCompleted`]
/// 6. Construction du [`A2AInvocationResult`]
///
/// N'est pas un acteur Tokio — struct clonable avec des handles internes.
#[derive(Clone)]
pub struct A2AInvoker {
    registry: AgentRegistryHandle,
    delegate_fn: A2aDelegateFn,
    event_bus: EventBusSender,
    /// Configuration des garde-fous appliqués à chaque invocation.
    config: A2AConfig,
    /// Logger de sidechains — `None` si la base SQLite n'est pas disponible.
    sidechain_logger: Option<crate::a2a::sidechain::SidechainLogger>,
}

impl A2AInvoker {
    /// Construit un `A2AInvoker` depuis les handles runtime et la configuration A2A.
    ///
    /// Générique sur `B: ExecutionBackend` — le résultat est non-générique grâce
    /// à l'erasure de type opérée par [`make_delegate_fn`].
    pub fn new<B>(
        registry: AgentRegistryHandle,
        router: TaskRouterHandle<B>,
        event_bus: EventBusSender,
        config: A2AConfig,
    ) -> Self
    where
        B: ExecutionBackend + Clone + Send + Sync + 'static,
    {
        let delegate_fn = make_delegate_fn(registry.clone(), router, event_bus.clone());
        Self {
            registry,
            delegate_fn,
            event_bus,
            config,
            sidechain_logger: None,
        }
    }

    /// Attache un [`SidechainLogger`] à cet invoker pour la traçabilité des délégations.
    ///
    /// Retourne `self` pour un usage en chaîne de construction.
    pub fn with_sidechain_logger(mut self, logger: crate::a2a::sidechain::SidechainLogger) -> Self {
        self.sidechain_logger = Some(logger);
        self
    }

    /// Retourne le [`SidechainLogger`] attaché, si disponible.
    pub fn sidechain_logger(&self) -> Option<&crate::a2a::sidechain::SidechainLogger> {
        self.sidechain_logger.as_ref()
    }

    /// Délègue une tâche à un agent cible avec logging sidechain best-effort.
    ///
    /// Enregistre le début de la délégation dans `task_sidechains` avant l'invocation,
    /// puis met à jour le statut (`"completed"` ou `"failed"`) après.
    /// Si le [`SidechainLogger`] est absent ou si le logging échoue, la délégation
    /// se poursuit normalement — le logging est toujours best-effort.
    ///
    /// # Arguments
    ///
    /// - `parent_task_id` : identifiant de la tâche parente qui initie la délégation.
    /// - Autres arguments : identiques à [`invoke`].
    #[allow(clippy::too_many_arguments)]
    pub async fn invoke_with_logging(
        &self,
        parent_task_id: &apollia_core::TaskId,
        skill_id: &str,
        input: serde_json::Value,
        caller: &str,
        a2a_depth: u32,
        timeout: Option<Duration>,
        chain_deadline: Option<Instant>,
    ) -> Result<A2AInvocationResult, A2AError> {
        let sidechain_n = if let Some(logger) = &self.sidechain_logger {
            logger.start(parent_task_id, skill_id, &input).await
        } else {
            0
        };

        let result = self
            .invoke(skill_id, input, caller, a2a_depth, timeout, chain_deadline)
            .await;

        if let Some(logger) = &self.sidechain_logger {
            let (output_summary, status) = match &result {
                Ok(r) => (serde_json::to_string(r).unwrap_or_default(), "completed"),
                Err(e) => (e.to_string(), "failed"),
            };
            logger
                .complete(parent_task_id, sidechain_n, &output_summary, status)
                .await;
        }

        result
    }

    /// Invoque un Worker Agent par son `skill_id`.
    ///
    /// Applique les garde-fous dans l'ordre suivant avant toute délégation :
    /// 1. Profondeur de récursivité (`a2a_depth >= config.max_depth` → [`A2AError::MaxDepthExceeded`])
    /// 2. Timeout de chaîne expiré (`chain_deadline` déjà passé → [`A2AError::ChainTimeoutExceeded`])
    /// 3. Auto-invocation (caller == agent cible → [`A2AError::SelfInvocation`])
    ///
    /// Résout ensuite le skill, valide l'état `Active`, délègue via le TaskRouter,
    /// et retourne un [`A2AInvocationResult`] enrichi.
    ///
    /// Le `timeout` effectif est le minimum entre `timeout` et le délai résiduel
    /// du `chain_deadline`. Si le timeout résulte du `chain_deadline`, l'erreur est
    /// [`A2AError::ChainTimeoutExceeded`] plutôt que [`A2AError::Timeout`].
    ///
    /// # Arguments
    ///
    /// - `a2a_depth` : profondeur courante de la chaîne (0 pour l'invocation racine).
    /// - `chain_deadline` : deadline cumulé de la chaîne ; `None` à la première
    ///   invocation, initialisé à `now + chain_timeout_secs` et propagé ensuite.
    ///
    /// # Errors
    ///
    /// - [`A2AError::MaxDepthExceeded`] si `a2a_depth >= config.max_depth`.
    /// - [`A2AError::ChainTimeoutExceeded`] si le deadline de chaîne est expiré.
    /// - [`A2AError::SelfInvocation`] si l'agent tente de s'invoquer lui-même.
    /// - [`A2AError::SkillNotFound`] si aucun agent A2A disponible ne déclare le skill.
    /// - [`A2AError::AgentNotActive`] si l'agent cible n'est pas en état `Active`.
    /// - [`A2AError::Timeout`] si la durée d'exécution dépasse le timeout d'invocation.
    /// - [`A2AError::ExecutionFailed`] si le Worker Agent retourne un échec.
    /// - [`A2AError::RegistryError`] en cas d'erreur de communication avec le registry.
    pub async fn invoke(
        &self,
        skill_id: &str,
        input: serde_json::Value,
        caller: &str,
        a2a_depth: u32,
        timeout: Option<Duration>,
        chain_deadline: Option<Instant>,
    ) -> Result<A2AInvocationResult, A2AError> {
        // ── Garde-fou 1 : profondeur de récursivité ────────────────────────────
        if a2a_depth >= self.config.max_depth {
            let detail = format!(
                "recursion depth {a2a_depth} reaches max_depth {} (caller: {caller}, skill: {skill_id})",
                self.config.max_depth
            );
            let _ = self.event_bus.send(RuntimeEvent::A2AGuardTriggered {
                guard_type: "max_depth".to_string(),
                caller: caller.to_string(),
                skill_id: skill_id.to_string(),
                detail,
            });
            return Err(A2AError::MaxDepthExceeded {
                current_depth: a2a_depth,
                max_depth: self.config.max_depth,
                caller: caller.to_string(),
                skill_id: skill_id.to_string(),
            });
        }

        // ── Garde-fou 2 : timeout cumulé de chaîne ────────────────────────────
        // Initialiser le deadline à la première invocation de la chaîne.
        let effective_deadline = chain_deadline.unwrap_or_else(|| {
            Instant::now() + Duration::from_secs(self.config.chain_timeout_secs)
        });

        let chain_remaining = effective_deadline.checked_duration_since(Instant::now());

        let (effective_timeout_secs, governed_by_chain) = match chain_remaining {
            None => {
                // Deadline déjà expiré avant même de commencer.
                let detail = format!(
                    "chain deadline already expired before invocation (caller: {caller}, skill: {skill_id})"
                );
                let _ = self.event_bus.send(RuntimeEvent::A2AGuardTriggered {
                    guard_type: "chain_timeout".to_string(),
                    caller: caller.to_string(),
                    skill_id: skill_id.to_string(),
                    detail,
                });
                return Err(A2AError::ChainTimeoutExceeded {
                    caller: caller.to_string(),
                    skill_id: skill_id.to_string(),
                });
            }
            Some(remaining) => {
                let invocation_timeout = timeout
                    .unwrap_or_else(|| Duration::from_secs(self.config.invocation_timeout_secs));
                if remaining < invocation_timeout {
                    // Le chain_deadline expire avant l'invocation_timeout.
                    (remaining.as_secs().max(1), true)
                } else {
                    (invocation_timeout.as_secs(), false)
                }
            }
        };

        // ── Résolution du skill ────────────────────────────────────────────────
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

        let target = matching
            .iter()
            .find(|e| e.process_state == ProcessState::Active)
            .ok_or_else(|| A2AError::AgentNotActive {
                agent_name: matching[0].manifest.name.clone(),
                state: format!("{:?}", matching[0].process_state),
            })?;

        let agent_name = target.manifest.name.clone();

        // ── Garde-fou 3 : auto-invocation ─────────────────────────────────────
        if agent_name == caller {
            let detail =
                format!("agent '{caller}' resolved as its own target for skill '{skill_id}'");
            let _ = self.event_bus.send(RuntimeEvent::A2AGuardTriggered {
                guard_type: "self_invocation".to_string(),
                caller: caller.to_string(),
                skill_id: skill_id.to_string(),
                detail,
            });
            return Err(A2AError::SelfInvocation {
                agent_name: caller.to_string(),
                skill_id: skill_id.to_string(),
            });
        }

        info!(
            skill_id = %skill_id,
            agent = %agent_name,
            caller = %caller,
            a2a_depth = a2a_depth,
            "A2A invocation starting"
        );

        let _ = self.event_bus.send(RuntimeEvent::A2AInvocationStarted {
            caller: caller.to_string(),
            target: agent_name.clone(),
            skill_id: skill_id.to_string(),
        });

        let start = Instant::now();

        let delegate_result =
            (self.delegate_fn)(skill_id.to_string(), input, effective_timeout_secs).await;
        let duration_ms = start.elapsed().as_millis() as u64;

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

        let delegate = match delegate_result {
            Ok(r) => r,
            Err(crate::a2a::A2aError::Timeout { .. }) if governed_by_chain => {
                let detail = format!(
                    "chain timeout exceeded during invocation (caller: {caller}, skill: {skill_id})"
                );
                let _ = self.event_bus.send(RuntimeEvent::A2AGuardTriggered {
                    guard_type: "chain_timeout".to_string(),
                    caller: caller.to_string(),
                    skill_id: skill_id.to_string(),
                    detail,
                });
                return Err(A2AError::ChainTimeoutExceeded {
                    caller: caller.to_string(),
                    skill_id: skill_id.to_string(),
                });
            }
            Err(e) => {
                return Err(map_delegate_err(
                    e,
                    skill_id,
                    &agent_name,
                    effective_timeout_secs,
                ))
            }
        };

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

    /// Constructeur de test — injecte une `A2aDelegateFn` personnalisée et une config.
    #[doc(hidden)]
    pub fn new_for_test(
        registry: AgentRegistryHandle,
        delegate_fn: A2aDelegateFn,
        event_bus: EventBusSender,
        config: A2AConfig,
    ) -> Self {
        Self {
            registry,
            delegate_fn,
            event_bus,
            config,
            sidechain_logger: None,
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
    use tokio::time::sleep;

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

        let invoker = A2AInvoker::new_for_test(
            registry,
            make_never_called_delegate(),
            bus_tx,
            A2AConfig::default(),
        );

        // WHEN
        let result = invoker
            .invoke(
                "unknown-skill",
                serde_json::json!({}),
                "director",
                0,
                None,
                None,
            )
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

        let invoker = A2AInvoker::new_for_test(
            registry,
            make_never_called_delegate(),
            bus_tx,
            A2AConfig::default(),
        );

        // WHEN
        let result = invoker
            .invoke(
                "read-excel",
                serde_json::json!({}),
                "director",
                0,
                None,
                None,
            )
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

        let invoker = A2AInvoker::new_for_test(
            registry,
            make_ok_delegate("colonnes: A, B, C"),
            bus_tx,
            A2AConfig::default(),
        );

        // WHEN
        let result = invoker
            .invoke(
                "read-excel",
                serde_json::json!({"text": "Lis ventes.xlsx"}),
                "director",
                0,
                None,
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

        let invoker = A2AInvoker::new_for_test(
            registry,
            make_timeout_delegate(),
            bus_tx,
            A2AConfig::default(),
        );

        // WHEN
        let result = invoker
            .invoke(
                "read-excel",
                serde_json::json!({}),
                "director",
                0,
                Some(Duration::from_secs(1)),
                None,
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

        let invoker = A2AInvoker::new_for_test(
            registry,
            make_never_called_delegate(),
            bus_tx,
            A2AConfig::default(),
        );

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
        let invoker = A2AInvoker::new_for_test(
            registry,
            make_never_called_delegate(),
            bus_tx,
            A2AConfig::default(),
        );

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

        let invoker = A2AInvoker::new_for_test(
            registry,
            make_never_called_delegate(),
            bus_tx,
            A2AConfig::default(),
        );

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

        let invoker = A2AInvoker::new_for_test(
            registry,
            make_never_called_delegate(),
            bus_tx,
            A2AConfig::default(),
        );

        // WHEN
        let skills = invoker.list_skills().await.expect("list failed");

        // THEN 3 skills triés par skill_id
        assert_eq!(skills.len(), 3);
        assert_eq!(skills[0].skill_id, "edit-excel");
        assert_eq!(skills[1].skill_id, "read-csv");
        assert_eq!(skills[2].skill_id, "read-excel");
    }
}

// ── A2A garde-fous ────────────────────────────────────────────────────────────
#[cfg(test)]
mod a2a_guard_tests {
    use super::*;
    use crate::a2a::A2aError as LowLevelA2aError;
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use apollia_core::{AgentManifest, AgentSkill, ProcessState};
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;

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
            tags: vec![],
            skills,
            execution_mode: "direct".to_string(),
            system_prompt: None,
            tools_requiring_approval: vec![],
            llm_backend: None,
            packages: vec![],
        }
    }

    fn make_ok_delegate() -> A2aDelegateFn {
        Arc::new(
            move |_skill_id: String, _input: serde_json::Value, _timeout: u64| {
                let fut: Pin<
                    Box<
                        dyn Future<Output = Result<crate::a2a::A2aDelegateResult, LowLevelA2aError>>
                            + Send,
                    >,
                > = Box::pin(async move {
                    Ok(crate::a2a::A2aDelegateResult {
                        task_id: "task-guard-test".to_string(),
                        agent_name: "excel-worker".to_string(),
                        output: "ok".to_string(),
                    })
                });
                fut
            },
        )
    }

    async fn make_active_invoker_with_config(
        agent_name: &str,
        skill_ids: &[&str],
        config: A2AConfig,
    ) -> (A2AInvoker, tokio::sync::broadcast::Receiver<RuntimeEvent>) {
        let (bus_tx, bus_rx) = EventBus::new();
        let registry = AgentRegistry::spawn(bus_tx.clone());
        let id = registry
            .register(make_a2a_manifest(agent_name, skill_ids))
            .await
            .expect("register failed");
        registry
            .update_state(id.as_str(), ProcessState::Active)
            .await
            .expect("update state failed");
        let invoker = A2AInvoker::new_for_test(registry, make_ok_delegate(), bus_tx, config);
        (invoker, bus_rx)
    }

    #[tokio::test]
    async fn test_max_depth_blocks_deep_recursion() {
        // GIVEN A2AInvoker avec max_depth = 2, a2a_depth = 2
        let config = A2AConfig {
            max_depth: 2,
            ..A2AConfig::default()
        };
        let (invoker, _) =
            make_active_invoker_with_config("excel-worker", &["read-excel"], config).await;

        // WHEN invoke avec a2a_depth = 2 (= max_depth)
        let result = invoker
            .invoke(
                "read-excel",
                serde_json::json!({}),
                "director",
                2,
                None,
                None,
            )
            .await;

        // THEN MaxDepthExceeded avec les champs corrects
        match result.expect_err("expected MaxDepthExceeded") {
            A2AError::MaxDepthExceeded {
                current_depth,
                max_depth,
                caller,
                skill_id,
            } => {
                assert_eq!(current_depth, 2);
                assert_eq!(max_depth, 2);
                assert_eq!(caller, "director");
                assert_eq!(skill_id, "read-excel");
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[tokio::test]
    async fn test_depth_below_max_passes() {
        // GIVEN max_depth = 3, a2a_depth = 1
        let config = A2AConfig {
            max_depth: 3,
            ..A2AConfig::default()
        };
        let (invoker, _) =
            make_active_invoker_with_config("excel-worker", &["read-excel"], config).await;

        // WHEN invoke avec a2a_depth = 1 (< max_depth)
        let result = invoker
            .invoke(
                "read-excel",
                serde_json::json!({}),
                "director",
                1,
                None,
                None,
            )
            .await;

        // THEN pas d'erreur de profondeur — invocation réussit
        assert!(
            result.is_ok(),
            "depth 1 < max_depth 3 should succeed, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_self_invocation_blocked() {
        // GIVEN excel-worker enregistré avec skill "read-excel"
        let (invoker, _) =
            make_active_invoker_with_config("excel-worker", &["read-excel"], A2AConfig::default())
                .await;

        // WHEN excel-worker s'invoque lui-même via le skill "read-excel"
        let result = invoker
            .invoke(
                "read-excel",
                serde_json::json!({}),
                "excel-worker",
                0,
                None,
                None,
            )
            .await;

        // THEN SelfInvocation avec les champs corrects
        match result.expect_err("expected SelfInvocation") {
            A2AError::SelfInvocation {
                agent_name,
                skill_id,
            } => {
                assert_eq!(agent_name, "excel-worker");
                assert_eq!(skill_id, "read-excel");
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[tokio::test]
    async fn test_chain_deadline_initialized_on_first_call() {
        // GIVEN chain_deadline = None (première invocation de la chaîne)
        let (invoker, _) =
            make_active_invoker_with_config("excel-worker", &["read-excel"], A2AConfig::default())
                .await;

        // WHEN invoke avec chain_deadline = None
        let result = invoker
            .invoke(
                "read-excel",
                serde_json::json!({}),
                "director",
                0,
                None,
                None,
            )
            .await;

        // THEN le deadline est initialisé à l'avenir — pas de ChainTimeoutExceeded
        assert!(
            result.is_ok(),
            "first call with chain_deadline=None must succeed (deadline initialized to future), got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_chain_timeout_exceeded() {
        // GIVEN chain_deadline dans le passé (expiré il y a 1 seconde)
        let past_deadline = Instant::now() - Duration::from_secs(1);
        let (invoker, _) =
            make_active_invoker_with_config("excel-worker", &["read-excel"], A2AConfig::default())
                .await;

        // WHEN invoke avec chain_deadline expiré
        let result = invoker
            .invoke(
                "read-excel",
                serde_json::json!({}),
                "director",
                0,
                None,
                Some(past_deadline),
            )
            .await;

        // THEN ChainTimeoutExceeded immédiat
        match result.expect_err("expected ChainTimeoutExceeded") {
            A2AError::ChainTimeoutExceeded { caller, skill_id } => {
                assert_eq!(caller, "director");
                assert_eq!(skill_id, "read-excel");
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[tokio::test]
    async fn test_guard_event_emitted_on_max_depth() {
        // GIVEN EventBus receiver + max_depth = 1
        let config = A2AConfig {
            max_depth: 1,
            ..A2AConfig::default()
        };
        let (invoker, mut bus_rx) =
            make_active_invoker_with_config("excel-worker", &["read-excel"], config).await;

        // WHEN invoke avec a2a_depth = max_depth
        let _ = invoker
            .invoke(
                "read-excel",
                serde_json::json!({}),
                "director",
                1,
                None,
                None,
            )
            .await;

        // THEN A2AGuardTriggered { guard_type: "max_depth" } émis
        let mut found = false;
        while let Ok(event) = bus_rx.try_recv() {
            if let RuntimeEvent::A2AGuardTriggered {
                ref guard_type,
                ref caller,
                ref skill_id,
                ..
            } = event
            {
                assert_eq!(guard_type, "max_depth");
                assert_eq!(caller, "director");
                assert_eq!(skill_id, "read-excel");
                found = true;
            }
        }
        assert!(
            found,
            "A2AGuardTriggered with guard_type=max_depth was not emitted"
        );
    }

    #[test]
    fn test_a2a_config_defaults() {
        // GIVEN A2AConfig désérialisé depuis JSON vide (toutes valeurs par défaut)
        let config: A2AConfig =
            serde_json::from_str("{}").expect("deserialization of empty object failed");

        // THEN valeurs par défaut saines
        assert_eq!(config.max_depth, 3);
        assert_eq!(config.invocation_timeout_secs, 120);
        assert_eq!(config.chain_timeout_secs, 300);
    }

    #[test]
    fn test_a2a_config_round_trip() {
        // GIVEN une config avec des valeurs personnalisées
        let config = A2AConfig {
            max_depth: 5,
            invocation_timeout_secs: 60,
            chain_timeout_secs: 600,
        };

        // WHEN sérialisation/désérialisation
        let json = serde_json::to_string(&config).expect("serialization failed");
        let restored: A2AConfig = serde_json::from_str(&json).expect("deserialization failed");

        // THEN valeurs préservées
        assert_eq!(restored.max_depth, 5);
        assert_eq!(restored.invocation_timeout_secs, 60);
        assert_eq!(restored.chain_timeout_secs, 600);
    }
}
