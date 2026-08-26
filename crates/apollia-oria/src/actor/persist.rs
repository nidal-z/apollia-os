//! Persisting a step's outcome and failing a plan.
//!
//! Split out of `actor.rs`: the loop's state stays in the parent, the writes
//! to the plan repository and the terminal-failure paths live here.

use std::collections::HashMap;

use apollia_core::events::RuntimeEvent;
use apollia_core::AIPResult;

use crate::actor::{interpolate_outputs, ActorLoop, StepError};
use crate::plan::PlanStep;

impl ActorLoop {
    /// Persists the side effects of a successfully completed step.
    ///
    /// Saves the observability output, marks the step complete in SQLite, and
    /// emits [`RuntimeEvent::StepCompleted`]. DB errors are logged and ignored
    /// (fire-and-forget), matching the surrounding execution loops.
    pub(super) fn persist_step_success(&self, step_id: &str, output: &str, duration_ms: u64) {
        if let Err(e) =
            self.db
                .save_step_output(step_id, &self.plan.plan_id, output, &self.obs_config)
        {
            tracing::warn!(
                error = %e,
                step_id = %step_id,
                detail = "ignored",
                "step.output.persist.failed"
            );
        }
        if let Err(e) = self.db.complete_step(&self.plan.plan_id, step_id, output) {
            tracing::warn!(
                error = %e,
                step_id = %step_id,
                detail = "ignored",
                "step.complete.persist.failed"
            );
        }
        let _ = self.event_bus.send(RuntimeEvent::StepCompleted {
            task_id: self.plan.task_id.clone().into(),
            plan_id: self.plan.plan_id.clone(),
            step_id: step_id.to_string(),
            duration_ms,
        });
    }
    /// Persists the common failure prefix for a failed step: saves the error
    /// detail and marks the step failed in SQLite.
    ///
    /// Shared by every error arm before more specific plan-level handling.
    /// DB errors are logged and ignored (fire-and-forget).
    pub(super) fn persist_step_failure(&self, step_id: &str, error_msg: &str) {
        if let Err(e) = self
            .db
            .save_step_error(step_id, &self.plan.plan_id, error_msg)
        {
            tracing::warn!(
                error = %e,
                step_id = %step_id,
                detail = "ignored",
                "step.error.persist.failed"
            );
        }
        if let Err(e) = self.db.fail_step(&self.plan.plan_id, step_id, error_msg) {
            tracing::warn!(
                error = %e,
                step_id = %step_id,
                detail = "ignored",
                "step.fail.persist.failed"
            );
        }
    }
    /// Emits a [`RuntimeEvent::StepFailed`] for `step_id`.
    pub(super) fn emit_step_failed(&self, step_id: &str, error: &str, retryable: bool) {
        let _ = self.event_bus.send(RuntimeEvent::StepFailed {
            task_id: self.plan.task_id.clone().into(),
            plan_id: self.plan.plan_id.clone(),
            step_id: step_id.to_string(),
            error: error.to_string(),
            retryable,
        });
    }
    /// Marks the plan failed in SQLite with `reason` and emits
    /// [`RuntimeEvent::PlanFailed`]. DB errors are logged and ignored.
    pub(super) fn fail_plan(&self, reason: &str) {
        if let Err(e) = self.db.fail_plan(&self.plan.plan_id, reason) {
            tracing::warn!(error = %e, detail = "ignored", "plan.fail.persist.failed");
        }
        let _ = self.event_bus.send(RuntimeEvent::PlanFailed {
            task_id: self.plan.task_id.clone().into(),
            plan_id: self.plan.plan_id.clone(),
            reason: reason.to_string(),
        });
    }
    /// Handles every terminal step failure (all error arms except the one that
    /// triggers a replan), performing the step- and plan-level persistence and
    /// events, then returning the matching terminal [`AIPResult`].
    ///
    /// Variants handled:
    /// - retryable error with replans exhausted → `MAX_REPLAN_EXCEEDED`
    /// - [`StepError::RejectedByUser`] → `REJECTED`
    /// - [`StepError::ApprovalChannelClosed`] → `APPROVAL_CHANNEL_CLOSED`
    /// - any other permanent error → `STEP_FAILED`
    ///
    /// `prefix_step_in_message` selects the `STEP_FAILED` detail wording: the
    /// initial-pass loops report `"Step {id} failed: {e}"`, while the
    /// post-replan loop reports the bare error string (preserving the original
    /// per-loop messages verbatim).
    pub(super) fn finalize_terminal_failure(
        &self,
        step_id: &str,
        err: &StepError,
        prefix_step_in_message: bool,
    ) -> AIPResult {
        match err {
            e if e.is_retryable() => {
                // Retryable but replan budget exhausted.
                self.persist_step_failure(step_id, &e.to_string());
                self.emit_step_failed(step_id, &e.to_string(), true);
                self.fail_plan("MAX_REPLAN_EXCEEDED");
                AIPResult::failed(
                    "MAX_REPLAN_EXCEEDED",
                    &format!("{} replanifications dépassées", self.max_replans),
                )
            }
            StepError::RejectedByUser { reason } => {
                self.persist_step_failure(step_id, reason);
                self.fail_plan("REJECTED");
                AIPResult::failed("REJECTED", reason)
            }
            StepError::ApprovalChannelClosed => {
                self.persist_step_failure(step_id, "approval_channel_closed");
                self.fail_plan("APPROVAL_CHANNEL_CLOSED");
                AIPResult::failed(
                    "APPROVAL_CHANNEL_CLOSED",
                    "Approval channel closed - runtime shutting down",
                )
            }
            e => {
                self.persist_step_failure(step_id, &e.to_string());
                self.emit_step_failed(step_id, &e.to_string(), false);
                self.fail_plan(&e.to_string());
                let detail = if prefix_step_in_message {
                    format!("Step {} failed: {}", step_id, e)
                } else {
                    e.to_string()
                };
                AIPResult::failed("STEP_FAILED", &detail)
            }
        }
    }
    /// Persists the pre-execution bookkeeping for `step`: emits
    /// [`RuntimeEvent::StepStarted`], marks the step started in SQLite, and
    /// saves the interpolated input and resolved tool name.
    ///
    /// `step_num` is the 1-based ordinal used for the StepStarted event.
    /// DB errors are logged and ignored (fire-and-forget).
    pub(super) fn persist_step_pre_execution(
        &self,
        step: &PlanStep,
        step_num: usize,
        completed_outputs: &HashMap<String, String>,
    ) {
        let step_id = &step.step_id;
        let total = self.plan.steps.len();
        let _ = self.event_bus.send(RuntimeEvent::StepStarted {
            task_id: self.plan.task_id.clone().into(),
            plan_id: self.plan.plan_id.clone(),
            step_id: step_id.clone(),
            step_num,
            total,
            desc: step.description.clone(),
        });
        if let Err(e) = self.db.start_step(&self.plan.plan_id, step_id) {
            tracing::warn!(
                error = %e,
                step_id = %step_id,
                detail = "ignored",
                "step.start.persist.failed"
            );
        }
        let rendered = interpolate_outputs(&step.description, completed_outputs);
        if let Err(e) =
            self.db
                .save_step_input(step_id, &self.plan.plan_id, &rendered, &self.obs_config)
        {
            tracing::warn!(
                error = %e,
                step_id = %step_id,
                detail = "ignored",
                "step.input.persist.failed"
            );
        }
        let actual_tool = step.tool_hint.as_deref().unwrap_or("llm");
        if let Err(e) = self
            .db
            .save_step_tool(step_id, &self.plan.plan_id, actual_tool)
        {
            tracing::warn!(
                error = %e,
                step_id = %step_id,
                detail = "ignored",
                "step.tool.persist.failed"
            );
        }
    }
    /// Marks the plan failed with `STEP_BUDGET_EXCEEDED` (DB + event) and
    /// returns the corresponding terminal [`AIPResult`] carrying `detail`.
    pub(super) fn fail_plan_budget_exhausted(&self, detail: &str) -> AIPResult {
        self.fail_plan("STEP_BUDGET_EXCEEDED");
        AIPResult::failed("STEP_BUDGET_EXCEEDED", detail)
    }
}
