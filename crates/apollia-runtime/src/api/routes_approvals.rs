//! REST route for HITL approvals.
//!
//! `GET /api/v1/approvals/pending` lets external clients (e.g. the desktop
//! app) query the pending Human-in-the-Loop approvals without direct access
//! to in-process actor handles. The decisions already taken are read from the
//! audit journal, which is their one record.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// One pending HITL approval entry.
///
/// Carries the same pause as a `GET /api/v1/tasks?status=input_required` item,
/// typed payload included, so a card can be drawn from either list.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PendingApprovalResponse {
    pub task_id: String,
    pub agent_name: String,
    /// Skill that paused, when the pause came from a skill.
    pub skill_id: Option<String>,
    pub prompt: String,
    #[schema(value_type = Option<Object>)]
    pub context: Option<serde_json::Value>,
    /// Typed question or approval of the pause, `null` for a prompt-only pause.
    /// The body of `POST /api/v1/tasks/{id}/resume` answers it.
    #[schema(value_type = Option<Object>)]
    pub payload: Option<serde_json::Value>,
    pub suspended_at: String,
}

/// Standard error response body.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// Handler for `GET /api/v1/approvals/pending`.
///
/// Returns all tasks currently waiting for human approval, enriched with
/// prompt and context from `TaskRepository`.
#[utoipa::path(
    get,
    path = "/api/v1/approvals/pending",
    tag = "governance",
    responses(
        (status = 200, description = "Tasks currently awaiting human approval", body = Vec<PendingApprovalResponse>),
    )
)]
pub async fn list_pending_approvals<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<Vec<PendingApprovalResponse>>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(task_pauses(&state).await))
}

/// Every task waiting for a human, with the prompt, the context and the
/// payload of its pause. Read by this route and by the list of decisions
/// waiting for the user.
pub(crate) async fn task_pauses<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Vec<PendingApprovalResponse> {
    let Some(pending) = state.pending_approvals.as_ref() else {
        return Vec::new();
    };
    let mut approvals = Vec::new();
    for task_id in pending.pending_task_ids() {
        let info = match state.task_repository.as_ref() {
            Some(repo) => repo.get_approval_info(&task_id).await.ok().flatten(),
            None => None,
        };
        approvals.push(match info {
            Some(info) => PendingApprovalResponse {
                task_id,
                agent_name: info.agent_name,
                skill_id: info.skill_id,
                prompt: info.prompt,
                context: Some(info.context),
                payload: info.payload,
                suspended_at: info.suspended_at,
            },
            None => PendingApprovalResponse {
                task_id,
                agent_name: String::new(),
                skill_id: None,
                prompt: String::new(),
                context: None,
                payload: None,
                suspended_at: String::new(),
            },
        });
    }
    approvals
}
