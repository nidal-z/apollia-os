//! `GET /api/v1/tasks/{id}/trace`, event-sourced execution trace.
//!
//! Single source of truth: the `runtime_events` table populated by
//! [`crate::observability::EventPersistorHandle`]. Pagination uses a UUIDv7
//! cursor (`?since=<event_id>`), not `OFFSET`: the lexicographic order of
//! UUIDv7 values is the causal order.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::observability::{RuntimeEventRecord, RuntimeEventsRepository};

/// Default cap on the number of events returned per call.
///
/// Enough to render 95% of typical traces (< 200 events) in a single fetch.
/// Long traces are paginated via `?since=<event_id>` with a configurable
/// `limit` (up to 5000).
const DEFAULT_LIMIT: usize = 500;

/// Upper bound so a client cannot request an entire database at once.
const MAX_LIMIT: usize = 5000;

/// Query string parameters.
#[derive(Debug, Deserialize)]
pub struct TraceQuery {
    /// Pagination cursor: returns only events *strictly* after this
    /// `event_id` (lex-ordered UUIDv7). Absent means from the start.
    #[serde(default)]
    pub since: Option<String>,
    /// Maximum number of events to return. Clamped to `MAX_LIMIT`.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// JSON response.
#[derive(Debug, Serialize)]
pub struct TraceResponse {
    /// Task identifier.
    pub task_id: String,
    /// Events ordered chronologically (UUIDv7 ASC).
    pub events: Vec<RuntimeEventRecord>,
    /// Cursor to pass as `?since=` on the next call to fetch the rest.
    /// `None` when the current page reaches the known end.
    pub next_cursor: Option<String>,
}

/// Structured error response.
#[derive(Debug, Serialize)]
pub struct TraceErrorResponse {
    /// Error message.
    pub error: String,
}

/// Handler for `GET /api/v1/tasks/{id}/trace`.
///
/// 200: `TraceResponse` (may be empty if the task has not yet produced any
/// persisted event).
/// 500: error opening the database or running the query.
pub async fn get_task_trace<B: ExecutionBackend + Clone>(
    Path(task_id): Path<String>,
    Query(q): Query<TraceQuery>,
    State(state): State<AppState<B>>,
) -> Result<Json<TraceResponse>, (StatusCode, Json<TraceErrorResponse>)> {
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT).max(1);
    let since = q.since.clone();
    let task_id_for_query = task_id.clone();

    let db_path = resolve_runtime_events_db(&state);

    let result = tokio::task::spawn_blocking(move || -> Result<Vec<RuntimeEventRecord>, String> {
        let repo = RuntimeEventsRepository::open(&db_path)
            .map_err(|e| format!("open runtime_events: {e}"))?;
        repo.list_for_task(&task_id_for_query, since.as_deref(), limit)
            .map_err(|e| format!("query runtime_events: {e}"))
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(TraceErrorResponse {
                error: format!("blocking task panicked: {e}"),
            }),
        )
    })?;

    let events = result.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(TraceErrorResponse { error: e }),
        )
    })?;

    // Cursor: if we returned exactly `limit` events, there are probably more,
    // so expose the last event_id as the next cursor. Otherwise (partial
    // page), no cursor.
    let next_cursor = if events.len() == limit {
        events.last().map(|e| e.event_id.clone())
    } else {
        None
    };

    Ok(Json(TraceResponse {
        task_id,
        events,
        next_cursor,
    }))
}

/// Resolves the path to `runtime_events.db` from the `AppState`.
///
/// For now this uses the same heuristic as `routes_timeline::resolve_data_dir`
/// (`$HOME/.apollia/`). Once the config dir becomes a structured field on the
/// `AppState`, move this helper into `api::server`.
fn resolve_runtime_events_db<B: ExecutionBackend + Clone>(
    _state: &AppState<B>,
) -> std::path::PathBuf {
    let base = std::env::var("HOME")
        .ok()
        .map(|h| std::path::PathBuf::from(format!("{h}/.apollia")))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/apollia"));
    base.join("runtime_events.db")
}
