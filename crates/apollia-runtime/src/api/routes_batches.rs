//! REST routes of the batch engine: launch a batch, read one, list them.
//!
//! `POST /api/v1/batches` launches an agent on every item of a source,
//! `GET /api/v1/batches/{id}` reads a batch with its items, and
//! `GET /api/v1/batches` lists the batches, newest first.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use apollia_core::batch_source::SourceError;
use apollia_tools::{BatchItemRow, BatchRow};

use crate::api::server::AppState;
use crate::batch::{BatchError, BatchHandle, BatchLaunch};
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
    /// `running`, `completed` or `interrupted`.
    pub state: String,
    pub total: u64,
    pub completed: u64,
    pub failed: u64,
    pub canceled: u64,
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
    /// `pending`, `running`, `completed`, `failed` or `canceled`.
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

/// The batch routes.
pub fn batch_routes<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/batches",
            get(list_batches::<B>).post(launch_batch::<B>),
        )
        .route("/api/v1/batches/:id", get(get_batch::<B>))
}

#[cfg(test)]
mod tests;
