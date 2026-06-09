//! Submission side of the plan gate.
//!
//! The ORIA engine registers a gate in [`PendingPlanGates`] and awaits a
//! decision. [`PlanApprovalHandle`] is the interface the API layer and the
//! EventBus consumer use to resolve that gate: approve to start execution, or
//! reject (with optional feedback) to trigger replanning.

use std::sync::Arc;

use apollia_oria::{PendingPlanGates, PlanGateDecision};

/// Error returned when submitting a plan gate decision.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ApprovalError {
    /// No gate is registered for the given run id (already resolved, expired,
    /// or never opened).
    #[error("no pending gate for run_id '{run_id}'")]
    UnknownRunId {
        /// The run id that had no pending gate.
        run_id: String,
    },
}

/// Handle for submitting a plan gate decision from outside the engine.
///
/// Injected into the API layer (via `AppState`) and the EventBus consumer.
/// Cloneable: the inner registry is shared through an `Arc`.
#[derive(Clone)]
pub struct PlanApprovalHandle {
    pending_gates: Arc<PendingPlanGates>,
}

impl PlanApprovalHandle {
    /// Wrap a shared gate registry.
    pub fn new(pending_gates: Arc<PendingPlanGates>) -> Self {
        Self { pending_gates }
    }

    /// Approve the plan for the given run, unblocking the engine.
    ///
    /// # Errors
    ///
    /// Returns [`ApprovalError::UnknownRunId`] when no gate is pending for
    /// `run_id` (unknown, already resolved, or the engine receiver was dropped).
    pub fn approve(&self, run_id: &str) -> Result<(), ApprovalError> {
        self.submit(run_id, PlanGateDecision::Approved)
    }

    /// Reject the plan for the given run, with optional replanning feedback.
    ///
    /// # Errors
    ///
    /// Returns [`ApprovalError::UnknownRunId`] when no gate is pending for
    /// `run_id`.
    pub fn reject(&self, run_id: &str, feedback: Option<String>) -> Result<(), ApprovalError> {
        self.submit(run_id, PlanGateDecision::Rejected { feedback })
    }

    /// Submit an edited plan for the given run, executed directly without a
    /// replanning round trip.
    ///
    /// # Errors
    ///
    /// Returns [`ApprovalError::UnknownRunId`] when no gate is pending for
    /// `run_id`.
    pub fn edit(
        &self,
        run_id: &str,
        revised_steps: Vec<apollia_core::TaskPlanStep>,
    ) -> Result<(), ApprovalError> {
        self.submit(run_id, PlanGateDecision::Edited { revised_steps })
    }

    fn submit(&self, run_id: &str, decision: PlanGateDecision) -> Result<(), ApprovalError> {
        if self.pending_gates.decide(run_id, decision) {
            Ok(())
        } else {
            Err(ApprovalError::UnknownRunId {
                run_id: run_id.to_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // AC-1: approval unblocks the gate.
    #[tokio::test]
    async fn test_approve_unblocks_gate() {
        // GIVEN a registered gate
        let gates = PendingPlanGates::new();
        let rx = gates.register("run-1");
        let handle = PlanApprovalHandle::new(gates);
        // WHEN it is approved
        handle.approve("run-1").expect("approve should succeed");
        // THEN the engine receiver observes Approved
        let decision = rx.await.expect("gate should resolve");
        assert!(matches!(decision, PlanGateDecision::Approved));
    }

    // AC-2: an unknown run id yields a structured error.
    #[test]
    fn test_approve_unknown_run_id_returns_error() {
        // GIVEN an empty registry
        let gates = PendingPlanGates::new();
        let handle = PlanApprovalHandle::new(gates);
        // WHEN approving an unknown run
        let result = handle.approve("unknown");
        // THEN UnknownRunId is returned
        assert!(matches!(result, Err(ApprovalError::UnknownRunId { .. })));
    }

    // AC-3: a second decision for an already-resolved gate errors cleanly.
    #[tokio::test]
    async fn test_double_approve_returns_error() {
        // GIVEN a gate that was just approved
        let gates = PendingPlanGates::new();
        let _rx = gates.register("run-2");
        let handle = PlanApprovalHandle::new(gates);
        handle.approve("run-2").expect("first approve succeeds");
        // WHEN a second approval arrives
        // THEN it reports the gate is no longer pending
        assert!(matches!(
            handle.approve("run-2"),
            Err(ApprovalError::UnknownRunId { .. })
        ));
    }

    // Rejection carries feedback through to the engine.
    #[tokio::test]
    async fn test_reject_forwards_feedback() {
        // GIVEN a registered gate
        let gates = PendingPlanGates::new();
        let rx = gates.register("run-3");
        let handle = PlanApprovalHandle::new(gates);
        // WHEN it is rejected with feedback
        handle
            .reject("run-3", Some("add a validation step".into()))
            .expect("reject should succeed");
        // THEN the engine receiver observes the feedback
        match rx.await.expect("gate should resolve") {
            PlanGateDecision::Rejected { feedback } => {
                assert_eq!(feedback.as_deref(), Some("add a validation step"));
            }
            PlanGateDecision::Approved | PlanGateDecision::Edited { .. } => {
                panic!("expected rejection")
            }
        }
    }

    // An edit carries the revised steps through to the engine.
    #[tokio::test]
    async fn test_edit_forwards_revised_steps() {
        // GIVEN a registered gate
        let gates = PendingPlanGates::new();
        let rx = gates.register("run-5");
        let handle = PlanApprovalHandle::new(gates);
        // WHEN an edited plan is submitted
        let revised = vec![apollia_core::TaskPlanStep {
            step_id: "s1".into(),
            description: "revised step".into(),
            tool_hint: None,
            depends_on: vec![],
            model_hint: None,
        }];
        handle.edit("run-5", revised).expect("edit should succeed");
        // THEN the engine receiver observes the revised steps
        match rx.await.expect("gate should resolve") {
            PlanGateDecision::Edited { revised_steps } => {
                assert_eq!(revised_steps.len(), 1);
                assert_eq!(revised_steps[0].description, "revised step");
            }
            PlanGateDecision::Approved | PlanGateDecision::Rejected { .. } => {
                panic!("expected edit")
            }
        }
    }
}
