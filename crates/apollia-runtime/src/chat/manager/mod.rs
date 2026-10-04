//! ChatSessionManager, Tokio actor managing chat session lifecycle.
//!
//! Central entry point for the chat subsystem. Handles session
//! creation, message routing, tool approval resolution, and lifecycle events.
//! Persists in SQLite via [`ChatSessionRepository`] and emits
//! [`RuntimeEvent`] on the EventBus.
//!
//! The chat path does NOT go through the `TaskRouter`, it has its own
//! execution path.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use apollia_core::plan::{Plan, PlanMutation};
use apollia_core::todo::TodoItem;
use apollia_core::{AutonomyLevel, RunId, RuntimeEvent, StepBudgetConfig};
use apollia_llm::{LlmRouter, ToolInvoker};
use apollia_mcp::session::LoadingMode;
use apollia_mcp::tool_search::{ToolIndexSnapshot, ToolSearchExecutor};
use apollia_memory::user_memory::UserMemoryRepository;
use apollia_tools::governance_db::GOVERNANCE_DB_FILENAME;
use apollia_tools::ToolRegistryHandle;

use super::a2a_tools::CompositeToolInvoker;
use super::agent_chat::{AgentChatExecutor, AgentChatRequest, ChatAgentRunner};
use super::agent_executor::{
    AgentExecutor, AgentExecutorDeps, AgentRunSpec, ApolliaContext, Composition, RunInput,
    SessionWiring,
};
use super::builtin_agent::{ChatAgentResponse, NativeChatToolInvoker, DEFAULT_CONTEXT_WINDOW_SIZE};
use super::extractor::UserMemoryExtractor;
use super::plan_actor::{spawn_plan_actor, PlanHandle};
use super::repository::{AppendMessageParams, ChatSessionRepository, ToolApprovalLogEntry};
use super::todo_actor::spawn_todo_actor;
use super::todo_handle::TodoHandle;
use super::types::{
    ChatError, ChatMessage, ChatMode, ChatRole, ChatSession, ExchangeState, InjectedInstruction,
    MessageId, PauseState, PendingChatApprovals, PlanPhase, RecentSessionSummary, SessionDetail,
    SessionId, SessionInfo, SessionMetrics, SessionStatus, ToolCallRecord, ToolDecision,
};
use crate::a2a::A2AInvoker;
use crate::api::routes_agents::AgentLoader;
use crate::eventbus::EventBusSender;
use crate::hooks::executor::HookExecutor;
use crate::registry::AgentRegistryHandle;

/// Maximum number of past sessions to inject as cross-session context.
const MAX_PAST_SESSIONS: usize = 3;

/// Minimum length (in bytes) of the first message to trigger cross-session recall.
///
/// Short greetings like "bonjour" or "hello" are filtered out to avoid
/// injecting irrelevant context from past sessions.
const MIN_MESSAGE_LENGTH_FOR_RECALL: usize = 20;

/// Synthetic directive injected on plan approval to resume execution.
///
/// Phrased as a multi-step actionable instruction so the turn router classifies
/// it as a plan-flow turn and the agent drives the approved plan step by step,
/// keeping the step statuses current through the `plan_*` tools.
const PLAN_EXECUTE_DIRECTIVE: &str =
    "The plan was approved. Execute it now: work through the plan steps in order, \
     update each step status as you start and finish it, and report progress.";

/// Synthetic directive injected on plan rejection to drive a revision turn.
///
/// Phrased as a multi-step actionable instruction so the turn router classifies
/// it as a plan-flow turn and the agent revises the submitted plan through the
/// `plan_*` tools, then re-submits it into the soft gate.
const PLAN_REVISE_DIRECTIVE: &str =
    "The plan was rejected. Revise it: adjust the plan steps to address the concern, \
     document the reason for each change through the plan tools, then re-submit the \
     revised plan for approval.";

/// Synthetic directive injected on resume to continue a paused plan execution.
///
/// Phrased as a multi-step actionable instruction so the turn router classifies it
/// as a plan-flow turn and the agent picks up the plan from the persisted step
/// statuses, continuing the remaining steps in order.
const PLAN_RESUME_DIRECTIVE: &str =
    "Execution resumed. Continue working through the remaining plan steps from where \
     they left off, updating each step status as you start and finish it.";

/// Internal state of the [`ChatSessionManager`] actor.
struct ChatSessionManager {
    /// In-memory session cache.
    sessions: HashMap<SessionId, ChatSession>,
    /// SQLite persistence.
    repository: ChatSessionRepository,
    /// LLM router for free-form / agent conversations.
    llm_router: Option<Arc<LlmRouter>>,
    /// Tool registry for tool descriptor resolution.
    tool_registry: ToolRegistryHandle,
    /// Agent registry for resolving agent names to IDs.
    registry_handle: AgentRegistryHandle,
    /// Agent runner for Chat Agent mode. `None` disables Agent mode.
    agent_runner: Option<Arc<dyn ChatAgentRunner>>,
    /// Event bus sender for runtime events.
    event_bus: EventBusSender,
    /// Runtime-level step budget configuration.
    runtime_budget: StepBudgetConfig,
    /// Default plan-mode state applied to every new session at creation.
    ///
    /// Sourced from the `[chat] plan_mode_default` config key. A new session
    /// inherits this value; the per-session toggle overrides it afterwards. The
    /// runtime owns this default so every entry point (desktop, API, CLI) is
    /// consistent.
    plan_mode_default: bool,
    /// Pending tool approval channels (operator HITL).
    pending_chat_approvals: PendingChatApprovals,
    /// Optional user memory repository for system prompt enrichment.
    user_memory: Option<Arc<std::sync::Mutex<UserMemoryRepository>>>,
    /// Stateful extractor for passive memory enrichment from conversations.
    enrichment_extractor: Option<Arc<tokio::sync::Mutex<UserMemoryExtractor>>>,
    /// Sender clone for spawned tasks to send commands back to the actor.
    tx: mpsc::Sender<ChatCommand>,
    /// Optional A2A invoker, when present, worker agent skills are exposed as virtual tools.
    a2a_invoker: Option<Arc<A2AInvoker>>,
    /// Pending user input registry for the `ask_user` tool.
    pending_user_inputs: apollia_tools::tools::ask_user::PendingUserInputs,
    /// MCP manager handle, populated when the supervisor's MCP bootstrap
    /// succeeds. Propagated to `NativeChatToolInvoker` per session so
    /// chat-libre invocations of `mcp:<server>/<tool>` can be routed
    /// through the manager.
    mcp_handle: Option<apollia_mcp::manager::McpClientManagerHandle>,
    /// Operator config bundled at supervisor boot (data dir, brave key,
    /// tools_config) for the chat dispatcher. `None` in tests / minimal
    /// runtimes, falls back to the minimal dispatcher.
    chat_tools_config: Option<Arc<ChatToolsConfig>>,
    /// Path of the governance database (`<data_dir>/governance.db`), derived
    /// at spawn from [`ChatToolsConfig::data_dir`]. `None` (tests, minimal
    /// runtimes) disables every governance read and write in this actor: no
    /// code path here resolves the process home directory on its own, so a
    /// test mount never reaches the operator's real profile.
    governance_db_path: Option<std::path::PathBuf>,
    /// Map of pending `ask_user` entries, keyed by request_id.
    /// Populated by the background drain task, resolved by `ResolveUserInput`.
    pending_user_replies: HashMap<
        String,
        (
            PendingUserInputMeta,
            tokio::sync::oneshot::Sender<apollia_tools::tools::ask_user::AskUserOutput>,
        ),
    >,
    /// Per-session aggregated metrics.
    ///
    /// In-memory only, rebuilt from history on session resume. Not persisted.
    metrics: HashMap<SessionId, SessionMetrics>,
    /// MCP tool loading strategy applied to every chat exchange.
    ///
    /// In [`LoadingMode::Deferred`] the LLM receives the synthetic `tool_search`
    /// tool instead of the individual MCP schemas. In [`LoadingMode::Eager`] the
    /// existing behavior is preserved (MCP schemas resolved from the registry).
    mcp_loading: LoadingMode,
    /// Maximum `limit` accepted by the synthetic `tool_search` tool, sourced from
    /// the `mcp.tool_search_limit` setting.
    tool_search_limit: usize,
    /// Clonable handle to the per-runtime todo actor, cloned into each exchange
    /// so the agent's `todo_write` tool persists session task state.
    todo_handle: Option<TodoHandle>,
    /// Clonable handle to the per-runtime plan actor, cloned into each exchange
    /// so the agent's `plan_*` tools persist the session plan.
    plan_handle: Option<PlanHandle>,
    /// Shared lifecycle hook executor, cloned into each exchange so the ReAct
    /// loop can run PreToolUse and the best-effort hooks. `None` disables hooks.
    hook_executor: Option<Arc<HookExecutor>>,
    /// Source of the executors of the package tools, set once the embedded
    /// interpreter is available. `None` leaves every `pkg:` tool unrunnable.
    package_tools: Option<Arc<dyn crate::package_tools::PackageToolHost>>,
    /// Reads and runs the agents and workflows an agent was granted, set once
    /// the workflow engine is up. `None` offers no `agent:` or `workflow:`
    /// tool.
    composer: Option<Arc<dyn crate::composition::Composer>>,
    /// The governance every tool call of a run is decided by, set once the
    /// supervisor has started it. `None` decides from an empty state.
    governance: Option<crate::governance::GovernanceHandle>,
    /// Per-session cooperative pause tokens.
    ///
    /// A fresh [`CancellationToken`] is inserted when a turn is dispatched and a
    /// clone is threaded into the ReAct loop. `pause_session` cancels it; the loop
    /// stops at its next checkpoint. Owned by this actor alone, so there is no
    /// shared lock across actors (principle #5).
    pause_tokens: HashMap<SessionId, CancellationToken>,
    /// Per-session cooperative pause state, mirroring the loop disposition.
    ///
    /// Absent means [`PauseState::Running`] (the steady state). `Pausing` is the
    /// transient window after a pause request; `Paused` once the loop stopped at a
    /// checkpoint. In-memory only, rebuilt on resume.
    pause_states: HashMap<SessionId, PauseState>,
    /// Operator instructions queued for a paused session, consumed on the next
    /// resume turn. At most one is held per session.
    pending_injections: HashMap<SessionId, InjectedInstruction>,
    /// Plan continuation directives that arrived while the session was still
    /// `Processing`, re-dispatched when its `ExchangeComplete` lands.
    ///
    /// A plan decision (approve / reject) can be taken mid-turn: the approval
    /// card is raised by the fast `PlanSubmitted` broadcast before this actor
    /// processes `ExchangeComplete`. Dispatching the continuation at that point
    /// fails with `SessionBusy`; instead of dropping the directive (which left
    /// approved plans unexecuted), it is parked here. At most one is held per
    /// session, latest decision wins.
    pending_plan_continuations: HashMap<SessionId, String>,
    /// The live sections of the configuration this actor follows, once the
    /// supervisor handed them over; `None` keeps the values it was spawned
    /// with.
    config_feed: Option<crate::config_reload::ConfigFeed>,
    /// Sends the plan-mode default to the handles, which answer it without a
    /// round trip.
    plan_mode_default_tx: tokio::sync::watch::Sender<bool>,
    /// Sends the hook executor in force to the handles, which list its
    /// handlers.
    hook_executor_tx: tokio::sync::watch::Sender<Option<Arc<HookExecutor>>>,
}

mod actor;
mod agent_task;
mod apollia_agent;
mod authz;
mod code_context;
mod command;
mod control;
mod declarative;
mod disabled;
mod dispatcher;
mod exchange;
mod handle;
mod handle_ext;
mod libre;
mod live_config;
mod plan;
mod session;
mod sources;
mod types;
mod user_input;

#[cfg(test)]
mod live_config_tests;
#[cfg(test)]
mod plan_regression_tests;
#[cfg(test)]
mod preferred_backend_tests;
#[cfg(test)]
mod tests;

pub(in crate::chat::manager) use dispatcher::{
    now_rfc3339, resolve_workspace_for_session, session_to_info,
};
pub(in crate::chat::manager) use libre::{
    accumulate_exchange_metrics, apply_libre_overrides, log_resolution_metadata,
    run_libre_exchange, session_run_grants,
};
pub(in crate::chat::manager) use types::{
    ChatLibreOverrides, ExchangeSummaryParams, FullDispatcherParams, LibreExchangeParams,
    LibreSessionDefaults, PendingUserInputMeta, RegisterUserInputReplyParams,
    ResolvedSessionInvoker, WorkspaceResolutionParams,
};

/// The executors of the package tools for one run of `caller`, stopped by
/// `cancel`; none while no host is set.
pub(in crate::chat::manager) fn package_executors(
    host: &Option<Arc<dyn crate::package_tools::PackageToolHost>>,
    caller: &str,
    cancel: &CancellationToken,
) -> Vec<Box<dyn apollia_tools::executor::ToolExecutor>> {
    host.as_ref()
        .map(|host| host.executors(caller, cancel))
        .unwrap_or_default()
}

pub use agent_task::AgentTaskRun;
pub use command::ChatCommand;
pub use handle::ChatSessionManagerHandle;
pub use types::{
    ChatPlanSnapshot, ChatToolsConfig, CreateSessionParams, InjectError, PauseError,
    PendingUserInputView, SessionAuthorizationView, APOLLIA_CHAT_AGENT_ID,
};
