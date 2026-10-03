//! REST routes of the workflow engine: launch a workflow, read its history,
//! read one run with its steps, stop a run.
//!
//! A workflow is named by its package and its name,
//! `/api/v1/workflows/{package}/{workflow}`; a run by its identifier,
//! `/api/v1/workflow-runs/{id}`.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use apollia_tools::{WorkflowRunRow, WorkflowStepRow};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::workflows::{WorkflowError, WorkflowHandle};

/// Error body of the workflow routes.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct WorkflowErrorResponse {
    /// What went wrong.
    pub error: String,
    /// The input at fault, when an input was refused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

type RouteError = (StatusCode, Json<WorkflowErrorResponse>);

fn error(status: StatusCode, message: impl ToString) -> RouteError {
    (
        status,
        Json(WorkflowErrorResponse {
            error: message.to_string(),
            field: None,
        }),
    )
}

fn engine<B: ExecutionBackend + Clone>(state: &AppState<B>) -> Result<WorkflowHandle, RouteError> {
    state.workflows.clone().ok_or_else(|| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "the workflow engine is not running in this runtime",
        )
    })
}

fn workflow_error(e: WorkflowError) -> RouteError {
    match e {
        WorkflowError::InvalidInput(input) => (
            StatusCode::BAD_REQUEST,
            Json(WorkflowErrorResponse {
                error: format!("input '{}': {}", input.field, input.message),
                field: Some(input.field),
            }),
        ),
        WorkflowError::BadIdentifier(_) | WorkflowError::TooDeep(_) => {
            error(StatusCode::BAD_REQUEST, e)
        }
        WorkflowError::NotFound(_) | WorkflowError::UnknownRun(_) => {
            error(StatusCode::NOT_FOUND, e)
        }
        WorkflowError::AlreadyEnded(_) => error(StatusCode::CONFLICT, e),
        WorkflowError::NoHost | WorkflowError::Stopped => error(StatusCode::SERVICE_UNAVAILABLE, e),
        other => error(StatusCode::INTERNAL_SERVER_ERROR, other),
    }
}

/// A workflow run: what it was launched with, its status and its result.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunView {
    pub run_id: String,
    /// `<package>/<workflow>`.
    pub workflow_id: String,
    /// The run whose step started this one, when one did.
    pub parent_run_id: Option<String>,
    /// 1 for a run started on its own, one more per nested call.
    pub depth: u32,
    #[schema(value_type = Object)]
    pub inputs: serde_json::Value,
    /// `running`, `completed`, `failed` or `canceled`.
    pub status: String,
    /// What the workflow returned.
    #[schema(value_type = Option<Object>)]
    pub output: Option<serde_json::Value>,
    /// Why it failed or stopped.
    pub error: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
}

impl From<WorkflowRunRow> for WorkflowRunView {
    fn from(row: WorkflowRunRow) -> Self {
        Self {
            run_id: row.run_id,
            workflow_id: row.workflow_id,
            parent_run_id: row.parent_run_id,
            depth: row.depth,
            inputs: row.inputs,
            status: row.status,
            output: row.output,
            error: row.error,
            started_at: row.started_at,
            ended_at: row.ended_at,
        }
    }
}

/// One declared step of a run.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStepView {
    /// Its position among the declared steps, from 0.
    pub rank: u32,
    pub name: String,
    /// `pending`, `running`, `completed`, `failed`, `canceled` or
    /// `not_started`.
    pub status: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    /// What the agent or workflow the step ran last returned.
    pub output: Option<String>,
    pub error: Option<String>,
}

impl From<WorkflowStepRow> for WorkflowStepView {
    fn from(row: WorkflowStepRow) -> Self {
        Self {
            rank: row.rank,
            name: row.name,
            status: row.status,
            started_at: row.started_at,
            ended_at: row.ended_at,
            output: row.output,
            error: row.error,
        }
    }
}

/// A run with its steps.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct WorkflowRunDetailView {
    pub run: WorkflowRunView,
    pub steps: Vec<WorkflowStepView>,
}

/// The runs of a workflow, newest first.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct WorkflowRunListView {
    pub runs: Vec<WorkflowRunView>,
}

/// Body of `POST /api/v1/workflows/{package}/{workflow}/runs`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct WorkflowLaunch {
    /// The inputs, checked against the schema the workflow declares. A text
    /// value given for a field of another type is read as that type when it
    /// parses as one.
    #[serde(default = "empty_object")]
    #[schema(value_type = Object)]
    pub inputs: serde_json::Value,
}

fn empty_object() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}

/// Query of `GET /api/v1/workflows/{package}/{workflow}/runs`.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct WorkflowRunsQuery {
    /// At most this many runs, 50 when absent.
    pub limit: Option<u32>,
}

/// `POST /api/v1/workflows/{package}/{workflow}/runs`: run a workflow.
///
/// The inputs are checked and the run recorded, with its declared steps,
/// before the answer; the run goes on in the background.
#[utoipa::path(
    post,
    path = "/api/v1/workflows/{package}/{workflow}/runs",
    tag = "workflows",
    params(
        ("package" = String, Path, description = "Package that declares the workflow"),
        ("workflow" = String, Path, description = "Workflow name in the package"),
    ),
    request_body = WorkflowLaunch,
    responses(
        (status = 202, description = "Run launched", body = WorkflowRunView),
        (status = 400, description = "An input does not fit the schema; `field` names it", body = WorkflowErrorResponse),
        (status = 404, description = "No installed workflow by that name", body = WorkflowErrorResponse),
        (status = 503, description = "Workflow engine or package host unavailable", body = WorkflowErrorResponse),
    )
)]
pub async fn launch_workflow<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path((package, workflow)): Path<(String, String)>,
    Json(launch): Json<WorkflowLaunch>,
) -> Result<(StatusCode, Json<WorkflowRunView>), RouteError> {
    let run = engine(&state)?
        .launch(&format!("{package}/{workflow}"), launch.inputs)
        .await
        .map_err(workflow_error)?;
    Ok((StatusCode::ACCEPTED, Json(run.into())))
}

/// `GET /api/v1/workflows/{package}/{workflow}/runs`: the history of a
/// workflow, newest first.
#[utoipa::path(
    get,
    path = "/api/v1/workflows/{package}/{workflow}/runs",
    tag = "workflows",
    params(
        ("package" = String, Path, description = "Package that declares the workflow"),
        ("workflow" = String, Path, description = "Workflow name in the package"),
        WorkflowRunsQuery,
    ),
    responses(
        (status = 200, description = "The runs of the workflow", body = WorkflowRunListView),
        (status = 503, description = "Workflow engine unavailable", body = WorkflowErrorResponse),
    )
)]
pub async fn list_workflow_runs<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path((package, workflow)): Path<(String, String)>,
    Query(query): Query<WorkflowRunsQuery>,
) -> Result<Json<WorkflowRunListView>, RouteError> {
    let runs = engine(&state)?
        .history(&format!("{package}/{workflow}"), query.limit.unwrap_or(50))
        .await
        .map_err(workflow_error)?;
    Ok(Json(WorkflowRunListView {
        runs: runs.into_iter().map(Into::into).collect(),
    }))
}

/// `GET /api/v1/workflow-runs/{id}`: a run with its steps.
#[utoipa::path(
    get,
    path = "/api/v1/workflow-runs/{id}",
    tag = "workflows",
    params(("id" = String, Path, description = "Run id")),
    responses(
        (status = 200, description = "The run and its steps", body = WorkflowRunDetailView),
        (status = 404, description = "No such run", body = WorkflowErrorResponse),
        (status = 503, description = "Workflow engine unavailable", body = WorkflowErrorResponse),
    )
)]
pub async fn get_workflow_run<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowRunDetailView>, RouteError> {
    let detail = engine(&state)?
        .detail(&id)
        .await
        .map_err(workflow_error)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, format!("no workflow run '{id}'")))?;
    Ok(Json(WorkflowRunDetailView {
        run: detail.run.into(),
        steps: detail.steps.into_iter().map(Into::into).collect(),
    }))
}

/// `POST /api/v1/workflow-runs/{id}/cancel`: stop a run.
///
/// The step in progress is interrupted; the answer comes once the run has
/// ended, `canceled`, its remaining steps `not_started`.
#[utoipa::path(
    post,
    path = "/api/v1/workflow-runs/{id}/cancel",
    tag = "workflows",
    params(("id" = String, Path, description = "Run id")),
    responses(
        (status = 200, description = "The run, stopped", body = WorkflowRunView),
        (status = 404, description = "No such run", body = WorkflowErrorResponse),
        (status = 409, description = "The run already ended", body = WorkflowErrorResponse),
        (status = 503, description = "Workflow engine unavailable", body = WorkflowErrorResponse),
    )
)]
pub async fn cancel_workflow_run<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowRunView>, RouteError> {
    let run = engine(&state)?.cancel(&id).await.map_err(workflow_error)?;
    Ok(Json(run.into()))
}

/// The workflow routes.
pub fn workflow_routes<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/workflows/:package/:workflow/runs",
            get(list_workflow_runs::<B>).post(launch_workflow::<B>),
        )
        .route("/api/v1/workflow-runs/:id", get(get_workflow_run::<B>))
        .route(
            "/api/v1/workflow-runs/:id/cancel",
            post(cancel_workflow_run::<B>),
        )
}

#[cfg(test)]
mod tests;
