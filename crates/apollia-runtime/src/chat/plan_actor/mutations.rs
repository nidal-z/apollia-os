//! Plan mutation algebra and the SQL shape of the three plan tables.
//!
//! `build_mutation` is the single place that turns a [`MutationOp`] into a
//! recorded [`PlanMutation`] with its before and after images; the writers
//! below persist the resulting step list and history row.

use apollia_core::plan::{PlanMutation, PlanMutationKind, PlanStatus, PlanStep, StepProvenance};
use rusqlite::params;

use crate::chat::plan_actor::{MutationOp, PlanStoreError};

/// Apply a [`MutationOp`] to `steps` in place and return the recorded mutation.
///
/// The returned [`PlanMutation`] carries the `before` and `after` images and
/// the reason so the history is a faithful, replayable record. A removal keeps
/// the `before` image (tombstone) and a `None` `after`.
pub(super) fn build_mutation(
    steps: &mut Vec<PlanStep>,
    op: MutationOp,
    at: i64,
) -> Result<PlanMutation, PlanStoreError> {
    match op {
        MutationOp::AddStep { step, reason } => {
            let after = step.clone();
            steps.push(step);
            Ok(PlanMutation {
                kind: PlanMutationKind::AddStep,
                step_id: Some(after.step_id.clone()),
                reason,
                before: None,
                after: Some(after),
                at,
            })
        }
        MutationOp::ModifyStep {
            step_id,
            after,
            reason,
        } => {
            let pos = step_position(steps, &step_id)?;
            let before = steps[pos].clone();
            steps[pos] = after.clone();
            Ok(PlanMutation {
                kind: PlanMutationKind::ModifyStep,
                step_id: Some(step_id),
                reason,
                before: Some(before),
                after: Some(after),
                at,
            })
        }
        MutationOp::RemoveStep {
            step_id,
            reason,
            origin,
        } => {
            let pos = step_position(steps, &step_id)?;
            let mut tombstone = steps.remove(pos);
            // Stamp the removal provenance on the tombstone so the history read
            // surfaces the origin, reason and timestamp of the removal itself,
            // not the provenance the step carried while it was live.
            tombstone.provenance = StepProvenance {
                origin,
                reason: reason.clone(),
                at,
            };
            Ok(PlanMutation {
                kind: PlanMutationKind::RemoveStep,
                step_id: Some(step_id),
                reason,
                before: Some(tombstone),
                after: None,
                at,
            })
        }
        MutationOp::Reorder {
            ordered_ids,
            reason,
        } => {
            reorder_steps(steps, &ordered_ids)?;
            Ok(PlanMutation {
                kind: PlanMutationKind::Reorder,
                step_id: None,
                reason,
                before: None,
                after: None,
                at,
            })
        }
        MutationOp::SetStepStatus {
            step_id,
            status,
            reason,
        } => {
            let pos = step_position(steps, &step_id)?;
            let before = steps[pos].clone();
            steps[pos].status = status;
            let after = steps[pos].clone();
            Ok(PlanMutation {
                kind: PlanMutationKind::StatusChange,
                step_id: Some(step_id),
                reason,
                before: Some(before),
                after: Some(after),
                at,
            })
        }
    }
}

/// Locate a step by identifier, erroring when it is absent.
pub(super) fn step_position(steps: &[PlanStep], step_id: &str) -> Result<usize, PlanStoreError> {
    steps
        .iter()
        .position(|s| s.step_id == step_id)
        .ok_or_else(|| PlanStoreError::UnknownStep {
            step_id: step_id.to_string(),
        })
}

/// Reorder `steps` in place to match `ordered_ids` exactly.
pub(super) fn reorder_steps(
    steps: &mut Vec<PlanStep>,
    ordered_ids: &[String],
) -> Result<(), PlanStoreError> {
    if ordered_ids.len() != steps.len() {
        return Err(PlanStoreError::ReorderMismatch);
    }
    let mut reordered = Vec::with_capacity(steps.len());
    for id in ordered_ids {
        let pos = steps
            .iter()
            .position(|s| &s.step_id == id)
            .ok_or(PlanStoreError::ReorderMismatch)?;
        reordered.push(steps.remove(pos));
    }
    *steps = reordered;
    Ok(())
}

/// Rewrite the full step set for a session inside the given transaction.
pub(super) fn write_steps(
    tx: &rusqlite::Transaction<'_>,
    session_id: &str,
    steps: &[PlanStep],
) -> Result<(), PlanStoreError> {
    tx.execute(
        "DELETE FROM session_plan_steps WHERE session_id = ?1",
        params![session_id],
    )?;
    for (ordinal, step) in steps.iter().enumerate() {
        let payload = serde_json::to_string(step)?;
        tx.execute(
            "INSERT INTO session_plan_steps (session_id, step_id, ordinal, payload)
             VALUES (?1, ?2, ?3, ?4)",
            params![session_id, step.step_id, ordinal as i64, payload],
        )?;
    }
    Ok(())
}

/// Append one mutation row to the session's history.
pub(super) fn write_mutation(
    tx: &rusqlite::Transaction<'_>,
    session_id: &str,
    mutation: &PlanMutation,
) -> Result<(), PlanStoreError> {
    let payload = serde_json::to_string(mutation)?;
    tx.execute(
        "INSERT INTO session_plan_mutations (session_id, payload) VALUES (?1, ?2)",
        params![session_id, payload],
    )?;
    Ok(())
}

/// Current Unix timestamp in seconds.
pub(super) fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

/// SQL-storable string for a [`PlanStatus`].
pub(super) fn status_as_sql(status: PlanStatus) -> &'static str {
    match status {
        PlanStatus::Draft => "draft",
        PlanStatus::AwaitingApproval => "awaiting_approval",
        PlanStatus::Executing => "executing",
        PlanStatus::Completed => "completed",
        PlanStatus::Abandoned => "abandoned",
    }
}

/// Parse a [`PlanStatus`] from its SQL string, `None` on an unknown value.
pub(super) fn status_from_sql(raw: &str) -> Option<PlanStatus> {
    match raw {
        "draft" => Some(PlanStatus::Draft),
        "awaiting_approval" => Some(PlanStatus::AwaitingApproval),
        "executing" => Some(PlanStatus::Executing),
        "completed" => Some(PlanStatus::Completed),
        "abandoned" => Some(PlanStatus::Abandoned),
        _ => None,
    }
}
