//! Thread-safe registry of in-flight plan gates.
//!
//! After the Reasoner produces a plan, the engine can suspend execution and wait
//! for an external decision (approve or reject) before starting the `ActorLoop`.
//! Each suspended run holds a [`oneshot::Sender`] registered here by the engine;
//! a consumer (CLI `run --plan`, Desktop review, or the EventBus handler) resolves
//! it through [`PendingPlanGates::decide`].
//!
//! Mirrors the `PendingApprovals` registry used for per-tool HITL: a deliberate
//! shared-state exception to the one-actor rule, kept because human approval sits
//! on the critical path and an extra actor hop would add latency.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

/// Decision received at the plan gate.
#[derive(Debug)]
pub enum PlanGateDecision {
    /// The operator approved the plan: execute it as generated.
    Approved,
    /// The operator rejected the plan, with optional feedback for replanning.
    Rejected {
        /// Free-text guidance injected into the next planning attempt.
        feedback: Option<String>,
    },
}

/// Registry of in-flight plan gates, keyed by run identifier.
///
/// Shared via `Arc` between the engine (which registers a gate and awaits the
/// receiver) and the resolver side (which sends a decision). The inner `Mutex`
/// guards a short critical section (insert or remove one entry); it is never held
/// across an `await`.
#[derive(Debug, Default)]
pub struct PendingPlanGates {
    inner: Mutex<HashMap<String, oneshot::Sender<PlanGateDecision>>>,
}

impl PendingPlanGates {
    /// Creates a new empty registry behind an `Arc`.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(HashMap::new()),
        })
    }

    /// Registers a gate for `run_id` and returns the receiver to await.
    ///
    /// If an entry already existed for this `run_id`, it is replaced: the old
    /// sender is dropped and its receiver observes a closed channel.
    pub fn register(&self, run_id: &str) -> oneshot::Receiver<PlanGateDecision> {
        let (tx, rx) = oneshot::channel();
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.insert(run_id.to_string(), tx);
        rx
    }

    /// Sends a decision for `run_id`, unblocking the engine that awaits it.
    ///
    /// Removes the entry from the registry (so a second decision is a no-op).
    /// Returns `false` when no gate is registered for `run_id`, or when the
    /// engine-side receiver was already dropped.
    pub fn decide(&self, run_id: &str, decision: PlanGateDecision) -> bool {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        match guard.remove(run_id) {
            Some(tx) => tx.send(decision).is_ok(),
            None => false,
        }
    }

    /// Returns the run identifiers currently awaiting a decision.
    pub fn pending_run_ids(&self) -> Vec<String> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // AC-5: two concurrent gates resolve independently.
    #[tokio::test]
    async fn test_two_concurrent_gates_independent() {
        // GIVEN two gates registered for distinct run ids
        let gates = PendingPlanGates::new();
        let rx1 = gates.register("run-1");
        let mut rx2 = gates.register("run-2");

        // WHEN a decision arrives for the first
        assert!(gates.decide("run-1", PlanGateDecision::Approved));

        // THEN the first unblocks and the second is still pending
        let d1 = rx1.await.expect("run-1 should resolve");
        assert!(matches!(d1, PlanGateDecision::Approved));
        assert!(rx2.try_recv().is_err(), "run-2 should still be pending");
    }

    // A decision for an unknown run id is ignored without panic.
    #[test]
    fn test_decide_unknown_run_id_returns_false() {
        // GIVEN an empty registry
        let gates = PendingPlanGates::new();
        // WHEN deciding on an unknown run id
        // THEN it returns false
        assert!(!gates.decide("unknown", PlanGateDecision::Approved));
    }

    // AC-3: dropping the receiver makes decide report a closed channel.
    #[tokio::test]
    async fn test_decide_after_receiver_dropped_returns_false() {
        // GIVEN a registered gate whose receiver is dropped
        let gates = PendingPlanGates::new();
        let rx = gates.register("run-3");
        drop(rx);
        // WHEN a decision is sent
        // THEN the send fails and decide returns false
        assert!(!gates.decide("run-3", PlanGateDecision::Rejected { feedback: None }));
    }

    // A second decision for an already-resolved gate is a silent no-op.
    #[tokio::test]
    async fn test_double_decide_returns_false() {
        // GIVEN a gate that was just resolved
        let gates = PendingPlanGates::new();
        let _rx = gates.register("run-4");
        assert!(gates.decide("run-4", PlanGateDecision::Approved));
        // WHEN a second decision arrives
        // THEN it returns false (entry already removed)
        assert!(!gates.decide("run-4", PlanGateDecision::Approved));
    }
}
