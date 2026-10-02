//! REST routes of the batch engine: launch a batch, read one, list them.
//!
//! `POST /api/v1/batches` launches an agent on every item of a source,
//! `GET /api/v1/batches/{id}` reads a batch with its items, and
//! `GET /api/v1/batches` lists the batches, newest first.
//!
//! `GET /api/v1/batches/accesses?agent=` lists the sensitive accesses a launch
//! may grant for the duration of a batch. `POST /api/v1/batches/{id}/pause`,
//! `/resume`, `/stop` and `/retry` drive a batch: pause and resume it, stop
//! it, run its failed and waiting items again.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use apollia_core::batch_source::SourceError;
use apollia_tools::{BatchItemRow, BatchRow};

use crate::api::server::AppState;
use crate::batch::{BatchAccess, BatchControl, BatchError, BatchHandle, BatchLaunch, BatchRetry};
use crate::coordinator::ExecutionBackend;

/// Error body of the batch routes.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BatchErrorResponse {
    /// What went wrong.
    pub error: String,
}

type RouteError = (StatusCode, Json<BatchErrorResponse>);

fn error(status: StatusCode, message: impl ToString) -> RouteError {
    (
        status,
        Json(BatchErrorResponse {
            error: message.to_string(),
        }),
    )
}

fn engine<B: ExecutionBackend + Clone>(state: &AppState<B>) -> Result<BatchHandle, RouteError> {
    state.batches.clone().ok_or_else(|| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "the batch engine is not running in this runtime",
        )
    })
}

fn batch_error(e: BatchError) -> RouteError {
    match e {
        BatchError::Invalid(_) | BatchError::Agent(_) => error(StatusCode::BAD_REQUEST, e),
        BatchError::Source(SourceError::UnknownKind { kind }) => error(
            StatusCode::BAD_REQUEST,
            format!("unknown source type '{kind}': no reader is installed for it"),
        ),
        BatchError::Source(_) => error(StatusCode::UNPROCESSABLE_ENTITY, e),
        BatchError::Stopped => error(StatusCode::SERVICE_UNAVAILABLE, e),
        BatchError::NotFound(_) => error(StatusCode::NOT_FOUND, e),
        BatchError::State(_) => error(StatusCode::CONFLICT, e),
        other => error(StatusCode::INTERNAL_SERVER_ERROR, other),
    }
}

/// A batch: what it was launched with, its state and its counters.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchView {
    pub batch_id: String,
    pub agent: String,
    /// The source descriptor, `{kind, location, options}`.
    #[schema(value_type = Object)]
    pub source: serde_json::Value,
    pub grants: Vec<String>,
    pub concurrency: u32,
    /// Failed items tolerated before the batch is interrupted.
    pub max_failures: u32,
    /// `running`, `paused`, `completed`, `interrupted` or `stopped`.
    pub state: String,
    pub total: u64,
    pub completed: u64,
    pub failed: u64,
    pub canceled: u64,
    /// Items waiting for an access to be granted.
    pub awaiting: u64,
    pub created_at: String,
    pub updated_at: String,
    pub ended_at: Option<String>,
}

impl From<BatchRow> for BatchView {
    fn from(row: BatchRow) -> Self {
        Self {
            batch_id: row.batch_id,
            agent: row.agent,
            source: row.source,
            grants: row.grants,
            concurrency: row.concurrency,
            max_failures: row.max_failures,
            state: row.state,
            total: row.total,
            completed: row.completed,
            failed: row.failed,
            canceled: row.canceled,
            awaiting: row.awaiting,
            created_at: row.created_at,
            updated_at: row.updated_at,
            ended_at: row.ended_at,
        }
    }
}

/// One item of a batch.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchItemView {
    /// Its position in the source, from 0.
    pub rank: u64,
    /// Where it comes from inside the source.
    pub origin: String,
    #[schema(value_type = Object)]
    pub input: serde_json::Value,
    /// `pending`, `running`, `completed`, `failed`, `canceled`, or `awaiting`
    /// a decision on an access nobody granted.
    pub status: String,
    pub output: Option<String>,
    #[schema(value_type = Option<Object>)]
    pub sources: Option<serde_json::Value>,
    /// The task that ran it.
    pub task_id: Option<String>,
}

impl From<BatchItemRow> for BatchItemView {
    fn from(row: BatchItemRow) -> Self {
        Self {
            rank: row.rank,
            origin: row.origin,
            input: row.input,
            status: row.status,
            output: row.output,
            sources: row.sources,
            task_id: row.task_id,
        }
    }
}

/// A batch with its items.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BatchDetailView {
    pub batch: BatchView,
    pub items: Vec<BatchItemView>,
}

/// The batches, newest first.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BatchListView {
    pub batches: Vec<BatchView>,
}

/// Query of `GET /api/v1/batches`.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BatchListQuery {
    /// At most this many batches, 50 when absent.
    pub limit: Option<u32>,
}

/// `POST /api/v1/batches`: launch an agent on every item of a source.
///
/// The items are read and recorded before the answer; they run in the
/// background, at most `concurrency` at once (1 by default, 16 at most).
/// Once more than `maxFailures` items have failed (10 by default), the
/// batch is interrupted: no further item starts.
#[utoipa::path(
    post,
    path = "/api/v1/batches",
    tag = "batches",
    request_body = BatchLaunch,
    responses(
        (status = 202, description = "Batch launched", body = BatchView),
        (status = 400, description = "Invalid launch, unknown source type, or an agent that cannot run a batch", body = BatchErrorResponse),
        (status = 422, description = "The source could not be read", body = BatchErrorResponse),
        (status = 503, description = "Batch engine unavailable", body = BatchErrorResponse),
    )
)]
pub async fn launch_batch<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(launch): Json<BatchLaunch>,
) -> Result<(StatusCode, Json<BatchView>), RouteError> {
    let batch = engine(&state)?.launch(launch).await.map_err(batch_error)?;
    Ok((StatusCode::ACCEPTED, Json(batch.into())))
}

/// `GET /api/v1/batches/{id}`: a batch with its items.
#[utoipa::path(
    get,
    path = "/api/v1/batches/{id}",
    tag = "batches",
    params(("id" = String, Path, description = "Batch id")),
    responses(
        (status = 200, description = "The batch and its items", body = BatchDetailView),
        (status = 404, description = "No such batch", body = BatchErrorResponse),
        (status = 503, description = "Batch engine unavailable", body = BatchErrorResponse),
    )
)]
pub async fn get_batch<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<BatchDetailView>, RouteError> {
    let detail = engine(&state)?
        .detail(&id)
        .await
        .map_err(batch_error)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, format!("no batch '{id}'")))?;
    Ok(Json(BatchDetailView {
        batch: detail.batch.into(),
        items: detail.items.into_iter().map(Into::into).collect(),
    }))
}

/// `GET /api/v1/batches`: the batches, newest first.
#[utoipa::path(
    get,
    path = "/api/v1/batches",
    tag = "batches",
    params(BatchListQuery),
    responses(
        (status = 200, description = "The batches", body = BatchListView),
        (status = 503, description = "Batch engine unavailable", body = BatchErrorResponse),
    )
)]
pub async fn list_batches<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<BatchListQuery>,
) -> Result<Json<BatchListView>, RouteError> {
    let batches = engine(&state)?
        .list(query.limit.unwrap_or(50))
        .await
        .map_err(batch_error)?;
    Ok(Json(BatchListView {
        batches: batches.into_iter().map(Into::into).collect(),
    }))
}

/// The sensitive accesses of an agent.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BatchAccessesView {
    pub agent: String,
    pub accesses: Vec<BatchAccess>,
}

/// Query of `GET /api/v1/batches/accesses`.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BatchAccessesQuery {
    /// The agent a batch would run.
    pub agent: String,
}

/// `GET /api/v1/batches/accesses`: the sensitive accesses of an agent, those
/// a launch may grant for the duration of a batch.
///
/// Reading is never listed: it runs without asking. An access that is not
/// `grantable` (executing code, or an access nobody classified) is listed
/// too: an item that reaches for it waits for a decision.
#[utoipa::path(
    get,
    path = "/api/v1/batches/accesses",
    tag = "batches",
    params(BatchAccessesQuery),
    responses(
        (status = 200, description = "The sensitive accesses of the agent", body = BatchAccessesView),
        (status = 400, description = "An agent that cannot run a batch", body = BatchErrorResponse),
        (status = 503, description = "Batch engine unavailable", body = BatchErrorResponse),
    )
)]
pub async fn batch_accesses<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<BatchAccessesQuery>,
) -> Result<Json<BatchAccessesView>, RouteError> {
    let accesses = engine(&state)?
        .accesses(&query.agent)
        .await
        .map_err(batch_error)?;
    Ok(Json(BatchAccessesView {
        agent: query.agent,
        accesses,
    }))
}

async fn control<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    id: &str,
    control: BatchControl,
) -> Result<Json<BatchView>, RouteError> {
    let batch = engine(state)?
        .control(id, control)
        .await
        .map_err(batch_error)?;
    Ok(Json(batch.into()))
}

/// `POST /api/v1/batches/{id}/pause`: no further item starts; those running
/// end on their own. A paused batch stays paused across a restart.
#[utoipa::path(
    post,
    path = "/api/v1/batches/{id}/pause",
    tag = "batches",
    params(("id" = String, Path, description = "Batch id")),
    responses(
        (status = 200, description = "The batch, paused", body = BatchView),
        (status = 404, description = "No such batch", body = BatchErrorResponse),
        (status = 409, description = "The batch is not running", body = BatchErrorResponse),
        (status = 503, description = "Batch engine unavailable", body = BatchErrorResponse),
    )
)]
pub async fn pause_batch<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<BatchView>, RouteError> {
    control(&state, &id, BatchControl::Pause).await
}

/// `POST /api/v1/batches/{id}/resume`: a paused batch starts its items again.
#[utoipa::path(
    post,
    path = "/api/v1/batches/{id}/resume",
    tag = "batches",
    params(("id" = String, Path, description = "Batch id")),
    responses(
        (status = 200, description = "The batch, running", body = BatchView),
        (status = 404, description = "No such batch", body = BatchErrorResponse),
        (status = 409, description = "The batch is not paused", body = BatchErrorResponse),
        (status = 503, description = "Batch engine unavailable", body = BatchErrorResponse),
    )
)]
pub async fn resume_batch<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<BatchView>, RouteError> {
    control(&state, &id, BatchControl::Resume).await
}

/// `POST /api/v1/batches/{id}/stop`: the running items are cancelled, the
/// pending ones are canceled without running, and the batch ends `stopped`.
#[utoipa::path(
    post,
    path = "/api/v1/batches/{id}/stop",
    tag = "batches",
    params(("id" = String, Path, description = "Batch id")),
    responses(
        (status = 200, description = "The batch, stopped", body = BatchView),
        (status = 404, description = "No such batch", body = BatchErrorResponse),
        (status = 409, description = "The batch has already ended", body = BatchErrorResponse),
        (status = 503, description = "Batch engine unavailable", body = BatchErrorResponse),
    )
)]
pub async fn stop_batch<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<BatchView>, RouteError> {
    control(&state, &id, BatchControl::Stop).await
}

/// `POST /api/v1/batches/{id}/retry`: the failed items, and those waiting for
/// an access, run again from their input as read at launch. `grants` adds
/// accesses granted for the rest of the batch.
#[utoipa::path(
    post,
    path = "/api/v1/batches/{id}/retry",
    tag = "batches",
    params(("id" = String, Path, description = "Batch id")),
    request_body = BatchRetry,
    responses(
        (status = 200, description = "The batch, running again", body = BatchView),
        (status = 400, description = "A grant the agent cannot take", body = BatchErrorResponse),
        (status = 404, description = "No such batch", body = BatchErrorResponse),
        (status = 409, description = "No failed or waiting item", body = BatchErrorResponse),
        (status = 503, description = "Batch engine unavailable", body = BatchErrorResponse),
    )
)]
pub async fn retry_batch<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Json(retry): Json<BatchRetry>,
) -> Result<Json<BatchView>, RouteError> {
    control(&state, &id, BatchControl::Retry(retry)).await
}

/// The batch routes.
pub fn batch_routes<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/batches",
            get(list_batches::<B>).post(launch_batch::<B>),
        )
        .route("/api/v1/batches/accesses", get(batch_accesses::<B>))
        .route("/api/v1/batches/:id", get(get_batch::<B>))
        .route("/api/v1/batches/:id/pause", post(pause_batch::<B>))
        .route("/api/v1/batches/:id/resume", post(resume_batch::<B>))
        .route("/api/v1/batches/:id/stop", post(stop_batch::<B>))
        .route("/api/v1/batches/:id/retry", post(retry_batch::<B>))
}

#[cfg(test)]
mod access_tests;
#[cfg(test)]
mod tests;
