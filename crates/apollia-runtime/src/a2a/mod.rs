//! Routing A2A (Agent-to-Agent) par skill ID.
//!
//! Ce module implémente la résolution d'un `skill_id` vers un [`AgentEntry`] actif,
//! les types d'erreur structurés retournés au Director Agent, la fonction de délégation
//! type-erasée [`A2aDelegateFn`], et la factory [`make_delegate_fn`].

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use apollia_core::{AIPInput, AIPPart, DataPart, ProcessState, RuntimeEvent};

use crate::coordinator::ExecutionBackend;
use crate::eventbus::EventBusSender;
use crate::registry::{AgentEntry, AgentRegistryError, AgentRegistryHandle};
use crate::router::{SubmitError, TaskRouterHandle};

/// Erreurs de résolution et de délégation A2A.
#[derive(Debug, thiserror::Error)]
pub enum A2aError {
    /// Aucun agent actif ne déclare le skill demandé.
    #[error("no active agent declares skill '{skill_id}' (available: {available})")]
    SkillNotFound {
        /// Identifiant du skill demandé.
        skill_id: String,
        /// Skills A2A disponibles, séparés par des virgules.
        available: String,
    },
    /// Plusieurs agents déclarent le même skill — résolution ambiguë.
    #[error("skill '{skill_id}' is declared by multiple agents: {agents}")]
    AmbiguousSkill {
        /// Identifiant du skill en conflit.
        skill_id: String,
        /// Noms des agents en conflit, séparés par des virgules.
        agents: String,
    },
    /// Erreur du registry sous-jacent.
    #[error("registry error: {0}")]
    Registry(#[from] AgentRegistryError),
    /// Le TaskRouter est mort ou indisponible.
    #[error("task router unavailable")]
    RouterDead,
    /// La délégation a expiré avant la fin du Worker Agent.
    #[error("delegation timed out after {timeout_secs}s")]
    Timeout {
        /// Nombre de secondes avant expiration.
        timeout_secs: u64,
    },
    /// Le Worker Agent a retourné un échec.
    #[error("worker agent failed: {reason}")]
    WorkerFailed {
        /// Raison de l'échec.
        reason: String,
    },
}

/// Résultat d'une délégation A2A réussie.
#[derive(Debug, Serialize, Deserialize)]
pub struct A2aDelegateResult {
    /// Identifiant de la tâche exécutée par le Worker Agent.
    pub task_id: String,
    /// Nom du Worker Agent qui a traité la délégation.
    pub agent_name: String,
    /// Sortie textuelle produite par le Worker Agent.
    pub output: String,
}

/// Corps de réponse HTTP structuré pour les erreurs A2A.
///
/// Serialisé en JSON dans les réponses 4xx/5xx des routes A2A.
#[derive(Debug, Serialize)]
pub struct A2aErrorResponse {
    /// Message d'erreur humain.
    pub error: String,
    /// Skill demandé (présent pour SkillNotFound et AmbiguousSkill).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_id: Option<String>,
    /// Skills disponibles (présent pour SkillNotFound).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_skills: Option<Vec<String>>,
    /// Agents en conflit (présent pour AmbiguousSkill).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflicting_agents: Option<Vec<String>>,
}

impl A2aErrorResponse {
    /// Construit depuis une [`A2aError`].
    pub fn from_error(err: &A2aError) -> Self {
        match err {
            A2aError::SkillNotFound {
                skill_id,
                available,
            } => Self {
                error: err.to_string(),
                skill_id: Some(skill_id.clone()),
                available_skills: Some(
                    available
                        .split(", ")
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect(),
                ),
                conflicting_agents: None,
            },
            A2aError::AmbiguousSkill { skill_id, agents } => Self {
                error: err.to_string(),
                skill_id: Some(skill_id.clone()),
                available_skills: None,
                conflicting_agents: Some(agents.split(", ").map(str::to_string).collect()),
            },
            _ => Self {
                error: err.to_string(),
                skill_id: None,
                available_skills: None,
                conflicting_agents: None,
            },
        }
    }
}

/// Résout un `skill_id` dans une liste d'entrées de registry.
///
/// Sélectionne uniquement les agents avec `supports_a2a = true` en état
/// [`ProcessState::Active`] ou [`ProcessState::Degraded`].
///
/// Retourne une référence vers l'entrée cible, ou une [`A2aError`] structurée.
pub fn resolve_skill<'a>(
    entries: &'a [AgentEntry],
    skill_id: &str,
) -> Result<&'a AgentEntry, A2aError> {
    let candidates: Vec<&AgentEntry> = entries
        .iter()
        .filter(|e| {
            e.manifest.supports_a2a
                && matches!(
                    e.process_state,
                    ProcessState::Active | ProcessState::Degraded
                )
                && e.manifest.skills.iter().any(|s| s.id == skill_id)
        })
        .collect();

    match candidates.len() {
        0 => {
            let mut available: Vec<String> = entries
                .iter()
                .filter(|e| {
                    e.manifest.supports_a2a
                        && matches!(
                            e.process_state,
                            ProcessState::Active | ProcessState::Degraded
                        )
                })
                .flat_map(|e| e.manifest.skills.iter().map(|s| s.id.clone()))
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            available.sort();
            Err(A2aError::SkillNotFound {
                skill_id: skill_id.to_string(),
                available: available.join(", "),
            })
        }
        1 => Ok(candidates[0]),
        _ => {
            let mut agent_names: Vec<String> =
                candidates.iter().map(|e| e.manifest.name.clone()).collect();
            agent_names.sort();
            Err(A2aError::AmbiguousSkill {
                skill_id: skill_id.to_string(),
                agents: agent_names.join(", "),
            })
        }
    }
}

/// Fonction de délégation A2A type-erasée.
///
/// Accepte `(skill_id, input_payload, timeout_secs)` et retourne une `Future`
/// résolvant en `Result<A2aDelegateResult, A2aError>`.
///
/// Construit via [`make_delegate_fn`]. Permet d'injecter la logique de délégation
/// dans `RuntimeContext` sans générique sur le backend d'exécution.
pub type A2aDelegateFn = Arc<
    dyn Fn(
            String,
            serde_json::Value,
            u64,
        ) -> Pin<Box<dyn Future<Output = Result<A2aDelegateResult, A2aError>> + Send>>
        + Send
        + Sync,
>;

/// Construit une [`A2aDelegateFn`] concrète à partir des handles runtime.
///
/// La fonction résout le `skill_id` vers un agent, soumet la tâche via le router,
/// attend la complétion sur l'EventBus, et retourne le résultat structuré.
pub fn make_delegate_fn<B>(
    registry: AgentRegistryHandle,
    router: TaskRouterHandle<B>,
    event_bus: EventBusSender,
) -> A2aDelegateFn
where
    B: ExecutionBackend + Clone + Send + Sync + 'static,
{
    Arc::new(
        move |skill_id: String, input_payload: serde_json::Value, timeout_secs: u64| {
            let registry = registry.clone();
            let router = router.clone();
            let event_bus = event_bus.clone();
            Box::pin(async move {
                delegate_inner(
                    &registry,
                    &router,
                    &event_bus,
                    &skill_id,
                    input_payload,
                    timeout_secs,
                )
                .await
            })
        },
    )
}

/// Logique de délégation A2A — appelable depuis le REST handler et depuis
/// la [`A2aDelegateFn`] type-erasée.
pub(crate) async fn delegate_inner<B: ExecutionBackend + Clone>(
    registry: &AgentRegistryHandle,
    router: &TaskRouterHandle<B>,
    event_bus: &EventBusSender,
    skill_id: &str,
    input_payload: serde_json::Value,
    timeout_secs: u64,
) -> Result<A2aDelegateResult, A2aError> {
    // 1. Résoudre skill_id → agent depuis le registry.
    let entries = registry.list_agents().await?;
    let target = resolve_skill(&entries, skill_id)?;
    let agent_id = target.id.to_string();
    let agent_name = target.manifest.name.clone();

    info!(skill_id = %skill_id, agent = %agent_name, "A2A delegation initiated");

    // 2. S'abonner à l'EventBus avant de soumettre (évite une race condition).
    let mut event_rx = event_bus.subscribe();

    // 3. Construire l'input AIP depuis le payload JSON.
    let input = AIPInput {
        parts: vec![AIPPart::Data(DataPart {
            data: input_payload,
        })],
    };

    // 4. Soumettre la tâche via le TaskRouter.
    let task_id = router.submit(&agent_id, input).await.map_err(|e| match e {
        SubmitError::ActorDead => A2aError::RouterDead,
        other => A2aError::WorkerFailed {
            reason: other.to_string(),
        },
    })?;

    let task_id_str = task_id.to_string();
    info!(task_id = %task_id_str, agent = %agent_name, "A2A task submitted");

    // 5. Attendre TaskCompleted avec timeout via l'EventBus.
    let wait_result = tokio::time::timeout(Duration::from_secs(timeout_secs), async {
        loop {
            match event_rx.recv().await {
                Ok(RuntimeEvent::TaskCompleted {
                    task_id: completed_id,
                    success,
                    output,
                    ..
                }) if completed_id.to_string() == task_id_str => {
                    return if success {
                        Ok(output.unwrap_or_default())
                    } else {
                        Err(A2aError::WorkerFailed {
                            reason: "worker agent reported failure".to_string(),
                        })
                    };
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!(
                        task_id = %task_id_str,
                        lagged = n,
                        "A2A EventBus lagged — checking router output directly"
                    );
                    // Fallback: consulter le routeur directement.
                    if let Ok(Some(out)) = router.get_output(&task_id_str).await {
                        return Ok(out);
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    return Err(A2aError::RouterDead);
                }
                _ => {
                    // Pas notre événement — continuer d'attendre.
                }
            }
        }
    })
    .await;

    let output = match wait_result {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return Err(e),
        Err(_elapsed) => return Err(A2aError::Timeout { timeout_secs }),
    };

    info!(task_id = %task_id_str, agent = %agent_name, "A2A delegation completed");

    Ok(A2aDelegateResult {
        task_id: task_id_str,
        agent_name,
        output,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{AgentId, AgentManifest, AgentSkill, ProcessState};
    use std::time::Instant;

    fn make_a2a_entry(
        name: &str,
        supports_a2a: bool,
        skill_ids: &[&str],
        state: ProcessState,
    ) -> AgentEntry {
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

        AgentEntry {
            id: AgentId::new_v4(),
            manifest: AgentManifest {
                name: name.to_string(),
                version: "0.1.0".to_string(),
                description: String::new(),
                tools_required: vec![],
                tools_optional: vec![],
                supports_streaming: false,
                supports_a2a,
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
            },
            process_state: state,
            registered_at: Instant::now(),
        }
    }

    #[test]
    fn test_resolve_skill_found() {
        // GIVEN un agent actif avec le skill "read-excel"
        let entries = vec![make_a2a_entry(
            "excel-worker",
            true,
            &["read-excel", "edit-excel"],
            ProcessState::Active,
        )];

        // WHEN on résout "read-excel"
        let result = resolve_skill(&entries, "read-excel");

        // THEN l'agent excel-worker est retourné
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
        assert_eq!(result.unwrap().manifest.name, "excel-worker");
    }

    #[test]
    fn test_resolve_skill_not_found_returns_available_list() {
        // GIVEN un agent actif avec "read-excel" mais pas "unknown-skill"
        let entries = vec![make_a2a_entry(
            "excel-worker",
            true,
            &["read-excel"],
            ProcessState::Active,
        )];

        // WHEN on résout "unknown-skill"
        let result = resolve_skill(&entries, "unknown-skill");

        // THEN erreur SkillNotFound avec la liste des skills disponibles
        assert!(result.is_err());
        match result.unwrap_err() {
            A2aError::SkillNotFound {
                skill_id,
                available,
            } => {
                assert_eq!(skill_id, "unknown-skill");
                assert!(
                    available.contains("read-excel"),
                    "expected 'read-excel' in available: {available}"
                );
            }
            other => panic!("unexpected error variant: {other}"),
        }
    }

    #[test]
    fn test_resolve_skill_ambiguous_returns_conflicting_agents() {
        // GIVEN deux agents actifs déclarant tous les deux "shared-skill"
        let entries = vec![
            make_a2a_entry("agent-a", true, &["shared-skill"], ProcessState::Active),
            make_a2a_entry("agent-b", true, &["shared-skill"], ProcessState::Active),
        ];

        // WHEN on résout "shared-skill"
        let result = resolve_skill(&entries, "shared-skill");

        // THEN erreur AmbiguousSkill avec les deux agents
        assert!(result.is_err());
        match result.unwrap_err() {
            A2aError::AmbiguousSkill { skill_id, agents } => {
                assert_eq!(skill_id, "shared-skill");
                assert!(agents.contains("agent-a"), "agents: {agents}");
                assert!(agents.contains("agent-b"), "agents: {agents}");
            }
            other => panic!("unexpected error variant: {other}"),
        }
    }

    #[test]
    fn test_resolve_skill_excludes_non_a2a_agents() {
        // GIVEN un agent sans supports_a2a ET un agent avec supports_a2a, même skill
        let entries = vec![
            make_a2a_entry(
                "generic-agent",
                false,
                &["read-excel"],
                ProcessState::Active,
            ),
            make_a2a_entry("excel-worker", true, &["read-excel"], ProcessState::Active),
        ];

        // WHEN on résout "read-excel"
        let result = resolve_skill(&entries, "read-excel");

        // THEN seul excel-worker est retourné
        assert!(result.is_ok());
        assert_eq!(result.unwrap().manifest.name, "excel-worker");
    }

    #[test]
    fn test_resolve_skill_excludes_stopped_agents() {
        // GIVEN un agent stopped avec le bon skill
        let entries = vec![make_a2a_entry(
            "excel-worker",
            true,
            &["read-excel"],
            ProcessState::Stopped,
        )];

        // WHEN on résout "read-excel"
        let result = resolve_skill(&entries, "read-excel");

        // THEN SkillNotFound car l'agent est stopped
        assert!(
            matches!(result.unwrap_err(), A2aError::SkillNotFound { .. }),
            "expected SkillNotFound for stopped agent"
        );
    }

    #[test]
    fn test_resolve_skill_accepts_degraded_agents() {
        // GIVEN un agent degraded avec le bon skill
        let entries = vec![make_a2a_entry(
            "excel-worker",
            true,
            &["read-excel"],
            ProcessState::Degraded,
        )];

        // WHEN on résout "read-excel"
        let result = resolve_skill(&entries, "read-excel");

        // THEN l'agent degraded est accepté
        assert!(result.is_ok());
        assert_eq!(result.unwrap().manifest.name, "excel-worker");
    }

    #[test]
    fn test_a2a_error_response_skill_not_found_structure() {
        // GIVEN une erreur SkillNotFound
        let err = A2aError::SkillNotFound {
            skill_id: "my-skill".to_string(),
            available: "read-excel, read-csv".to_string(),
        };

        // WHEN on construit la réponse HTTP
        let resp = A2aErrorResponse::from_error(&err);

        // THEN skill_id et available_skills sont présents, conflicting_agents absent
        assert_eq!(resp.skill_id.as_deref(), Some("my-skill"));
        let avail = resp
            .available_skills
            .as_ref()
            .expect("available_skills must be Some");
        assert!(avail.iter().any(|s| s == "read-excel"));
        assert!(avail.iter().any(|s| s == "read-csv"));
        assert!(resp.conflicting_agents.is_none());
    }

    #[test]
    fn test_a2a_error_response_ambiguous_skill_structure() {
        // GIVEN une erreur AmbiguousSkill
        let err = A2aError::AmbiguousSkill {
            skill_id: "conflict".to_string(),
            agents: "agent-a, agent-b".to_string(),
        };

        // WHEN on construit la réponse HTTP
        let resp = A2aErrorResponse::from_error(&err);

        // THEN conflicting_agents est présent, available_skills absent
        assert_eq!(resp.skill_id.as_deref(), Some("conflict"));
        assert!(resp.available_skills.is_none());
        let conflicts = resp
            .conflicting_agents
            .as_ref()
            .expect("conflicting_agents must be Some");
        assert!(conflicts.iter().any(|s| s == "agent-a"));
    }

    #[test]
    fn test_a2a_error_response_other_errors_no_extra_fields() {
        // GIVEN une erreur RouterDead (pas de skill_id ni available)
        let err = A2aError::RouterDead;

        // WHEN on construit la réponse HTTP
        let resp = A2aErrorResponse::from_error(&err);

        // THEN skill_id et listes sont absents
        assert!(resp.skill_id.is_none());
        assert!(resp.available_skills.is_none());
        assert!(resp.conflicting_agents.is_none());
        assert!(!resp.error.is_empty());
    }
}
