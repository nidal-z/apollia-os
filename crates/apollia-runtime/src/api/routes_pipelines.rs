//! REST routes for pipeline orchestration — `/api/v1/pipelines`.
//!
//! Implements CRUD endpoints for pipeline definitions and
//! run management endpoints:
//!
//! | Method | Path                                  | Handler                |
//! |--------|---------------------------------------|------------------------|
//! | GET    | `/api/v1/pipelines`                   | [`list_pipelines`]     |
//! | GET    | `/api/v1/pipelines/:id`               | [`get_pipeline`]       |
//! | POST   | `/api/v1/pipelines`                   | [`create_pipeline`]    |
//! | PUT    | `/api/v1/pipelines/:id`               | [`update_pipeline`]    |
//! | DELETE | `/api/v1/pipelines/:id`               | [`delete_pipeline`]    |
//! | POST   | `/api/v1/pipelines/:id/run`           | [`run_pipeline`]       |
//! | GET    | `/api/v1/pipelines/:id/runs`          | [`list_runs`]          |
//! | GET    | `/api/v1/pipelines/:id/runs/:run_id`  | [`get_run`]            |
//!
//! All handlers return 503 when `AppState.pipeline_engine` is `None`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_pipelines::{
    ConditionKind, GlobalFailurePolicy, PipelineDefinition, PipelineDefinitionError,
    PipelineDefinitionRepository, PipelineDefinitionRow, PipelineEngineError, PipelineId,
    PipelineRun, PipelineStatus, PipelineStepDef, RunId, StepCondition, StepFailurePolicy, StepId,
};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

// ── Run response types ───────────────────────────────────────────────────────

/// Request body for `POST /api/v1/pipelines/:id/run`.
#[derive(Debug, Deserialize)]
pub struct RunPipelineRequest {
    /// Optional trigger payload forwarded as `{{trigger.payload}}` in templates.
    ///
    /// Equivalent to the payload a file-watch or webhook trigger would produce.
    #[serde(default)]
    pub input: Option<String>,
}

/// Response body returned by `POST /api/v1/pipelines/:id/run` (202 Accepted)
/// and by `GET /api/v1/pipelines/:id/runs` list items.
#[derive(Debug, Serialize)]
pub struct PipelineRunResponse {
    /// Unique identifier of the newly created run.
    pub run_id: String,
    /// Pipeline that was triggered.
    pub pipeline_id: String,
    /// Current aggregate status.
    pub status: String,
    /// ISO 8601 timestamp when the run started.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    /// ISO 8601 timestamp when the run reached a terminal state; `null` while running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
}

/// Response body for a step within a detailed run response.
#[derive(Debug, Serialize)]
pub struct StepRunResponse {
    /// Step identifier as declared in the pipeline definition.
    pub step_id: String,
    /// Current step status (`"pending"`, `"running"`, `"completed"`, etc.).
    pub status: String,
    /// Agent output — populated once the step completes successfully.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Error message — populated if the step failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// ISO 8601 timestamp of step submission to the TaskRouter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    /// ISO 8601 timestamp of step terminal state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
}

/// Detailed response body for a pipeline run including all step runs.
///
/// Returned by `GET /api/v1/pipelines/:id/runs/:run_id`.
#[derive(Debug, Serialize)]
pub struct PipelineRunDetailResponse {
    /// Unique identifier of this run.
    pub run_id: String,
    /// Pipeline this run belongs to.
    pub pipeline_id: String,
    /// Aggregate status of the run.
    pub status: String,
    /// Per-step execution records, ordered by step declaration order.
    pub step_runs: Vec<StepRunResponse>,
    /// ISO 8601 timestamp of run creation.
    pub started_at: String,
    /// ISO 8601 timestamp of run completion; `null` while still running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
}

// ── CRUD request types ───────────────────────────────────────────────────────

/// Request body for `POST /api/v1/pipelines` — create a pipeline definition.
#[derive(Debug, Deserialize)]
pub struct CreatePipelineRequest {
    /// Unique pipeline identifier.
    pub id: String,
    /// Human-readable description.
    pub description: Option<String>,
    /// Global failure policy: `"fail"` (default) or `"continue"`.
    pub on_failure: Option<String>,
    /// Whether the pipeline is active (default: `true`).
    pub enabled: Option<bool>,
    /// Ordered list of pipeline steps.
    pub steps: Vec<PipelineStepInput>,
}

/// A step within a create/update pipeline request.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PipelineStepInput {
    /// Step identifier, unique within the pipeline.
    pub id: String,
    /// Target agent name.
    pub agent: String,
    /// Input template supporting `{{steps.x.output}}` etc.
    pub input: String,
    /// Steps that must complete before this step can execute.
    pub depends_on: Option<Vec<String>>,
    /// Per-step failure policy: `"fail"` (default), `"skip"`, or `"fallback"`.
    pub on_failure: Option<String>,
    /// Optional execution condition.
    pub condition: Option<StepConditionInput>,
    /// If set, this step acts as a fallback for the referenced step.
    pub fallback_for: Option<String>,
}

/// Condition within a step input request.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StepConditionInput {
    /// Comparison operator: `"contains"`, `"equals"`, `"starts_with"`, `"ends_with"`, `"regex"`.
    pub when: String,
    /// Dot-path to the value to inspect.
    pub field: String,
    /// Reference value for comparison.
    pub value: String,
}

// ── CRUD response types ───────────────────────────────────────────────────────

/// Full pipeline definition response (returned by CRUD operations).
#[derive(Debug, Serialize)]
pub struct PipelineDefinitionResponse {
    /// Pipeline identifier.
    pub id: String,
    /// Human-readable description.
    pub description: String,
    /// Global failure policy.
    pub on_failure: String,
    /// Steps with full detail.
    pub steps: Vec<PipelineStepResponse>,
    /// Whether the pipeline is active.
    pub enabled: bool,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last modification timestamp.
    pub updated_at: String,
}

/// A step within a pipeline definition response.
#[derive(Debug, Serialize)]
pub struct PipelineStepResponse {
    /// Step identifier.
    pub id: String,
    /// Target agent name.
    pub agent: String,
    /// Input template.
    pub input: String,
    /// Dependencies.
    pub depends_on: Vec<String>,
    /// Per-step failure policy.
    pub on_failure: String,
    /// Optional execution condition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<StepConditionInput>,
    /// Fallback target step, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_for: Option<String>,
}

/// Response body for `DELETE /api/v1/pipelines/:id`.
#[derive(Debug, Serialize)]
pub struct DeletePipelineResponse {
    /// Identifier of the deleted pipeline.
    pub deleted: String,
}

/// Standard error response body.
#[derive(Debug, Serialize)]
pub struct PipelineErrorResponse {
    /// Human-readable error description.
    pub error: String,
}

// ── Helpers — run responses ──────────────────────────────────────────────────

/// Convert a [`PipelineStatus`] into a short, human-readable string.
fn status_to_str(status: &PipelineStatus) -> String {
    match status {
        PipelineStatus::Running => "running".into(),
        PipelineStatus::WaitingApproval { .. } => "waiting_approval".into(),
        PipelineStatus::Completed => "completed".into(),
        PipelineStatus::Failed { .. } => "failed".into(),
    }
}

/// Convert a [`PipelineEngineError`] into an HTTP status code and error body.
fn engine_error_to_response(err: PipelineEngineError) -> (StatusCode, Json<PipelineErrorResponse>) {
    let (code, message) = match &err {
        PipelineEngineError::PipelineNotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        PipelineEngineError::EngineUnavailable => {
            (StatusCode::SERVICE_UNAVAILABLE, err.to_string())
        }
        PipelineEngineError::Repository(_) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    };
    (code, Json(PipelineErrorResponse { error: message }))
}

/// Build a minimal [`PipelineRunResponse`] from a [`PipelineRun`].
fn run_to_summary(run: &PipelineRun) -> PipelineRunResponse {
    PipelineRunResponse {
        run_id: run.run_id.to_string(),
        pipeline_id: run.pipeline_id.to_string(),
        status: status_to_str(&run.status),
        started_at: Some(run.started_at.to_rfc3339()),
        ended_at: run.ended_at.map(|t| t.to_rfc3339()),
    }
}

/// Build a detailed [`PipelineRunDetailResponse`] from a [`PipelineRun`].
fn run_to_detail(run: PipelineRun) -> PipelineRunDetailResponse {
    let mut step_runs: Vec<StepRunResponse> = run
        .step_runs
        .values()
        .map(|s| StepRunResponse {
            step_id: s.step_id.to_string(),
            status: format!("{:?}", s.status).to_lowercase(),
            output: s.output.clone(),
            error: s.error.clone(),
            started_at: s.started_at.map(|t| t.to_rfc3339()),
            ended_at: s.ended_at.map(|t| t.to_rfc3339()),
        })
        .collect();
    // Sort by step_id for deterministic output.
    step_runs.sort_by(|a, b| a.step_id.cmp(&b.step_id));

    PipelineRunDetailResponse {
        run_id: run.run_id.to_string(),
        pipeline_id: run.pipeline_id.to_string(),
        status: status_to_str(&run.status),
        step_runs,
        started_at: run.started_at.to_rfc3339(),
        ended_at: run.ended_at.map(|t| t.to_rfc3339()),
    }
}

/// Shared 503 response emitted when `AppState.pipeline_engine` is `None`.
fn engine_unavailable() -> (StatusCode, Json<PipelineErrorResponse>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(PipelineErrorResponse {
            error: "pipeline engine not available".into(),
        }),
    )
}

// ── Helpers — CRUD ───────────────────────────────────────────────────────────

/// Maps a [`PipelineDefinitionError`] to an HTTP status code and error body.
fn definition_error_to_response(
    err: PipelineDefinitionError,
) -> (StatusCode, Json<PipelineErrorResponse>) {
    let (status, msg) = match &err {
        PipelineDefinitionError::NotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        PipelineDefinitionError::DuplicateId(_) => (StatusCode::CONFLICT, err.to_string()),
        PipelineDefinitionError::ValidationError(_) => {
            (StatusCode::UNPROCESSABLE_ENTITY, err.to_string())
        }
        PipelineDefinitionError::Database(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
        }
    };
    (status, Json(PipelineErrorResponse { error: msg }))
}

/// Extracts the pipeline definition repository from state, returning 503 if absent.
fn require_repo<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Result<
    &std::sync::Arc<std::sync::Mutex<PipelineDefinitionRepository>>,
    (StatusCode, Json<PipelineErrorResponse>),
> {
    state.pipeline_def_repo.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(PipelineErrorResponse {
                error: "pipeline definition repository not available".into(),
            }),
        )
    })
}

/// Lock the repository mutex, mapping poison errors to 500.
fn lock_repo(
    repo: &std::sync::Arc<std::sync::Mutex<PipelineDefinitionRepository>>,
) -> Result<
    std::sync::MutexGuard<'_, PipelineDefinitionRepository>,
    (StatusCode, Json<PipelineErrorResponse>),
> {
    repo.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PipelineErrorResponse {
                error: "repository lock poisoned".into(),
            }),
        )
    })
}

/// Reloads the pipeline engine's in-memory definitions from the repository.
///
/// Reads all enabled definitions from SQLite, converts them to
/// [`PipelineDefinition`], and sends them to the engine actor.
async fn reload_engine_from_repo<B: ExecutionBackend + Clone>(state: &AppState<B>) {
    let engine = match &state.pipeline_engine {
        Some(e) => e.clone(),
        None => return,
    };
    let repo = match &state.pipeline_def_repo {
        Some(r) => r.clone(),
        None => return,
    };

    let definitions = {
        let guard = match repo.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        match guard.list() {
            Ok(rows) => rows,
            Err(_) => return,
        }
    };

    let rich_defs: Vec<PipelineDefinition> = definitions
        .into_iter()
        .filter(|row| row.enabled)
        .map(PipelineDefinition::from)
        .collect();

    engine.reload(rich_defs).await;
}

/// Parses a `GlobalFailurePolicy` from an optional string (default: `"fail"`).
fn parse_on_failure(
    value: Option<&str>,
) -> Result<GlobalFailurePolicy, (StatusCode, Json<PipelineErrorResponse>)> {
    match value {
        None | Some("fail") => Ok(GlobalFailurePolicy::Fail),
        Some("continue") => Ok(GlobalFailurePolicy::Continue),
        Some(other) => Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(PipelineErrorResponse {
                error: format!("validation error: invalid on_failure value: {other}"),
            }),
        )),
    }
}

/// Parses a `StepFailurePolicy` from an optional string (default: `"fail"`).
fn parse_step_on_failure(value: Option<&str>) -> Result<StepFailurePolicy, String> {
    match value {
        None | Some("fail") => Ok(StepFailurePolicy::Fail),
        Some("skip") => Ok(StepFailurePolicy::Skip),
        Some("fallback") => Ok(StepFailurePolicy::Fallback),
        Some(other) => Err(format!("invalid step on_failure value: {other}")),
    }
}

/// Parses a `ConditionKind` from a string.
fn parse_condition_kind(value: &str) -> Result<ConditionKind, String> {
    match value {
        "contains" => Ok(ConditionKind::Contains),
        "equals" => Ok(ConditionKind::Equals),
        "starts_with" => Ok(ConditionKind::StartsWith),
        "ends_with" => Ok(ConditionKind::EndsWith),
        "regex" => Ok(ConditionKind::Regex),
        other => Err(format!("invalid condition kind: {other}")),
    }
}

/// Converts a [`PipelineStepInput`] into a [`PipelineStepDef`].
fn step_input_to_def(
    input: &PipelineStepInput,
) -> Result<PipelineStepDef, (StatusCode, Json<PipelineErrorResponse>)> {
    let on_failure = parse_step_on_failure(input.on_failure.as_deref()).map_err(|msg| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(PipelineErrorResponse {
                error: format!("validation error: {msg}"),
            }),
        )
    })?;

    let condition = match &input.condition {
        Some(c) => {
            let kind = parse_condition_kind(&c.when).map_err(|msg| {
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(PipelineErrorResponse {
                        error: format!("validation error: {msg}"),
                    }),
                )
            })?;
            Some(StepCondition {
                when: kind,
                field: c.field.clone(),
                value: c.value.clone(),
            })
        }
        None => None,
    };

    Ok(PipelineStepDef {
        id: StepId(input.id.clone()),
        agent: input.agent.clone(),
        input: input.input.clone(),
        depends_on: input
            .depends_on
            .as_ref()
            .map(|deps| deps.iter().map(|d| StepId(d.clone())).collect())
            .unwrap_or_default(),
        on_failure,
        condition,
        fallback_for: input.fallback_for.as_ref().map(|s| StepId(s.clone())),
    })
}

/// Converts a [`PipelineDefinitionRow`] into a [`PipelineDefinitionResponse`].
fn row_to_response(row: PipelineDefinitionRow) -> PipelineDefinitionResponse {
    let on_failure = match row.on_failure {
        GlobalFailurePolicy::Fail => "fail".to_string(),
        GlobalFailurePolicy::Continue => "continue".to_string(),
    };

    let steps = row
        .steps
        .iter()
        .map(|s| {
            let step_on_failure = match s.on_failure {
                StepFailurePolicy::Fail => "fail",
                StepFailurePolicy::Skip => "skip",
                StepFailurePolicy::Fallback => "fallback",
            };

            let condition = s.condition.as_ref().map(|c| {
                let when = match c.when {
                    ConditionKind::Contains => "contains",
                    ConditionKind::Equals => "equals",
                    ConditionKind::StartsWith => "starts_with",
                    ConditionKind::EndsWith => "ends_with",
                    ConditionKind::Regex => "regex",
                };
                StepConditionInput {
                    when: when.to_string(),
                    field: c.field.clone(),
                    value: c.value.clone(),
                }
            });

            PipelineStepResponse {
                id: s.id.0.clone(),
                agent: s.agent.clone(),
                input: s.input.clone(),
                depends_on: s.depends_on.iter().map(|d| d.0.clone()).collect(),
                on_failure: step_on_failure.to_string(),
                condition,
                fallback_for: s.fallback_for.as_ref().map(|f| f.0.clone()),
            }
        })
        .collect();

    PipelineDefinitionResponse {
        id: row.id,
        description: row.description,
        on_failure,
        steps,
        enabled: row.enabled,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

// ── CRUD Handlers ────────────────────────────────────────────────────────────

/// Handler for `POST /api/v1/pipelines` — create a pipeline definition.
///
/// Validates the DAG structure, inserts into `pipelines.db`, reloads the
/// engine, and returns 201 with the full definition.
///
/// ## Codes HTTP
/// - `201 Created` — definition created successfully
/// - `409 Conflict` — a pipeline with the same id already exists
/// - `422 Unprocessable Entity` — validation error (cycle, duplicates, etc.)
/// - `500 Internal Server Error` — database error
/// - `503 Service Unavailable` — repository not available
pub async fn create_pipeline<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<CreatePipelineRequest>,
) -> Result<(StatusCode, Json<PipelineDefinitionResponse>), (StatusCode, Json<PipelineErrorResponse>)>
{
    let repo = require_repo(&state)?;
    let on_failure = parse_on_failure(body.on_failure.as_deref())?;

    let steps: Vec<PipelineStepDef> = body
        .steps
        .iter()
        .map(step_input_to_def)
        .collect::<Result<Vec<_>, _>>()?;

    let row = PipelineDefinitionRow {
        id: body.id.clone(),
        description: body.description.unwrap_or_default(),
        on_failure,
        steps,
        enabled: body.enabled.unwrap_or(true),
        created_at: String::new(),
        updated_at: String::new(),
    };

    let created = {
        let guard = lock_repo(repo)?;
        guard.insert(&row).map_err(definition_error_to_response)?;
        guard
            .get(&body.id)
            .map_err(definition_error_to_response)?
            .ok_or_else(|| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(PipelineErrorResponse {
                        error: "pipeline inserted but not found".into(),
                    }),
                )
            })?
    };

    reload_engine_from_repo(&state).await;

    Ok((StatusCode::CREATED, Json(row_to_response(created))))
}

/// Handler for `PUT /api/v1/pipelines/:id` — update a pipeline definition.
///
/// Re-validates the DAG, updates in `pipelines.db`, reloads the engine,
/// and returns 200 with the updated definition.
///
/// ## Codes HTTP
/// - `200 OK` — definition updated successfully
/// - `404 Not Found` — pipeline does not exist
/// - `422 Unprocessable Entity` — validation error
/// - `500 Internal Server Error` — database error
/// - `503 Service Unavailable` — repository not available
pub async fn update_pipeline<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Json(body): Json<CreatePipelineRequest>,
) -> Result<Json<PipelineDefinitionResponse>, (StatusCode, Json<PipelineErrorResponse>)> {
    let repo = require_repo(&state)?;
    let on_failure = parse_on_failure(body.on_failure.as_deref())?;

    let steps: Vec<PipelineStepDef> = body
        .steps
        .iter()
        .map(step_input_to_def)
        .collect::<Result<Vec<_>, _>>()?;

    let row = PipelineDefinitionRow {
        id: id.clone(),
        description: body.description.unwrap_or_default(),
        on_failure,
        steps,
        enabled: body.enabled.unwrap_or(true),
        created_at: String::new(),
        updated_at: String::new(),
    };

    let updated = {
        let guard = lock_repo(repo)?;
        guard
            .update(&id, &row)
            .map_err(definition_error_to_response)?;
        guard
            .get(&id)
            .map_err(definition_error_to_response)?
            .ok_or_else(|| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(PipelineErrorResponse {
                        error: "pipeline updated but not found".into(),
                    }),
                )
            })?
    };

    reload_engine_from_repo(&state).await;

    Ok(Json(row_to_response(updated)))
}

/// Handler for `DELETE /api/v1/pipelines/:id` — delete a pipeline definition.
///
/// Removes the definition from `pipelines.db` (runs are preserved), reloads
/// the engine, and returns 200 with the deleted id.
///
/// ## Codes HTTP
/// - `200 OK` — definition deleted
/// - `404 Not Found` — pipeline does not exist
/// - `500 Internal Server Error` — database error
/// - `503 Service Unavailable` — repository not available
pub async fn delete_pipeline<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<DeletePipelineResponse>, (StatusCode, Json<PipelineErrorResponse>)> {
    let repo = require_repo(&state)?;

    {
        let guard = lock_repo(repo)?;
        guard.delete(&id).map_err(definition_error_to_response)?;
    }

    reload_engine_from_repo(&state).await;

    Ok(Json(DeletePipelineResponse { deleted: id }))
}

/// Handler for `GET /api/v1/pipelines/:id` — get a single pipeline definition.
///
/// ## Codes HTTP
/// - `200 OK` — definition found
/// - `404 Not Found` — pipeline does not exist
/// - `500 Internal Server Error` — database error
/// - `503 Service Unavailable` — repository not available
pub async fn get_pipeline<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<PipelineDefinitionResponse>, (StatusCode, Json<PipelineErrorResponse>)> {
    let repo = require_repo(&state)?;

    let row = {
        let guard = lock_repo(repo)?;
        guard.get(&id).map_err(definition_error_to_response)?
    };

    let row = row.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(PipelineErrorResponse {
                error: format!("pipeline not found: {id}"),
            }),
        )
    })?;

    Ok(Json(row_to_response(row)))
}

// ── Run Handlers (existing) ──────────────────────────────────────────────────

/// Handler for `POST /api/v1/pipelines/:id/run`.
///
/// Triggers a new run for the named pipeline and returns 202 Accepted with the
/// newly created `run_id`. The run executes asynchronously in the background.
///
/// ## Codes HTTP
/// - `202 Accepted` — run created and executor spawned
/// - `404 Not Found` — no pipeline with that `id` is registered
/// - `503 Service Unavailable` — `PipelineEngine` not configured
/// - `500 Internal Server Error` — SQLite persistence failure
pub async fn run_pipeline<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(pipeline_id): Path<String>,
    Json(req): Json<RunPipelineRequest>,
) -> Result<(StatusCode, Json<PipelineRunResponse>), (StatusCode, Json<PipelineErrorResponse>)> {
    let engine = state
        .pipeline_engine
        .as_ref()
        .ok_or_else(engine_unavailable)?;

    let run_id = engine
        .run_pipeline(&pipeline_id, None, req.input)
        .await
        .map_err(engine_error_to_response)?;

    Ok((
        StatusCode::ACCEPTED,
        Json(PipelineRunResponse {
            run_id: run_id.to_string(),
            pipeline_id,
            status: "running".into(),
            started_at: None,
            ended_at: None,
        }),
    ))
}

/// Handler for `GET /api/v1/pipelines/:id/runs`.
///
/// Returns the 20 most recent runs for the given pipeline, ordered by
/// `started_at` descending. Returns an empty list if the pipeline exists
/// but has never been executed.
///
/// ## Codes HTTP
/// - `200 OK` — list returned (may be empty)
/// - `503 Service Unavailable` — `PipelineEngine` not configured
/// - `500 Internal Server Error` — SQLite error
pub async fn list_runs<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(pipeline_id): Path<String>,
) -> Result<Json<Vec<PipelineRunResponse>>, (StatusCode, Json<PipelineErrorResponse>)> {
    let engine = state
        .pipeline_engine
        .as_ref()
        .ok_or_else(engine_unavailable)?;

    let runs = engine
        .list_runs(&PipelineId(pipeline_id), 20)
        .await
        .map_err(engine_error_to_response)?;

    let response: Vec<PipelineRunResponse> = runs.iter().map(run_to_summary).collect();
    Ok(Json(response))
}

/// Handler for `GET /api/v1/pipelines/:id/runs/:run_id`.
///
/// Returns the detailed state of a pipeline run, including all step runs.
///
/// ## Codes HTTP
/// - `200 OK` — run found, detail returned
/// - `404 Not Found` — run not found in SQLite
/// - `503 Service Unavailable` — `PipelineEngine` not configured
/// - `500 Internal Server Error` — SQLite error
pub async fn get_run<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path((_pipeline_id, run_id)): Path<(String, String)>,
) -> Result<Json<PipelineRunDetailResponse>, (StatusCode, Json<PipelineErrorResponse>)> {
    let engine = state
        .pipeline_engine
        .as_ref()
        .ok_or_else(engine_unavailable)?;

    let run = engine
        .get_run(&RunId(run_id.clone()))
        .await
        .map_err(engine_error_to_response)?;

    match run {
        Some(r) => Ok(Json(run_to_detail(r))),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(PipelineErrorResponse {
                error: format!("run not found: {run_id}"),
            }),
        )),
    }
}

/// Handler for `GET /api/v1/pipelines` — list all pipeline definitions.
///
/// Returns definitions from the repository if available, otherwise falls
/// back to the engine's in-memory list.
///
/// ## Codes HTTP
/// - `200 OK` — list returned (may be empty)
/// - `503 Service Unavailable` — neither repository nor engine available
pub async fn list_pipelines<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<Vec<PipelineDefinitionResponse>>, (StatusCode, Json<PipelineErrorResponse>)> {
    // Prefer repository (full definitions with timestamps)
    if let Some(repo) = &state.pipeline_def_repo {
        let guard = lock_repo(repo)?;
        let rows = guard.list().map_err(definition_error_to_response)?;
        return Ok(Json(rows.into_iter().map(row_to_response).collect()));
    }

    // Fallback to engine list (id + description only) — return 503 if neither available
    let engine = state
        .pipeline_engine
        .as_ref()
        .ok_or_else(engine_unavailable)?;

    let pipelines = engine.list_pipelines().await;
    let items: Vec<PipelineDefinitionResponse> = pipelines
        .into_iter()
        .map(|(id, description)| PipelineDefinitionResponse {
            id: id.to_string(),
            description,
            on_failure: "fail".to_string(),
            steps: Vec::new(),
            enabled: true,
            created_at: String::new(),
            updated_at: String::new(),
        })
        .collect();
    Ok(Json(items))
}

/// Handler for `GET /api/v1/runs/:run_id`.
///
/// Returns the detailed state of a pipeline run by its `run_id` alone,
/// without requiring the pipeline identifier in the URL path.
/// Used by `apollia-os pipeline status <run_id>`.
///
/// ## Codes HTTP
/// - `200 OK` — run found
/// - `404 Not Found` — no run with that `run_id`
/// - `503 Service Unavailable` — `PipelineEngine` not configured
pub async fn get_run_by_id<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(run_id): Path<String>,
) -> Result<Json<PipelineRunDetailResponse>, (StatusCode, Json<PipelineErrorResponse>)> {
    let engine = state
        .pipeline_engine
        .as_ref()
        .ok_or_else(engine_unavailable)?;

    let run = engine
        .get_run(&RunId(run_id.clone()))
        .await
        .map_err(engine_error_to_response)?;

    match run {
        Some(r) => Ok(Json(run_to_detail(r))),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(PipelineErrorResponse {
                error: format!("run not found: {run_id}"),
            }),
        )),
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::routes_agents::StubAgentLoader;
    use crate::coordinator::ExecutionBackend;
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use crate::router::TaskRouterHandle;
    use apollia_core::{AIPResult, AIPTask, TaskStatus};
    use apollia_pipelines::{ExecutorError, PipelineEngine, PipelineRepository, TaskSubmitter};
    use async_trait::async_trait;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::{delete, get, post, put};
    use axum::Router;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;

    // ── Test infrastructure ────────────────────────────────────────────────────

    /// Minimal `ExecutionBackend` for tests — never actually called by these routes.
    #[derive(Clone)]
    struct MockBackend;

    impl ExecutionBackend for MockBackend {
        fn execute(
            &self,
            _task: AIPTask,
        ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
            Box::pin(async {
                Ok(AIPResult {
                    task_id: String::new(),
                    status: TaskStatus::Completed,
                    output: Vec::new(),
                    error: None,
                    artifacts: Vec::new(),
                    input_required_data: None,
                })
            })
        }
    }

    /// `TaskSubmitter` that always accepts tasks but never emits `TaskCompleted`.
    struct NeverCompletingSubmitter;

    #[async_trait]
    impl TaskSubmitter for NeverCompletingSubmitter {
        async fn submit_task(&self, _agent: &str, _input: &str) -> Result<String, ExecutorError> {
            Ok(format!("task-test-{}", uuid::Uuid::new_v4()))
        }
    }

    /// Build a simple pipeline definition with two sequential steps.
    fn make_test_pipeline(id: &str) -> PipelineDefinition {
        PipelineDefinition {
            id: PipelineId(id.to_owned()),
            description: format!("Test pipeline {id}"),
            on_failure: GlobalFailurePolicy::Fail,
            steps: vec![
                PipelineStepDef {
                    id: StepId("step-a".into()),
                    agent: "agent-a".into(),
                    input: "{{trigger.payload}}".into(),
                    depends_on: vec![],
                    on_failure: StepFailurePolicy::Fail,
                    condition: None,
                    fallback_for: None,
                },
                PipelineStepDef {
                    id: StepId("step-b".into()),
                    agent: "agent-b".into(),
                    input: "{{steps.step-a.output}}".into(),
                    depends_on: vec![StepId("step-a".into())],
                    on_failure: StepFailurePolicy::Fail,
                    condition: None,
                    fallback_for: None,
                },
            ],
        }
    }

    /// Shared helper: build an `AppState` with a live `PipelineEngine` and a
    /// `PipelineDefinitionRepository` for CRUD tests.
    fn app_state_with_engine_and_repo(pipelines: Vec<PipelineDefinition>) -> AppState<MockBackend> {
        let (event_tx, _) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);

        let repo = PipelineRepository::open_in_memory().expect("in-memory repo must open");
        let repo = Arc::new(Mutex::new(repo));
        let submitter: Arc<dyn TaskSubmitter> = Arc::new(NeverCompletingSubmitter);
        let engine = PipelineEngine::spawn(
            pipelines,
            repo,
            submitter,
            event_tx.clone(),
            apollia_core::PipelinesConfig::default(),
        );

        let def_repo =
            PipelineDefinitionRepository::open_in_memory().expect("in-memory def repo must open");
        let def_repo = Arc::new(Mutex::new(def_repo));

        AppState {
            router_handle,
            registry_handle,
            event_sender: event_tx,
            agent_loader: Arc::new(StubAgentLoader),
            backend: MockBackend,
            llm_router: None,
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            notification_config: None,
            pipeline_engine: Some(engine),
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            pipeline_def_repo: Some(def_repo),
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
        }
    }

    /// `AppState` with no engine/repo — tests 503 responses.
    fn app_state_no_engine() -> AppState<MockBackend> {
        let (event_tx, _) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);
        AppState {
            router_handle,
            registry_handle,
            event_sender: event_tx,
            agent_loader: Arc::new(StubAgentLoader),
            backend: MockBackend,
            llm_router: None,
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            notification_config: None,
            pipeline_engine: None,
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            pipeline_def_repo: None,
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
        }
    }

    /// Build a test `Router` with all pipeline routes (CRUD + runs).
    fn test_router(state: AppState<MockBackend>) -> Router {
        Router::new()
            .route(
                "/api/v1/pipelines",
                get(list_pipelines::<MockBackend>).post(create_pipeline::<MockBackend>),
            )
            .route(
                "/api/v1/pipelines/:id",
                get(get_pipeline::<MockBackend>)
                    .put(update_pipeline::<MockBackend>)
                    .delete(delete_pipeline::<MockBackend>),
            )
            .route(
                "/api/v1/pipelines/:id/run",
                post(run_pipeline::<MockBackend>),
            )
            .route("/api/v1/pipelines/:id/runs", get(list_runs::<MockBackend>))
            .route(
                "/api/v1/pipelines/:id/runs/:run_id",
                get(get_run::<MockBackend>),
            )
            .with_state(state)
    }

    /// Decode the response body as a `serde_json::Value`.
    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), 8192)
            .await
            .expect("read body");
        serde_json::from_slice(&bytes).expect("parse json")
    }

    /// JSON body for creating a valid 2-step pipeline.
    fn valid_create_body(id: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "description": "Test pipeline",
            "steps": [
                { "id": "step-a", "agent": "agent-a", "input": "hello" },
                { "id": "step-b", "agent": "agent-b", "input": "world", "depends_on": ["step-a"] }
            ]
        })
    }

    /// Send a POST JSON request and return the response.
    async fn post_json(
        router: &Router,
        uri: &str,
        body: serde_json::Value,
    ) -> axum::response::Response {
        let req = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");
        router.clone().oneshot(req).await.expect("request failed")
    }

    /// Send a PUT JSON request and return the response.
    async fn put_json(
        router: &Router,
        uri: &str,
        body: serde_json::Value,
    ) -> axum::response::Response {
        let req = Request::builder()
            .method("PUT")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");
        router.clone().oneshot(req).await.expect("request failed")
    }

    /// Send a DELETE request and return the response.
    async fn delete_req(router: &Router, uri: &str) -> axum::response::Response {
        let req = Request::builder()
            .method("DELETE")
            .uri(uri)
            .body(Body::empty())
            .expect("build request");
        router.clone().oneshot(req).await.expect("request failed")
    }

    /// Send a GET request and return the response.
    async fn get_req(router: &Router, uri: &str) -> axum::response::Response {
        let req = Request::builder()
            .uri(uri)
            .body(Body::empty())
            .expect("build request");
        router.clone().oneshot(req).await.expect("request failed")
    }

    // ── POST /api/v1/pipelines -> 201 ────────────────────────────────────────

    #[tokio::test]
    async fn test_create_pipeline_201() {
        // GIVEN a state with an empty repository
        let state = app_state_with_engine_and_repo(vec![]);
        let router = test_router(state);

        // WHEN POST /api/v1/pipelines with a valid body
        let resp = post_json(&router, "/api/v1/pipelines", valid_create_body("pipe-1")).await;

        // THEN 201 Created with full definition
        assert_eq!(resp.status(), StatusCode::CREATED);
        let json = body_json(resp).await;
        assert_eq!(json["id"], "pipe-1");
        assert_eq!(json["description"], "Test pipeline");
        assert_eq!(json["on_failure"], "fail");
        assert!(json["enabled"].as_bool().expect("enabled must be bool"));
        assert_eq!(json["steps"].as_array().expect("steps array").len(), 2);
        assert!(!json["created_at"].as_str().unwrap_or("").is_empty());
        assert!(!json["updated_at"].as_str().unwrap_or("").is_empty());
    }

    // ── PUT /api/v1/pipelines/:id -> 200 ─────────────────────────────────────

    #[tokio::test]
    async fn test_update_pipeline_200() {
        // GIVEN a pipeline "pipe-update" exists
        let state = app_state_with_engine_and_repo(vec![]);
        let router = test_router(state);
        let resp = post_json(
            &router,
            "/api/v1/pipelines",
            valid_create_body("pipe-update"),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        // WHEN PUT with modified description and different steps
        let update_body = serde_json::json!({
            "id": "pipe-update",
            "description": "Updated description",
            "steps": [
                { "id": "new-step", "agent": "new-agent", "input": "new input" }
            ]
        });
        let resp = put_json(&router, "/api/v1/pipelines/pipe-update", update_body).await;

        // THEN 200 OK with updated definition
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["description"], "Updated description");
        assert_eq!(json["steps"].as_array().expect("steps").len(), 1);
        assert_eq!(json["steps"][0]["id"], "new-step");
    }

    // ── DELETE /api/v1/pipelines/:id -> 200 ───────────────────────────────────

    #[tokio::test]
    async fn test_delete_pipeline_200() {
        // GIVEN a pipeline "pipe-del" exists
        let state = app_state_with_engine_and_repo(vec![]);
        let router = test_router(state);
        let resp = post_json(&router, "/api/v1/pipelines", valid_create_body("pipe-del")).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        // WHEN DELETE /api/v1/pipelines/pipe-del
        let resp = delete_req(&router, "/api/v1/pipelines/pipe-del").await;

        // THEN 200 with {"deleted": "pipe-del"}
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["deleted"], "pipe-del");

        // AND the pipeline is no longer found
        let resp = get_req(&router, "/api/v1/pipelines/pipe-del").await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── GET /api/v1/pipelines/:id -> 200 ─────────────────────────────────────

    #[tokio::test]
    async fn test_get_pipeline_200() {
        // GIVEN a pipeline "pipe-get" exists with 2 steps
        let state = app_state_with_engine_and_repo(vec![]);
        let router = test_router(state);
        let resp = post_json(&router, "/api/v1/pipelines", valid_create_body("pipe-get")).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        // WHEN GET /api/v1/pipelines/pipe-get
        let resp = get_req(&router, "/api/v1/pipelines/pipe-get").await;

        // THEN 200 with full definition and deserialized steps
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["id"], "pipe-get");
        assert_eq!(json["steps"].as_array().expect("steps").len(), 2);
        assert_eq!(json["steps"][0]["id"], "step-a");
        assert_eq!(json["steps"][1]["id"], "step-b");
        assert_eq!(json["steps"][1]["depends_on"][0], "step-a");
    }

    // ── Validation cycle -> 422 ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_create_cycle_422() {
        // GIVEN a body with a cycle A->B->A
        let state = app_state_with_engine_and_repo(vec![]);
        let router = test_router(state);
        let body = serde_json::json!({
            "id": "cyclic",
            "steps": [
                { "id": "A", "agent": "a", "input": "x", "depends_on": ["B"] },
                { "id": "B", "agent": "b", "input": "y", "depends_on": ["A"] }
            ]
        });

        // WHEN POST /api/v1/pipelines
        let resp = post_json(&router, "/api/v1/pipelines", body).await;

        // THEN 422 with cycle error message
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let json = body_json(resp).await;
        assert!(
            json["error"]
                .as_str()
                .unwrap_or("")
                .contains("cycle detected"),
            "error must mention cycle, got: {}",
            json["error"]
        );
    }

    // ── Duplicate pipeline ID -> 409 ──────────────────────────────────────────

    #[tokio::test]
    async fn test_create_duplicate_409() {
        // GIVEN a pipeline "dup-pipe" already exists
        let state = app_state_with_engine_and_repo(vec![]);
        let router = test_router(state);
        let resp = post_json(&router, "/api/v1/pipelines", valid_create_body("dup-pipe")).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        // WHEN POST with the same id
        let resp = post_json(&router, "/api/v1/pipelines", valid_create_body("dup-pipe")).await;

        // THEN 409 Conflict
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let json = body_json(resp).await;
        assert!(
            json["error"].as_str().unwrap_or("").contains("dup-pipe"),
            "error must mention the duplicate id, got: {}",
            json["error"]
        );
    }

    // ── GET /api/v1/pipelines returns definitions ─────────────────────────────

    #[tokio::test]
    async fn test_list_returns_definitions() {
        // GIVEN 3 pipeline definitions in the repository
        let state = app_state_with_engine_and_repo(vec![]);
        let router = test_router(state);
        for name in &["list-1", "list-2", "list-3"] {
            let resp = post_json(&router, "/api/v1/pipelines", valid_create_body(name)).await;
            assert_eq!(resp.status(), StatusCode::CREATED);
        }

        // WHEN GET /api/v1/pipelines
        let resp = get_req(&router, "/api/v1/pipelines").await;

        // THEN 200 with 3 definitions containing full details
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let arr = json.as_array().expect("body must be a JSON array");
        assert_eq!(arr.len(), 3, "expected 3 definitions, got {}", arr.len());
        for item in arr {
            assert!(item["id"].is_string());
            assert!(item["steps"].is_array());
            assert!(!item["created_at"].as_str().unwrap_or("").is_empty());
        }
    }

    // ── Existing run tests ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_run_pipeline_returns_202() {
        // GIVEN an engine with "test-pipe" registered
        let state = app_state_with_engine_and_repo(vec![make_test_pipeline("test-pipe")]);
        let router = test_router(state);

        // WHEN POST /api/v1/pipelines/test-pipe/run {"input": "acme.pdf"}
        let body = serde_json::json!({ "input": "acme.pdf" });
        let resp = post_json(&router, "/api/v1/pipelines/test-pipe/run", body).await;

        // THEN 202 Accepted with run_id and status "running"
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let json = body_json(resp).await;
        assert!(
            json["run_id"].as_str().unwrap_or("").starts_with("r-"),
            "run_id must start with 'r-', got: {}",
            json["run_id"]
        );
        assert_eq!(json["pipeline_id"], "test-pipe");
        assert_eq!(json["status"], "running");
    }

    #[tokio::test]
    async fn test_no_engine_returns_503() {
        // GIVEN AppState with pipeline_engine == None
        let state = app_state_no_engine();
        let router = test_router(state);

        // WHEN POST /api/v1/pipelines/x/run
        let body = serde_json::json!({});
        let resp = post_json(&router, "/api/v1/pipelines/x/run", body).await;

        // THEN 503 Service Unavailable
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
