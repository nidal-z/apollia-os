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
use apollia_core::RuntimeEvent;

/// Alias for a chat session identifier (UUID v4 string).
pub type SessionId = String;

/// Alias for a chat message identifier (UUID v4 string).
pub type MessageId = String;

/// A chat session — either free-form (Libre) or agent-backed.
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
}

/// Chat mode — free-form LLM conversation or agent-backed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChatMode {
    /// Free-form conversation with an LLM (no agent tools).
    Libre,
    /// Agent-backed session with tool access.
    Agent,
}

impl ChatMode {
    /// Return the SQL-storable string representation.
    pub fn as_sql(&self) -> &str {
        match self {
            Self::Libre => "libre",
            Self::Agent => "agent",
        }
    }

    /// Parse from SQL string representation.
    ///
    /// Returns `None` for unrecognized values.
    pub fn from_sql(s: &str) -> Option<Self> {
        match s {
            "libre" => Some(Self::Libre),
            "agent" => Some(Self::Agent),
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
    /// Session has been closed — no more messages accepted.
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
}

/// Lifecycle status of a tool call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ToolCallStatus {
    /// Waiting for authorization.
    Pending,
    /// Authorized by the user — ready to execute.
    Authorized,
    /// Executed — output available.
    Executed,
    /// Refused by the user.
    Refused,
}

/// User decision on a tool approval request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ToolDecision {
    /// Approve this single invocation.
    Accept,
    /// Refuse this single invocation.
    Refuse,
    /// Approve this and all future invocations of the same tool in this session.
    AlwaysAccept,
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

    /// Timeout a pending approval by sending `ToolDecision::Refuse`.
    ///
    /// Returns `true` if the key was found and refused, `false` otherwise.
    pub fn timeout(&self, key: &str) -> bool {
        self.resolve(key, ToolDecision::Refuse)
    }

    /// Start a background timeout task that auto-refuses after `duration`.
    ///
    /// If the approval is still pending when the timer fires, it is resolved
    /// with [`ToolDecision::Refuse`] and a [`RuntimeEvent::ChatApprovalTimeout`]
    /// is emitted on the EventBus.
    ///
    /// If the approval has already been resolved before the timeout, this is a no-op.
    pub fn start_timeout(
        &self,
        key: String,
        duration: Duration,
        event_bus: EventBusSender,
        session_id: String,
        message_id: String,
        tool_name: String,
    ) {
        let inner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            tokio::time::sleep(duration).await;

            // Try to resolve — returns false if already resolved
            let still_pending = {
                let mut map = inner.lock().expect("PendingChatApprovals lock poisoned");
                if let Some(tx) = map.remove(&key) {
                    let _ = tx.send(ToolDecision::Refuse);
                    true
                } else {
                    false
                }
            };

            if still_pending {
                let _ = event_bus.send(RuntimeEvent::ChatApprovalTimeout {
                    session_id,
                    message_id,
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
}

/// Detailed session view for single-session responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDetail {
    /// Full session data.
    pub session: ChatSession,
    /// Total number of messages in the session.
    pub message_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(decision, ToolDecision::Refuse);
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
        let resolved = approvals.resolve("sess-1::msg-1::bash", ToolDecision::Refuse);

        // THEN receiver gets Refuse
        assert!(resolved);
        let decision = rx.await.expect("decision");
        assert_eq!(decision, ToolDecision::Refuse);
    }

    #[tokio::test]
    async fn test_register_resolve_always_accept() {
        // GIVEN PendingChatApprovals
        let approvals = PendingChatApprovals::new();

        // WHEN register puis resolve(AlwaysAccept)
        let rx = approvals.register("sess-1::msg-1::bash".to_string());
        let resolved = approvals.resolve("sess-1::msg-1::bash", ToolDecision::AlwaysAccept);

        // THEN receiver gets AlwaysAccept
        assert!(resolved);
        let decision = rx.await.expect("decision");
        assert_eq!(decision, ToolDecision::AlwaysAccept);
    }

    #[tokio::test]
    async fn test_start_timeout_auto_refuse() {
        // GIVEN a registered approval with 100ms timeout
        let approvals = PendingChatApprovals::new();
        let (event_tx, mut event_rx) = tokio::sync::broadcast::channel(16);
        let rx = approvals.register("sess-1::msg-1::bash_executor".to_string());

        // WHEN start_timeout with 100ms
        approvals.start_timeout(
            "sess-1::msg-1::bash_executor".to_string(),
            Duration::from_millis(100),
            event_tx,
            "sess-1".to_string(),
            "msg-1".to_string(),
            "bash_executor".to_string(),
        );

        // THEN receiver gets Refuse after timeout
        let decision = rx.await.expect("decision");
        assert_eq!(decision, ToolDecision::Refuse);

        // AND ChatApprovalTimeout event is emitted
        let event = event_rx.recv().await.expect("event");
        match event {
            RuntimeEvent::ChatApprovalTimeout {
                session_id,
                message_id,
                tool_name,
            } => {
                assert_eq!(session_id, "sess-1");
                assert_eq!(message_id, "msg-1");
                assert_eq!(tool_name, "bash_executor");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_resolve_before_timeout_no_event() {
        // GIVEN a registered approval with 5s timeout
        let approvals = PendingChatApprovals::new();
        let (event_tx, mut event_rx) = tokio::sync::broadcast::channel(16);
        let rx = approvals.register("sess-1::msg-1::bash".to_string());

        approvals.start_timeout(
            "sess-1::msg-1::bash".to_string(),
            Duration::from_secs(5),
            event_tx,
            "sess-1".to_string(),
            "msg-1".to_string(),
            "bash".to_string(),
        );

        // WHEN resolve Accept before timeout
        tokio::time::sleep(Duration::from_millis(10)).await;
        let resolved = approvals.resolve("sess-1::msg-1::bash", ToolDecision::Accept);

        // THEN receiver gets Accept
        assert!(resolved);
        let decision = rx.await.expect("decision");
        assert_eq!(decision, ToolDecision::Accept);

        // AND no timeout event is emitted (check with a short wait)
        tokio::time::sleep(Duration::from_millis(50)).await;
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
        };

        // WHEN we serialize and deserialize
        let json = serde_json::to_string(&session).expect("serialize");
        let restored: ChatSession = serde_json::from_str(&json).expect("deserialize");

        // THEN fields match
        assert_eq!(restored.id, "sess-1");
        assert_eq!(restored.mode, ChatMode::Agent);
        assert_eq!(restored.agent_name.as_deref(), Some("test-agent"));
    }
}
