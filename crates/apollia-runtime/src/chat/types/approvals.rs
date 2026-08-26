//! Pending-approval stores for the chat subsystem.
//!
//! Holds the two waiting rooms a chat turn can park in: tool approvals keyed
//! by call, and filesystem HITL requests keyed by operation.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::chat::types::ToolDecision;
use crate::eventbus::EventBusSender;
use apollia_core::RuntimeEvent;

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
    /// The map guard, recovered when a panic poisoned the mutex.
    ///
    /// Poisoning records that a previous holder panicked; the map it protects
    /// is a `HashMap` of oneshot senders, which no panic can leave half
    /// written. Recovering the guard keeps one panic from turning every later
    /// approval into a second panic, which would strand every ReAct loop
    /// waiting on a decision.
    fn locked(
        inner: &Mutex<HashMap<String, oneshot::Sender<ToolDecision>>>,
    ) -> MutexGuard<'_, HashMap<String, oneshot::Sender<ToolDecision>>> {
        inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

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
        let mut map = Self::locked(&self.inner);
        map.insert(key, tx);
        rx
    }

    /// Resolve a pending approval by sending the decision to the waiting receiver.
    ///
    /// Returns `true` if the key was found and the decision was sent, `false` otherwise.
    pub fn resolve(&self, key: &str, decision: ToolDecision) -> bool {
        let mut map = Self::locked(&self.inner);
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
        let mut map = Self::locked(&self.inner);
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
                let mut map = Self::locked(&inner);
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
/// - `ThisTool` and `ThisAgent` are persisted as `PrefixRuleEngine` rows keyed
///   by `tool_name`; later sessions re-read those rows to seed the name-only
///   authorization set (no argument-prefix matching happens on the chat path).
/// - `ThisSession` stays in-memory for the current `ChatManager` session.
/// - `ThisProject` and `Global` are persisted in the user-wide
///   `governance.db`, with the project's canonical path attached for
///   `ThisProject`.
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
    /// The map guard, recovered when a panic poisoned the mutex.
    ///
    /// Poisoning records that a previous holder panicked; the map it protects
    /// is a `HashMap` of oneshot senders, which no panic can leave half
    /// written. Recovering the guard keeps one panic from turning every later
    /// approval into a second panic, which would strand every ReAct loop
    /// waiting on a decision.
    fn locked(
        inner: &Mutex<HashMap<String, oneshot::Sender<FsHitlDecision>>>,
    ) -> MutexGuard<'_, HashMap<String, oneshot::Sender<FsHitlDecision>>> {
        inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Create a new empty store.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a pending HITL request and return a receiver to await the decision.
    pub fn register(&self, request_id: String) -> oneshot::Receiver<FsHitlDecision> {
        let (tx, rx) = oneshot::channel();
        let mut map = Self::locked(&self.inner);
        map.insert(request_id, tx);
        rx
    }

    /// Resolve a pending request by sending the decision.
    ///
    /// Returns `true` if the request was found and resolved, `false` otherwise.
    pub fn resolve(&self, request_id: &str, decision: FsHitlDecision) -> bool {
        let mut map = Self::locked(&self.inner);
        if let Some(tx) = map.remove(request_id) {
            let _ = tx.send(decision);
            true
        } else {
            false
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

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::RuntimeEvent;

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
        // THEN the authorized tool is found and the other one is not
        assert!(authorized.contains("bash_executor"));
        assert!(!authorized.contains("file_io"));
    }
}
