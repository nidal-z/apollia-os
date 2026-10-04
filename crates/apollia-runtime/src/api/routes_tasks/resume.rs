//! Answering a task pause: the call shared by `POST /api/v1/tasks/{id}/resume`
//! and by a decision taken from the list of decisions waiting for the user.

use axum::http::StatusCode;
use axum::Json;

use apollia_core::{RuntimeEvent, TaskId};

use super::{check_resume, plain, ResumeErrorBody, ResumeRequest, ResumeResponse};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Answer the pause of `task_id` and let its run go on: the one call behind
/// `POST /api/v1/tasks/{id}/resume` and behind a decision taken from the list
/// of decisions waiting for the user.
pub(crate) async fn resume_paused_task<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    task_id: String,
    body: ResumeRequest,
) -> Result<ResumeResponse, (StatusCode, Json<ResumeErrorBody>)> {
    // Check that the TaskRepository is available.
    let repo = match state.task_repository.as_ref() {
        Some(r) => r,
        None => {
            return Err(plain(
                StatusCode::SERVICE_UNAVAILABLE,
                "HITL not configured - task_repository absent".into(),
            ));
        }
    };

    // Check the status via the TaskRepository.
    let db_status = repo.get_task_status(&task_id).await.map_err(|e| {
        tracing::error!(task_id = %task_id, error = %e, "task.status.read.failed");
        plain(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("database error: {e}"),
        )
    })?;

    match db_status.as_deref() {
        // Task absent from the tasks table: 404.
        None => {
            return Err(plain(
                StatusCode::NOT_FOUND,
                format!("task not found: {task_id}"),
            ));
        }
        // Task present but not in input_required: 409.
        Some(status) if status != "input_required" => {
            return Err(plain(
                StatusCode::CONFLICT,
                format!("task '{task_id}' is not in input_required status (current: {status})"),
            ));
        }
        _ => {}
    }

    // Check the answer against the pause it answers, before anything is
    // written: a refused answer leaves the task paused and resumable.
    let pending = repo.pending_pause(&task_id).await.map_err(|e| {
        tracing::error!(task_id = %task_id, error = %e, "task.pause.read.failed");
        plain(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("database error: {e}"),
        )
    })?;
    let input_response = check_resume(
        pending.as_ref(),
        body.approved,
        body.reason.clone(),
        body.answer.clone(),
    )?;

    // Durability before notification: write to the DB before the EventBus.
    repo.save_input_response(&task_id, &input_response)
        .await
        .map_err(|e| {
            tracing::error!(task_id = %task_id, error = %e, "task.input_response.save.failed");
            plain(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to persist response: {e}"),
            )
        })?;

    // Emit TaskResumed on the EventBus.
    let _ = state.event_sender.send(RuntimeEvent::TaskResumed {
        task_id: TaskId::from(task_id.as_str()),
        approved: body.approved,
    });

    // Rebuild the enriched AIPTask and resolve the ORIA oneshot.
    match repo.rebuild_for_resume(&task_id).await {
        Ok(enriched_task) => {
            // Resolve the PendingApprovals oneshot to unblock execute_direct().
            if let Some(pending) = state.pending_approvals.as_ref() {
                match pending.resolve(
                    &task_id,
                    enriched_task
                        .input_response
                        .clone()
                        .unwrap_or_else(|| input_response.clone()),
                ) {
                    Ok(()) => {
                        tracing::info!(
                            task_id = %task_id,
                            approved = body.approved,
                            "task.approval.resolved"
                        );
                    }
                    Err(e) => {
                        // Not a fatal error, the task may have been cleaned up or
                        // PendingApprovals was not registered (e.g. non-ORIA execution).
                        tracing::warn!(
                            task_id = %task_id,
                            error = %e,
                            "task.approval.resolve.failed"
                        );
                    }
                }
            } else {
                tracing::warn!(
                    task_id = %task_id,
                    detail = "the suspended ORIA run stays blocked",
                    "task.approval.resolver.missing"
                );
            }
        }
        Err(e) => {
            tracing::error!(task_id = %task_id, error = %e, "task.resume.rebuild.failed");
            return Err(plain(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to rebuild task for resume: {e}"),
            ));
        }
    }

    Ok(ResumeResponse {
        task_id,
        approved: body.approved,
        status: "working".into(),
    })
}
