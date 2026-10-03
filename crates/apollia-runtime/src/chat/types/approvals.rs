//! Pending-approval stores for the chat subsystem.
//!
//! Holds the waiting room a chat turn parks in: tool approvals keyed by call.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

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
