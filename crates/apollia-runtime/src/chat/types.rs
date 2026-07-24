//! Chat domain types.
//!
//! Defines sessions, messages, roles, tool approval mechanics,
//! and the `ChatError` hierarchy used throughout the chat subsystem.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::eventbus::EventBusSender;
use apollia_core::{RunId, RuntimeEvent};

/// Alias for a chat session identifier (UUID v4 string).
pub type SessionId = String;

/// Alias for a chat message identifier (UUID v4 string).
pub type MessageId = String;

/// A chat session, either free-form (Libre) or agent-backed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    /// Unique identifier (UUID v4).
    pub id: SessionId,
    /// Mode of the session (Libre or Agent).
    pub mode: ChatMode,
    /// Agent name, present only in Agent mode.
    pub agent_name: Option<String>,
    /// System prompt used to prime the LLM.
    pub system_prompt: String,
    /// Current lifecycle status.
    pub status: SessionStatus,
    /// In-memory message history (populated on demand).
    pub history: Vec<ChatMessage>,
    /// Tools the user has permanently authorized for this session.
    pub authorized_tools: std::collections::HashSet<String>,
    /// Tools available in this session (from agent manifest or config).
    pub available_tools: Vec<String>,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
    /// Active exchange state (set while an LLM response is being generated).
    pub active_exchange: Option<ExchangeState>,
    /// Preferred LLM backend name (None = runtime default).
    pub llm_backend: Option<String>,
    /// User-defined display title (falls back to agent_name or mode).
    pub title: Option<String>,
    /// Parent session identifier when this session is a fork.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<SessionId>,
    /// Fork depth (0 = root session, 1 = first-level fork, etc.).
    #[serde(default)]
    pub fork_depth: i64,
    /// Project this session belongs to (None = standalone).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Force project context injection on the next user message.
    ///
    /// Set when a project link is changed after the session has started; the
    /// initial-injection path (gated on `is_first_message`) would otherwise
    /// skip context for already-active sessions. Transient, not persisted.
    #[serde(skip)]
    pub force_project_context_inject: bool,
    /// In-memory filesystem allow rules for this session (not persisted).
    ///
    /// Set by the user via "Always allow (this session)" in `HitlFilesystemModal`.
    /// Format: `"op:level"` e.g., `"write:medium"`.
    /// Cleared when the session ends (intentional, session-scoped only).
    #[serde(skip)]
    pub fs_allow_rules: Arc<Mutex<std::collections::HashSet<String>>>,
    /// Whether this session runs in first-class plan mode.
    ///
    /// When `true`, the `plan_*` tool surface is advertised and the plan-mode
    /// system-prompt block is injected. Persisted across restarts.
    #[serde(default)]
    pub plan_mode: bool,
    /// Current plan-mode lifecycle phase.
    ///
    /// Defaults to [`PlanPhase::Done`] for sessions that never enabled plan mode.
    /// Persisted across restarts.
    #[serde(default)]
    pub plan_phase: PlanPhase,
}

/// Lifecycle phase of a session running in plan mode.
///
/// Drives the conversational gate: discovery via `ask_user`, drafting the plan,
/// awaiting a soft approval, executing, then done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanPhase {
    /// Asking the user for missing or ambiguous inputs.
    Discovery,
    /// Building the plan steps with rationale.
    Drafting,
    /// Plan submitted, waiting for approve, reject, or conversational revision.
    AwaitingApproval,
    /// Plan approved, agent is executing the steps.
    Executing,
    /// Plan finished or plan mode disabled.
    #[default]
    Done,
}

impl PlanPhase {
    /// Returns the stable SQL representation for persistence.
    pub fn as_sql(&self) -> &'static str {
        match self {
            Self::Discovery => "discovery",
            Self::Drafting => "drafting",
            Self::AwaitingApproval => "awaiting_approval",
            Self::Executing => "executing",
            Self::Done => "done",
        }
    }

    /// Parses a SQL representation, returning `None` for unknown values.
    pub fn from_sql(s: &str) -> Option<Self> {
        match s {
            "discovery" => Some(Self::Discovery),
            "drafting" => Some(Self::Drafting),
            "awaiting_approval" => Some(Self::AwaitingApproval),
            "executing" => Some(Self::Executing),
            "done" => Some(Self::Done),
            _ => None,
        }
    }
}

/// Cooperative pause state for a chat session running a ReAct turn.
///
/// `Running` is the steady state. `Pausing` is the transient window between a
/// pause request and the loop reaching its next checkpoint. `Paused` means the
/// loop has exited cleanly with partial step statuses already persisted by the
/// `PlanActor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PauseState {
    /// The turn runs normally, no pause requested.
    #[default]
    Running,
    /// A pause was requested; the loop has not yet reached a checkpoint.
    Pausing,
    /// The loop stopped cleanly at a checkpoint; partial progress is persisted.
    Paused,
}

/// Terminal disposition of a single ReAct turn, distinguishing a normal
/// convergence from a cooperative pause so the manager can persist the right
/// session state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnOutcome {
    /// The loop converged or stopped on its own terms.
    Completed,
    /// The loop stopped at a checkpoint because its token was cancelled.
    Paused,
}

/// A natural-language instruction injected by the operator while a session is
/// paused.
///
/// It is consumed as a user message on the resume turn, prompting the agent to
/// adjust its plan via the `plan_*` tools. Any plan step the agent creates or
/// modifies in response is stamped with [`apollia_core::plan::StepOrigin::UserInject`]
/// provenance carrying `text` as the reason, so the provenance is deterministic
/// and never depends on the model emitting the right origin enum.
#[derive(Debug, Clone)]
pub struct InjectedInstruction {
    /// Target session identifier.
    pub session_id: String,
    /// Raw operator text, e.g. "before step X, do Y".
    pub text: String,
}

impl InjectedInstruction {
    /// Builds the provenance stamped on steps created from this injection:
    /// origin [`apollia_core::plan::StepOrigin::UserInject`] with the operator
    /// text as the reason and the current unix timestamp (seconds).
    pub fn provenance(&self) -> apollia_core::plan::StepProvenance {
        apollia_core::plan::StepProvenance {
            origin: apollia_core::plan::StepOrigin::UserInject,
            reason: Some(self.text.clone()),
            at: now_unix_secs(),
        }
    }
}

/// Returns the current unix timestamp in seconds, or `0` if the system clock is
/// before the epoch (which never happens on a sane host).
fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Chat mode, free-form LLM conversation or agent-backed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChatMode {
    /// Free-form conversation with an LLM (no agent tools).
    Libre,
    /// Agent-backed session with tool access.
    Agent,
    /// Contextual platform assistant embedded in the UI.
    ///
    /// Like `Libre` but intentionally isolated from user memory and cross-session
    /// history, the companion helps navigate the platform, not the user's personal
    /// projects. Memory is never injected automatically (Principle #6).
    Companion,
}

impl ChatMode {
    /// Return the SQL-storable string representation.
    pub fn as_sql(&self) -> &str {
        match self {
            Self::Libre => "libre",
            Self::Agent => "agent",
            Self::Companion => "companion",
        }
    }

    /// Parse from SQL string representation.
    ///
    /// Returns `None` for unrecognized values.
    pub fn from_sql(s: &str) -> Option<Self> {
        match s {
            "libre" => Some(Self::Libre),
            "agent" => Some(Self::Agent),
            "companion" => Some(Self::Companion),
            _ => None,
        }
    }
}

/// Lifecycle status of a chat session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionStatus {
    /// Session is open and accepting messages.
    Active,
    /// An LLM response is being generated.
    Processing,
    /// Session has been closed, no more messages accepted.
    Closed,
}

impl SessionStatus {
    /// Return the SQL-storable string representation.
    pub fn as_sql(&self) -> &str {
        match self {
            Self::Active => "active",
            Self::Processing => "processing",
            Self::Closed => "closed",
        }
    }

    /// Parse from SQL string representation.
    ///
    /// Returns `None` for unrecognized values.
    pub fn from_sql(s: &str) -> Option<Self> {
        match s {
            "active" => Some(Self::Active),
            "processing" => Some(Self::Processing),
            "closed" => Some(Self::Closed),
            _ => None,
        }
    }
}

/// Tracks the in-progress exchange (message being generated).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeState {
    /// ID of the response message being generated.
    pub message_id: MessageId,
    /// ISO-8601 timestamp when the exchange started.
    pub started_at: String,
    /// Stable run identifier for this exchange (one user turn, one response).
    ///
    /// Generated once when the exchange starts and propagated to the
    /// `RuntimeEvent` variants emitted during the run. Distinct from
    /// [`SessionId`] (conversation lifetime) and [`MessageId`] (single message).
    #[serde(default)]
    pub run_id: RunId,
}

/// A single message in a chat session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Unique identifier (UUID v4).
    pub id: MessageId,
    /// Role of the message sender.
    pub role: ChatRole,
    /// Text content of the message.
    pub content: String,
    /// Tool calls attached to this message (assistant role only).
    pub tool_calls: Option<Vec<ToolCallRecord>>,
    /// Name of the tool that produced this message (tool role only).
    pub tool_name: Option<String>,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
    /// Sequence number within the session (1-based, ascending).
    pub seq: u32,
    /// Optional key-value metadata attached by the runtime (e.g. cross-session markers).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Role of a chat message sender.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChatRole {
    /// Message from the human user.
    User,
    /// Message from the LLM assistant.
    Assistant,
    /// System prompt message.
    System,
    /// Tool execution result.
    Tool,
}

impl ChatRole {
    /// Return the SQL-storable string representation.
    pub fn as_sql(&self) -> &str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
            Self::Tool => "tool",
        }
    }

    /// Parse from SQL string representation.
    ///
    /// Returns `None` for unrecognized values.
    pub fn from_sql(s: &str) -> Option<Self> {
        match s {
            "user" => Some(Self::User),
            "assistant" => Some(Self::Assistant),
            "system" => Some(Self::System),
            "tool" => Some(Self::Tool),
            _ => None,
        }
    }
}

/// Record of a single tool call within an assistant message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    /// Name of the invoked tool.
    pub tool_name: String,
    /// JSON input arguments.
    pub input: serde_json::Value,
    /// Output produced by the tool (after execution).
    pub output: Option<String>,
    /// Current status of the tool call.
    pub status: ToolCallStatus,
    /// Meta-LLM narration of the call. Persisted so the UI
    /// can re-render rationale when reopening a past session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<apollia_core::ToolCallRationale>,
    /// Retry chain captured for this invocation.
    /// Empty on first-try success; each element represents one attempt.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retry_attempts: Vec<apollia_core::RetryAttempt>,
}

/// Lifecycle status of a tool call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ToolCallStatus {
    /// Waiting for authorization.
    #[serde(alias = "Pending")]
    Pending,
    /// Authorized by the user, ready to execute.
    #[serde(alias = "Authorized")]
    Authorized,
    /// Executed, output available.
    #[serde(alias = "Executed")]
    Executed,
    /// Refused by the user.
    #[serde(alias = "Refused")]
    Refused,
}

/// User decision on a tool approval request.
///
/// The enum mirrors the three buttons surfaced by `ApprovalCardV2`
/// (approve / reject / always-accept). `Refuse` optionally carries a free-form
/// reason forwarded to the agent so it can adapt its plan, and `AlwaysAccept`
/// carries the sticky scope picked by the operator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ToolDecision {
    /// Approve this single invocation.
    Accept,
    /// Refuse this single invocation. `reason` is `None` when the operator
    /// declines without providing a justification (or when the manifest does
    /// not flag `reject_reason_required`).
    Refuse {
        /// Free-form reason shared with the agent.
        #[serde(default)]
        reason: Option<String>,
    },
    /// Approve this invocation **and** install an "always accept" rule whose
    /// stickiness is controlled by `scope` (cf. [`AlwaysAcceptScope`]).
    AlwaysAccept {
        /// Scope picked by the operator. Defaults to
        /// [`AlwaysAcceptScope::ThisSession`] for safety.
        #[serde(default = "AlwaysAcceptScope::safe_default")]
        scope: AlwaysAcceptScope,
    },
}

impl ToolDecision {
    /// Convenience: "plain refuse" without a reason (legacy call-sites).
    #[must_use]
    pub fn refuse() -> Self {
        Self::Refuse { reason: None }
    }

    /// Convenience: "always accept" with the safe default scope.
    #[must_use]
    pub fn always_accept_default() -> Self {
        Self::AlwaysAccept {
            scope: AlwaysAcceptScope::safe_default(),
        }
    }

    /// Returns `true` if this decision is an `AlwaysAccept` of any scope.
    #[must_use]
    pub fn is_always_accept(&self) -> bool {
        matches!(self, Self::AlwaysAccept { .. })
    }

    /// Serde-friendly identifier used for log rows / event payloads.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Refuse { .. } => "refuse",
            Self::AlwaysAccept { .. } => "always_accept",
        }
    }
}

/// Per-tool aggregated statistics within a session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolStatEntry {
    /// Tool name (scoped, e.g. `"bash"` or `"native:read_file"`).
    pub tool_name: String,
    /// Number of invocations (any status).
    pub calls: u32,
    /// Number of invocations that ended with `Refused` status.
    pub refused: u32,
    /// Number of invocations that ended with `Executed` status.
    pub executed: u32,
}

/// Aggregated metrics for a chat session.
///
/// Accumulated by the [`ChatSessionManager`] on each `ExchangeComplete`.
/// Backend-local: never persisted in SQLite, rebuilt from memory on restart
/// (minimal slice; persistence is a future concern).
///
/// `cost_usd` is `None` when the backend does not report pricing (e.g. local
/// runner sidecar via `RunnerLlmBackend`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionMetrics {
    /// Target session.
    pub session_id: SessionId,
    /// Input tokens consumed (prompt + cache reads + cache writes).
    pub prompt_tokens: u32,
    /// Output tokens generated.
    pub completion_tokens: u32,
    /// Cached input tokens read (subset of `prompt_tokens`).
    pub cache_read_input_tokens: u32,
    /// Cached input tokens written (subset of `prompt_tokens`).
    pub cache_write_input_tokens: u32,
    /// Estimated cost in USD (`None` for local backends).
    pub cost_usd: Option<f64>,
    /// Step budget in effect for this session (max steps).
    pub budget_max_steps: u32,
    /// Steps consumed so far (exchanges producing assistant responses).
    pub steps_used: u32,
    /// Context window size (sliding window on history).
    pub context_window_size: u32,
    /// Current history length (messages kept in memory).
    pub messages_in_history: u32,
    /// Aggregated per-tool statistics.
    pub tool_stats: Vec<ToolStatEntry>,
    /// Number of exchanges completed.
    pub exchanges_count: u32,
    /// RFC-3339 timestamp of the first assistant response (session start, LLM time).
    pub started_at: Option<String>,
    /// RFC-3339 timestamp of the last update.
    pub updated_at: Option<String>,
    /// Cumulative active duration in milliseconds (LLM + tool execution time estimates).
    #[serde(default)]
    pub active_duration_ms: u64,
    /// Backend/model name associated with the session (for pricing resolution).
    pub llm_backend: Option<String>,
}

impl SessionMetrics {
    /// Create an empty metrics record for a session.
    pub fn new(session_id: impl Into<SessionId>) -> Self {
        Self {
            session_id: session_id.into(),
            ..Default::default()
        }
    }

    /// Increment per-tool counters from a list of tool call records.
    pub fn record_tool_calls(&mut self, calls: &[ToolCallRecord]) {
        for call in calls {
            let entry = self
                .tool_stats
                .iter_mut()
                .find(|e| e.tool_name == call.tool_name);
            match entry {
                Some(e) => {
                    e.calls += 1;
                    match call.status {
                        ToolCallStatus::Executed => e.executed += 1,
                        ToolCallStatus::Refused => e.refused += 1,
                        _ => {}
                    }
                }
                None => {
                    let mut e = ToolStatEntry {
                        tool_name: call.tool_name.clone(),
                        calls: 1,
                        refused: 0,
                        executed: 0,
                    };
                    match call.status {
                        ToolCallStatus::Executed => e.executed = 1,
                        ToolCallStatus::Refused => e.refused = 1,
                        _ => {}
                    }
                    self.tool_stats.push(e);
                }
            }
        }
    }
}

/// Errors that can occur in the chat subsystem.
#[derive(Debug, thiserror::Error)]
pub enum ChatError {
    /// The requested session does not exist.
    #[error("session not found: {0}")]
    SessionNotFound(String),
    /// The session has already been closed.
    #[error("session closed: {0}")]
    SessionClosed(String),
    /// The session is busy (an exchange is already in progress).
    #[error("session busy (exchange in progress): {0}")]
    SessionBusy(String),
    /// The referenced agent does not exist in the registry.
    #[error("agent not found: {0}")]
    AgentNotFound(String),
    /// The agent failed to load (Python import error, manifest error, etc.).
    #[error("agent load failed: {0}")]
    AgentLoadFailed(String),
    /// No LLM backend is configured in the runtime.
    #[error("no LLM configured")]
    NoLlmConfigured,
    /// The step budget for this session/agent has been exhausted.
    #[error("step budget exhausted")]
    BudgetExhausted,
    /// An internal error occurred (SQLite, serialization, etc.).
    #[error("internal error: {0}")]
    InternalError(String),
    /// The project referenced by the chat session was not found in the repository.
    #[error("project '{0}' referenced by chat session not found")]
    ProjectNotFound(String),
    /// An approve or reject was requested on a session that is not awaiting
    /// approval. The soft plan gate only resolves from the awaiting-approval
    /// phase, so this fails fast instead of starting execution out of order.
    #[error("session {session_id} is not awaiting approval (current phase: {current_phase})")]
    NotAwaitingApproval {
        /// Session that received the approve/reject request.
        session_id: String,
        /// The phase the session was actually in, as a stable lowercase string.
        current_phase: String,
    },
    /// The hybrid routing cost ceiling was reached and `ceiling_action` is `HardStop`.
    ///
    /// The run stopped cleanly with no data loss. It can be resumed in a new run
    /// with a higher ceiling or with `ceiling_action = "stay_local"`.
    #[error("cost ceiling exceeded: {cost_usd:.4} USD >= {ceiling_usd:.4} USD")]
    CostCeilingExceeded {
        /// Accumulated session cost at the stop, in USD.
        cost_usd: f64,
        /// Configured ceiling, in USD.
        ceiling_usd: f64,
    },
}

/// Parameters for [`PendingChatApprovals::start_timeout`].
pub struct ApprovalTimeoutParams {
    /// Pending-approval key (`"session_id::message_id::tool_call_id"`).
    pub key: String,
    /// Delay after which the approval is auto-refused.
    pub duration: Duration,
    /// Event bus for emitting the timeout event.
    pub event_bus: EventBusSender,
    /// Owning session identifier.
    pub session_id: String,
    /// Message that triggered the approval.
    pub message_id: String,
    /// Unique id of the tool call awaiting approval.
    pub tool_call_id: String,
    /// Name of the tool awaiting approval.
    pub tool_name: String,
}

/// Thread-safe store for pending chat tool approvals.
///
/// Used by `ChatSessionManager` to track oneshot channels for tool approval
/// requests that are waiting for a user decision.
///
/// Key format: `"session_id::message_id::tool_name"`.
///
/// This uses `Arc<Mutex<>>` which is acceptable here because it is a local
/// data structure within the `ChatSessionManager` actor, not shared cross-actors.
pub struct PendingChatApprovals {
    inner: Arc<Mutex<HashMap<String, oneshot::Sender<ToolDecision>>>>,
}

impl PendingChatApprovals {
    /// Create a new empty approval store.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a pending approval and return a receiver to await the decision.
    ///
    /// The key should follow the format `"session_id::message_id::tool_name"`.
    pub fn register(&self, key: String) -> oneshot::Receiver<ToolDecision> {
        let (tx, rx) = oneshot::channel();
        let mut map = self
            .inner
            .lock()
            .expect("PendingChatApprovals lock poisoned");
        map.insert(key, tx);
        rx
    }

    /// Resolve a pending approval by sending the decision to the waiting receiver.
    ///
    /// Returns `true` if the key was found and the decision was sent, `false` otherwise.
    pub fn resolve(&self, key: &str, decision: ToolDecision) -> bool {
        let mut map = self
            .inner
            .lock()
            .expect("PendingChatApprovals lock poisoned");
        if let Some(tx) = map.remove(key) {
            // If the receiver has been dropped, we silently ignore the error.
            let _ = tx.send(decision);
            true
        } else {
            false
        }
    }

    /// Timeout a pending approval by sending a plain `Refuse` (no reason).
    ///
    /// Returns `true` if the key was found and refused, `false` otherwise.
    pub fn timeout(&self, key: &str) -> bool {
        self.resolve(key, ToolDecision::refuse())
    }

    /// Refuse and drop every pending approval belonging to a session.
    ///
    /// Keys follow `"session_id::message_id::tool_name"`, so every entry whose
    /// key starts with `"{session_id}::"` is removed and its waiting receiver
    /// gets a plain [`ToolDecision::refuse()`]. Used when a session closes so an
    /// in-flight ReAct loop blocked on approval unblocks instead of leaking.
    ///
    /// Returns the number of approvals refused.
    pub fn refuse_session(&self, session_id: &str) -> usize {
        let prefix = format!("{session_id}::");
        let mut map = self
            .inner
            .lock()
            .expect("PendingChatApprovals lock poisoned");
        let keys: Vec<String> = map
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect();
        let mut refused = 0;
        for key in keys {
            if let Some(tx) = map.remove(&key) {
                let _ = tx.send(ToolDecision::refuse());
                refused += 1;
            }
        }
        refused
    }

    /// Start a background timeout task that auto-refuses after `duration`.
    ///
    /// If the approval is still pending when the timer fires, it is resolved
    /// with [`ToolDecision::refuse()`] and a [`RuntimeEvent::ChatApprovalTimeout`]
    /// is emitted on the EventBus.
    ///
    /// If the approval has already been resolved before the timeout, this is a no-op.
    pub fn start_timeout(&self, params: ApprovalTimeoutParams) {
        let ApprovalTimeoutParams {
            key,
            duration,
            event_bus,
            session_id,
            message_id,
            tool_call_id,
            tool_name,
        } = params;
        let inner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            tokio::time::sleep(duration).await;

            // Try to resolve, returns false if already resolved
            let still_pending = {
                let mut map = inner.lock().expect("PendingChatApprovals lock poisoned");
                if let Some(tx) = map.remove(&key) {
                    let _ = tx.send(ToolDecision::refuse());
                    true
                } else {
                    false
                }
            };

            if still_pending {
                let _ = event_bus.send(RuntimeEvent::ChatApprovalTimeout {
                    session_id,
                    message_id,
                    tool_call_id,
                    tool_name,
                });
            }
        });
    }
}

impl Clone for PendingChatApprovals {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Default for PendingChatApprovals {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────
// FsHitlDecision + PendingFilesystemApprovals
// ─────────────────────────────────────────────

/// Decision returned by the user for a filesystem HITL request.
#[derive(Debug, Clone)]
pub enum FsHitlDecision {
    /// Approve this specific operation.
    Approve,
    /// Deny this specific operation.
    ///
    /// `reason` is required when the caller tool flagged `reject_reason_required`
    /// and is forwarded to the agent so it can adapt its plan. Callers that do
    /// not require a reason pass `None`.
    Deny { reason: Option<String> },
    /// Approve this operation and also install an "always accept" rule.
    ///
    /// The rule scope is encoded by [`AlwaysAcceptScope`], from the most
    /// restrictive (this tool only) to the most permissive (global).
    ///
    /// `op` and `level` identify the filesystem rule bucket (e.g., `"write"` +
    /// `"medium"`). They are kept alongside the scope so `PrefixRuleEngine` can
    /// persist the correct row.
    AlwaysAllow {
        /// Scope picked by the operator in the approval card.
        scope: AlwaysAcceptScope,
        /// Filesystem operation bucket (`write` / `delete` / `chmod` / `read`).
        op: String,
        /// Risk level bucket (`medium` / `high` / `critical`).
        level: String,
    },
}

impl FsHitlDecision {
    /// Convenience constructor: Deny without a reason (legacy code path).
    #[must_use]
    pub fn deny() -> Self {
        Self::Deny { reason: None }
    }
}

/// Scope of a user-issued "always accept" approval.
///
/// Ordered from least to most permissive. The persistence layer is free to
/// interpret each scope differently:
///
/// - `ThisTool` and `ThisAgent` land in `PrefixRuleEngine` with a matching
///   `tool_name` predicate.
/// - `ThisSession` stays in-memory for the current `ChatManager` session.
/// - `ThisProject` is persisted in `apollia.toml` (or equivalent).
/// - `Global` is persisted in the user-wide `governance.db`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlwaysAcceptScope {
    /// Auto-approve only this exact tool name for the rest of the session.
    ThisTool,
    /// Auto-approve matching ops for the rest of the current chat session.
    /// This is the **default** scope, least sticky, safest.
    ThisSession,
    /// Auto-approve matching ops whenever the requesting agent runs.
    ThisAgent,
    /// Auto-approve matching ops inside the current project workspace.
    ThisProject,
    /// Auto-approve matching ops machine-wide.
    Global,
}

impl AlwaysAcceptScope {
    /// Safe default picked by the UI when the operator clicks "Always accept"
    /// without opening the scope disclosure.
    #[must_use]
    pub fn safe_default() -> Self {
        Self::ThisSession
    }
}

/// Thread-safe store for pending filesystem HITL requests.
///
/// Keyed by `request_id` (UUID v4). Shared between `NativeChatToolInvoker`
/// (which registers and awaits decisions) and `ChatSessionManager` (which resolves
/// them when a `respond_hitl_filesystem` Tauri command arrives).
///
/// This uses `Arc<Mutex<>>`, acceptable because it is an internal data structure
/// coordinating two concurrent async tasks, not a cross-actor shared state.
pub struct PendingFilesystemApprovals {
    inner: Arc<Mutex<HashMap<String, oneshot::Sender<FsHitlDecision>>>>,
}

impl PendingFilesystemApprovals {
    /// Create a new empty store.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a pending HITL request and return a receiver to await the decision.
    pub fn register(&self, request_id: String) -> oneshot::Receiver<FsHitlDecision> {
        let (tx, rx) = oneshot::channel();
        let mut map = self
            .inner
            .lock()
            .expect("PendingFilesystemApprovals lock poisoned");
        map.insert(request_id, tx);
        rx
    }

    /// Resolve a pending request by sending the decision.
    ///
    /// Returns `true` if the request was found and resolved, `false` otherwise.
    pub fn resolve(&self, request_id: &str, decision: FsHitlDecision) -> bool {
        let mut map = self
            .inner
            .lock()
            .expect("PendingFilesystemApprovals lock poisoned");
        if let Some(tx) = map.remove(request_id) {
            let _ = tx.send(decision);
            true
        } else {
            false
        }
    }

    /// Deny all pending requests (used when session is closed).
    pub fn deny_all_pending(&self) {
        let mut map = self
            .inner
            .lock()
            .expect("PendingFilesystemApprovals lock poisoned");
        for (_, tx) in map.drain() {
            let _ = tx.send(FsHitlDecision::deny());
        }
    }
}

impl Clone for PendingFilesystemApprovals {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Default for PendingFilesystemApprovals {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for chat session context window management.
///
/// Controls how many messages from the conversation history are included
/// in the LLM context window and when summarization is triggered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionConfig {
    /// Number of recent messages to include in the sliding window (default 20).
    pub context_window_size: u32,
}

/// Default context window size (number of messages).
const DEFAULT_CONTEXT_WINDOW_SIZE: u32 = 20;

impl Default for ChatSessionConfig {
    fn default() -> Self {
        Self {
            context_window_size: DEFAULT_CONTEXT_WINDOW_SIZE,
        }
    }
}

/// Lightweight summary of a recent session for CLI list display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentSessionSummary {
    /// Session identifier.
    pub id: String,
    /// Chat mode string (`"libre"` or `"agent"`).
    pub mode: String,
    /// Session status string.
    pub status: String,
    /// Content of the first user message in the session, if any.
    pub first_message: Option<String>,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
}

/// Summary of a past session for cross-session context injection.
///
/// Returned by [`ChatSessionRepository::find_relevant_sessions`] and used
/// to build a context block that gives the LLM awareness of previous
/// conversations related to the current topic.
#[derive(Debug, Clone, Serialize)]
pub struct PastSessionSummary {
    /// Identifier of the past session.
    pub session_id: String,
    /// ISO-8601 creation timestamp of the session.
    pub created_at: String,
    /// Summary text produced by the conversation summarizer.
    pub summary: String,
}

/// Lightweight session info for list responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// Session identifier.
    pub id: SessionId,
    /// Chat mode.
    pub mode: ChatMode,
    /// Agent name (if agent mode).
    pub agent_name: Option<String>,
    /// Current status.
    pub status: SessionStatus,
    /// Creation timestamp.
    pub created_at: String,
    /// User-defined display title (falls back to agent_name or mode).
    pub title: Option<String>,
    /// Project this session belongs to (None = standalone).
    pub project_id: Option<String>,
}

/// Detailed session view for single-session responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDetail {
    /// Full session data.
    pub session: ChatSession,
    /// Total number of messages in the session.
    pub message_count: u32,
}

/// Trait for injecting project context into chat system prompts.
///
/// Implemented outside `apollia-runtime` (e.g. in `apollia-desktop`) because
/// the actual project data lives in `apollia-tools` which is a sibling crate.
/// This avoids coupling the runtime to the project persistence layer.
#[async_trait::async_trait]
pub trait ProjectContextProvider: Send + Sync {
    /// Build a context block for a given project ID.
    ///
    /// Returns `None` if the project doesn't exist or has no useful context.
    /// The returned string is injected into the system prompt on the first message.
    async fn build_context(&self, project_id: &str) -> Option<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injected_instruction_provenance_is_user_inject_with_reason() {
        // GIVEN an operator instruction
        let inj = InjectedInstruction {
            session_id: "s1".into(),
            text: "before step X, do Y".into(),
        };

        // WHEN building the provenance for steps created from it
        let prov = inj.provenance();

        // THEN the origin is UserInject and the reason is the operator text
        assert_eq!(prov.origin, apollia_core::plan::StepOrigin::UserInject);
        assert_eq!(prov.reason.as_deref(), Some("before step X, do Y"));
        assert!(prov.at >= 0);
    }

    #[test]
    fn pause_state_defaults_to_running() {
        // GIVEN the default pause state
        // THEN it is the steady Running state
        assert_eq!(PauseState::default(), PauseState::Running);
    }

    #[tokio::test]
    async fn test_pending_approvals_register_resolve() {
        // GIVEN a fresh PendingChatApprovals
        let approvals = PendingChatApprovals::new();

        // WHEN we register a key and then resolve it with Accept
        let rx = approvals.register("sess::msg::tool".to_string());
        let resolved = approvals.resolve("sess::msg::tool", ToolDecision::Accept);

        // THEN the receiver gets Accept and resolve returns true
        assert!(resolved);
        let decision = rx.await.expect("receiver should get a decision");
        assert_eq!(decision, ToolDecision::Accept);
    }

    #[tokio::test]
    async fn test_pending_approvals_resolve_unknown_key() {
        // GIVEN an empty PendingChatApprovals
        let approvals = PendingChatApprovals::new();

        // WHEN we try to resolve a key that was never registered
        let resolved = approvals.resolve("unknown::key::tool", ToolDecision::Accept);

        // THEN it returns false
        assert!(!resolved);
    }

    #[tokio::test]
    async fn test_pending_approvals_timeout() {
        // GIVEN a registered approval
        let approvals = PendingChatApprovals::new();
        let rx = approvals.register("sess::msg::tool".to_string());

        // WHEN we timeout the approval
        let timed_out = approvals.timeout("sess::msg::tool");

        // THEN the receiver gets Refuse
        assert!(timed_out);
        let decision = rx.await.expect("receiver should get a decision");
        assert_eq!(decision, ToolDecision::refuse());
    }

    #[test]
    fn test_chat_mode_sql_roundtrip() {
        // GIVEN each ChatMode variant
        for mode in [ChatMode::Libre, ChatMode::Agent] {
            // WHEN we convert to SQL and back
            let sql = mode.as_sql();
            let restored = ChatMode::from_sql(sql);
            // THEN we get the original value back
            assert_eq!(restored, Some(mode));
        }
        assert_eq!(ChatMode::from_sql("invalid"), None);
    }

    #[test]
    fn test_session_status_sql_roundtrip() {
        // GIVEN each SessionStatus variant
        for status in [
            SessionStatus::Active,
            SessionStatus::Processing,
            SessionStatus::Closed,
        ] {
            // WHEN we convert to SQL and back
            let sql = status.as_sql();
            let restored = SessionStatus::from_sql(sql);
            // THEN we get the original value back
            assert_eq!(restored, Some(status));
        }
        assert_eq!(SessionStatus::from_sql("invalid"), None);
    }

    #[test]
    fn test_chat_role_sql_roundtrip() {
        // GIVEN each ChatRole variant
        for role in [
            ChatRole::User,
            ChatRole::Assistant,
            ChatRole::System,
            ChatRole::Tool,
        ] {
            // WHEN we convert to SQL and back
            let sql = role.as_sql();
            let restored = ChatRole::from_sql(sql);
            // THEN we get the original value back
            assert_eq!(restored, Some(role));
        }
        assert_eq!(ChatRole::from_sql("invalid"), None);
    }

    #[test]
    fn test_chat_error_variants() {
        // GIVEN all ChatError variants
        let errors: Vec<ChatError> = vec![
            ChatError::SessionNotFound("s1".into()),
            ChatError::SessionClosed("s2".into()),
            ChatError::SessionBusy("s3".into()),
            ChatError::AgentNotFound("a1".into()),
            ChatError::AgentLoadFailed("reason".into()),
            ChatError::NoLlmConfigured,
            ChatError::BudgetExhausted,
            ChatError::InternalError("db error".into()),
        ];

        // THEN each has a non-empty Display message
        for err in &errors {
            let msg = format!("{err}");
            assert!(!msg.is_empty());
        }
        // 8 variants (7 from spec + InternalError)
        assert_eq!(errors.len(), 8);
    }

    #[tokio::test]
    async fn test_register_resolve_refuse() {
        // GIVEN PendingChatApprovals
        let approvals = PendingChatApprovals::new();

        // WHEN register puis resolve(Refuse)
        let rx = approvals.register("sess-1::msg-1::bash".to_string());
        let resolved = approvals.resolve("sess-1::msg-1::bash", ToolDecision::refuse());

        // THEN receiver gets Refuse
        assert!(resolved);
        let decision = rx.await.expect("decision");
        assert_eq!(decision, ToolDecision::refuse());
    }

    #[tokio::test]
    async fn test_register_resolve_always_accept() {
        // GIVEN PendingChatApprovals
        let approvals = PendingChatApprovals::new();

        // WHEN register puis resolve(AlwaysAccept)
        let rx = approvals.register("sess-1::msg-1::bash".to_string());
        let resolved =
            approvals.resolve("sess-1::msg-1::bash", ToolDecision::always_accept_default());

        // THEN receiver gets AlwaysAccept
        assert!(resolved);
        let decision = rx.await.expect("decision");
        assert_eq!(decision, ToolDecision::always_accept_default());
    }

    #[tokio::test]
    async fn test_start_timeout_auto_refuse() {
        // GIVEN a registered approval with 100ms timeout
        let approvals = PendingChatApprovals::new();
        let (event_tx, mut event_rx) = tokio::sync::broadcast::channel(16);
        let rx = approvals.register("sess-1::msg-1::bash_executor".to_string());

        // WHEN start_timeout with 100ms
        approvals.start_timeout(ApprovalTimeoutParams {
            key: "sess-1::msg-1::bash_executor".to_string(),
            duration: Duration::from_millis(100),
            event_bus: event_tx,
            session_id: "sess-1".to_string(),
            message_id: "msg-1".to_string(),
            tool_call_id: "bash_executor".to_string(),
            tool_name: "bash_executor".to_string(),
        });

        // THEN receiver gets Refuse after timeout
        let decision = rx.await.expect("decision");
        assert_eq!(decision, ToolDecision::refuse());

        // AND ChatApprovalTimeout event is emitted
        let event = event_rx.recv().await.expect("event");
        match event {
            RuntimeEvent::ChatApprovalTimeout {
                session_id,
                message_id,
                tool_call_id,
                tool_name,
            } => {
                assert_eq!(session_id, "sess-1");
                assert_eq!(message_id, "msg-1");
                assert_eq!(tool_call_id, "bash_executor");
                assert_eq!(tool_name, "bash_executor");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_resolve_before_timeout_no_event() {
        // GIVEN a registered approval guarded by a long (5s) timeout
        let approvals = PendingChatApprovals::new();
        let (event_tx, mut event_rx) = tokio::sync::broadcast::channel(16);
        let rx = approvals.register("sess-1::msg-1::bash".to_string());

        approvals.start_timeout(ApprovalTimeoutParams {
            key: "sess-1::msg-1::bash".to_string(),
            duration: Duration::from_secs(5),
            event_bus: event_tx,
            session_id: "sess-1".to_string(),
            message_id: "msg-1".to_string(),
            tool_call_id: "bash".to_string(),
            tool_name: "bash".to_string(),
        });

        // WHEN the approval is resolved. Registration happened synchronously, so
        // resolve succeeds at once, no delay needed to "let execute register".
        let resolved = approvals.resolve("sess-1::msg-1::bash", ToolDecision::Accept);

        // THEN the receiver gets Accept
        assert!(resolved);
        let decision = rx.await.expect("decision");
        assert_eq!(decision, ToolDecision::Accept);

        // AND no timeout event is queued. The 5s timer is still parked (a fast
        // test cannot span 5s of wall-clock), and even were it to fire it would
        // find the approval already resolved and emit nothing, so the check is
        // independent of timing.
        assert!(event_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_multi_tool_authorization() {
        // GIVEN a session-like setup with bash AlwaysAccept, file_io not authorized
        let authorized: std::collections::HashSet<String> =
            ["bash_executor".to_string()].into_iter().collect();

        // WHEN check bash_executor → authorized, file_io → not authorized
        assert!(authorized.contains("bash_executor"));
        assert!(!authorized.contains("file_io"));
    }

    #[test]
    fn test_chat_session_serialization() {
        // GIVEN a ChatSession
        let session = ChatSession {
            id: "sess-1".into(),
            mode: ChatMode::Agent,
            agent_name: Some("test-agent".into()),
            system_prompt: "You are helpful.".into(),
            status: SessionStatus::Active,
            history: vec![],
            authorized_tools: std::collections::HashSet::new(),
            available_tools: vec!["bash_executor".into()],
            created_at: "2026-03-20T10:00:00Z".into(),
            active_exchange: None,
            llm_backend: None,
            title: None,
            parent_session_id: None,
            fork_depth: 0,
            project_id: None,
            force_project_context_inject: false,
            fs_allow_rules: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashSet::new(),
            )),
            plan_mode: false,
            plan_phase: PlanPhase::Done,
        };

        // WHEN we serialize and deserialize
        let json = serde_json::to_string(&session).expect("serialize");
        let restored: ChatSession = serde_json::from_str(&json).expect("deserialize");

        // THEN fields match
        assert_eq!(restored.id, "sess-1");
        assert_eq!(restored.mode, ChatMode::Agent);
        assert_eq!(restored.agent_name.as_deref(), Some("test-agent"));
    }

    #[test]
    fn test_plan_phase_sql_round_trip() {
        // GIVEN every PlanPhase variant
        for phase in [
            PlanPhase::Discovery,
            PlanPhase::Drafting,
            PlanPhase::AwaitingApproval,
            PlanPhase::Executing,
            PlanPhase::Done,
        ] {
            // WHEN serialized to SQL and parsed back
            let parsed = PlanPhase::from_sql(phase.as_sql());
            // THEN it round-trips
            assert_eq!(parsed, Some(phase));
        }
        // AND an unknown value yields None
        assert_eq!(PlanPhase::from_sql("bogus"), None);
    }

    #[test]
    fn test_plan_phase_default_is_done() {
        // GIVEN the default PlanPhase
        // WHEN constructed via Default
        let phase = PlanPhase::default();
        // THEN it is Done (neutral, never stuck awaiting approval)
        assert_eq!(phase, PlanPhase::Done);
    }

    #[test]
    fn session_metrics_records_mixed_tool_outcomes() {
        // GIVEN a session metrics entry and 3 tool calls across 2 distinct tools
        let mut metrics = SessionMetrics::new("sess-42");
        let calls = vec![
            ToolCallRecord {
                tool_name: "bash".into(),
                input: serde_json::json!({"cmd": "ls"}),
                output: Some("ok".into()),
                status: ToolCallStatus::Executed,
                rationale: None,
                retry_attempts: Vec::new(),
            },
            ToolCallRecord {
                tool_name: "bash".into(),
                input: serde_json::json!({"cmd": "rm -rf /"}),
                output: None,
                status: ToolCallStatus::Refused,
                rationale: None,
                retry_attempts: Vec::new(),
            },
            ToolCallRecord {
                tool_name: "read_file".into(),
                input: serde_json::json!({"path": "/etc/hosts"}),
                output: Some("…".into()),
                status: ToolCallStatus::Executed,
                rationale: None,
                retry_attempts: Vec::new(),
            },
        ];

        // WHEN we aggregate
        metrics.record_tool_calls(&calls);

        // THEN tool_stats has 2 entries with correct counters
        assert_eq!(metrics.tool_stats.len(), 2);
        let bash = metrics
            .tool_stats
            .iter()
            .find(|e| e.tool_name == "bash")
            .expect("bash entry");
        assert_eq!(bash.calls, 2);
        assert_eq!(bash.executed, 1);
        assert_eq!(bash.refused, 1);
        let read = metrics
            .tool_stats
            .iter()
            .find(|e| e.tool_name == "read_file")
            .expect("read_file entry");
        assert_eq!(read.calls, 1);
        assert_eq!(read.executed, 1);
        assert_eq!(read.refused, 0);
    }
}
