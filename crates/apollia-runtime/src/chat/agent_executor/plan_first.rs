//! Plan first: the executor's mode for a task that reads as several steps.
//!
//! The agent writes the plan of the task before acting on it: a list of
//! steps, each a tool call or a model call, with the steps it depends on. The
//! plan is checked (unique steps, known dependencies, no cycle, no more steps
//! than the budget allows), then run level by level, the independent
//! read-only steps of a level together. The arguments of a tool step are
//! resolved against the tool's schema. A failed step is replanned, and so is
//! a result the verification refuses, at the tiers that verify. The plan is
//! kept in the plan cache, keyed on the agent as resolved and the task, so the
//! same task of the same agent runs it again without planning.
//!
//! The steps call tools the way the loop does: only the tools granted to the
//! agent, through the same approval, and an output too long for the context
//! kept whole on disk. Every model call of a step goes to the agent's model.
//! Nothing is written to memory on the agent's behalf, and there is no gate of
//! its own on the plan: in a conversation, approving a plan is the plan mode,
//! which plan first then leaves to the loop.
//!
//! The agent's planning mode decides when the mode engages: never, always, or
//! on its own judgement, when the task scores as a multi-step one on ORIA's
//! observer, the request alone or the task over the agent.

mod proxy;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use apollia_core::{
    AIPInput, AIPPart, AIPResult, AIPTask, AgentManifest, ORIAConfig, StepBudgetConfig, TaskStatus,
    TextPart,
};
use apollia_llm::types::TokenUsage;
use apollia_llm::{LlmRouter, ToolInvoker};
use apollia_oria::engine::{AIPAgent, ORIAEngine};
use apollia_oria::observer::{classify, orchestrated_threshold, score_turn_text, ExecutionMode};
use apollia_oria::plan_cache::PlanCacheRepository;
use apollia_tools::{PlanningMode, ToolRegistryHandle};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use super::{AgentRunSpec, RunInput};
use crate::chat::builtin_agent::ChatAgentResponse;
use crate::chat::types::ChatError;
use crate::eventbus::EventBusSender;

pub(super) use proxy::GrantedToolProxy;

/// The plan cache shared by the runtime.
pub type SharedPlanCache = Arc<Mutex<PlanCacheRepository>>;

/// Instructions of an agent that names no mission, so the planner always has
/// one: ORIA refuses to plan without them.
const DEFAULT_MISSION: &str = "Plan the steps that complete the task, then carry them out.";

/// Whether the run of `message` plans first.
///
/// Never in a conversation's plan mode, which is the plan approval of the
/// conversation. Otherwise as the agent's planning mode says: never, always,
/// or, on its own judgement, when the request reads as several steps on the
/// signal the plan mode of a conversation uses, or when ORIA's classification
/// of the task over the agent reads it as orchestrated.
pub(super) fn engages(
    spec: &AgentRunSpec,
    budget: &StepBudgetConfig,
    message: &str,
    plan_mode: bool,
) -> bool {
    if plan_mode {
        return false;
    }
    match spec.planning {
        PlanningMode::Never => false,
        PlanningMode::Always => true,
        PlanningMode::Auto => {
            let mut manifest = match manifest_of(spec, budget, &spec.mission) {
                Ok(manifest) => manifest,
                Err(_) => return false,
            };
            manifest.execution_mode = "auto".into();
            manifest.system_prompt = Some(spec.mission.clone());
            let threshold = orchestrated_threshold();
            let task_reads_as_steps = score_turn_text(message) >= threshold;
            task_reads_as_steps
                || matches!(
                    classify(&task_of("classify", message), &manifest, None, threshold),
                    ExecutionMode::Orchestrated
                )
        }
    }
}

/// What a plan-first run needs from the executor, kept when it is built.
pub(super) struct PlanFirstDeps {
    pub llm_router: Arc<LlmRouter>,
    pub tool_registry: ToolRegistryHandle,
    pub tool_invoker: Arc<dyn ToolInvoker>,
    pub event_bus: EventBusSender,
    pub plan_cache: Option<SharedPlanCache>,
    pub workspace_path: Option<PathBuf>,
    pub tool_output_dir: Option<PathBuf>,
    pub runtime_ceiling: StepBudgetConfig,
}

/// The run of one task in plan-first mode.
pub(super) struct PlanFirstRun<'a> {
    pub spec: &'a AgentRunSpec,
    pub deps: &'a PlanFirstDeps,
    pub budget: &'a StepBudgetConfig,
    pub system_prompt: String,
    pub cancel: CancellationToken,
}

impl PlanFirstRun<'_> {
    /// Plan, run, verify, and answer as the loop answers.
    ///
    /// # Errors
    ///
    /// [`ChatError::InternalError`] when the agent cannot be described to the
    /// planner, or when the plan fails: no plan could be read, a step failed
    /// past its replans, or the budget ran out.
    pub(super) async fn run(self, input: RunInput<'_>) -> Result<ChatAgentResponse, ChatError> {
        let manifest = manifest_of(self.spec, self.budget, &self.system_prompt)?;
        let proxy = Arc::new(
            GrantedToolProxy::new(proxy::ProxyWiring {
                spec: self.spec,
                deps: self.deps,
                session_id: input.session_id,
                message_id: input.message_id,
                authorized: input.authorized_tools,
                pending_approvals: input.pending_approvals,
            })
            .await,
        );
        let reasoner_model = self.deps.llm_router.route(self.spec.model.as_deref());
        let mut engine = ORIAEngine::new()
            .with_reasoner(reasoner_model, self.budget.max_steps)
            .with_llm_router(LlmRouter::clone(&self.deps.llm_router))
            .with_tool_proxy(Arc::clone(&proxy) as Arc<dyn apollia_oria::actor::ToolProxyTrait>)
            .with_event_bus(self.deps.event_bus.clone())
            .with_oria_config(ORIAConfig {
                autonomy_level: Some(self.spec.autonomy),
                ..ORIAConfig::default()
            })
            .with_runtime_budget(self.deps.runtime_ceiling.clone())
            .with_plan_gate_override(Some(false))
            .with_agent_name(self.spec.agent_id.clone());
        if let Some(cache) = &self.deps.plan_cache {
            engine = engine.with_shared_plan_cache(Arc::clone(cache));
        }
        if let Some(workspace) = &self.deps.workspace_path {
            engine = engine.with_cwd(workspace.clone());
        }
        tracing::event!(
            tracing::Level::INFO,
            agent = %self.spec.agent_id,
            task_id = %input.session_id,
            "agent.executor.plan_first.started"
        );
        let agent = PlannedAgent { manifest };
        let task = task_of(input.session_id, input.user_message);
        let result = tokio::select! {
            biased;
            () = self.cancel.cancelled() => None,
            result = engine.execute(task, &agent) => Some(result),
        };
        let paused = result.is_none();
        response_of(result, proxy.records(), paused)
    }
}

/// The agent as the plan-first engine reads it.
struct PlannedAgent {
    manifest: AgentManifest,
}

impl AIPAgent for PlannedAgent {
    fn manifest(&self) -> AgentManifest {
        self.manifest.clone()
    }
}

/// The version the plan cache keys a plan of `spec` under: its definition's
/// version, then a digest of its mission, so a new mission plans anew.
fn cache_version(spec: &AgentRunSpec) -> String {
    let digest = Sha256::digest(spec.mission.as_bytes());
    format!("{}+{}", spec.version, &hex::encode(digest)[..16])
}

/// `spec` described to ORIA: its identifier and cache version, its granted
/// tools, its budget, its mission as instructions, its model.
fn manifest_of(
    spec: &AgentRunSpec,
    budget: &StepBudgetConfig,
    system_prompt: &str,
) -> Result<AgentManifest, ChatError> {
    let instructions = if system_prompt.trim().is_empty() {
        DEFAULT_MISSION
    } else {
        system_prompt
    };
    serde_json::from_value(serde_json::json!({
        "name": spec.agent_id,
        "version": cache_version(spec),
        "description": spec.mission,
        "tools_required": spec.granted_tools,
        "step_budget": budget,
        "execution_mode": "orchestrated",
        "system_prompt": instructions,
        "llm_backend": spec.model,
    }))
    .map_err(|e| ChatError::InternalError(format!("cannot describe the agent to the planner: {e}")))
}

fn task_of(task_id: &str, message: &str) -> AIPTask {
    AIPTask {
        task_id: task_id.to_string(),
        input: AIPInput {
            parts: vec![AIPPart::Text(TextPart {
                text: message.to_string(),
            })],
        },
        ..AIPTask::default()
    }
}

/// The answer of a plan-first run, shaped as the loop's.
fn response_of(
    result: Option<AIPResult>,
    tool_calls: Vec<crate::chat::types::ToolCallRecord>,
    paused: bool,
) -> Result<ChatAgentResponse, ChatError> {
    let content = match result {
        None => String::new(),
        Some(result) if result.status == TaskStatus::Completed => text_of(&result),
        Some(result) => {
            let reason = result
                .error
                .map(|e| format!("{}: {}", e.code, e.message))
                .unwrap_or_else(|| format!("the plan ended {:?}", result.status));
            return Err(ChatError::InternalError(reason));
        }
    };
    Ok(ChatAgentResponse {
        content,
        tool_calls,
        newly_authorized: Vec::new(),
        tokens_used: TokenUsage::default(),
        thinking_trace: None,
        reasoning_boundaries: Vec::new(),
        verification_report: None,
        frontier_ceiling_reached: false,
        final_plan_phase: None,
        paused,
        context_window_tokens: None,
        context_tokens_used: 0,
    })
}

fn text_of(result: &AIPResult) -> String {
    result
        .output
        .iter()
        .filter_map(|part| match part {
            AIPPart::Text(text) => Some(text.text.clone()),
            AIPPart::Data(data) => serde_json::to_string(&data.data).ok(),
            AIPPart::File(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every tool `spec` may call, for the proxy's grant check.
pub(super) fn granted_set(spec: &AgentRunSpec) -> HashSet<String> {
    spec.granted_tools.iter().cloned().collect()
}

#[cfg(test)]
mod tests;
