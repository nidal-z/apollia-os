//! REST routes for task management, POST/GET/DELETE/resume `/api/v1/tasks`.
//!
//! These routes are the core API surface for submitting, querying, canceling,
//! and resuming agent tasks. They delegate to [`TaskRouterHandle`] for dispatch
//! and status tracking, and to [`TaskRepository`] for HITL persistence.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::router::SubmitError;

use apollia_core::token_budget::TokenBudget;
use apollia_core::{
    AIPInput, AIPPart, DataPart, InputResponseData, RuntimeEvent, TaskId, TaskStatus,
};

/// Request body for `POST /api/v1/tasks`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SubmitTaskRequest {
    /// Identifier of the target agent.
    pub agent_id: String,
    /// Free-form JSON input for the task.
    #[schema(value_type = Object)]
    pub input: serde_json::Value,
    /// Per-run control options (plan-gate / autonomy overrides).
    #[serde(default)]
    #[schema(value_type = Object)]
    pub run_options: apollia_core::RunOptions,
}

/// Response body for task operations.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TaskResponse {
    /// Unique task identifier (UUID v4).
    pub task_id: String,
    /// Current task status.
    pub status: String,
    /// Stable run identifier this task belongs to, used by `audit verify`.
    /// Kept unconditionally so the schema is stable for automation.
    pub run_id: Option<String>,
    /// Task result payload (present when completed).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub result: Option<serde_json::Value>,
    /// Error message (present when failed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Structured failure code parsed from the error (e.g. `BAD_MESSAGE`), so
    /// automation can branch on the code without string-matching the message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// Token budget accumulated over all LLM calls for this task.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub token_budget: Option<TokenBudget>,
}

/// Standard error response body.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Human-readable error description.
    pub error: String,
}

/// Convert a [`SubmitError`] into an HTTP status code and error response.
fn submit_error_to_response(err: SubmitError) -> (StatusCode, Json<ErrorResponse>) {
    let (status, message) = match &err {
        SubmitError::AgentNotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        SubmitError::AgentNotReady(_) => (StatusCode::SERVICE_UNAVAILABLE, err.to_string()),
        SubmitError::AgentUnavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, err.to_string()),
        SubmitError::ConcurrencyLimit(_) => (StatusCode::SERVICE_UNAVAILABLE, err.to_string()),
        SubmitError::NoCoordinator(_) => (StatusCode::SERVICE_UNAVAILABLE, err.to_string()),
        SubmitError::ActorDead => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    };
    (status, Json(ErrorResponse { error: message }))
}

/// Convert JSON input into an [`AIPInput`].
///
/// If `value` is already an A2A-aligned `AIPInput` (i.e. a JSON object with a non-empty
/// `"parts"` array whose items have a `"type"` discriminant), it is deserialized directly.
/// This is the preferred format for CLI and API callers:
/// `{"parts": [{"type": "text", "text": "..."}]}`.
///
/// Any other shape is wrapped as a single `DataPart` for backward compatibility.
fn json_to_aip_input(value: serde_json::Value) -> AIPInput {
    if let Ok(input) = serde_json::from_value::<AIPInput>(value.clone()) {
        if !input.parts.is_empty() {
            return input;
        }
    }
    AIPInput {
        parts: vec![AIPPart::Data(DataPart { data: value })],
    }
}

/// Query parameters for `GET /api/v1/tasks`.
#[derive(Debug, Deserialize)]
pub struct ListTasksQuery {
    /// Optional status filter (e.g. `status=input_required`).
    pub status: Option<String>,
}

/// Response body for `GET /api/v1/tasks`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TaskListResponse {
    /// All tasks matching the optional filter.
    pub tasks: Vec<TaskListItem>,
}

/// One entry in the task list.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TaskListItem {
    /// Unique task identifier.
    pub task_id: String,
    /// Agent that owns this task.
    pub agent_id: String,
    /// Current task status.
    pub status: String,
    /// Failure reason for a failed task (parity with `task status`); `null`
    /// otherwise. Kept unconditionally so the schema is stable for automation.
    pub error: Option<String>,
    /// Structured failure code parsed from the error (e.g. `BAD_MESSAGE`).
    pub error_code: Option<String>,
}

/// Extract the `[CODE]` prefix from an error string, e.g. `[BAD_MESSAGE] ...`
/// -> `BAD_MESSAGE`. Returns `None` when the text has no bracketed code.
fn extract_error_code(error_text: &str) -> Option<String> {
    let rest = error_text.trim_start().strip_prefix('[')?;
    let end = rest.find(']')?;
    let code = &rest[..end];
    (!code.is_empty()).then(|| code.to_string())
}

/// Handler for `GET /api/v1/tasks`.
///
/// Returns all known tasks with their agent and status.
/// Supports an optional `?status=<value>` query parameter to filter results.
#[utoipa::path(
    get,
    path = "/api/v1/tasks",
    tag = "tasks",
    params(("status" = Option<String>, Query, description = "Filter by task status")),
    responses(
        (status = 200, description = "Task list", body = TaskListResponse),
        (status = 503, description = "Task subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_tasks<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<ListTasksQuery>,
) -> Result<Json<TaskListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let all = state
        .router_handle
        .all_tasks()
        .await
        .map_err(submit_error_to_response)?;

    let mut tasks = Vec::new();
    for (task_id, agent_id, status) in all {
        let status_str = serde_json::to_value(&status)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| format!("{status:?}"));

        if let Some(ref filter) = query.status {
            if &status_str != filter {
                continue;
            }
        }

        let task_id = task_id.to_string();
        // Parity with `task status`: surface the failure reason (and structured
        // code) on failed tasks so dashboards built on `task list` see it too.
        let (error, error_code) = if status == TaskStatus::Failed {
            let err = state
                .router_handle
                .get_output(&task_id)
                .await
                .ok()
                .flatten();
            let code = err.as_deref().and_then(extract_error_code);
            (err, code)
        } else {
            (None, None)
        };

        tasks.push(TaskListItem {
            task_id,
            agent_id: agent_id.to_string(),
            status: status_str,
            error,
            error_code,
        });
    }

    Ok(Json(TaskListResponse { tasks }))
}

/// Handler for `POST /api/v1/tasks`.
///
/// Submits a new task to the specified agent via the TaskRouter.
/// Returns 202 Accepted with the generated task_id on success.
#[utoipa::path(
    post,
    path = "/api/v1/tasks",
    tag = "tasks",
    request_body = SubmitTaskRequest,
    responses(
        (status = 202, description = "Task accepted", body = TaskResponse),
        (status = 404, description = "Agent not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Agent unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn submit_task<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(req): Json<SubmitTaskRequest>,
) -> Result<(StatusCode, Json<TaskResponse>), (StatusCode, Json<ErrorResponse>)> {
    let input = json_to_aip_input(req.input);

    let task_id = state
        .router_handle
        .submit_with_options(&req.agent_id, input, req.run_options)
        .await
        .map_err(submit_error_to_response)?;

    Ok((
        StatusCode::ACCEPTED,
        Json(TaskResponse {
            task_id: task_id.to_string(),
            status: "submitted".into(),
            run_id: None,
            result: None,
            error: None,
            error_code: None,
            token_budget: None,
        }),
    ))
}

/// Handler for `GET /api/v1/tasks/{id}`.
///
/// Returns the current status of a task. Returns 404 if the task does not exist.
#[utoipa::path(
    get,
    path = "/api/v1/tasks/{id}",
    tag = "tasks",
    params(("id" = String, Path, description = "Task id")),
    responses(
        (status = 200, description = "Task detail", body = TaskResponse),
        (status = 404, description = "Task not found", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_task<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(task_id): Path<String>,
) -> Result<Json<TaskResponse>, (StatusCode, Json<ErrorResponse>)> {
    let status = state
        .router_handle
        .get_status(&task_id)
        .await
        .map_err(submit_error_to_response)?;

    match status {
        Some(s) => {
            let is_terminal = matches!(
                s,
                TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Canceled
            );
            // The router stores the *same* output text for both Completed and
            // Failed tasks (the coordinator encodes [CODE] message details=...
            // when a worker reports failure). Route it to `result` on success
            // and `error` on failure so a single GET tells the CLI the full
            // story without having to also tail the SSE stream.
            let stored_output = if is_terminal {
                state
                    .router_handle
                    .get_output(&task_id)
                    .await
                    .ok()
                    .flatten()
            } else {
                None
            };
            let (result, error) = match s {
                TaskStatus::Completed => (stored_output.map(serde_json::Value::String), None),
                TaskStatus::Failed => (None, stored_output),
                _ => (None, None),
            };
            // Fetch token budget for terminal tasks.
            let token_budget = if is_terminal {
                state.router_handle.get_budget().await.ok().flatten()
            } else {
                None
            };
            let run_id = match &state.task_repository {
                Some(repo) => repo.get_run_id(&task_id).await.ok().flatten(),
                None => None,
            };
            let error_code = error.as_deref().and_then(extract_error_code);
            Ok(Json(TaskResponse {
                task_id,
                status: serde_json::to_value(&s)
                    .ok()
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_else(|| format!("{s:?}")),
                run_id,
                result,
                error,
                error_code,
                token_budget,
            }))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("task not found: {task_id}"),
            }),
        )),
    }
}

/// Handler for `DELETE /api/v1/tasks/{id}`.
///
/// Cancels a running task. Returns 404 if the task does not exist.
#[utoipa::path(
    delete,
    path = "/api/v1/tasks/{id}",
    tag = "tasks",
    params(("id" = String, Path, description = "Task id")),
    responses(
        (status = 200, description = "Task cancelled", body = TaskResponse),
        (status = 404, description = "Task not found", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn cancel_task<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(task_id): Path<String>,
) -> Result<Json<TaskResponse>, (StatusCode, Json<ErrorResponse>)> {
    let status = state
        .router_handle
        .cancel(&task_id)
        .await
        .map_err(submit_error_to_response)?;

    match status {
        Some(s) => Ok(Json(TaskResponse {
            task_id,
            status: serde_json::to_value(&s)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| format!("{s:?}")),
            run_id: None,
            result: None,
            error: None,
            error_code: None,
            token_budget: None,
        })),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("task not found: {task_id}"),
            }),
        )),
    }
}

// ResumeHandler, POST /api/v1/tasks/{id}/resume

/// Request body for `POST /api/v1/tasks/{id}/resume`.
///
/// The operator submits a decision (`approved`) and an optional reason.
/// The `approved` field is mandatory; omitting it produces HTTP 422.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ResumeRequest {
    /// `true` to approve, `false` to reject.
    pub approved: bool,
    /// Reason for the decision, optional, mainly useful when rejecting.
    pub reason: Option<String>,
}

/// Response body for `POST /api/v1/tasks/{id}/resume`.
///
/// Returned with HTTP 200 when the resume is recorded successfully.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ResumeResponse {
    /// Identifier of the resumed task.
    pub task_id: String,
    /// Operator decision.
    pub approved: bool,
    /// New task status (`"working"` after approval or rejection).
    pub status: String,
}

/// Handler for `POST /api/v1/tasks/{id}/resume`.
///
/// Validates that the task is in `input_required` status, persists the human
/// decision to SQLite, emits `RuntimeEvent::TaskResumed` on the EventBus,
/// and rebuilds the enriched `AIPTask` for the ORIA relaunch.
///
/// ## HTTP codes
/// - `200 OK`, resume recorded successfully
/// - `404 Not Found`, task unknown to the HITL system
/// - `409 Conflict`, task known but not in `input_required` status
/// - `503 Service Unavailable`, HITL not configured (`task_repository` absent)
/// - `500 Internal Server Error`, SQLite or internal error
#[utoipa::path(
    post,
    path = "/api/v1/tasks/{id}/resume",
    tag = "tasks",
    params(("id" = String, Path, description = "Task id")),
    request_body = ResumeRequest,
    responses(
        (status = 200, description = "Resume recorded", body = ResumeResponse),
        (status = 404, description = "Task not found", body = crate::api::openapi::ApiErrorBody),
        (status = 409, description = "Task not awaiting input", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "HITL not configured", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn resume_task<B: ExecutionBackend + Clone>(
    Path(task_id): Path<String>,
    State(state): State<AppState<B>>,
    Json(body): Json<ResumeRequest>,
) -> Result<Json<ResumeResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Check that the TaskRepository is available.
    let repo = match state.task_repository.as_ref() {
        Some(r) => r,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "HITL not configured - task_repository absent".into(),
                }),
            ));
        }
    };

    // Check the status via the TaskRepository.
    let db_status = repo.get_task_status(&task_id).await.map_err(|e| {
        tracing::error!(task_id = %task_id, error = %e, "get_task_status failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("database error: {e}"),
            }),
        )
    })?;

    match db_status.as_deref() {
        // Task absent from the tasks table: 404.
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("task not found: {task_id}"),
                }),
            ));
        }
        // Task present but not in input_required: 409.
        Some(status) if status != "input_required" => {
            return Err((
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    error: format!(
                        "task '{task_id}' is not in input_required status (current: {status})"
                    ),
                }),
            ));
        }
        _ => {}
    }

    // Build the human response with an ISO 8601 timestamp.
    let responded_at = chrono::Utc::now().to_rfc3339();
    let input_response = InputResponseData {
        approved: body.approved,
        reason: body.reason.clone(),
        context: serde_json::Value::Object(serde_json::Map::new()),
        responded_at,
    };

    // Durability before notification: write to the DB before the EventBus.
    repo.save_input_response(&task_id, &input_response)
        .await
        .map_err(|e| {
            tracing::error!(task_id = %task_id, error = %e, "save_input_response failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("failed to persist response: {e}"),
                }),
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
                    enriched_task.input_response.clone().unwrap_or(
                        apollia_core::InputResponseData {
                            approved: body.approved,
                            reason: body.reason.clone(),
                            context: serde_json::Value::Null,
                            responded_at: chrono::Utc::now().to_rfc3339(),
                        },
                    ),
                ) {
                    Ok(()) => {
                        tracing::info!(
                            task_id = %task_id,
                            approved = body.approved,
                            "PendingApprovals resolved - ORIA execute_direct unblocked"
                        );
                    }
                    Err(e) => {
                        // Not a fatal error, the task may have been cleaned up or
                        // PendingApprovals was not registered (e.g. non-ORIA execution).
                        tracing::warn!(
                            task_id = %task_id,
                            error = %e,
                            "PendingApprovals.resolve failed - task may not be suspended via ORIA"
                        );
                    }
                }
            } else {
                tracing::warn!(
                    task_id = %task_id,
                    "PendingApprovals not configured in AppState - ORIA will not be unblocked"
                );
            }
        }
        Err(e) => {
            tracing::error!(task_id = %task_id, error = %e, "rebuild_for_resume failed");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("failed to rebuild task for resume: {e}"),
                }),
            ));
        }
    }

    Ok(Json(ResumeResponse {
        task_id,
        approved: body.approved,
        status: "working".into(),
    }))
}

// Plan gate decision, POST /api/v1/tasks/{id}/plan-decision

/// Request body for `POST /api/v1/tasks/{id}/plan-decision`.
///
/// The operator approves the generated plan or rejects it with optional
/// feedback used to guide replanning.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct PlanDecisionRequest {
    /// `"approved"` to execute the plan, `"rejected"` to replan.
    pub decision: String,
    /// Optional feedback injected into the next planning attempt on rejection.
    #[serde(default)]
    pub feedback: Option<String>,
}

/// Response body for `POST /api/v1/tasks/{id}/plan-decision`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PlanDecisionResponse {
    /// Run identifier whose gate was resolved.
    pub run_id: String,
    /// Decision applied (`"approved"` or `"rejected"`).
    pub decision: String,
}

/// Handler for `POST /api/v1/tasks/{id}/plan-decision`.
///
/// Resolves the plan gate registered for the run, unblocking the engine that
/// paused after plan generation. The path id is the run identifier (the task
/// id on the orchestrated path).
///
/// ## HTTP codes
/// - `200 OK`, decision recorded and the gate resolved
/// - `400 Bad Request`, unknown `decision` value
/// - `404 Not Found`, no gate pending for this run (unknown, resolved, expired)
/// - `503 Service Unavailable`, the plan gate is not configured
#[utoipa::path(
    post,
    path = "/api/v1/tasks/{id}/plan-decision",
    tag = "tasks",
    params(("id" = String, Path, description = "Run id (task id on the orchestrated path)")),
    request_body = PlanDecisionRequest,
    responses(
        (status = 200, description = "Plan decision recorded", body = PlanDecisionResponse),
        (status = 400, description = "Unknown decision value", body = crate::api::openapi::ApiErrorBody),
        (status = 404, description = "No gate pending", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Plan gate not configured", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn submit_plan_decision<B: ExecutionBackend + Clone>(
    Path(run_id): Path<String>,
    State(state): State<AppState<B>>,
    Json(body): Json<PlanDecisionRequest>,
) -> Result<Json<PlanDecisionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let gates = match state.plan_gates.as_ref() {
        Some(g) => g,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "plan gate not configured".into(),
                }),
            ));
        }
    };
    let handle = crate::plan_approval::PlanApprovalHandle::new(gates.clone());

    let outcome = match body.decision.as_str() {
        "approved" | "approve" => handle.approve(&run_id),
        "rejected" | "reject" => handle.reject(&run_id, body.feedback.clone()),
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("invalid decision '{other}', expected 'approved' or 'rejected'"),
                }),
            ));
        }
    };

    match outcome {
        Ok(()) => Ok(Json(PlanDecisionResponse {
            run_id,
            decision: body.decision,
        })),
        Err(e) => {
            tracing::warn!(run_id = %run_id, error = %e, "plan.gate.decision.no_pending");
            Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinator::ExecutionBackend;
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use crate::router::TaskRouterHandle;
    use apollia_core::{
        AIPResult, AgentManifest, InputResponseData, ProcessState, RuntimeEvent, TaskStatus,
    };
    use std::future::Future;
    use std::pin::Pin;
    use tokio::sync::broadcast;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::{get, post};
    use axum::Router;
    use tower::ServiceExt;

    use crate::coordinator::ExecutionCoordinator;

    /// Minimal ExecutionBackend for testing (completes instantly).
    #[derive(Clone)]
    struct MockBackend;

    impl From<crate::coordinator::DynBackend> for MockBackend {
        fn from(_: crate::coordinator::DynBackend) -> Self {
            MockBackend
        }
    }

    impl ExecutionBackend for MockBackend {
        fn execute(
            &self,
            task: apollia_core::AIPTask,
        ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
            Box::pin(async move {
                Ok(AIPResult {
                    task_id: task.task_id,
                    status: TaskStatus::Completed,
                    output: Vec::new(),
                    error: None,
                    artifacts: Vec::new(),
                    input_required_data: None,
                })
            })
        }
    }

    /// Backend that blocks forever, used for cancel tests to avoid race with TaskCompleted.
    #[derive(Clone)]
    struct NeverMockBackend;

    impl ExecutionBackend for NeverMockBackend {
        fn execute(
            &self,
            _task: apollia_core::AIPTask,
        ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
            Box::pin(async {
                std::future::pending::<()>().await;
                unreachable!()
            })
        }
    }

    fn test_manifest(name: &str) -> AgentManifest {
        AgentManifest {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            description: String::new(),
            tools_required: vec![],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
            execution_mode: "auto".to_string(),
            supports_mailbox: false,
            mailbox_allowlist: None,
            system_prompt: None,
            tools_requiring_approval: vec![],
            llm_backend: None,
            packages: vec![],
            memory_config: None,
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            user_memory_write: false,
            datasources: vec![],
            templates: vec![],
            secrets: vec![],
            check_commands: vec![],
        }
    }

    fn test_router() -> Router {
        let (event_tx, _) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);
        let state = AppState {
            router_handle,
            registry_handle,
            event_sender: event_tx,
            agent_loader: std::sync::Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: MockBackend,
            llm_router: crate::api::server::empty_shared_llm_router(),
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            plan_gates: None,
            notification_config: None,
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: None,
            audit_journal: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            notification_repo: None,
            notification_engine_handle: None,
            chat_manager: None,
            plan_cache: None,
            mailbox_handle: None,
            user_memory: None,
            stt_engine: None,
            stt_repository: None,
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
        };
        Router::new()
            .route("/api/v1/tasks", post(submit_task::<MockBackend>))
            .route(
                "/api/v1/tasks/:id",
                get(get_task::<MockBackend>).delete(cancel_task::<MockBackend>),
            )
            .with_state(state)
    }

    fn state_with_plan_gates(
        gates: std::sync::Arc<apollia_oria::PendingPlanGates>,
    ) -> AppState<MockBackend> {
        let (event_tx, _) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);
        AppState {
            router_handle,
            registry_handle,
            event_sender: event_tx,
            agent_loader: std::sync::Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: MockBackend,
            llm_router: crate::api::server::empty_shared_llm_router(),
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            plan_gates: Some(gates),
            notification_config: None,
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: None,
            audit_journal: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            notification_repo: None,
            notification_engine_handle: None,
            chat_manager: None,
            plan_cache: None,
            mailbox_handle: None,
            user_memory: None,
            stt_engine: None,
            stt_repository: None,
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
        }
    }

    /// GIVEN a run paused at the plan gate
    /// WHEN POST plan-decision with `approved` is handled
    /// THEN it returns 200 and the engine-side receiver observes Approved.
    #[tokio::test]
    async fn test_plan_decision_approves_pending_gate() {
        // GIVEN a registered gate
        let gates = apollia_oria::PendingPlanGates::new();
        let rx = gates.register("run-approve");
        let state = state_with_plan_gates(gates);

        // WHEN the approval decision is submitted
        let resp = submit_plan_decision(
            Path("run-approve".to_string()),
            State(state),
            Json(PlanDecisionRequest {
                decision: "approved".into(),
                feedback: None,
            }),
        )
        .await;

        // THEN the handler succeeds and the gate resolves to Approved
        assert!(resp.is_ok());
        let decision = rx.await.expect("gate should resolve");
        assert!(matches!(decision, apollia_oria::PlanGateDecision::Approved));
    }

    /// GIVEN no gate pending for a run
    /// WHEN POST plan-decision is handled
    /// THEN it returns 404 Not Found.
    #[tokio::test]
    async fn test_plan_decision_unknown_run_is_not_found() {
        // GIVEN an empty gate registry
        let gates = apollia_oria::PendingPlanGates::new();
        let state = state_with_plan_gates(gates);

        // WHEN a decision is submitted for an unknown run
        let resp = submit_plan_decision(
            Path("missing".to_string()),
            State(state),
            Json(PlanDecisionRequest {
                decision: "approved".into(),
                feedback: None,
            }),
        )
        .await;

        // THEN it is a 404
        let err = resp.err().expect("should fail");
        assert_eq!(err.0, StatusCode::NOT_FOUND);
    }

    /// Build a test environment with an active agent and coordinator registered.
    async fn test_router_with_agent() -> (Router, String) {
        let (event_tx, _) = broadcast::channel::<RuntimeEvent>(64);
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);

        // Register agent and set Active
        let agent_id = registry_handle
            .register(test_manifest("test-agent"))
            .await
            .expect("register failed");
        registry_handle
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .expect("activate failed");

        // Register coordinator
        let coordinator =
            ExecutionCoordinator::new(agent_id.clone(), 1, event_tx.clone(), MockBackend);
        router_handle
            .register_coordinator(agent_id.clone(), coordinator)
            .await
            .expect("register coordinator failed");

        let state = AppState {
            router_handle,
            registry_handle,
            event_sender: event_tx,
            agent_loader: std::sync::Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: MockBackend,
            llm_router: crate::api::server::empty_shared_llm_router(),
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            plan_gates: None,
            notification_config: None,
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: None,
            audit_journal: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            notification_repo: None,
            notification_engine_handle: None,
            chat_manager: None,
            plan_cache: None,
            mailbox_handle: None,
            user_memory: None,
            stt_engine: None,
            stt_repository: None,
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
        };
        let router = Router::new()
            .route("/api/v1/tasks", post(submit_task::<MockBackend>))
            .route(
                "/api/v1/tasks/:id",
                get(get_task::<MockBackend>).delete(cancel_task::<MockBackend>),
            )
            .with_state(state);

        (router, agent_id.to_string())
    }

    /// Build a test environment with a never-completing backend, for cancel tests.
    async fn test_router_with_blocking_agent() -> (Router, String) {
        let (event_tx, _) = broadcast::channel::<RuntimeEvent>(64);
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<NeverMockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);

        let agent_id = registry_handle
            .register(test_manifest("blocking-agent"))
            .await
            .expect("register failed");
        registry_handle
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .expect("activate failed");

        let coordinator =
            ExecutionCoordinator::new(agent_id.clone(), 1, event_tx.clone(), NeverMockBackend);
        router_handle
            .register_coordinator(agent_id.clone(), coordinator)
            .await
            .expect("register coordinator failed");

        let state = AppState {
            router_handle,
            registry_handle,
            event_sender: event_tx,
            agent_loader: std::sync::Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: NeverMockBackend,
            llm_router: crate::api::server::empty_shared_llm_router(),
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            plan_gates: None,
            notification_config: None,
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: None,
            audit_journal: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            notification_repo: None,
            notification_engine_handle: None,
            chat_manager: None,
            plan_cache: None,
            mailbox_handle: None,
            user_memory: None,
            stt_engine: None,
            stt_repository: None,
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
        };
        let router = Router::new()
            .route("/api/v1/tasks", post(submit_task::<NeverMockBackend>))
            .route(
                "/api/v1/tasks/:id",
                get(get_task::<NeverMockBackend>).delete(cancel_task::<NeverMockBackend>),
            )
            .with_state(state);

        (router, agent_id.to_string())
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), 4096)
            .await
            .expect("read body");
        serde_json::from_slice(&bytes).expect("parse json")
    }

    #[tokio::test]
    async fn test_submit_task_returns_202() {
        // GIVEN an active agent with a coordinator
        let (router, agent_id) = test_router_with_agent().await;

        // WHEN POST /api/v1/tasks with a valid agent_id
        let body = serde_json::json!({
            "agent_id": agent_id,
            "input": {"prompt": "Bonjour"}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/tasks")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 202 Accepted with a task_id and status "submitted"
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let json = body_json(resp).await;
        assert_eq!(json["status"], "submitted");
        assert!(json["task_id"].is_string());
        assert!(!json["task_id"].as_str().expect("task_id str").is_empty());
    }

    #[tokio::test]
    async fn test_submit_task_unknown_agent_returns_404() {
        // GIVEN no agent "ghost-agent" registered
        let router = test_router();

        // WHEN POST /api/v1/tasks with agent_id "ghost-agent"
        let body = serde_json::json!({
            "agent_id": "ghost-agent",
            "input": {"prompt": "Hello"}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/tasks")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 404 with an error
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let json = body_json(resp).await;
        assert!(json["error"]
            .as_str()
            .expect("error str")
            .contains("not found"));
    }

    #[tokio::test]
    async fn test_submit_task_invalid_body_returns_400() {
        // GIVEN an invalid JSON body (missing field)
        let router = test_router();

        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/tasks")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"bad": "data"}"#))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 422 (axum returns 422 for deserialization errors)
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn test_get_task_status_returns_200() {
        // GIVEN a submitted task
        let (router, agent_id) = test_router_with_agent().await;

        // Submit a task first
        let body = serde_json::json!({
            "agent_id": agent_id,
            "input": {"prompt": "Test"}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/tasks")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");
        let resp = router.clone().oneshot(req).await.expect("request failed");
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let submit_json = body_json(resp).await;
        let task_id = submit_json["task_id"].as_str().expect("task_id");

        // WHEN GET /api/v1/tasks/{id}
        let req = Request::builder()
            .uri(format!("/api/v1/tasks/{task_id}"))
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 200 with the status
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["task_id"], task_id);
        assert!(json["status"].is_string());
    }

    #[tokio::test]
    async fn test_get_task_not_found_returns_404() {
        // GIVEN no task "unknown-task"
        let router = test_router();

        // WHEN GET /api/v1/tasks/unknown-task
        let req = Request::builder()
            .uri("/api/v1/tasks/unknown-task")
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 404
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let json = body_json(resp).await;
        assert!(json["error"]
            .as_str()
            .expect("error str")
            .contains("task not found"));
    }

    #[tokio::test]
    async fn test_cancel_task_returns_200() {
        // GIVEN a task submitted on a blocking backend (never completes)
        // Uses NeverMockBackend to avoid the race condition between TaskCompleted and Cancel
        let (router, agent_id) = test_router_with_blocking_agent().await;

        // Submit a task first
        let body = serde_json::json!({
            "agent_id": agent_id,
            "input": {"prompt": "Cancel me"}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/tasks")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");
        let resp = router.clone().oneshot(req).await.expect("request failed");
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let submit_json = body_json(resp).await;
        let task_id = submit_json["task_id"].as_str().expect("task_id");

        // WHEN DELETE /api/v1/tasks/{id}
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/api/v1/tasks/{task_id}"))
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 200 with status "canceled"
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["task_id"], task_id);
        assert_eq!(json["status"], "canceled");
    }

    // ResumeHandler tests, POST /api/v1/tasks/{id}/resume

    /// Open a `TaskRepository` on a unique temporary file.
    async fn open_test_repo() -> apollia_tools::TaskRepository {
        let path = std::env::temp_dir().join(format!("apollia_resume_{}.db", uuid::Uuid::new_v4()));
        apollia_tools::TaskRepository::open(&path)
            .await
            .expect("TaskRepository::open failed")
    }

    /// Build a Router with the resume route and an active `TaskRepository`.
    async fn resume_router_with_repo(repo: apollia_tools::TaskRepository) -> axum::Router {
        let (event_tx, _) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);
        let state = AppState {
            router_handle,
            registry_handle,
            event_sender: event_tx,
            agent_loader: std::sync::Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: MockBackend,
            llm_router: crate::api::server::empty_shared_llm_router(),
            trigger_engine: None,
            config_path: None,
            task_repository: Some(std::sync::Arc::new(repo)),
            pending_approvals: None,
            plan_gates: None,
            notification_config: None,
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: None,
            audit_journal: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            notification_repo: None,
            notification_engine_handle: None,
            chat_manager: None,
            plan_cache: None,
            mailbox_handle: None,
            user_memory: None,
            stt_engine: None,
            stt_repository: None,
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
        };
        Router::new()
            .route("/api/v1/tasks/:id/resume", post(resume_task::<MockBackend>))
            .with_state(state)
    }

    // Valid approval: 200 OK + TaskResumed emitted on the EventBus

    #[tokio::test]
    async fn test_resume_approve_returns_200() {
        // GIVEN a task in input_required status in the HITL DB
        let repo = open_test_repo().await;
        let task_id = "t-0042";
        repo.save_input_required(task_id, None, "Confirmer ?", &serde_json::json!({}))
            .await
            .expect("save_input_required failed");

        let router = resume_router_with_repo(repo).await;

        // WHEN POST /api/v1/tasks/t-0042/resume { "approved": true }
        let body = serde_json::json!({ "approved": true });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/tasks/{task_id}/resume"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 200 with approved=true and status="working"
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["task_id"], task_id);
        assert_eq!(json["approved"], true);
        assert_eq!(json["status"], "working");
    }

    // Valid rejection with reason: 200 OK

    #[tokio::test]
    async fn test_resume_reject_with_reason() {
        // GIVEN a task in input_required status
        let repo = open_test_repo().await;
        let task_id = "t-0043";
        repo.save_input_required(task_id, None, "Budget OK ?", &serde_json::json!({}))
            .await
            .expect("save_input_required failed");

        let router = resume_router_with_repo(repo).await;

        // WHEN POST /resume { "approved": false, "reason": "Budget insuffisant" }
        let body = serde_json::json!({
            "approved": false,
            "reason": "Budget insuffisant"
        });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/tasks/{task_id}/resume"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 200 with approved=false and status="working"
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["approved"], false);
        assert_eq!(json["status"], "working");
    }

    // Task not in input_required: 409 CONFLICT

    #[tokio::test]
    async fn test_resume_not_input_required_returns_409() {
        // GIVEN a task in working status (input_required, save_input_response, working)
        let repo = open_test_repo().await;
        let task_id = "t-0044";
        repo.save_input_required(task_id, None, "Prompt", &serde_json::json!({}))
            .await
            .expect("save_input_required failed");
        // Transition to working via save_input_response
        let resp_data = apollia_core::InputResponseData {
            approved: true,
            reason: None,
            context: serde_json::json!({}),
            responded_at: "2026-03-09T10:00:00Z".into(),
        };
        repo.save_input_response(task_id, &resp_data)
            .await
            .expect("save_input_response failed");

        let router = resume_router_with_repo(repo).await;

        // WHEN POST /resume { "approved": true } on a task already in working
        let body = serde_json::json!({ "approved": true });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/tasks/{task_id}/resume"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 409 CONFLICT
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let json = body_json(resp).await;
        assert!(
            json["error"]
                .as_str()
                .unwrap_or("")
                .contains("not in input_required"),
            "error should mention not in input_required, got: {}",
            json["error"]
        );
    }

    // Nonexistent task: 404 NOT FOUND

    #[tokio::test]
    async fn test_resume_task_not_found_returns_404() {
        // GIVEN an empty TaskRepository (no tasks)
        let repo = open_test_repo().await;
        let router = resume_router_with_repo(repo).await;

        // WHEN POST /resume on a nonexistent task_id
        let body = serde_json::json!({ "approved": true });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/tasks/t-9999/resume")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 404 NOT FOUND
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let json = body_json(resp).await;
        assert!(
            json["error"].as_str().unwrap_or("").contains("t-9999"),
            "error should mention task_id, got: {}",
            json["error"]
        );
    }

    // json_to_aip_input unit tests

    /// A2A-aligned format with a TextPart must deserialize directly (not wrapped).
    #[test]
    fn test_json_to_aip_input_text_part() {
        // GIVEN a well-formed AIPInput JSON (A2A format)
        let value = serde_json::json!({
            "parts": [{"type": "text", "text": "/home/user/project"}]
        });

        // WHEN
        let input = json_to_aip_input(value);

        // THEN exactly one TextPart with the correct text
        assert_eq!(input.parts.len(), 1);
        match &input.parts[0] {
            AIPPart::Text(tp) => assert_eq!(tp.text, "/home/user/project"),
            other => panic!("expected TextPart, got {other:?}"),
        }
    }

    /// Legacy free-form JSON (no "parts" key) must be wrapped as a DataPart.
    #[test]
    fn test_json_to_aip_input_legacy_wraps_as_data() {
        // GIVEN legacy input shape (no "parts" key)
        let value = serde_json::json!({ "prompt": "review this" });

        // WHEN
        let input = json_to_aip_input(value.clone());

        // THEN wrapped as a single DataPart
        assert_eq!(input.parts.len(), 1);
        match &input.parts[0] {
            AIPPart::Data(dp) => assert_eq!(dp.data, value),
            other => panic!("expected DataPart, got {other:?}"),
        }
    }

    /// Empty parts array must fall back to DataPart wrapping.
    #[test]
    fn test_json_to_aip_input_empty_parts_falls_back() {
        // GIVEN an AIPInput-shaped JSON but with no parts
        let value = serde_json::json!({ "parts": [] });

        // WHEN
        let input = json_to_aip_input(value.clone());

        // THEN wrapped as DataPart (empty parts would silently discard the input)
        assert_eq!(input.parts.len(), 1);
        assert!(matches!(&input.parts[0], AIPPart::Data(_)));
    }
}
