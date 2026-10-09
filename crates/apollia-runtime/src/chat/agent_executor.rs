//! The agent executor: the one place the built-in agent loop is built and run.
//!
//! An executor is built from an agent as resolved (its package definition,
//! then the user's overrides, then the model assigned to it) and runs one
//! exchange of that agent. The mission is the system prompt, the granted tools
//! are what the model is offered, the model is the backend each call tries
//! first, and the autonomy tier sets the budget, the verification and the
//! critic. The budget is capped by the runtime ceiling and never raised.
//!
//! A task that reads as several steps may be planned first (see
//! [`plan_first`]): the agent's planning mode says when.
//!
//! The agents and workflows the agent was granted (`agent:<id>`,
//! `workflow:<id>`) are tools of the run when a composer is wired: a call
//! runs nested in this run, see [`crate::composition`].
//!
//! The two context injections that principle 6 allows (the user persona
//! brief, the summaries of past sessions) belong to the Apollia agent alone. An executor built for any other agent drops them, so
//! no other agent ever receives memory it did not ask for.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use apollia_core::{AutonomyConfig, AutonomyLevel, AutonomyLevelConfig, RunId, StepBudgetConfig};
use apollia_llm::{LlmRouter, ToolInvoker};
use apollia_mcp::tool_search::ToolIndexSnapshot;
use apollia_memory::user_memory::UserMemoryRepository;
use apollia_oria::budget::StepBudget;
use apollia_oria::verification::{CriticPass, VerificationLoop};
use apollia_tools::{PlanningMode, ResolvedAgent, ToolRegistryHandle};
use tokio_util::sync::CancellationToken;

use crate::chat::builtin_agent::{BuiltInChatAgent, BuiltInChatAgentDeps, ChatAgentResponse};
use crate::chat::manager::APOLLIA_CHAT_AGENT_ID;
use crate::chat::plan_actor::PlanHandle;
use crate::chat::todo_handle::TodoHandle;
use crate::chat::types::{
    ChatError, ChatMessage, InjectedInstruction, PendingChatApprovals, PlanPhase,
};
use crate::composition::{CallerRun, Composer, ComposingInvoker};
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
    /// Whether the agent plans a task before acting on it.
    pub planning: PlanningMode,
    /// Version of the agent's definition, part of the key of its cached plans.
    pub version: String,
}

impl AgentRunSpec {
    /// Read a resolved agent: its mission, its granted tools, its model, its
    /// autonomy tier, its planning mode and its version.
    pub fn from_resolved(agent: &ResolvedAgent) -> Self {
        Self {
            agent_id: agent.id.clone(),
            mission: agent.mission.clone(),
            granted_tools: agent.access.clone(),
            model: agent.model.as_ref().map(|m| m.model_id.clone()),
            autonomy: agent.autonomy,
            planning: agent.planning,
            version: agent.version.clone(),
        }
    }

    /// Whether this is the Apollia agent, the only one the principle 6
    /// injections are made for.
    pub fn is_apollia(&self) -> bool {
        self.agent_id == APOLLIA_CHAT_AGENT_ID
    }
}

/// The two injections principle 6 reserves to the Apollia agent. Handed to
/// [`AgentExecutor::new`], they are kept only when the agent is Apollia.
#[derive(Default)]
pub struct ApolliaContext {
    /// Profile store the profile block is read from. The caller hands it only
    /// when the profile switch of the conversation is on.
    pub user_memory: Option<Arc<std::sync::Mutex<UserMemoryRepository>>>,
    /// Profile the conversation designates; `None` reads the default profile.
    pub profile_id: Option<String>,
    /// Block summarising earlier, unrelated sessions.
    pub past_sessions: Option<String>,
}

impl ApolliaContext {
    fn is_empty(&self) -> bool {
        self.user_memory.is_none() && self.past_sessions.is_none()
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
    /// Task list of the conversation, enabling `todo_write`.
    pub todo: Option<TodoHandle>,
    /// Plan store of the conversation, enabling the `plan_*` tools.
    pub plan: Option<PlanHandle>,
    /// Plan cache of the runtime, read and written by a plan-first run.
    pub plan_cache: Option<plan_first::SharedPlanCache>,
}

/// Settings of the conversation the run belongs to. The default is a run
/// outside any conversation: no workspace, no deferred tool index, plan mode
/// off, no hook, no injected instruction, decisions read from an empty
/// governance state, a run someone watches, and an approval that waits for
/// the human however long it takes.
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
    /// Images of the message answered and of the earlier user messages.
    pub turn_images: crate::chat::images::TurnImages,
    /// Temperature of a turn that offers tools; the tuned default when absent.
    pub tool_turn_temperature: Option<f32>,
    /// The governance every tool call of the run is decided by; `None`
    /// decides from an empty state, where reading runs and everything
    /// sensitive asks.
    pub governance: Option<crate::governance::GovernanceHandle>,
    /// Nobody watches the run (an item of a batch): a call that would ask
    /// waits instead.
    pub unattended: bool,
    /// Delay after which a pending tool approval is refused; `None` never
    /// refuses on its own.
    pub approval_timeout: Option<Duration>,
    /// Where a tool output too long for the context is kept whole; `None`
    /// cuts it instead.
    pub tool_output_dir: Option<PathBuf>,
    /// The budget of the caller, when the run spends from it (a workflow
    /// step, an agent that called this one); `None` gives the run the budget
    /// of its tier.
    pub shared_budget: Option<Arc<StepBudget>>,
    /// Runs the agents and workflows the agent was granted; `None` offers
    /// none of them.
    pub composition: Option<Composition>,
    /// The conversation turn the run answers, the identifier of its answer
    /// message; `None` outside a conversation.
    pub turn_id: Option<String>,
    /// What the conversation changes from its agent, for itself alone: its
    /// instructions, added after the mission whatever the agent, its request
    /// settings, its reasoning, what its context does at the limit, and the
    /// accesses it proposes. The default changes nothing.
    pub session: crate::chat::builtin_agent::SessionOverrides,
}

/// What the granted agent and workflow calls of a run go through.
pub struct Composition {
    /// Reads and runs the agents and the workflows.
    pub composer: Arc<dyn Composer>,
    /// The level of the run in the nesting, 1 for a run started on its own.
    pub depth: u32,
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
    budget: Arc<StepBudget>,
    verification: Option<VerificationLoop>,
    critic: Option<CriticPass>,
    cancel: CancellationToken,
    past_sessions: Option<String>,
    plan_mode: bool,
    plan_first: plan_first::PlanFirstDeps,
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
        let budget = wiring.shared_budget.unwrap_or_else(|| {
            Arc::new(StepBudget::from_capped(
                &level_config.budget,
                runtime_ceiling,
            ))
        });
        let (tool_invoker, composer) = match wiring.composition {
            Some(Composition { composer, depth }) => {
                let caller = CallerRun {
                    agent_id: spec.agent_id.clone(),
                    depth,
                    cancel: cancel.clone(),
                    budget: Arc::clone(&budget),
                };
                let invoker: Arc<dyn ToolInvoker> = Arc::new(ComposingInvoker::new(
                    deps.tool_invoker,
                    Arc::clone(&composer),
                    caller,
                ));
                (invoker, Some(composer))
            }
            None => (deps.tool_invoker, None),
        };
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
        let plan_first = plan_first::PlanFirstDeps {
            llm_router: Arc::clone(&deps.llm_router),
            tool_registry: deps.tool_registry.clone(),
            tool_invoker: Arc::clone(&tool_invoker),
            event_bus: deps.event_bus.clone(),
            plan_cache: deps.plan_cache,
            workspace_path: wiring.workspace_path.clone(),
            tool_output_dir: wiring.tool_output_dir.clone(),
            runtime_ceiling: runtime_ceiling.clone(),
            governance: wiring.governance.clone().unwrap_or_else(|| {
                crate::governance::GovernanceHandle::detached(
                    crate::governance::GovernanceState::default(),
                    None,
                )
            }),
            unattended: wiring.unattended,
        };
        let plan_mode = wiring.plan_mode;
        let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
            llm_router: deps.llm_router,
            tool_registry: deps.tool_registry,
            tool_invoker,
            event_bus: deps.event_bus,
            user_memory: apollia.user_memory,
            todo: deps.todo,
            plan: deps.plan,
        })
        .with_agent_id(spec.agent_id.clone())
        .with_conversation_profile(apollia.profile_id)
        .with_composer(composer)
        .with_workspace_path(wiring.workspace_path)
        .with_mcp_index(wiring.mcp_index, wiring.tool_search_limit)
        .with_plan_mode(wiring.plan_mode)
        .with_plan_phase_start(wiring.plan_phase)
        .with_hook_executor(wiring.hook_executor)
        .with_pending_injection(wiring.pending_injection)
        .with_turn_images(wiring.turn_images)
        .with_tool_turn_temperature(wiring.tool_turn_temperature)
        .with_governance(wiring.governance)
        .with_unattended(wiring.unattended)
        .with_approval_timeout(wiring.approval_timeout)
        .with_tool_output_dir(wiring.tool_output_dir)
        .with_preferred_backend(spec.model.clone())
        .with_turn_id(wiring.turn_id)
        .with_session_overrides(wiring.session);
        Self {
            spec,
            agent,
            level_config,
            budget,
            verification,
            critic,
            cancel,
            past_sessions: apollia.past_sessions,
            plan_mode,
            plan_first,
        }
    }

    /// Makes the run continue `answer`, the last answer of the conversation,
    /// instead of answering anew. `None` leaves an ordinary run.
    pub fn continuing(mut self, answer: Option<String>) -> Self {
        self.agent = self.agent.with_answer_prefill(answer);
        self
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
    /// the past sessions block.
    pub async fn system_prompt(&self) -> String {
        let mut prompt = self.spec.mission.clone();
        if let Some(block) = &self.past_sessions {
            prompt.push_str("\n\n");
            prompt.push_str(block);
        }
        prompt
    }

    /// Whether the run of `message` plans first.
    pub fn plans_first(&self, message: &str) -> bool {
        plan_first::engages(
            &self.spec,
            &self.level_config.budget,
            message,
            self.plan_mode,
        )
    }

    /// Run one exchange to its end, its pause, or its budget: planned first
    /// when [`Self::plans_first`] says so, step by step otherwise.
    ///
    /// # Errors
    ///
    /// The errors of [`BuiltInChatAgent::execute`], or of a plan that fails.
    pub async fn run(&self, input: RunInput<'_>) -> Result<ChatAgentResponse, ChatError> {
        let system_prompt = self.system_prompt().await;
        if self.plans_first(input.user_message) {
            return plan_first::PlanFirstRun {
                spec: &self.spec,
                deps: &self.plan_first,
                budget: &self.level_config.budget,
                system_prompt,
                cancel: self.cancel.clone(),
            }
            .run(input)
            .await;
        }
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

pub mod plan_first;

#[cfg(test)]
mod governance_tests;
#[cfg(test)]
mod mcp_governance_tests;
#[cfg(test)]
mod session_tests;
#[cfg(test)]
mod tests;
