//! The agent executor: the one place the built-in agent loop is built and run.
//!
//! An executor is built from an agent as resolved (its package definition,
//! then the user's overrides, then the model assigned to it) and runs one
//! exchange of that agent. The mission is the system prompt, the granted tools
//! are what the model is offered, the model is the backend each call tries
//! first, and the autonomy tier sets the budget, the verification and the
//! critic. The budget is capped by the runtime ceiling and never raised.
//!
//! The three context injections that principle 6 allows (the user persona
//! brief, the summaries of past sessions, the project context) belong to the
//! Apollia agent alone. An executor built for any other agent drops them, so
//! no other agent ever receives memory it did not ask for.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use apollia_core::{AutonomyConfig, AutonomyLevel, AutonomyLevelConfig, RunId, StepBudgetConfig};
use apollia_llm::{LlmRouter, ToolInvoker};
use apollia_mcp::tool_search::ToolIndexSnapshot;
use apollia_memory::user_memory::UserMemoryRepository;
use apollia_oria::budget::StepBudget;
use apollia_oria::verification::{CriticPass, VerificationLoop};
use apollia_tools::{ResolvedAgent, ToolRegistryHandle};
use tokio_util::sync::CancellationToken;

use crate::a2a::A2AInvoker;
use crate::chat::builtin_agent::{
    BuiltInChatAgent, BuiltInChatAgentDeps, ChatAgentResponse, PrefixChecker,
};
use crate::chat::manager::APOLLIA_CHAT_AGENT_ID;
use crate::chat::plan_actor::PlanHandle;
use crate::chat::todo_handle::TodoHandle;
use crate::chat::types::{
    ChatError, ChatMessage, InjectedInstruction, PendingChatApprovals, PlanPhase,
    ProjectContextProvider,
};
use crate::eventbus::EventBusSender;
use crate::hooks::executor::HookExecutor;

/// The agent as the executor reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunSpec {
    /// Identifier of the agent; the model is resolved under it.
    pub agent_id: String,
    /// What the agent is for, used as its system prompt. Empty leaves the
    /// built-in prompt of the autonomy tier.
    pub mission: String,
    /// Tools offered to the model.
    pub granted_tools: Vec<String>,
    /// Backend each call tries first; the default when absent or unreachable.
    pub model: Option<String>,
    /// Autonomy tier: budget, verification, critic, memory gate.
    pub autonomy: AutonomyLevel,
}

impl AgentRunSpec {
    /// Read a resolved agent: its mission, its granted tools, its model, its
    /// autonomy tier.
    // TEST-ONLY: no agent runs outside a conversation yet, and the one agent
    // that runs in it, Apollia, still reads its spec from the session.
    pub fn from_resolved(agent: &ResolvedAgent) -> Self {
        Self {
            agent_id: agent.id.clone(),
            mission: agent.mission.clone(),
            granted_tools: agent.access.clone(),
            model: agent.model.as_ref().map(|m| m.model_id.clone()),
            autonomy: agent.autonomy,
        }
    }

    /// Whether this is the Apollia agent, the only one the principle 6
    /// injections are made for.
    pub fn is_apollia(&self) -> bool {
        self.agent_id == APOLLIA_CHAT_AGENT_ID
    }
}

/// Where the project context block is read from.
pub struct ProjectContextSource {
    /// The project linked to the conversation.
    pub project_id: String,
    /// Builds the block for that project.
    pub provider: Arc<dyn ProjectContextProvider>,
}

/// The three injections principle 6 reserves to the Apollia agent. Handed to
/// [`AgentExecutor::new`], they are kept only when the agent is Apollia.
#[derive(Default)]
pub struct ApolliaContext {
    /// Memory the user persona brief is read from, at the tiers that inject
    /// memory.
    pub user_memory: Option<Arc<std::sync::Mutex<UserMemoryRepository>>>,
    /// Block summarising earlier, unrelated sessions.
    pub past_sessions: Option<String>,
    /// Project context to append to the system prompt.
    pub project: Option<ProjectContextSource>,
}

impl ApolliaContext {
    fn is_empty(&self) -> bool {
        self.user_memory.is_none() && self.past_sessions.is_none() && self.project.is_none()
    }
}

/// Live handles the loop runs on.
pub struct AgentExecutorDeps {
    /// Router of the model calls.
    pub llm_router: Arc<LlmRouter>,
    /// Registry the tool specs are resolved from.
    pub tool_registry: ToolRegistryHandle,
    /// Runs the tool calls.
    pub tool_invoker: Arc<dyn ToolInvoker>,
    /// Bus the loop emits on.
    pub event_bus: EventBusSender,
    /// Invoker of the A2A skills offered as tools.
    pub a2a_invoker: Option<Arc<A2AInvoker>>,
    /// Task list of the conversation, enabling `todo_write`.
    pub todo: Option<TodoHandle>,
    /// Plan store of the conversation, enabling the `plan_*` tools.
    pub plan: Option<PlanHandle>,
}

/// Settings of the conversation the run belongs to. The default is a run
/// outside any conversation: no workspace, no deferred tool index, plan mode
/// off, no hook, no injected instruction, no prefix rule.
#[derive(Default)]
pub struct SessionWiring {
    /// Working directory named in the prompt and used by the tools.
    pub workspace_path: Option<PathBuf>,
    /// Deferred MCP tool index; `None` offers every schema eagerly.
    pub mcp_index: Option<Vec<ToolIndexSnapshot>>,
    /// Largest `limit` the `tool_search` tool advertises.
    pub tool_search_limit: usize,
    /// Plan mode of the conversation.
    pub plan_mode: bool,
    /// Plan phase the conversation is in when the run starts.
    pub plan_phase: PlanPhase,
    /// Lifecycle hooks.
    pub hook_executor: Option<Arc<HookExecutor>>,
    /// Operator instruction queued while the conversation was paused.
    pub pending_injection: Option<InjectedInstruction>,
    /// Temperature of a turn that offers tools; the tuned default when absent.
    pub tool_turn_temperature: Option<f32>,
    /// Per-invocation permission rules consulted before a human approval.
    pub prefix_checker: Option<Arc<PrefixChecker>>,
}

/// One exchange to run.
pub struct RunInput<'a> {
    /// Conversation the exchange belongs to.
    pub session_id: &'a str,
    /// Identifier of the answer being produced.
    pub message_id: &'a str,
    /// Identifier of the run.
    pub run_id: &'a RunId,
    /// The request.
    pub user_message: &'a str,
    /// Earlier messages, without the request.
    pub history: &'a [ChatMessage],
    /// Tools that run without asking.
    pub authorized_tools: &'a HashSet<String>,
    /// Approvals waiting on the operator.
    pub pending_approvals: &'a PendingChatApprovals,
    /// Summary of the history that no longer fits the window.
    pub summary: Option<&'a str>,
    /// Number of recent messages kept in the window.
    pub context_window_size: usize,
}

/// Runs one agent with the built-in agent loop.
pub struct AgentExecutor {
    spec: AgentRunSpec,
    agent: BuiltInChatAgent,
    level_config: AutonomyLevelConfig,
    budget: StepBudget,
    verification: Option<VerificationLoop>,
    critic: Option<CriticPass>,
    cancel: CancellationToken,
    past_sessions: Option<String>,
    project: Option<ProjectContextSource>,
}

impl AgentExecutor {
    /// Build the executor of `spec`.
    ///
    /// The budget is the tier's, capped by `runtime_ceiling`; the verification
    /// loop and the critic exist only when the tier asks for verification.
    /// `apollia` is kept when the agent is Apollia and dropped otherwise.
    /// `cancel` stops the run at its next checkpoint.
    // REASON: each argument is a distinct input of the run: the agent, the
    // live handles, the Apollia injections, the conversation, the ceiling and
    // the cancellation token.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        spec: AgentRunSpec,
        deps: AgentExecutorDeps,
        apollia: ApolliaContext,
        wiring: SessionWiring,
        runtime_ceiling: &StepBudgetConfig,
        cancel: CancellationToken,
    ) -> Self {
        let level_config = AutonomyConfig::default().level_config(spec.autonomy);
        let budget = StepBudget::from_capped(&level_config.budget, runtime_ceiling);
        let (verification, critic) = if level_config.run_verification {
            (
                Some(VerificationLoop::new(Vec::new(), Vec::new())),
                Some(CriticPass::new(Arc::clone(&deps.llm_router))),
            )
        } else {
            (None, None)
        };
        let apollia = if spec.is_apollia() {
            apollia
        } else {
            if !apollia.is_empty() {
                tracing::event!(
                    tracing::Level::DEBUG,
                    agent = %spec.agent_id,
                    "agent.executor.apollia_context.dropped"
                );
            }
            ApolliaContext::default()
        };
        let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: deps.llm_router,
            tool_registry: deps.tool_registry,
            tool_invoker: deps.tool_invoker,
            event_bus: deps.event_bus,
            user_memory: apollia.user_memory,
            a2a_invoker: deps.a2a_invoker,
            todo: deps.todo,
            plan: deps.plan,
        })
        .with_agent_id(spec.agent_id.clone())
        .with_workspace_path(wiring.workspace_path)
        .with_mcp_index(wiring.mcp_index, wiring.tool_search_limit)
        .with_plan_mode(wiring.plan_mode)
        .with_plan_phase_start(wiring.plan_phase)
        .with_hook_executor(wiring.hook_executor)
        .with_pending_injection(wiring.pending_injection)
        .with_tool_turn_temperature(wiring.tool_turn_temperature)
        .with_prefix_checker(wiring.prefix_checker)
        .with_preferred_backend(spec.model.clone());
        Self {
            spec,
            agent,
            level_config,
            budget,
            verification,
            critic,
            cancel,
            past_sessions: apollia.past_sessions,
            project: apollia.project,
        }
    }

    /// The agent this executor runs.
    pub fn spec(&self) -> &AgentRunSpec {
        &self.spec
    }

    /// The tier settings in force.
    #[cfg(test)]
    pub(crate) fn level_config(&self) -> &AutonomyLevelConfig {
        &self.level_config
    }

    /// The budget of the run, already capped.
    #[cfg(test)]
    pub(crate) fn budget(&self) -> &StepBudget {
        &self.budget
    }

    /// Whether the run ends with a verification pass and a critic.
    #[cfg(test)]
    pub(crate) fn verifies(&self) -> bool {
        self.verification.is_some() && self.critic.is_some()
    }

    /// The mission, then the Apollia injections when the agent is Apollia:
    /// the past sessions block, then the project context block.
    pub async fn system_prompt(&self) -> String {
        let mut prompt = self.spec.mission.clone();
        if let Some(block) = &self.past_sessions {
            prompt.push_str("\n\n");
            prompt.push_str(block);
        }
        if let Some(source) = &self.project {
            if let Some(block) = source.provider.build_context(&source.project_id).await {
                prompt.push_str("\n\n");
                prompt.push_str(&block);
            }
        }
        prompt
    }

    /// Run one exchange to its end, its pause, or its budget.
    ///
    /// # Errors
    ///
    /// The errors of [`BuiltInChatAgent::execute`].
    pub async fn run(&self, input: RunInput<'_>) -> Result<ChatAgentResponse, ChatError> {
        let system_prompt = self.system_prompt().await;
        self.agent
            .execute(
                input.session_id,
                input.message_id,
                input.run_id,
                input.user_message,
                input.history,
                &system_prompt,
                &self.spec.granted_tools,
                input.authorized_tools,
                input.pending_approvals,
                &self.budget,
                input.summary,
                input.context_window_size,
                Some(&self.spec.autonomy),
                self.verification.as_ref(),
                self.critic.as_ref(),
                Some(&self.level_config),
                self.cancel.clone(),
            )
            .await
    }

    /// The built loop, for the tests that inspect it.
    #[cfg(test)]
    pub(crate) fn agent(&self) -> &BuiltInChatAgent {
        &self.agent
    }
}

#[cfg(test)]
mod tests;
