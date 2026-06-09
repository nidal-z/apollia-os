//! Tauri IPC command for the plan-mode approval gate.
//!
//! When the ORIA engine pauses after plan generation it emits
//! `RuntimeEvent::PlanApprovalRequired` and awaits a decision. This command is
//! the desktop resolver: it forwards the operator's decision (approve or
//! reject) to the pending gate through
//! [`apollia_runtime::plan_approval::PlanApprovalHandle`].

use apollia_runtime::embedded::RuntimeHandle;
use apollia_runtime::plan_approval::PlanApprovalHandle;
use tauri::State;

/// Operator decision on a proposed plan, deserialized from the frontend.
///
/// Internally tagged on the `decision` field so the Svelte layer sends
/// `{ decision: "approve" }` or `{ decision: "reject", reason: "..." }`.
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum PlanDecisionDto {
    /// Approve the plan as generated; execution starts.
    Approve,
    /// Reject the plan, with optional feedback injected into replanning.
    Reject {
        /// Free-text guidance for the next planning attempt.
        #[serde(default)]
        reason: Option<String>,
    },
    /// Submit an edited plan, executed directly without replanning.
    Edit {
        /// Revised steps validated by the engine before execution.
        revised_steps: Vec<apollia_core::TaskPlanStep>,
    },
}

/// Submits the operator's decision on the plan awaiting approval for `run_id`.
///
/// Resolves the pending gate via [`PlanApprovalHandle`]: approval unblocks
/// execution, rejection triggers replanning with the optional reason.
///
/// # Errors
///
/// Returns `Err(String)` when the plan-gate registry is unavailable, or when no
/// gate is pending for `run_id` (unknown, already resolved, or expired).
#[tauri::command]
pub async fn submit_plan_decision(
    state: State<'_, RuntimeHandle>,
    run_id: String,
    decision: PlanDecisionDto,
) -> Result<(), String> {
    let gates = state
        .plan_gates
        .as_ref()
        .ok_or_else(|| "plan-gate registry not available".to_string())?;
    let handle = PlanApprovalHandle::new(gates.clone());

    match decision {
        PlanDecisionDto::Approve => {
            handle.approve(&run_id).map_err(|e| e.to_string())?;
            tracing::info!(run_id = %run_id, "plan.gate.approved");
        }
        PlanDecisionDto::Reject { reason } => {
            handle.reject(&run_id, reason).map_err(|e| e.to_string())?;
            tracing::info!(run_id = %run_id, "plan.gate.rejected");
        }
        PlanDecisionDto::Edit { revised_steps } => {
            handle
                .edit(&run_id, revised_steps)
                .map_err(|e| e.to_string())?;
            tracing::info!(run_id = %run_id, "plan.gate.edited");
        }
    }

    Ok(())
}
