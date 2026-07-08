//! REST routes for HITL approvals.
//!
//! Exposes two endpoints so external clients (e.g. the desktop app) can query
//! pending and resolved Human-in-the-Loop approvals without direct access to
//! in-process actor handles.
//!
//! `GET /api/v1/approvals/pending`
//! `GET /api/v1/approvals/resolved?limit=<n>&days=<n>`

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// One pending HITL approval entry.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PendingApprovalResponse {
    pub task_id: String,
    pub agent_name: String,
    pub prompt: String,
    #[schema(value_type = Option<Object>)]
    pub context: Option<serde_json::Value>,
    pub suspended_at: String,
}

/// One resolved HITL approval entry.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ResolvedApprovalResponse {
    pub task_id: String,
    pub agent_name: String,
    pub approved: bool,
    pub reason: Option<String>,
    pub wait_duration_ms: Option<i64>,
    pub responded_at: Option<String>,
}

/// Query parameters for `GET /api/v1/approvals/resolved`.
#[derive(Debug, Deserialize)]
pub struct ResolvedQuery {
    pub limit: Option<u32>,
    pub days: Option<u32>,
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
    let pending = match state.pending_approvals.as_ref() {
        Some(pa) => pa,
        None => return Ok(Json(Vec::new())),
    };

    let task_ids = pending.pending_task_ids();
    if task_ids.is_empty() {
        return Ok(Json(Vec::new()));
    }

    let mut approvals = Vec::with_capacity(task_ids.len());

    for task_id in task_ids {
        let entry = match state.task_repository.as_ref() {
            Some(repo) => match repo.get_approval_info(&task_id).await {
                Ok(Some(info)) => PendingApprovalResponse {
                    task_id,
                    agent_name: info.agent_name,
                    prompt: info.prompt,
                    context: Some(info.context),
                    suspended_at: info.suspended_at,
                },
                _ => PendingApprovalResponse {
                    task_id,
                    agent_name: String::new(),
                    prompt: String::new(),
                    context: None,
                    suspended_at: String::new(),
                },
            },
            None => PendingApprovalResponse {
                task_id,
                agent_name: String::new(),
                prompt: String::new(),
                context: None,
                suspended_at: String::new(),
            },
        };
        approvals.push(entry);
    }

    Ok(Json(approvals))
}

/// Handler for `GET /api/v1/approvals/resolved`.
///
/// Returns the last `limit` resolved approvals (approved or rejected)
/// within the last `days` days.
#[utoipa::path(
    get,
    path = "/api/v1/approvals/resolved",
    tag = "governance",
    params(
        ("limit" = Option<u32>, Query, description = "Maximum number of resolved approvals to return (default 20)"),
        ("days" = Option<u32>, Query, description = "Look-back window in days (default 7)"),
    ),
    responses(
        (status = 200, description = "Recently resolved approvals", body = Vec<ResolvedApprovalResponse>),
        (status = 500, description = "Internal error", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_resolved_approvals<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<ResolvedQuery>,
) -> Result<Json<Vec<ResolvedApprovalResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let repo = match state.task_repository.as_ref() {
        Some(r) => r,
        None => return Ok(Json(Vec::new())),
    };

    let limit = query.limit.unwrap_or(20);
    let days = query.days.unwrap_or(7);

    let rows = repo
        .list_resolved_approvals(limit, days)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("failed to list resolved approvals: {e}"),
                }),
            )
        })?;

    let approvals = rows
        .into_iter()
        .map(|row| ResolvedApprovalResponse {
            task_id: row.task_id,
            agent_name: row.agent_name,
            approved: row.approved,
            reason: row.reason,
            wait_duration_ms: row.wait_duration_ms,
            responded_at: row.responded_at,
        })
        .collect();

    Ok(Json(approvals))
}
