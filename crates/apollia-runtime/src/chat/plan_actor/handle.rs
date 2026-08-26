//! Clonable caller-side handle to the plan actor.
//!
//! One async method per [`PlanMessage`] variant: each sends the command over
//! the bounded channel and awaits the oneshot reply.

use apollia_core::plan::{Plan, PlanMutation, PlanStep, StepOrigin, StepStatus};
use tokio::sync::{mpsc, oneshot};

use crate::chat::plan_actor::{MutationOp, PlanMessage, PlanStoreError};

/// Clonable handle to the plan actor.
///
/// All methods are async and communicate via a bounded `mpsc` channel. Cloning
/// is cheap (it clones the sender); there is no `Arc<Mutex>`.
#[derive(Clone)]
pub struct PlanHandle {
    pub(crate) tx: mpsc::Sender<PlanMessage>,
}

impl PlanHandle {
    /// Returns the current plan for `session_id`, or `None` when none exists.
    ///
    /// # Errors
    ///
    /// - [`PlanStoreError::Sqlite`] / [`PlanStoreError::Serde`] on a read failure.
    /// - [`PlanStoreError::ActorGone`] when the actor has stopped.
    pub async fn get_plan(&self, session_id: &str) -> Result<Option<Plan>, PlanStoreError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(PlanMessage::GetPlan {
                session_id: session_id.to_string(),
                reply,
            })
            .await
            .map_err(|_| PlanStoreError::ActorGone)?;
        rx.await.map_err(|_| PlanStoreError::ActorGone)?
    }

    /// Replaces the draft plan of `session_id` with `steps`.
    ///
    /// Validates the DAG before writing; on rejection nothing is mutated. The
    /// resulting plan is a fresh [`PlanStatus::Draft`] at revision `0`.
    ///
    /// # Errors
    ///
    /// - [`PlanStoreError::Validation`] when the step set is not a valid DAG.
    /// - [`PlanStoreError::Sqlite`] / [`PlanStoreError::Serde`] on a write failure.
    /// - [`PlanStoreError::ActorGone`] when the actor has stopped.
    pub async fn propose(
        &self,
        session_id: &str,
        steps: Vec<PlanStep>,
        summary: Option<String>,
    ) -> Result<Plan, PlanStoreError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(PlanMessage::Propose {
                session_id: session_id.to_string(),
                steps,
                summary,
                reply,
            })
            .await
            .map_err(|_| PlanStoreError::ActorGone)?;
        rx.await.map_err(|_| PlanStoreError::ActorGone)?
    }

    /// Adds a step to the session plan with a documented reason.
    ///
    /// # Errors
    ///
    /// See [`PlanHandle::propose`] for the shared error variants; additionally
    /// [`PlanStoreError::NoPlan`] when the session has no plan yet.
    pub async fn add_step(
        &self,
        session_id: &str,
        step: PlanStep,
        reason: Option<String>,
    ) -> Result<Plan, PlanStoreError> {
        self.mutate(session_id, MutationOp::AddStep { step, reason })
            .await
    }

    /// Replaces an existing step in place.
    ///
    /// # Errors
    ///
    /// [`PlanStoreError::UnknownStep`] when `step_id` is not in the plan, plus
    /// the shared variants from [`PlanHandle::propose`].
    pub async fn modify_step(
        &self,
        session_id: &str,
        step_id: &str,
        after: PlanStep,
        reason: Option<String>,
    ) -> Result<Plan, PlanStoreError> {
        self.mutate(
            session_id,
            MutationOp::ModifyStep {
                step_id: step_id.to_string(),
                after,
                reason,
            },
        )
        .await
    }

    /// Removes a step, keeping it as a tombstone with the removal provenance.
    ///
    /// The removal is attributed to [`StepOrigin::AgentEdit`]. Use
    /// [`PlanHandle::remove_step_with_origin`] for a replan-driven removal. The
    /// removed step stays retrievable through
    /// [`PlanHandle::plan_with_tombstones`]; [`PlanHandle::get_plan`] excludes it.
    ///
    /// # Errors
    ///
    /// [`PlanStoreError::UnknownStep`] when `step_id` is not in the plan, plus
    /// the shared variants from [`PlanHandle::propose`].
    pub async fn remove_step(
        &self,
        session_id: &str,
        step_id: &str,
        reason: Option<String>,
    ) -> Result<Plan, PlanStoreError> {
        self.remove_step_with_origin(session_id, step_id, reason, StepOrigin::AgentEdit)
            .await
    }

    /// Removes a step, stamping the tombstone with an explicit `origin`.
    ///
    /// A replan-driven removal passes [`StepOrigin::Replan`] so the tombstone
    /// records the revision that dropped the step. Same code path as
    /// [`PlanHandle::remove_step`]: no parallel removal route.
    ///
    /// # Errors
    ///
    /// [`PlanStoreError::UnknownStep`] when `step_id` is not in the plan, plus
    /// the shared variants from [`PlanHandle::propose`].
    pub async fn remove_step_with_origin(
        &self,
        session_id: &str,
        step_id: &str,
        reason: Option<String>,
        origin: StepOrigin,
    ) -> Result<Plan, PlanStoreError> {
        self.mutate(
            session_id,
            MutationOp::RemoveStep {
                step_id: step_id.to_string(),
                reason,
                origin,
            },
        )
        .await
    }

    /// Reorders the steps to match `ordered_ids`.
    ///
    /// # Errors
    ///
    /// [`PlanStoreError::ReorderMismatch`] when `ordered_ids` is not exactly the
    /// current step set, plus the shared variants from [`PlanHandle::propose`].
    pub async fn reorder(
        &self,
        session_id: &str,
        ordered_ids: Vec<String>,
        reason: Option<String>,
    ) -> Result<Plan, PlanStoreError> {
        self.mutate(
            session_id,
            MutationOp::Reorder {
                ordered_ids,
                reason,
            },
        )
        .await
    }

    /// Changes the execution status of a step.
    ///
    /// # Errors
    ///
    /// [`PlanStoreError::UnknownStep`] when `step_id` is not in the plan, plus
    /// the shared variants from [`PlanHandle::propose`].
    pub async fn set_step_status(
        &self,
        session_id: &str,
        step_id: &str,
        status: StepStatus,
        reason: Option<String>,
    ) -> Result<Plan, PlanStoreError> {
        self.mutate(
            session_id,
            MutationOp::SetStepStatus {
                step_id: step_id.to_string(),
                status,
                reason,
            },
        )
        .await
    }

    /// Submits the plan for approval (status becomes `AwaitingApproval`).
    ///
    /// # Errors
    ///
    /// - [`PlanStoreError::NoPlan`] when the session has no plan yet.
    /// - [`PlanStoreError::Sqlite`] / [`PlanStoreError::Serde`] on a write failure.
    /// - [`PlanStoreError::ActorGone`] when the actor has stopped.
    pub async fn submit(&self, session_id: &str) -> Result<Plan, PlanStoreError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(PlanMessage::Submit {
                session_id: session_id.to_string(),
                reply,
            })
            .await
            .map_err(|_| PlanStoreError::ActorGone)?;
        rx.await.map_err(|_| PlanStoreError::ActorGone)?
    }

    /// Records the operator approval (status becomes `Executing`).
    ///
    /// No-op (plan returned unchanged) when the plan is not in
    /// `AwaitingApproval`, so a raced or repeated approval never corrupts the
    /// status or the mutation history.
    ///
    /// # Errors
    ///
    /// - [`PlanStoreError::NoPlan`] when the session has no plan yet.
    /// - [`PlanStoreError::Sqlite`] / [`PlanStoreError::Serde`] on a write failure.
    /// - [`PlanStoreError::ActorGone`] when the actor has stopped.
    pub async fn mark_executing(&self, session_id: &str) -> Result<Plan, PlanStoreError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(PlanMessage::MarkExecuting {
                session_id: session_id.to_string(),
                reply,
            })
            .await
            .map_err(|_| PlanStoreError::ActorGone)?;
        rx.await.map_err(|_| PlanStoreError::ActorGone)?
    }

    /// Returns the recorded mutation history for `session_id`, oldest first.
    ///
    /// The history is the source of truth for replay and audit: it includes the
    /// `before` tombstone of every removed step.
    ///
    /// # Errors
    ///
    /// - [`PlanStoreError::Sqlite`] / [`PlanStoreError::Serde`] on a read failure.
    /// - [`PlanStoreError::ActorGone`] when the actor has stopped.
    pub async fn read_mutations(
        &self,
        session_id: &str,
    ) -> Result<Vec<PlanMutation>, PlanStoreError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(PlanMessage::ReadMutations {
                session_id: session_id.to_string(),
                reply,
            })
            .await
            .map_err(|_| PlanStoreError::ActorGone)?;
        rx.await.map_err(|_| PlanStoreError::ActorGone)?
    }

    /// Returns the plan for `session_id` including tombstoned steps, or `None`.
    ///
    /// Unlike [`PlanHandle::get_plan`], the returned plan re-includes every
    /// removed step as a tombstone carrying its removal provenance (origin,
    /// reason, timestamp), so the construction history can replay a removal
    /// without recomposing the step from the raw mutation log. Tombstones are
    /// appended after the live steps, ordered by removal time (oldest first).
    ///
    /// # Errors
    ///
    /// - [`PlanStoreError::Sqlite`] / [`PlanStoreError::Serde`] on a read failure.
    /// - [`PlanStoreError::ActorGone`] when the actor has stopped.
    pub async fn plan_with_tombstones(
        &self,
        session_id: &str,
    ) -> Result<Option<Plan>, PlanStoreError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(PlanMessage::PlanWithTombstones {
                session_id: session_id.to_string(),
                reply,
            })
            .await
            .map_err(|_| PlanStoreError::ActorGone)?;
        rx.await.map_err(|_| PlanStoreError::ActorGone)?
    }

    /// Signals the actor to stop. Best-effort; ignores a closed channel.
    pub async fn shutdown(&self) {
        let _ = self.tx.send(PlanMessage::Shutdown).await;
    }

    async fn mutate(&self, session_id: &str, op: MutationOp) -> Result<Plan, PlanStoreError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(PlanMessage::Mutate {
                session_id: session_id.to_string(),
                op: Box::new(op),
                reply,
            })
            .await
            .map_err(|_| PlanStoreError::ActorGone)?;
        rx.await.map_err(|_| PlanStoreError::ActorGone)?
    }
}
