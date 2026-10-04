//! `DELETE /api/v1/tasks/{id}/record`, the hard deletion of a stored task.
//!
//! Distinct from `DELETE /api/v1/tasks/{id}`, which cancels a task in flight
//! and keeps its history: this removes the record and its approval rows, the
//! "delete" affordance on a finished or cancelled task.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use super::ErrorResponse;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Response to a deletion.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TaskRecordDeleted {
    /// `true` when a record was removed, `false` when no stored task matched
    /// (already gone, or never persisted).
    pub deleted: bool,
}

/// Handler for `DELETE /api/v1/tasks/{id}/record`.
#[utoipa::path(
    delete,
    path = "/api/v1/tasks/{id}/record",
    tag = "tasks",
    params(("id" = String, Path, description = "Task id")),
    responses(
        (status = 200, description = "Whether a stored record was removed", body = TaskRecordDeleted),
        (status = 500, description = "The task store refused the deletion", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Task persistence is not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn delete_task_record<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(task_id): Path<String>,
) -> Result<Json<TaskRecordDeleted>, (StatusCode, Json<ErrorResponse>)> {
    let repo = state.task_repository.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "task persistence is not available".to_owned(),
            }),
        )
    })?;
    let deleted = repo.delete_task(&task_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;
    Ok(Json(TaskRecordDeleted { deleted }))
}

#[cfg(test)]
mod tests;
