//! Tauri IPC command for the plan-mode approval gate.
//!
//! When the ORIA engine pauses after plan generation it emits
//! `RuntimeEvent::PlanApprovalRequired` and awaits a decision. This command is
//! the desktop resolver: it forwards the operator's decision (approve or
//! reject) to the pending gate through
//! [`apollia_runtime::plan_approval::PlanApprovalHandle`].

use apollia_core::plan::PlanMutation;
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
        /// Revised steps (unified plan step), validated by the engine before
        /// execution.
        revised_steps: Vec<apollia_core::plan::PlanStep>,
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

/// Returns the runtime-level default plan-mode state for new chat sessions.
///
/// Read from the `[chat] plan_mode_default` key of `apollia.toml` at boot. The
/// desktop seeds its per-user preference from this value so the config stays the
/// single source of truth for the default.
#[tauri::command]
pub async fn get_plan_mode_default(state: State<'_, RuntimeHandle>) -> Result<bool, String> {
    Ok(state.plan_mode_default)
}

/// Enables or disables plan mode for `session_id`.
///
/// Delegates to the chat manager, which initializes the phase to `Discovery`
/// when enabling and resets it to a neutral state when disabling. Returns an
/// error string when the chat subsystem is unavailable or the session does not
/// exist.
///
/// # Errors
///
/// Returns `Err(String)` when no chat manager is wired into the runtime handle,
/// or when the toggle targets a session that does not exist.
#[tauri::command]
pub async fn set_plan_mode(
    state: State<'_, RuntimeHandle>,
    session_id: String,
    enabled: bool,
) -> Result<(), String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;
    manager
        .set_plan_mode(session_id, enabled)
        .await
        .map_err(|e| e.to_string())
}

/// Approves the plan awaiting approval for `session_id` (soft conversational gate).
///
/// Moves the session to the executing phase, emits `ChatPlanApproved`, and
/// resumes the agent so it executes the approved plan. Returns an error string
/// when the chat subsystem is unavailable or the session is not awaiting
/// approval.
///
/// # Errors
///
/// Returns `Err(String)` when no chat manager is wired into the runtime handle,
/// when the session does not exist, or when it is not awaiting approval.
#[tauri::command]
pub async fn approve_plan(
    state: State<'_, RuntimeHandle>,
    session_id: String,
) -> Result<(), String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;
    manager
        .approve_plan(&session_id)
        .await
        .map_err(|e| e.to_string())
}

/// Rejects the plan awaiting approval for `session_id` with an optional reason.
///
/// Emits `ChatPlanRejected` and triggers a revision turn; the session stays in
/// the awaiting-approval phase (the gate is soft). Returns an error string when
/// the chat subsystem is unavailable or the session is not awaiting approval.
///
/// # Errors
///
/// Returns `Err(String)` when no chat manager is wired into the runtime handle,
/// when the session does not exist, or when it is not awaiting approval.
#[tauri::command]
pub async fn reject_plan(
    state: State<'_, RuntimeHandle>,
    session_id: String,
    reason: Option<String>,
) -> Result<(), String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;
    manager
        .reject_plan(&session_id, reason)
        .await
        .map_err(|e| e.to_string())
}

/// Returns the ordered plan mutation history recorded for `session_id`.
///
/// Mutations come back in insertion order (oldest first), including removal
/// tombstones, so the desktop scrubber can replay the plan construction
/// revision by revision. A known session with no recorded mutations yields an
/// empty list.
///
/// # Errors
///
/// Returns `Err(String)` when the chat subsystem is unavailable, when the
/// session does not exist, or when the plan history read fails.
#[tauri::command]
pub async fn list_plan_mutations(
    state: State<'_, RuntimeHandle>,
    session_id: String,
) -> Result<Vec<PlanMutation>, String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;
    manager
        .read_plan_mutations(session_id)
        .await
        .map_err(|e| e.to_string())
}
