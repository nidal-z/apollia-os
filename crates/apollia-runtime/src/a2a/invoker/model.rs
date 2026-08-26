//! Wire shapes and error surface of the A2A invocation layer.
//!
//! The request and result types the invoker takes and returns, the agent card
//! it publishes, and the mapping from the delegation layer errors onto the
//! domain error callers see.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use apollia_core::{AIPPart, AIPResult, DataPart, TaskStatus};

use crate::registry::AgentEntry;

/// Reconstructs an [`AIPResult`] from the flattened text emitted by
/// `RuntimeEvent::TaskCompleted`.
///
/// The coordinator serialises [`AIPPart::Data`] payloads to JSON when the
/// agent returned a dict/list (cf. `coordinator::aip_result_to_text`).
/// We try to recover the structured shape here so A2A callers (CLI, agent
/// `ctx.a2a.invoke`) see the data part, not a text part wrapping JSON.
pub(super) fn build_aip_result_from_flattened_output(flattened: &str) -> AIPResult {
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

/// Converts an [`AgentEntry`] into an [`A2AAgentCard`].
pub(super) fn to_agent_card(entry: &AgentEntry) -> A2AAgentCard {
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
pub(super) fn map_delegate_err(
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
