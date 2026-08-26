//! A2AInvoker, high-level orchestrator for inter-agent invocations by skill ID.
//!
//! Handles the full lifecycle of an A2A invocation:
//! skill resolution (`Active` state required), runtime event emission,
//! delegation to the TaskRouter with a timeout, and construction of the
//! structured result.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::info;

use apollia_core::{
    A2AConfig, AIPPart, AIPResult, DataPart, ProcessState, RuntimeEvent, TaskStatus,
};

use crate::a2a::telemetry::{make_excerpt, A2AStepProvenance, InvocationRecord, TelemetryHandle};
use crate::a2a::{check_compatibility, make_delegate_fn, A2aDelegateFn, DEFAULT_A2A_MAX_HOPS};
use crate::coordinator::ExecutionBackend;
use crate::eventbus::EventBusSender;
use crate::registry::{AgentEntry, AgentRegistryHandle};
use crate::router::TaskRouterHandle;

/// Reconstructs an [`AIPResult`] from the flattened text emitted by
/// `RuntimeEvent::TaskCompleted`.
///
/// The coordinator serialises [`AIPPart::Data`] payloads to JSON when the
/// agent returned a dict/list (cf. `coordinator::aip_result_to_text`).
/// We try to recover the structured shape here so A2A callers (CLI, agent
/// `ctx.a2a.invoke`) see the data part, not a text part wrapping JSON.
fn build_aip_result_from_flattened_output(flattened: &str) -> AIPResult {
    let trimmed = flattened.trim();
    if !trimmed.is_empty() {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if value.is_object() || value.is_array() {
                return AIPResult {
                    task_id: String::new(),
                    status: TaskStatus::Completed,
                    output: vec![AIPPart::Data(DataPart { data: value })],
                    error: None,
                    artifacts: Vec::new(),
                    input_required_data: None,
                };
            }
        }
    }
    AIPResult::completed(flattened)
}

/// Execution context configuration for an agent invoked via A2A.
///
/// Produced by [`A2AInvoker::build_a2a_context`] and consumed by the runtime
/// when building the PyO3 [`RuntimeContext`] for the delegated task.
///
/// Reading the global `__user__` namespace is now unconditional (always active
/// as soon as a `user_manager` is provided to the `MemoryInterface`). This
/// config only controls *writes* to `__user__`, which are reserved for agents
/// whose manifest declares `user_memory_write = true`.
#[derive(Debug, Clone)]
pub struct RuntimeContextConfig {
    /// If `true`, the agent may write to the `__user__` namespace via
    /// `ctx.memory.remember_user()`. Defaults to `false`: A2A invocations
    /// never grant this right, the manifest decides.
    pub user_memory_writable: bool,
    /// Hop limit for the A2A delegation chain.
    ///
    /// `None` falls back to the runtime default (5). Optional field with a
    /// default of 5, not persisted; configurable later via the
    /// `system.db runtime_config` table.
    pub a2a_max_hops: Option<usize>,
}

/// Structured errors returned by [`A2AInvoker`].
///
/// A domain-oriented error surface, distinct from the low-level
/// [`crate::a2a::A2aError`] that covers the delegation layer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum A2AError {
    /// No available A2A agent declares the requested skill.
    #[error("skill '{skill_id}' not found - available: {available:?}")]
    SkillNotFound {
        /// Identifier of the requested skill.
        skill_id: String,
        /// Skill IDs available across active or degraded A2A agents.
        available: Vec<String>,
    },

    /// An agent declares the skill but is not in the `Active` state.
    ///
    /// Only the `Active` state is accepted for invocation (fail-fast).
    #[error("agent '{agent_name}' is not active (state: {state})")]
    AgentNotActive {
        /// Name of the target agent.
        agent_name: String,
        /// Current state of the agent (e.g. `"Degraded"`, `"Stopping"`).
        state: String,
    },

    /// The A2A invocation timed out before the Worker Agent responded.
    #[error(
        "A2A invocation timed out after {timeout_secs}s (skill: {skill_id}, agent: {agent_name})"
    )]
    Timeout {
        /// Identifier of the invoked skill.
        skill_id: String,
        /// Name of the target agent.
        agent_name: String,
        /// Configured timeout in seconds.
        timeout_secs: u64,
    },

    /// The Worker Agent returned a failure result.
    #[error("agent '{agent_name}' execution failed: {message}")]
    ExecutionFailed {
        /// Name of the target agent.
        agent_name: String,
        /// Reason for the failure.
        message: String,
    },

    /// Communication error with the registry or the router.
    #[error("A2A infrastructure error: {0}")]
    RegistryError(String),

    /// The maximum A2A recursion depth was reached.
    ///
    /// Enforced by the runtime before skill resolution. Guards against
    /// infinite recursive chains between agents.
    #[error("a2a max depth {max_depth} exceeded (current: {current_depth}, caller: {caller}, skill: {skill_id})")]
    MaxDepthExceeded {
        /// Current depth of the invocation.
        current_depth: u32,
        /// Configured maximum depth.
        max_depth: u32,
        /// Name of the initiating agent.
        caller: String,
        /// Identifier of the requested skill.
        skill_id: String,
    },

    /// An agent tries to invoke itself via an A2A skill.
    ///
    /// Enforced after target skill resolution. Prevents direct loops where an
    /// agent exposes a skill and then invokes it on itself.
    #[error("agent '{agent_name}' cannot invoke itself via skill '{skill_id}'")]
    SelfInvocation {
        /// Name of the agent attempting self-invocation.
        agent_name: String,
        /// Identifier of the target skill.
        skill_id: String,
    },

    /// The cumulative A2A chain timeout was exceeded.
    ///
    /// Triggered either immediately if the `chain_deadline` is already expired
    /// on entry to `invoke()`, or after delegation when it is the
    /// `chain_deadline` (and not the `invocation_timeout`) that expired first.
    #[error("a2a chain timeout exceeded (caller: {caller}, skill: {skill_id})")]
    ChainTimeoutExceeded {
        /// Name of the initiating agent.
        caller: String,
        /// Identifier of the requested skill.
        skill_id: String,
    },
}

/// Result of a successful A2A invocation.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct A2AInvocationResult {
    /// AIP result returned by the Worker Agent.
    #[schema(value_type = Object)]
    pub result: AIPResult,
    /// Name of the Worker Agent that handled the invocation.
    pub agent_name: String,
    /// Identifier of the invoked skill.
    pub skill_id: String,
    /// Total invocation duration in milliseconds.
    pub duration_ms: u64,
}

/// Discovery information for an A2A skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2ASkillInfo {
    /// Unique skill identifier (e.g. `"read-excel"`).
    pub id: String,
    /// Human-readable skill name.
    pub name: String,
    /// Description of what the skill does.
    pub description: String,
    /// Supported input modes (e.g. `["text", "data"]`).
    pub input_modes: Vec<String>,
    /// Supported output modes (e.g. `["text", "file"]`).
    pub output_modes: Vec<String>,
    /// Apollia schema for the payload fields (cf. `AgentSkill::input_schema`).
    #[serde(default)]
    pub input_schema: Option<serde_json::Value>,
    /// Examples of valid payloads, propagated to the LLM-facing tool descriptor
    /// (cf. `AgentSkill::examples`). Empty by default.
    #[serde(default)]
    pub examples: Vec<serde_json::Value>,
}

/// Discovery card for an A2A agent.
///
/// Returned by [`A2AInvoker::discover`] and [`A2AInvoker::list_agent_cards`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2AAgentCard {
    /// Unique agent name.
    pub name: String,
    /// Agent semver version.
    pub version: String,
    /// Agent description.
    pub description: String,
    /// Skills declared by this agent.
    pub skills: Vec<A2ASkillInfo>,
    /// Tags associated with this agent.
    pub tags: Vec<String>,
}

/// Entry in the list of available skills.
///
/// Returned by [`A2AInvoker::list_skills`] and used by `ctx.a2a_list_skills()`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkillListing {
    /// Skill identifier.
    pub skill_id: String,
    /// Name of the agent that provides this skill.
    pub agent_name: String,
    /// Human-readable skill name.
    pub skill_name: String,
    /// Skill description.
    pub description: String,
    /// Apollia schema for the payload fields (cf. `AgentSkill::input_schema`).
    /// Used by `generate_a2a_tool_specs` to expose the worker's real contract
    /// to the LLM (instead of a generic schema).
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub input_schema: Option<serde_json::Value>,
}

/// Parameters of an A2A invocation via [`A2AInvoker::invoke`]: target skill,
/// payload, caller, and depth / chain-timeout guards.
pub struct A2AInvokeRequest<'a> {
    pub skill_id: &'a str,
    pub input: serde_json::Value,
    pub caller: &'a str,
    pub a2a_depth: u32,
    pub timeout: Option<Duration>,
    pub chain_deadline: Option<Instant>,
}

/// High-level orchestrator for inter-agent invocations by skill ID.
///
/// Orchestrates the full lifecycle of an A2A invocation:
/// 1. Apply the guards (depth, self-invocation, chain timeout)
/// 2. Resolve `skill_id` to an agent (`Active` state required)
/// 3. Emit [`RuntimeEvent::A2AInvocationStarted`]
/// 4. Delegate via the TaskRouter with the effective timeout
/// 5. Emit [`RuntimeEvent::A2AInvocationCompleted`]
/// 6. Build the [`A2AInvocationResult`]
///
/// Not a Tokio actor: a clonable struct holding internal handles.
#[derive(Clone)]
pub struct A2AInvoker {
    registry: AgentRegistryHandle,
    delegate_fn: A2aDelegateFn,
    event_bus: EventBusSender,
    /// Configuration of the guards applied to every invocation.
    config: A2AConfig,
    /// Sidechain logger, `None` if the SQLite database is unavailable.
    sidechain_logger: Option<crate::a2a::sidechain::SidechainLogger>,
    /// A2A telemetry store, `None` if per-skill observability is disabled.
    telemetry: Option<TelemetryHandle>,
}

impl A2AInvoker {
    /// Builds an `A2AInvoker` from the runtime handles and the A2A config.
    ///
    /// Generic over `B: ExecutionBackend`; the result is non-generic thanks to
    /// the type erasure performed by [`make_delegate_fn`].
    pub fn new<B>(
        registry: AgentRegistryHandle,
        router: TaskRouterHandle<B>,
        event_bus: EventBusSender,
        config: A2AConfig,
    ) -> Self
    where
        B: ExecutionBackend + Clone + Send + Sync + 'static,
    {
        let delegate_fn = make_delegate_fn(
            registry.clone(),
            router,
            event_bus.clone(),
            DEFAULT_A2A_MAX_HOPS,
        );
        Self {
            registry,
            delegate_fn,
            event_bus,
            config,
            sidechain_logger: None,
            telemetry: None,
        }
    }

    /// Attaches a [`SidechainLogger`] to this invoker for delegation traceability.
    ///
    /// Returns `self` for use in a builder chain.
    pub fn with_sidechain_logger(mut self, logger: crate::a2a::sidechain::SidechainLogger) -> Self {
        self.sidechain_logger = Some(logger);
        self
    }

    /// Returns the attached [`TelemetryHandle`], if any.
    pub fn telemetry(&self) -> Option<&TelemetryHandle> {
        self.telemetry.as_ref()
    }

    /// Returns the attached [`SidechainLogger`], if any.
    pub fn sidechain_logger(&self) -> Option<&crate::a2a::sidechain::SidechainLogger> {
        self.sidechain_logger.as_ref()
    }

    /// Invokes a Worker Agent by its `skill_id`.
    ///
    /// Applies the guards in this order before any delegation:
    /// 1. Recursion depth (`a2a_depth >= config.max_depth` -> [`A2AError::MaxDepthExceeded`])
    /// 2. Expired chain timeout (`chain_deadline` already passed -> [`A2AError::ChainTimeoutExceeded`])
    /// 3. Self-invocation (caller == target agent -> [`A2AError::SelfInvocation`])
    ///
    /// Then resolves the skill, validates the `Active` state, delegates via the
    /// TaskRouter, and returns an enriched [`A2AInvocationResult`].
    ///
    /// The effective `timeout` is the minimum of `timeout` and the remaining
    /// delay of `chain_deadline`. If the timeout comes from `chain_deadline`,
    /// the error is [`A2AError::ChainTimeoutExceeded`] rather than
    /// [`A2AError::Timeout`].
    ///
    /// # Arguments
    ///
    /// - `a2a_depth`: current depth of the chain (0 for the root invocation).
    /// - `chain_deadline`: cumulative chain deadline; `None` on the first
    ///   invocation, initialized to `now + chain_timeout_secs` and propagated after.
    ///
    /// # Errors
    ///
    /// - [`A2AError::MaxDepthExceeded`] if `a2a_depth >= config.max_depth`.
    /// - [`A2AError::ChainTimeoutExceeded`] if the chain deadline is expired.
    /// - [`A2AError::SelfInvocation`] if the agent tries to invoke itself.
    /// - [`A2AError::SkillNotFound`] if no available A2A agent declares the skill.
    /// - [`A2AError::AgentNotActive`] if the target agent is not in the `Active` state.
    /// - [`A2AError::Timeout`] if execution exceeds the invocation timeout.
    /// - [`A2AError::ExecutionFailed`] if the Worker Agent returns a failure.
    /// - [`A2AError::RegistryError`] on a communication error with the registry.
    pub async fn invoke(
        &self,
        request: A2AInvokeRequest<'_>,
    ) -> Result<A2AInvocationResult, A2AError> {
        let A2AInvokeRequest {
            skill_id,
            input,
            caller,
            a2a_depth,
            timeout,
            chain_deadline,
        } = request;
        // Guard 1: recursion depth.
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

        // Guard 2: cumulative chain timeout.
        // Initialize the deadline on the first invocation of the chain.
        let effective_deadline = chain_deadline.unwrap_or_else(|| {
            Instant::now() + Duration::from_secs(self.config.chain_timeout_secs)
        });

        let chain_remaining = effective_deadline.checked_duration_since(Instant::now());

        let (effective_timeout_secs, governed_by_chain) = match chain_remaining {
            None => {
                // Deadline already expired before we even start.
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
                    // The chain_deadline expires before the invocation_timeout.
                    (remaining.as_secs().max(1), true)
                } else {
                    (invocation_timeout.as_secs(), false)
                }
            }
        };

        // Skill resolution.
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

        // Guard 3: self-invocation.
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

        // Step-level telemetry & provenance.
        let step_id = format!(
            "a2a-{skill_id}-{}-{}",
            agent_name,
            uuid::Uuid::new_v4().simple()
        );
        let input_excerpt = make_excerpt(&input.to_string());
        let worker_version = target.manifest.version.clone();
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let _ = self.event_bus.send(RuntimeEvent::A2ASkillInvoked {
            step_id: step_id.clone(),
            skill_id: skill_id.to_string(),
            agent_name: agent_name.clone(),
            version: worker_version.clone(),
            input_excerpt: input_excerpt.clone(),
            caller: caller.to_string(),
            parent_step: None,
        });

        let start = Instant::now();

        let delegate_result = (self.delegate_fn)(
            skill_id.to_string(),
            input,
            effective_timeout_secs,
            Vec::new(),
            apollia_core::AgentId::from(caller),
        )
        .await;
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

        // Step telemetry / provenance emission.
        let (success, output_excerpt) = match &delegate_result {
            Ok(r) => (true, Some(make_excerpt(&r.output))),
            Err(_) => (false, None),
        };
        let _ = self.event_bus.send(RuntimeEvent::A2ASkillCompleted {
            step_id: step_id.clone(),
            skill_id: skill_id.to_string(),
            agent_name: agent_name.clone(),
            duration_ms,
            success,
            tokens_delta: 0,
            output_excerpt: output_excerpt.clone(),
        });
        if let Some(telemetry) = &self.telemetry {
            telemetry
                .record_invocation(
                    &agent_name,
                    skill_id,
                    &worker_version,
                    InvocationRecord {
                        duration_ms,
                        success,
                        tokens: 0,
                        timestamp_ms,
                    },
                )
                .await;
            telemetry
                .record_step(A2AStepProvenance {
                    step_id: step_id.clone(),
                    input_excerpt: input_excerpt.clone(),
                    output_excerpt,
                    agent_from: caller.to_string(),
                    agent_to: agent_name.clone(),
                    parent_step: None,
                    skill_id: skill_id.to_string(),
                    timestamp_ms,
                })
                .await;
        }

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

        let aip_result = build_aip_result_from_flattened_output(&delegate.output);

        Ok(A2AInvocationResult {
            result: aip_result,
            agent_name: delegate.agent_name,
            skill_id: skill_id.to_string(),
            duration_ms,
        })
    }

    /// Discovers the agent that exposes `skill_id` and returns its discovery card.
    ///
    /// Searches agents with `supports_a2a = true` in `Active` or `Degraded` state.
    /// Returns `None` if no available agent declares this skill.
    /// Returns whether an agent named `name` is registered.
    ///
    /// Registry-backed existence check used by the mailbox (`ctx.mail.send`) to
    /// fail-fast on an unknown recipient instead of silently enqueuing a message
    /// that would only expire via TTL. Any registry communication error resolves
    /// to `false` (treated as absent).
    pub async fn agent_exists(&self, name: &str) -> bool {
        self.registry
            .find_by_name(name)
            .await
            .ok()
            .flatten()
            .is_some()
    }

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

    /// Lists all discovery cards for available A2A agents.
    ///
    /// Includes agents in `Active` or `Degraded` state with `supports_a2a = true`.
    /// The list is sorted by agent name.
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

    /// Lists every available skill across all A2A cards.
    ///
    /// Returns a flat list of [`SkillListing`], sorted by `skill_id`.
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
                    input_schema: s.input_schema.clone(),
                })
            })
            .collect();

        skills.sort_by(|a, b| a.skill_id.cmp(&b.skill_id));
        Ok(skills)
    }

    /// Checks semver compatibility between `required_version` and the version
    /// advertised by the Worker that provides `skill_id`.
    ///
    /// Emits a [`RuntimeEvent::A2ACompatibilityWarning`] on the EventBus if a
    /// mismatch is detected. Returns `Ok(None)` if the versions are compatible.
    pub async fn check_skill_compatibility(
        &self,
        skill_id: &str,
        required_version: &str,
    ) -> Result<Option<crate::a2a::A2ACompatibilityWarning>, A2AError> {
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

        let target = pool
            .iter()
            .find(|e| e.manifest.skills.iter().any(|s| s.id == skill_id));
        let Some(target) = target else {
            return Ok(None);
        };

        let alternatives: Vec<(String, String)> = pool
            .iter()
            .filter(|e| {
                e.manifest.name != target.manifest.name
                    && e.manifest.skills.iter().any(|s| s.id == skill_id)
            })
            .map(|e| (e.manifest.name.clone(), e.manifest.version.clone()))
            .collect();

        let Some(warning) = check_compatibility(
            skill_id,
            &target.manifest.name,
            required_version,
            &target.manifest.version,
        ) else {
            return Ok(None);
        };

        let enriched = crate::a2a::compatibility::with_alternative(warning, &alternatives);

        let severity_str = match enriched.severity {
            crate::a2a::CompatSeverity::Warning => "warning",
            crate::a2a::CompatSeverity::Incompatible => "incompatible",
        };
        let _ = self.event_bus.send(RuntimeEvent::A2ACompatibilityWarning {
            skill_id: enriched.skill_id.clone(),
            agent_name: enriched.agent_name.clone(),
            required_version: enriched.required_version.clone(),
            advertised_version: enriched.advertised_version.clone(),
            severity: severity_str.to_string(),
            message: enriched.message.clone(),
            alternative_agent: enriched.alternative_agent.clone(),
        });

        Ok(Some(enriched))
    }

    /// Builds the execution context configuration for an agent invoked via A2A.
    ///
    /// Reading the `__user__` namespace is handled directly by the
    /// `MemoryInterface` (always active as soon as a `user_manager` is provided)
    /// and no longer needs to be encoded in this config. Writes stay forbidden
    /// by default and are allowed only when the manifest declares
    /// `user_memory_write = true`.
    pub fn build_a2a_context(&self) -> RuntimeContextConfig {
        RuntimeContextConfig {
            user_memory_writable: false,
            a2a_max_hops: None,
        }
    }

    /// Test constructor: injects a custom `A2aDelegateFn` and a config.
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
            telemetry: None,
        }
    }
}

/// Converts an [`AgentEntry`] into an [`A2AAgentCard`].
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
            input_schema: s.input_schema.clone(),
            examples: s.examples.clone(),
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

/// Maps a [`crate::a2a::A2aError`] onto a high-level [`A2AError`].
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
                input_schema: None,
                examples: vec![],
            })
            .collect();

        AgentManifest {
            format_version: 1,
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

    fn make_never_called_delegate() -> A2aDelegateFn {
        Arc::new(
            |_skill_id: String,
             _input: serde_json::Value,
             _timeout: u64,
             _chain: Vec<apollia_core::AgentId>,
             _caller: apollia_core::AgentId| {
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
            move |_skill_id: String,
                  _input: serde_json::Value,
                  _timeout: u64,
                  _chain: Vec<apollia_core::AgentId>,
                  _caller: apollia_core::AgentId| {
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
            |_skill_id: String,
             _input: serde_json::Value,
             _timeout: u64,
             _chain: Vec<apollia_core::AgentId>,
             _caller: apollia_core::AgentId| {
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

    // Pure function tests.

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
            input_schema: None,
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
                input_schema: None,
                examples: vec![],
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

    // Registry-based async tests.

    #[tokio::test]
    async fn test_invoke_unknown_skill_returns_skill_not_found_with_available() {
        // GIVEN excel-worker Active with "read-excel", invoke for "unknown-skill"
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
            .invoke(A2AInvokeRequest {
                skill_id: "unknown-skill",
                input: serde_json::json!({}),
                caller: "director",
                a2a_depth: 0,
                timeout: None,
                chain_deadline: None,
            })
            .await;

        // THEN Err(SkillNotFound) with available containing "read-excel" and "edit-excel"
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
    async fn test_agent_exists_reflects_registry() {
        // GIVEN a registry with one registered agent
        let (bus_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(bus_tx.clone());
        registry
            .register(make_a2a_manifest("excel-worker", &["read-excel"]))
            .await
            .expect("register failed");
        let invoker = A2AInvoker::new_for_test(
            registry,
            make_never_called_delegate(),
            bus_tx,
            A2AConfig::default(),
        );

        // WHEN/THEN a registered name exists; an unknown one does not
        assert!(invoker.agent_exists("excel-worker").await);
        assert!(!invoker.agent_exists("ghost").await);
    }

    #[tokio::test]
    async fn test_invoke_degraded_agent_returns_not_active() {
        // GIVEN excel-worker Degraded with "read-excel"
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
            .invoke(A2AInvokeRequest {
                skill_id: "read-excel",
                input: serde_json::json!({}),
                caller: "director",
                a2a_depth: 0,
                timeout: None,
                chain_deadline: None,
            })
            .await;

        // THEN Err(AgentNotActive) with state == "Degraded"
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
        // GIVEN excel-worker Active, delegate returns Ok
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
            .invoke(A2AInvokeRequest {
                skill_id: "read-excel",
                input: serde_json::json!({"text": "Lis ventes.xlsx"}),
                caller: "director",
                a2a_depth: 0,
                timeout: None,
                chain_deadline: None,
            })
            .await
            .expect("invoke failed");

        // THEN result is correct
        assert_eq!(result.skill_id, "read-excel");
        assert_eq!(result.agent_name, "excel-worker");
        assert!(
            result.duration_ms < 5000,
            "duration: {}ms",
            result.duration_ms
        );
        assert_eq!(result.result.status, apollia_core::TaskStatus::Completed);

        // THEN A2AInvocationStarted emitted
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
        // GIVEN excel-worker Active, delegate returns Timeout
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
            .invoke(A2AInvokeRequest {
                skill_id: "read-excel",
                input: serde_json::json!({}),
                caller: "director",
                a2a_depth: 0,
                timeout: Some(Duration::from_secs(1)),
                chain_deadline: None,
            })
            .await;

        // THEN Err(Timeout)
        assert!(
            matches!(result, Err(A2AError::Timeout { .. })),
            "expected Timeout, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_discover_returns_agent_card() {
        // GIVEN excel-worker registered and Active
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

        // THEN card is correct
        assert_eq!(card.name, "excel-worker");
        assert_eq!(card.skills.len(), 2);
    }

    #[tokio::test]
    async fn test_discover_unknown_skill_returns_none() {
        // GIVEN empty registry
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
        // GIVEN 2 Active A2A agents (zebra-worker before alpha-worker in insertion order)
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

        // THEN sorted by name, alpha-worker first
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].name, "alpha-worker");
        assert_eq!(cards[1].name, "zebra-worker");
    }

    #[tokio::test]
    async fn test_list_skills_aggregates_all_a2a_skills() {
        // GIVEN 2 A2A agents with distinct skills
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

        // THEN 3 skills sorted by skill_id
        assert_eq!(skills.len(), 3);
        assert_eq!(skills[0].skill_id, "edit-excel");
        assert_eq!(skills[1].skill_id, "read-csv");
        assert_eq!(skills[2].skill_id, "read-excel");
    }
}

// A2A guards.
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
                input_schema: None,
                examples: vec![],
            })
            .collect();

        AgentManifest {
            format_version: 1,
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

    fn make_ok_delegate() -> A2aDelegateFn {
        Arc::new(
            move |_skill_id: String,
                  _input: serde_json::Value,
                  _timeout: u64,
                  _chain: Vec<apollia_core::AgentId>,
                  _caller: apollia_core::AgentId| {
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
        // GIVEN A2AInvoker with max_depth = 2, a2a_depth = 2
        let config = A2AConfig {
            max_depth: 2,
            ..A2AConfig::default()
        };
        let (invoker, _) =
            make_active_invoker_with_config("excel-worker", &["read-excel"], config).await;

        // WHEN invoke with a2a_depth = 2 (= max_depth)
        let result = invoker
            .invoke(A2AInvokeRequest {
                skill_id: "read-excel",
                input: serde_json::json!({}),
                caller: "director",
                a2a_depth: 2,
                timeout: None,
                chain_deadline: None,
            })
            .await;

        // THEN MaxDepthExceeded with the correct fields
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

        // WHEN invoke with a2a_depth = 1 (< max_depth)
        let result = invoker
            .invoke(A2AInvokeRequest {
                skill_id: "read-excel",
                input: serde_json::json!({}),
                caller: "director",
                a2a_depth: 1,
                timeout: None,
                chain_deadline: None,
            })
            .await;

        // THEN no depth error, invocation succeeds
        assert!(
            result.is_ok(),
            "depth 1 < max_depth 3 should succeed, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_self_invocation_blocked() {
        // GIVEN excel-worker registered with skill "read-excel"
        let (invoker, _) =
            make_active_invoker_with_config("excel-worker", &["read-excel"], A2AConfig::default())
                .await;

        // WHEN excel-worker invokes itself via the skill "read-excel"
        let result = invoker
            .invoke(A2AInvokeRequest {
                skill_id: "read-excel",
                input: serde_json::json!({}),
                caller: "excel-worker",
                a2a_depth: 0,
                timeout: None,
                chain_deadline: None,
            })
            .await;

        // THEN SelfInvocation with the correct fields
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
        // GIVEN chain_deadline = None (first invocation of the chain)
        let (invoker, _) =
            make_active_invoker_with_config("excel-worker", &["read-excel"], A2AConfig::default())
                .await;

        // WHEN invoke with chain_deadline = None
        let result = invoker
            .invoke(A2AInvokeRequest {
                skill_id: "read-excel",
                input: serde_json::json!({}),
                caller: "director",
                a2a_depth: 0,
                timeout: None,
                chain_deadline: None,
            })
            .await;

        // THEN the deadline is initialized in the future, no ChainTimeoutExceeded
        assert!(
            result.is_ok(),
            "first call with chain_deadline=None must succeed (deadline initialized to future), got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_chain_timeout_exceeded() {
        // GIVEN chain_deadline in the past (expired 1 second ago)
        let past_deadline = Instant::now() - Duration::from_secs(1);
        let (invoker, _) =
            make_active_invoker_with_config("excel-worker", &["read-excel"], A2AConfig::default())
                .await;

        // WHEN invoke with expired chain_deadline
        let result = invoker
            .invoke(A2AInvokeRequest {
                skill_id: "read-excel",
                input: serde_json::json!({}),
                caller: "director",
                a2a_depth: 0,
                timeout: None,
                chain_deadline: Some(past_deadline),
            })
            .await;

        // THEN immediate ChainTimeoutExceeded
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
        // GIVEN EventBus receiver and max_depth = 1
        let config = A2AConfig {
            max_depth: 1,
            ..A2AConfig::default()
        };
        let (invoker, mut bus_rx) =
            make_active_invoker_with_config("excel-worker", &["read-excel"], config).await;

        // WHEN invoke with a2a_depth = max_depth
        let _ = invoker
            .invoke(A2AInvokeRequest {
                skill_id: "read-excel",
                input: serde_json::json!({}),
                caller: "director",
                a2a_depth: 1,
                timeout: None,
                chain_deadline: None,
            })
            .await;

        // THEN A2AGuardTriggered { guard_type: "max_depth" } emitted
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
        // GIVEN A2AConfig deserialized from empty JSON (all default values)
        let config: A2AConfig =
            serde_json::from_str("{}").expect("deserialization of empty object failed");

        // THEN sane default values
        assert_eq!(config.max_depth, 3);
        assert_eq!(config.invocation_timeout_secs, 120);
        assert_eq!(config.chain_timeout_secs, 300);
    }

    #[test]
    fn test_a2a_config_round_trip() {
        // GIVEN a config with custom values
        let config = A2AConfig {
            max_depth: 5,
            invocation_timeout_secs: 60,
            chain_timeout_secs: 600,
        };

        // WHEN serialized then deserialized
        let json = serde_json::to_string(&config).expect("serialization failed");
        let restored: A2AConfig = serde_json::from_str(&json).expect("deserialization failed");

        // THEN values are preserved
        assert_eq!(restored.max_depth, 5);
        assert_eq!(restored.invocation_timeout_secs, 60);
        assert_eq!(restored.chain_timeout_secs, 600);
    }
}
