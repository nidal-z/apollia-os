//! Thread-safe registry of pending HITL approvals.
//!
//! Defined in `apollia-core` so it can be shared without a circular dependency
//! between `apollia-oria` (which registers) and `apollia-runtime` (which resolves).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

use crate::result::InputResponseData;

// PendingApprovalError

/// Errors from the pending approvals registry.
#[derive(Debug, thiserror::Error)]
pub enum PendingApprovalError {
    /// No pending approval for the given task.
    #[error("aucune approbation en attente pour la tâche : {0}")]
    NotFound(String),
}

// PendingApprovals

/// Thread-safe registry of tasks awaiting human approval (HITL).
///
/// Each suspended task holds a [`oneshot::Sender`] registered by
/// [`ORIAEngine::execute_direct`]. The `ResumeHandler`
/// calls [`resolve`] to unblock the wait and forward the human decision.
///
/// Cloneable via `Arc`: shared between `ORIAEngine` and the REST routes via `AppState`.
///
/// [`ORIAEngine::execute_direct`]: apollia_oria::engine::ORIAEngine::execute_direct
/// [`resolve`]: PendingApprovals::resolve
#[derive(Debug, Clone)]
pub struct PendingApprovals {
    // Deliberate exception to the one-actor-one-responsibility rule:
    // PendingApprovals is shared between ORIAEngine (which registers the oneshot on
    // an HITL suspension) and the REST routes (which resolve the approval from a
    // separate HTTP thread). Refactoring this into a dedicated actor would add
    // unacceptable latency on the critical path of human approval.
    inner: Arc<Mutex<HashMap<String, oneshot::Sender<InputResponseData>>>>,
}

impl PendingApprovals {
    /// Creates a new empty registry.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Creates a oneshot channel for `task_id`, stores the sender, returns the receiver.
    ///
    /// The caller (typically `ORIAEngine::execute_direct`) awaits `rx` to block until
    /// [`resolve`] is called by the `ResumeHandler`.
    ///
    /// If an entry already existed for this `task_id`, it is replaced: the old sender
    /// is dropped and its receiver gets a closed-channel error.
    ///
    /// [`resolve`]: PendingApprovals::resolve
    pub fn register(&self, task_id: &str) -> oneshot::Receiver<InputResponseData> {
        let (tx, rx) = oneshot::channel();
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.insert(task_id.to_string(), tx);
        rx
    }

    /// Returns the identifiers of the tasks currently awaiting approval.
    ///
    /// Used by the desktop IPC commands to list pending approvals.
    pub fn pending_task_ids(&self) -> Vec<String> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.keys().cloned().collect()
    }

    /// Resolves the pending approval for `task_id`, called by the `ResumeHandler`.
    ///
    /// Removes the entry from the registry and sends `response` on the oneshot channel.
    /// If the ORIA-side receiver has already been dropped (e.g. runtime shutdown),
    /// the send error is silently ignored.
    ///
    /// Returns [`PendingApprovalError::NotFound`] if no approval is registered for
    /// `task_id`, typically when the task is not in `input_required` status.
    pub fn resolve(
        &self,
        task_id: &str,
        response: InputResponseData,
    ) -> Result<(), PendingApprovalError> {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        match guard.remove(task_id) {
            Some(tx) => {
                // Ignore send error - receiver may have been dropped on runtime shutdown.
                let _ = tx.send(response);
                Ok(())
            }
            None => Err(PendingApprovalError::NotFound(task_id.to_string())),
        }
    }
}

impl Default for PendingApprovals {
    fn default() -> Self {
        Self::new()
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn make_response(approved: bool) -> InputResponseData {
        InputResponseData {
            approved,
            reason: None,
            context: Value::Null,
            responded_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    // two concurrent tasks stay isolated from each other
    #[tokio::test]
    async fn test_two_tasks_isolated() {
        // GIVEN two registrations for t-0001 and t-0002
        let pending = PendingApprovals::new();
        let rx1 = pending.register("t-0001");
        let mut rx2 = pending.register("t-0002");

        // WHEN resolve(t-0001, approved)
        pending
            .resolve("t-0001", make_response(true))
            .expect("resolve t-0001 failed");

        // THEN t-0001 receives the response
        let resp1 = rx1.await.expect("rx1 should receive");
        assert!(resp1.approved);

        // THEN t-0002 is still pending (receiver not consumed)
        assert!(rx2.try_recv().is_err(), "t-0002 should still be pending");
    }

    #[tokio::test]
    async fn test_resolve_not_found_returns_error() {
        // GIVEN an empty registry
        let pending = PendingApprovals::new();

        // WHEN resolve is called on a nonexistent task
        let result = pending.resolve("t-9999", make_response(true));

        // THEN NotFound
        assert!(
            matches!(result, Err(PendingApprovalError::NotFound(_))),
            "expected NotFound error"
        );
    }

    #[tokio::test]
    async fn test_register_and_resolve_approved() {
        // GIVEN a registry with t-0001
        let pending = PendingApprovals::new();
        let rx = pending.register("t-0001");

        // WHEN resolve is called with approved=true
        pending
            .resolve("t-0001", make_response(true))
            .expect("resolve failed");

        // THEN rx receives the response with approved=true
        let resp = rx.await.expect("rx should receive");
        assert!(resp.approved);
    }

    #[tokio::test]
    async fn test_register_and_resolve_rejected_with_reason() {
        // GIVEN a registry with t-0002
        let pending = PendingApprovals::new();
        let rx = pending.register("t-0002");

        // WHEN resolve is called with approved=false and a reason
        let mut resp_data = make_response(false);
        resp_data.reason = Some("Budget insuffisant".into());
        pending
            .resolve("t-0002", resp_data)
            .expect("resolve failed");

        // THEN rx receives the response with approved=false and the forwarded reason
        let resp = rx.await.expect("rx should receive");
        assert!(!resp.approved);
        assert_eq!(resp.reason.as_deref(), Some("Budget insuffisant"));
    }
}
