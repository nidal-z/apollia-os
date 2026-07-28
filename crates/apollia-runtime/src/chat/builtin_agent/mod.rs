//! BuiltInChatAgent, Rust-native ReAct loop for Chat Libre mode.
//!
//! Implements the core reasoning loop: LLM, tool call, approval, result, LLM.
//! Protected by [`StepBudget`] (the runtime step safeguard) and integrated with
//! the HITL approval flow via [`PendingChatApprovals`].
//!
//! Uses `LlmRouter.stream()` for token-by-token streaming.
//! Each token emits a `ChatToken` RuntimeEvent on the EventBus so the SSE
//! stream can forward it to the client in real time.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use tracing::{info, warn};

use apollia_core::{
    AutonomyLevel, AutonomyLevelConfig, CeilingAction, ORIAConfig, RunId, RuntimeEvent,
};
use apollia_llm::routing_level::{EscalationSignal, LlmRoutingLevel};
use apollia_llm::types::{
    ChatMessage as LlmChatMessage, CompletionRequest, StreamChunk, TokenUsage, ToolCall, ToolSpec,
};
use apollia_llm::{LlmRouter, MetaOrchestratorHandle, ObservabilityConfig, ToolInvoker};
use apollia_mcp::tool_search::{tool_search_input_schema, ToolIndexSnapshot};
use apollia_memory::user_memory::UserMemoryRepository;
use apollia_oria::budget::StepBudget;
use apollia_oria::context_manager::ContextManager;
use apollia_oria::verification::{
    CheckFailure, CheckInvoker, CheckOutcome, Correction, CriticPass, CriticReport,
    VerificationLoop,
};
use apollia_tools::ToolRegistryHandle;
use tokio_util::sync::CancellationToken;

use super::types::{
    ApprovalTimeoutParams, ChatError, ChatMessage, ChatRole, InjectedInstruction,
    PendingChatApprovals, PlanPhase, ToolCallRecord, ToolCallStatus, ToolDecision, TurnOutcome,
};
use crate::a2a::A2AInvoker;
use crate::chat::a2a_tools::generate_a2a_tool_specs;
use crate::chat::plan_actor::PlanHandle;
use crate::chat::plan_tool::{
    self, PLAN_ADD_STEP_TOOL_NAME, PLAN_MODIFY_STEP_TOOL_NAME, PLAN_PROPOSE_TOOL_NAME,
    PLAN_REMOVE_STEP_TOOL_NAME, PLAN_REORDER_TOOL_NAME, PLAN_SET_STEP_STATUS_TOOL_NAME,
    PLAN_SUBMIT_TOOL_NAME,
};
use crate::chat::todo_handle::TodoHandle;
use crate::chat::todo_tool::{run_todo_write, todo_write_spec, TODO_WRITE_TOOL_NAME};
use crate::chat::turn_router::classify_turn;
use crate::eventbus::EventBusSender;
use crate::hooks::executor::{HookDecision, HookExecutor};

mod builder;
mod context_window;
mod helpers;
mod hitl;
mod invoker;
mod plan;
mod prompt;
mod react_loop;
mod response;
mod stream;
mod tools;

pub(in crate::chat::builtin_agent) use helpers::*;
pub use invoker::*;
pub use response::*;

/// Cooperative pause/resume of the chat ReAct loop.
#[cfg(test)]
mod pause_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod todo_compaction_tests;
#[cfg(test)]
mod verification_wire_tests;

/// Default timeout for chat tool approval requests (5 minutes).
const CHAT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(300);

/// Maximum number of read-only tool calls executed concurrently within a single
/// agent turn. Write calls and read-only calls awaiting approval stay
/// sequential regardless of this cap. Mirrors the ORIA batch cap.
const MAX_CONCURRENT_READONLY_TOOL_CALLS: usize = 10;

/// Default number of recent messages in the sliding context window.
pub const DEFAULT_CONTEXT_WINDOW_SIZE: usize = 20;

/// Temperature used for a turn that advertises tools when `[chat]
/// tool_turn_temperature` is unset. Low enough to make structured tool-call
/// output reliable on small local models without going fully greedy.
pub const DEFAULT_TOOL_TURN_TEMPERATURE: f32 = 0.3;

/// Refusal injected when the plan-mode hard gate blocks an execution tool.
///
/// Surfaced to the model as a tool result so it reacts on the next turn by
/// proposing and submitting a plan instead of acting directly.
const PLAN_GATE_DENY_REASON: &str = "plan mode is active: this is an execution tool and no execution may run before the plan is approved. You MAY use read-only tools (web_search, file_read, etc.) and ask_user to gather context, then propose and submit a plan with the plan_* tools and wait for approval. Once approved, this gate opens and you execute the steps.";

/// Prefix of the reminder message re-injected after a context compaction so the
/// agent keeps its task list in view once the history is truncated.
const TODO_REMINDER_PREFIX: &str =
    "[System reminder] Your current task list after context compaction:";

/// Prefix of the reminder message re-injected after a context compaction so the
/// agent keeps an active plan in view once the history is truncated.
const PLAN_REMINDER_PREFIX: &str =
    "[System reminder] The active plan for this session after context compaction \
     (do not re-propose or re-submit it, continue executing the pending steps):";

/// Deterministic assistant message used when a plan is submitted but the model
/// produced no prose for the turn.
///
/// Local models frequently emit the `plan_submit` / `plan_propose` call with no
/// accompanying text, which would persist an empty assistant message and trip
/// the UI empty-response fallback. This runtime-generated summary replaces the
/// empty content so the turn always carries a meaningful message. English by
/// convention: like the other plan directives in this crate, the source string
/// is English (the frontend localizes static UI, and model-generated content is
/// localized via [`apollia_prompts::LANGUAGE_FOOTER`]). A deterministic message
/// that bypasses the model has no per-session locale available in the runtime,
/// so it stays English, the platform's canonical source language.
fn plan_ready_message(step_count: usize) -> String {
    match step_count {
        1 => "Plan ready - 1 step awaiting your approval.".to_string(),
        n => format!("Plan ready - {n} steps awaiting your approval."),
    }
}

/// Maximum number of characters for input/output previews in events.
const PREVIEW_MAX_LEN: usize = 200;

/// Maximum number of characters for tool output injected into LLM context.
/// Outputs longer than this are truncated with a notice so the LLM knows
/// results were cut and can refine its command.
const TOOL_OUTPUT_MAX_LEN: usize = 4000;

/// Re-exported base system prompts. The single source of truth lives in the
/// `apollia-prompts` crate (English; tier-selected by `build_system_prompt`).
pub use apollia_prompts::blocks::{DEFAULT_SYSTEM_PROMPT, PERSEVERANCE_SYSTEM_PROMPT};

/// Maximum number of verification retry iterations per run.
///
/// Bounded to a small number so a failing check cannot loop indefinitely; each
/// retry still consumes from the shared [`StepBudget`].
const VERIFICATION_MAX_RETRIES: u32 = 2;

/// Number of consecutive tool failures before the ReAct loop emits an
/// escalation signal toward the frontier backend.
///
/// Conservative surface heuristic: it counts consecutive failed tool calls
/// (execution error, non-zero exit code, or operator refusal) and resets on the
/// first success. A richer signal based on a model confidence score is out of
/// scope for this iteration.
const ESCALATION_FAILURE_THRESHOLD: u32 = 3;

/// Dependencies required to construct a [`BuiltInChatAgent`].
pub struct BuiltInChatAgentDeps {
    pub llm_router: Arc<LlmRouter>,
    pub tool_registry: ToolRegistryHandle,
    pub tool_invoker: Arc<dyn ToolInvoker>,
    pub event_bus: EventBusSender,
    pub user_memory: Option<Arc<std::sync::Mutex<UserMemoryRepository>>>,
    pub a2a_invoker: Option<Arc<A2AInvoker>>,
    /// Optional per-session todo store. When present, the `todo_write` built-in
    /// tool is advertised to the LLM and handled inside the ReAct loop.
    pub todo: Option<TodoHandle>,
    /// Optional per-session plan store. When present, the `plan_*` built-in
    /// tools are advertised and handled inside the ReAct loop while the session
    /// is in plan mode.
    pub plan: Option<PlanHandle>,
}

/// Rust-native chat agent implementing a ReAct loop for Chat Libre mode.
///
/// Stateless, all mutable state is passed as parameters to [`execute`](Self::execute).
/// Tool execution is delegated to a [`ToolInvoker`].
pub struct BuiltInChatAgent {
    /// LLM router for completion calls.
    llm_router: Arc<LlmRouter>,
    /// Tool registry for resolving tool descriptors into LLM-compatible specs.
    tool_registry: ToolRegistryHandle,
    /// Tool invoker for actual tool execution.
    tool_invoker: Arc<dyn ToolInvoker>,
    /// Event bus for emitting chat lifecycle events.
    event_bus: EventBusSender,
    /// Optional user memory repository for injecting user context into the system prompt.
    user_memory: Option<Arc<std::sync::Mutex<UserMemoryRepository>>>,
    /// Optional A2A invoker for discovering worker agent skills as virtual tools.
    a2a_invoker: Option<Arc<A2AInvoker>>,
    /// Context window manager: compacts `llm_messages` inside the ReAct loop
    /// when accumulated messages exceed the model's window threshold.
    context_manager: ContextManager,
    /// Optional handle to the `MetaLlmOrchestrator`, used to produce the
    /// `ToolCallRationale` narrated before each tool execution.
    /// Absent by default for backward compatibility; injected by the manager
    /// when the "Explain tool calls" main toggle is active.
    meta_handle: Option<MetaOrchestratorHandle>,
    /// Workspace directory injected into the system prompt so the LLM knows its
    /// effective working directory (project workspace or ~/.apollia/ for free chat).
    workspace_path: Option<std::path::PathBuf>,
    /// Aggregated MCP tool index for deferred mode.
    ///
    /// `Some` only when the session runs in deferred mode: `build_tool_specs`
    /// then injects the synthetic `tool_search` spec and omits the individual MCP
    /// schemas. `None` keeps the eager spec path unchanged.
    mcp_index: Option<Vec<ToolIndexSnapshot>>,
    /// Maximum `limit` advertised for the synthetic `tool_search` tool.
    tool_search_limit: usize,
    /// Optional per-session todo store. `None` disables the `todo_write` tool.
    todo: Option<TodoHandle>,
    /// Optional per-session plan store. `None` disables the `plan_*` tools.
    plan: Option<PlanHandle>,
    /// Whether the owning session has plan mode enabled.
    ///
    /// This is the real gate consulted by [`plan_mode_active`](Self::plan_mode_active):
    /// the `plan_*` surface is advertised and dispatched only when the session
    /// flag is set, not merely when a plan store happens to be attached.
    session_plan_mode: bool,
    /// Plan phase the owning session is in when the turn starts.
    ///
    /// Defaults to [`PlanPhase::Done`]. When a substantive plan-mode turn opens
    /// while the session is already in [`PlanPhase::AwaitingApproval`], the turn
    /// is a revision turn (the soft gate is open): it starts the tracker in
    /// awaiting-approval instead of reopening discovery, so the phase stays put
    /// unless the agent re-submits.
    session_plan_phase: PlanPhase,
    /// Optional lifecycle hook executor shared across sessions. `None` means no
    /// hooks are configured: the ReAct loop behaves exactly as before, with no
    /// interception and zero overhead.
    hook_executor: Option<Arc<HookExecutor>>,
    /// Operator instruction injected into a resume turn while the session was
    /// paused.
    ///
    /// `Some` only on a resume turn that carries an injection: the instruction is
    /// prepended as a user message and any plan step the agent creates or modifies
    /// during the turn is stamped with [`StepOrigin::UserInject`] provenance and
    /// the operator text as reason. `None` for every ordinary turn, so behavior is
    /// unchanged when no injection is pending.
    pending_injection: Option<InjectedInstruction>,
    /// Temperature applied to a turn that advertises tools to the model.
    ///
    /// Lowering it whenever tools are exposed makes structured tool-call output
    /// far more reliable on small local models. A turn with no tools keeps the
    /// backend default (the request leaves `temperature` unset). Seeded from
    /// `[chat] tool_turn_temperature`, defaulting to
    /// [`DEFAULT_TOOL_TURN_TEMPERATURE`].
    tool_turn_temperature: f32,
}

/// Mutable accumulators threaded through one ReAct turn's tool-call handling.
struct ReactAccumulators {
    all_tool_calls: Vec<ToolCallRecord>,
    newly_authorized: Vec<String>,
    authorized: HashSet<String>,
}

/// Owned/borrowed state needed to build the terminal [`ChatAgentResponse`]
/// (final text or stream-error path).
struct ResponseContext<'a> {
    acc: ReactAccumulators,
    total_usage: TokenUsage,
    session_id: &'a str,
    message_id: &'a str,
    run_id: &'a RunId,
    frontier_ceiling_reached: bool,
    /// Terminal plan-mode phase to carry on the response. `None` outside the
    /// plan flow; `Some` when this turn ran discovery and (possibly) drafting.
    final_plan_phase: Option<PlanPhase>,
    /// Real context window of the active model in tokens, when known. `None`
    /// leaves the context gauge in an "unknown" state.
    context_window_tokens: Option<u32>,
    /// Prompt-token count of the most recent LLM call, i.e. the current context
    /// occupancy. Zero when no usage was reported.
    context_tokens_used: u32,
}

/// Borrowed context for processing a single tool call inside the ReAct loop.
struct ToolCallContext<'a> {
    session_id: &'a str,
    message_id: &'a str,
    call: &'a ToolCall,
    run_id: &'a RunId,
    pending_approvals: &'a PendingChatApprovals,
}

/// Borrowed identifiers shared by every tool call in a single ReAct turn
/// (the per-call [`ToolCall`] is supplied separately while iterating).
///
/// Carries the cooperative pause token threaded into the turn. The token is a
/// cheap `Arc` clone, so the struct is `Clone` (no longer `Copy`); callers clone
/// it explicitly when they reuse it across the loop and a verification retry.
#[derive(Clone)]
struct ToolCallContextIds<'a> {
    session_id: &'a str,
    message_id: &'a str,
    run_id: &'a RunId,
    pending_approvals: &'a PendingChatApprovals,
    /// Cooperative pause token. When cancelled, the loop persists nothing extra
    /// (the `PlanActor` already owns step statuses) and returns a paused
    /// response at the next checkpoint, never mid-tool.
    cancel: CancellationToken,
}

/// Borrowed read-only inputs for [`BuiltInChatAgent::record_tool_turn`]:
/// the raw LLM output, the parsed tool calls, the step budget, and the
/// per-turn identifiers. The mutable accumulators are passed separately.
struct RecordTurnInput<'a> {
    accumulated_text: &'a str,
    tool_calls: &'a [ToolCall],
    budget: &'a StepBudget,
    ids: ToolCallContextIds<'a>,
    /// Names of every tool advertised to the model this turn (registry tools,
    /// A2A skills, and the in-loop built-ins). A call to any other name is a
    /// hallucination: it is refused with a corrective tool result rather than
    /// dropped silently, so the model can recover on its next turn.
    valid_tool_names: &'a HashSet<String>,
}

/// Borrowed identifiers locating a single tool call being executed
/// (session + message scope plus the call itself).
struct ToolExecTarget<'a> {
    session_id: &'a str,
    message_id: &'a str,
    call: &'a ToolCall,
    run_id: &'a RunId,
}

/// Result of processing one tool call.
///
/// `failed` feeds the escalation counter. `executed` carries the LLM-facing
/// output and success flag when the tool actually ran, so the loop can fire the
/// `PostToolUse` hook; it is `None` when the call was refused and never invoked.
struct ToolCallOutcome {
    failed: bool,
    executed: Option<(String, bool)>,
}

/// Outcome of running the `PreToolUse` hooks over one turn's tool calls.
///
/// `calls` is the working set to execute: borrowed (no hook, no change) or owned
/// with any `Rewrite` applied. `denied[i]` carries the refusal reason when call
/// `i` was blocked; it is index-aligned with `calls`.
struct PreToolUseOutcome<'a> {
    calls: std::borrow::Cow<'a, [ToolCall]>,
    denied: Vec<Option<String>>,
}
