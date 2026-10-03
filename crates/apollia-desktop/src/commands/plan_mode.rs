//! Tauri IPC commands for plan mode and the plan-mode approval gate.
//!
//! When the ORIA engine pauses after plan generation it emits
//! `RuntimeEvent::PlanApprovalRequired` and awaits a decision.
//! [`submit_plan_decision`] is the desktop resolver: it forwards the
//! operator's decision (approve, reject or edit) to the pending gate through
//! [`apollia_runtime::plan_approval::PlanApprovalHandle`]. The other commands
//! drive the soft plan gate of a chat session.
//!
//! Every command serves both arms of [`RuntimeLink`]: embedded, it calls the
//! runtime in this process; attached, the daemon's routes under
//! `/api/v1/sessions` and `/api/v1/tasks/{id}/plan-decision`, which call the
//! same runtime methods and answer with the same messages.

use apollia_core::plan::PlanMutation;
use apollia_runtime::api::routes_chat::plan::{
    PlanModeDefault, RejectPlanRequest, SetPlanModeRequest,
};
use apollia_runtime::api::routes_tasks::PlanDecisionRequest;
use apollia_runtime::chat::manager::ChatSessionManagerHandle;
use apollia_runtime::chat::ChatPlanSnapshot;
use apollia_runtime::embedded::RuntimeHandle;
use apollia_runtime::plan_approval::PlanApprovalHandle;
use serde_json::Value;
use tauri::State;

use crate::attached::{segment, AttachedClient, AttachedError, RuntimeLink};

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

impl PlanDecisionDto {
    /// The body of `POST /api/v1/tasks/{id}/plan-decision` for this decision.
    fn into_request(self) -> PlanDecisionRequest {
        let (decision, feedback, revised_steps) = match self {
            Self::Approve => ("approved", None, None),
            Self::Reject { reason } => ("rejected", reason, None),
            Self::Edit { revised_steps } => ("edited", None, Some(revised_steps)),
        };
        PlanDecisionRequest {
            decision: decision.to_owned(),
            feedback,
            revised_steps,
        }
    }
}

/// The chat manager of the runtime in this process.
fn chat_manager(runtime: &RuntimeHandle) -> Result<&ChatSessionManagerHandle, String> {
    runtime
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())
}

/// `/api/v1/sessions/{id}/<tail>` on the daemon, the id encoded.
fn session_path(session_id: &str, tail: &str) -> String {
    format!("/api/v1/sessions/{}/{tail}", segment(session_id))
}

/// Submits the operator's decision on the plan awaiting approval for `run_id`.
///
/// Resolves the pending gate via [`PlanApprovalHandle`]: approval unblocks
/// execution, rejection triggers replanning with the optional reason, an edit
/// runs the revised steps.
///
/// # Errors
///
/// Returns `Err(String)` when the plan-gate registry is unavailable, or when no
/// gate is pending for `run_id` (unknown, already resolved, or expired).
#[tauri::command]
pub async fn submit_plan_decision(
    link: State<'_, RuntimeLink>,
    run_id: String,
    decision: PlanDecisionDto,
) -> Result<(), String> {
    let state = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => return Ok(decide(remote, &run_id, decision).await?),
    };
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
pub async fn get_plan_mode_default(link: State<'_, RuntimeLink>) -> Result<bool, String> {
    match link.inner() {
        RuntimeLink::Embedded(runtime) => Ok(runtime.plan_mode_default),
        RuntimeLink::Attached(remote) => Ok(plan_mode_default(remote).await?),
    }
}

/// Enables or disables plan mode for `session_id`.
///
/// Delegates to the chat manager, which initializes the phase to `Discovery`
/// when enabling and resets it to a neutral state when disabling.
///
/// # Errors
///
/// Returns `Err(String)` when no chat manager is wired into the runtime,
/// or when the toggle targets a session that does not exist.
#[tauri::command]
pub async fn set_plan_mode(
    link: State<'_, RuntimeLink>,
    session_id: String,
    enabled: bool,
) -> Result<(), String> {
    match link.inner() {
        RuntimeLink::Embedded(runtime) => chat_manager(runtime)?
            .set_plan_mode(session_id, enabled)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => Ok(put_plan_mode(remote, &session_id, enabled).await?),
    }
}

/// Approves the plan awaiting approval for `session_id` (soft conversational gate).
///
/// Moves the session to the executing phase, emits `ChatPlanApproved`, and
/// resumes the agent so it executes the approved plan.
///
/// # Errors
///
/// Returns `Err(String)` when no chat manager is wired into the runtime,
/// when the session does not exist, or when it is not awaiting approval.
#[tauri::command]
pub async fn approve_plan(link: State<'_, RuntimeLink>, session_id: String) -> Result<(), String> {
    match link.inner() {
        RuntimeLink::Embedded(runtime) => chat_manager(runtime)?
            .approve_plan(&session_id)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => Ok(approve(remote, &session_id).await?),
    }
}

/// Rejects the plan awaiting approval for `session_id` with an optional reason.
///
/// Emits `ChatPlanRejected` and triggers a revision turn; the session stays in
/// the awaiting-approval phase (the gate is soft).
///
/// # Errors
///
/// Returns `Err(String)` when no chat manager is wired into the runtime,
/// when the session does not exist, or when it is not awaiting approval.
#[tauri::command]
pub async fn reject_plan(
    link: State<'_, RuntimeLink>,
    session_id: String,
    reason: Option<String>,
) -> Result<(), String> {
    match link.inner() {
        RuntimeLink::Embedded(runtime) => chat_manager(runtime)?
            .reject_plan(&session_id, reason)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => Ok(reject(remote, &session_id, reason).await?),
    }
}

/// Returns the current plan snapshot for `session_id`, its plan absent when
/// the session has no plan yet.
///
/// Lets the desktop hydrate the plan tab on mount from authoritative runtime
/// state, so a plan proposed before the tab was opened still renders instead of
/// relying solely on live events.
///
/// # Errors
///
/// Returns `Err(String)` when the chat subsystem is unavailable, when the
/// session does not exist, or when the plan snapshot read fails.
#[tauri::command]
pub async fn get_chat_plan(
    link: State<'_, RuntimeLink>,
    session_id: String,
) -> Result<ChatPlanSnapshot, String> {
    match link.inner() {
        RuntimeLink::Embedded(runtime) => chat_manager(runtime)?
            .get_plan(session_id)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => Ok(remote.get(&session_path(&session_id, "plan")).await?),
    }
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
    link: State<'_, RuntimeLink>,
    session_id: String,
) -> Result<Vec<PlanMutation>, String> {
    match link.inner() {
        RuntimeLink::Embedded(runtime) => chat_manager(runtime)?
            .read_plan_mutations(session_id)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => Ok(remote
            .get(&session_path(&session_id, "plan/mutations"))
            .await?),
    }
}

// ── The attached arm ─────────────────────────────────────────────────────

/// `POST /api/v1/tasks/{run_id}/plan-decision`.
async fn decide(
    remote: &AttachedClient,
    run_id: &str,
    decision: PlanDecisionDto,
) -> Result<(), AttachedError> {
    let path = format!("/api/v1/tasks/{}/plan-decision", segment(run_id));
    let _: Value = remote.post(&path, &decision.into_request()).await?;
    Ok(())
}

/// `GET /api/v1/sessions/plan-mode-default`.
async fn plan_mode_default(remote: &AttachedClient) -> Result<bool, AttachedError> {
    let default: PlanModeDefault = remote.get("/api/v1/sessions/plan-mode-default").await?;
    Ok(default.enabled)
}

/// `PUT /api/v1/sessions/{id}/plan-mode`.
async fn put_plan_mode(
    remote: &AttachedClient,
    session_id: &str,
    enabled: bool,
) -> Result<(), AttachedError> {
    let body = SetPlanModeRequest { enabled };
    let _: Value = remote
        .put(&session_path(session_id, "plan-mode"), &body)
        .await?;
    Ok(())
}

/// `POST /api/v1/sessions/{id}/plan/approve`.
async fn approve(remote: &AttachedClient, session_id: &str) -> Result<(), AttachedError> {
    let _: Value = remote
        .post(&session_path(session_id, "plan/approve"), &())
        .await?;
    Ok(())
}

/// `POST /api/v1/sessions/{id}/plan/reject`.
async fn reject(
    remote: &AttachedClient,
    session_id: &str,
    reason: Option<String>,
) -> Result<(), AttachedError> {
    let body = RejectPlanRequest { reason };
    let _: Value = remote
        .post(&session_path(session_id, "plan/reject"), &body)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests;
