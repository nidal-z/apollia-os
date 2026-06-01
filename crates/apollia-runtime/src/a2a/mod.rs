//! A2A (Agent-to-Agent) routing by skill ID.
//!
//! This module implements resolution of a `skill_id` to an active [`AgentEntry`],
//! the structured error types returned to the Director Agent, the type-erased
//! delegation function [`A2aDelegateFn`], the [`make_delegate_fn`] factory, and
//! the high-level orchestrator [`invoker::A2AInvoker`].

pub mod compatibility;
pub mod invoker;
pub mod sidechain;
pub mod telemetry;
pub mod tools_provider;

pub use compatibility::{check_compatibility, A2ACompatibilityWarning, CompatSeverity, SemVer};
pub use telemetry::{
    A2ASkillTelemetry, A2AStepProvenance, InvocationRecord, TelemetryHandle, TelemetryStore,
};

pub use invoker::{
    A2AAgentCard, A2AError, A2AInvocationResult, A2AInvokeRequest, A2AInvoker, A2ASkillInfo,
    RuntimeContextConfig, SkillListing,
};
pub use sidechain::{SidechainLogger, SidechainRepository, SidechainRow};
pub use tools_provider::A2AToolsProvider;

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use apollia_core::{AIPInput, AIPPart, AgentId, DataPart, ProcessState, RuntimeEvent};

use crate::coordinator::ExecutionBackend;
use crate::eventbus::EventBusSender;
use crate::registry::{AgentEntry, AgentRegistryError, AgentRegistryHandle};
use crate::router::{SubmitError, TaskRouterHandle};

/// A2A resolution and delegation errors.
#[derive(Debug, thiserror::Error)]
pub enum A2aError {
    /// No active agent declares the requested skill.
    #[error("no active agent declares skill '{skill_id}' (available: {available})")]
    SkillNotFound {
        /// Identifier of the requested skill.
        skill_id: String,
        /// Available A2A skills, comma-separated.
        available: String,
    },
    /// Several agents declare the same skill: ambiguous resolution.
    #[error("skill '{skill_id}' is declared by multiple agents: {agents}")]
    AmbiguousSkill {
        /// Identifier of the conflicting skill.
        skill_id: String,
        /// Names of the conflicting agents, comma-separated.
        agents: String,
    },
    /// Error from the underlying registry.
    #[error("registry error: {0}")]
    Registry(#[from] AgentRegistryError),
    /// The TaskRouter is dead or unavailable.
    #[error("task router unavailable")]
    RouterDead,
    /// The delegation timed out before the Worker Agent finished.
    #[error("delegation timed out after {timeout_secs}s")]
    Timeout {
        /// Number of seconds before expiry.
        timeout_secs: u64,
    },
    /// The Worker Agent returned a failure.
    #[error("worker agent failed: {reason}")]
    WorkerFailed {
        /// Reason for the failure.
        reason: String,
    },
    /// The target agent is already in the delegation chain: cycle detected.
    #[error("A2A cycle: agent {agent_id} already in delegation chain")]
    CycleDetected {
        /// ID of the target agent already present in the chain.
        agent_id: AgentId,
    },
    /// The maximum delegation chain depth has been reached.
    #[error("A2A max hops exceeded: limit is {limit}")]
    MaxHopsExceeded {
        /// Configured hop limit (default 5).
        limit: usize,
    },
}

/// Result of a successful A2A delegation.
#[derive(Debug, Serialize, Deserialize)]
pub struct A2aDelegateResult {
    /// Identifier of the task executed by the Worker Agent.
    pub task_id: String,
    /// Name of the Worker Agent that handled the delegation.
    pub agent_name: String,
    /// Text output produced by the Worker Agent.
    pub output: String,
}

/// Structured HTTP response body for A2A errors.
///
/// Serialized as JSON in the 4xx/5xx responses of the A2A routes.
#[derive(Debug, Serialize)]
pub struct A2aErrorResponse {
    /// Human-readable error message.
    pub error: String,
    /// Requested skill (present for SkillNotFound and AmbiguousSkill).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_id: Option<String>,
    /// Available skills (present for SkillNotFound).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_skills: Option<Vec<String>>,
    /// Conflicting agents (present for AmbiguousSkill).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflicting_agents: Option<Vec<String>>,
}

impl A2aErrorResponse {
    /// Builds one from an [`A2aError`].
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

/// Validates an A2A delegation before submission to the router.
///
/// Applies two guardrails that agents cannot bypass:
/// - **Hop limit**: the current chain cannot exceed `max_hops` levels.
/// - **Cycle detection**: the target agent cannot already appear in the parent chain.
///
/// Returns the child chain (`parent_chain` + `current_agent`) to propagate into
/// the delegated task if validation succeeds.
///
/// # Arguments
/// - `parent_chain`: delegation chain of the current task (`AIPTask::delegation_chain`).
/// - `current_agent`: ID of the agent initiating the delegation.
/// - `target_agent`: ID of the delegation's target agent.
/// - `max_hops`: configurable limit (runtime default: 5).
pub fn validate_chain(
    parent_chain: &[AgentId],
    current_agent: &AgentId,
    target_agent: &AgentId,
    max_hops: usize,
) -> Result<Vec<AgentId>, A2aError> {
    if parent_chain.len() >= max_hops {
        return Err(A2aError::MaxHopsExceeded { limit: max_hops });
    }
    if parent_chain.contains(target_agent) || current_agent == target_agent {
        return Err(A2aError::CycleDetected {
            agent_id: target_agent.clone(),
        });
    }
    let mut child_chain = parent_chain.to_vec();
    child_chain.push(current_agent.clone());
    Ok(child_chain)
}

/// Resolves a `skill_id` within a list of registry entries.
///
/// Selects only agents with `supports_a2a = true` in state
/// [`ProcessState::Active`] or [`ProcessState::Degraded`].
///
/// Returns a reference to the target entry, or a structured [`A2aError`].
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

/// Type-erased A2A delegation function.
///
/// Accepts `(skill_id, input_payload, timeout_secs, parent_chain, current_agent)`
/// and returns a `Future` resolving to `Result<A2aDelegateResult, A2aError>`.
///
/// `parent_chain` is the current delegation chain of the calling task
/// (`AIPTask::delegation_chain`). `current_agent` is the ID of the agent initiating
/// the delegation. Validation (cycle + max_hops) is applied by [`delegate_inner`].
///
/// Built via [`make_delegate_fn`]. Allows injecting the delegation logic into
/// `RuntimeContext` without a generic over the execution backend.
pub type A2aDelegateFn = Arc<
    dyn Fn(
            String,
            serde_json::Value,
            u64,
            Vec<AgentId>,
            AgentId,
        ) -> Pin<Box<dyn Future<Output = Result<A2aDelegateResult, A2aError>> + Send>>
        + Send
        + Sync,
>;

/// Builds a concrete [`A2aDelegateFn`] from runtime handles.
///
/// The function resolves the `skill_id` to an agent, validates the delegation
/// chain, submits the task via the router, waits for completion on the EventBus,
/// and returns the structured result.
///
/// `max_hops` bounds the chain depth (runtime default: 5).
pub fn make_delegate_fn<B>(
    registry: AgentRegistryHandle,
    router: TaskRouterHandle<B>,
    event_bus: EventBusSender,
    max_hops: usize,
) -> A2aDelegateFn
where
    B: ExecutionBackend + Clone + Send + Sync + 'static,
{
    Arc::new(
        move |skill_id: String,
              input_payload: serde_json::Value,
              timeout_secs: u64,
              parent_chain: Vec<AgentId>,
              current_agent: AgentId| {
            let registry = registry.clone();
            let router = router.clone();
            let event_bus = event_bus.clone();
            Box::pin(async move {
                delegate_inner(DelegateInner {
                    registry: &registry,
                    router: &router,
                    event_bus: &event_bus,
                    skill_id: &skill_id,
                    input_payload,
                    timeout_secs,
                    parent_chain: &parent_chain,
                    current_agent: &current_agent,
                    max_hops,
                })
                .await
            })
        },
    )
}

/// Limite par défaut de hops dans la chaîne de délégation A2A (ADR-D7).
pub const DEFAULT_A2A_MAX_HOPS: usize = 5;

/// Arguments de [`delegate_inner`] : handles runtime + paramètres de la
/// délégation A2A (skill ciblé, payload, chaîne courante, garde-fous).
pub(crate) struct DelegateInner<'a, B: ExecutionBackend + Clone> {
    pub registry: &'a AgentRegistryHandle,
    pub router: &'a TaskRouterHandle<B>,
    pub event_bus: &'a EventBusSender,
    pub skill_id: &'a str,
    pub input_payload: serde_json::Value,
    pub timeout_secs: u64,
    pub parent_chain: &'a [AgentId],
    pub current_agent: &'a AgentId,
    pub max_hops: usize,
}

/// Logique de délégation A2A, appelable depuis le REST handler et depuis
/// la [`A2aDelegateFn`] type-erasée.
pub(crate) async fn delegate_inner<B: ExecutionBackend + Clone>(
    args: DelegateInner<'_, B>,
) -> Result<A2aDelegateResult, A2aError> {
    let DelegateInner {
        registry,
        router,
        event_bus,
        skill_id,
        input_payload,
        timeout_secs,
        parent_chain,
        current_agent,
        max_hops,
    } = args;
    // 1. Résoudre skill_id → agent depuis le registry.
    let entries = registry.list_agents().await?;
    let target = resolve_skill(&entries, skill_id)?;
    let agent_id = target.id.clone();
    let agent_name = target.manifest.name.clone();

    // 2. Valider la chaîne avant toute soumission (cycle + max_hops, ADR-D7).
    let child_chain = validate_chain(parent_chain, current_agent, &agent_id, max_hops)?;
    let agent_id = agent_id.to_string();

    info!(skill_id = %skill_id, agent = %agent_name, "A2A delegation initiated");

    // 3. S'abonner à l'EventBus avant de soumettre (évite une race condition).
    let mut event_rx = event_bus.subscribe();

    // 4. Construire l'input AIP depuis le payload JSON. Le `skill_id` est
    //    propagé séparément via `AIPTask.skill_id` (cf. submit_with_chain
    //    plus bas), les workers multi-skills le lisent via le décorateur
    //    `@skill` qui se charge du dispatch automatique.
    let input = AIPInput {
        parts: vec![AIPPart::Data(DataPart {
            data: input_payload,
        })],
    };

    // 5. Soumettre la tâche via le TaskRouter avec la chaîne étendue et le
    //    skill_id ciblé, propagé jusqu'à `AIPTask.skill_id` côté Python.
    let task_id = router
        .submit_with_chain(&agent_id, input, Some(skill_id.to_string()), child_chain)
        .await
        .map_err(|e| match e {
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
                        // The coordinator embeds the worker's structured error
                        // (`[CODE] message details={...}`) in `output` when the
                        // result is `Failed`. Surface it to the caller instead
                        // of a generic message so debugging is possible.
                        let reason = output
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| "worker agent reported failure".to_string());
                        Err(A2aError::WorkerFailed { reason })
                    };
                }
                Ok(RuntimeEvent::TaskCanceled {
                    task_id: canceled_id,
                }) if canceled_id.to_string() == task_id_str => {
                    return Err(A2aError::WorkerFailed {
                        reason: "worker agent task was canceled".to_string(),
                    });
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!(
                        task_id = %task_id_str,
                        lagged = n,
                        "A2A EventBus lagged - checking router output directly"
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
                    // Pas notre événement, continuer d'attendre.
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
                input_schema: None,
                examples: vec![],
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

    #[tokio::test]
    async fn test_a2a_rejects_cycle() {
        // GIVEN une chaîne contenant déjà agent_a
        let agent_a = AgentId::from("agent-a");
        let agent_b = AgentId::from("agent-b");
        let parent_chain = vec![agent_a.clone()];

        // WHEN agent_b tente de déléguer vers agent_a (déjà dans la chaîne)
        let result = validate_chain(&parent_chain, &agent_b, &agent_a, 5);

        // THEN A2aError::CycleDetected
        match result {
            Err(A2aError::CycleDetected { agent_id }) => {
                assert_eq!(agent_id, agent_a);
            }
            other => panic!("expected CycleDetected, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_a2a_rejects_max_hops() {
        // GIVEN une tâche avec delegation_chain = [a, b, c, d, e] (5 hops)
        let parent_chain: Vec<AgentId> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|s| AgentId::from(*s))
            .collect();
        let current = AgentId::from("e");
        let target = AgentId::from("f");

        // WHEN la délégation suivante est tentée avec max_hops = 5
        let result = validate_chain(&parent_chain, &current, &target, 5);

        // THEN A2aError::MaxHopsExceeded { limit: 5 }
        match result {
            Err(A2aError::MaxHopsExceeded { limit }) => {
                assert_eq!(limit, 5);
            }
            other => panic!("expected MaxHopsExceeded, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_a2a_validate_chain_extends_chain_on_success() {
        // GIVEN une chaîne courte sans conflit
        let parent_chain = vec![AgentId::from("a")];
        let current = AgentId::from("b");
        let target = AgentId::from("c");

        // WHEN la validation passe
        let result = validate_chain(&parent_chain, &current, &target, 5);

        // THEN la chaîne enfant contient parent + current_agent
        let child = result.expect("validation must succeed");
        assert_eq!(child.len(), 2);
        assert_eq!(child[0], AgentId::from("a"));
        assert_eq!(child[1], AgentId::from("b"));
    }

    #[tokio::test]
    async fn test_a2a_rejects_self_invocation() {
        // GIVEN agent_a tente de se déléguer à lui-même (chaîne vide)
        let agent_a = AgentId::from("agent-a");

        // WHEN
        let result = validate_chain(&[], &agent_a, &agent_a, 5);

        // THEN A2aError::CycleDetected (auto-invocation = cycle de longueur 0)
        assert!(matches!(result, Err(A2aError::CycleDetected { .. })));
    }
}
