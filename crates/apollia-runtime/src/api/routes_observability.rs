//! The reads of the Observability view that span every store of the data
//! directory: the global timeline and the inter-agent mailbox.
//!
//! - `GET /api/v1/observability/timeline?window_minutes=N`: every event of
//!   the last `N` minutes, most recent first.
//! - `GET /api/v1/mailbox/messages?limit=N`: the most recent mailbox
//!   messages across every recipient, newest first.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::observability::global_timeline::{read_global_timeline, GlobalTimelineEvent};
use crate::observability::mailbox_rows::{read_mailbox_messages, MailboxMessageRow};

type ApiError = (StatusCode, Json<ApiErrorBody>);

/// The widest window the timeline scans: one week.
const MAX_WINDOW_MINUTES: u32 = 7 * 24 * 60;
/// Mailbox rows returned when the request names no limit.
const DEFAULT_MAILBOX_LIMIT: u32 = 100;
/// The most mailbox rows one request returns.
const MAX_MAILBOX_LIMIT: u32 = 1000;

/// Query of `GET /api/v1/observability/timeline`.
#[derive(Debug, Deserialize)]
pub struct TimelineQuery {
    /// Trailing window, in minutes, between 1 and one week.
    pub window_minutes: Option<u32>,
}

/// Query of `GET /api/v1/mailbox/messages`.
#[derive(Debug, Deserialize)]
pub struct MailboxMessagesQuery {
    /// Maximum rows (default 100, capped at 1000).
    pub limit: Option<u32>,
}

/// Build the observability router.
pub fn observability_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/observability/timeline",
            get(get_global_timeline::<B>),
        )
        .route("/api/v1/mailbox/messages", get(list_mailbox_messages::<B>))
}

fn refusal(status: StatusCode, error: String) -> ApiError {
    (status, Json(ApiErrorBody { error }))
}

/// `GET /api/v1/observability/timeline`: the global timeline of a window.
#[utoipa::path(
    get,
    path = "/api/v1/observability/timeline",
    tag = "audit",
    params(
        ("window_minutes" = u32, Query, description = "Trailing window in minutes, from 1 to 10080"),
    ),
    responses(
        (status = 200, description = "Events of the window, most recent first", body = [GlobalTimelineEvent]),
        (status = 400, description = "Window missing or out of range", body = ApiErrorBody),
        (status = 500, description = "The scan failed", body = ApiErrorBody),
    )
)]
pub(crate) async fn get_global_timeline<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<Vec<GlobalTimelineEvent>>, ApiError> {
    let window = match query.window_minutes {
        Some(minutes) if (1..=MAX_WINDOW_MINUTES).contains(&minutes) => minutes,
        _ => {
            return Err(refusal(
                StatusCode::BAD_REQUEST,
                format!("window_minutes must be between 1 and {MAX_WINDOW_MINUTES}"),
            ))
        }
    };
    read_global_timeline(&state.registry_handle, state.data_dir.clone(), window)
        .await
        .map(Json)
        .map_err(|e| refusal(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// `GET /api/v1/mailbox/messages`: the most recent inter-agent messages.
#[utoipa::path(
    get,
    path = "/api/v1/mailbox/messages",
    tag = "agents",
    params(
        ("limit" = Option<u32>, Query, description = "Maximum rows (default 100, capped at 1000; 0 is refused)"),
    ),
    responses(
        (status = 200, description = "Mailbox rows, newest first", body = [MailboxMessageRow]),
        (status = 400, description = "A limit of 0", body = ApiErrorBody),
        (status = 500, description = "The store could not be read", body = ApiErrorBody),
    )
)]
pub(crate) async fn list_mailbox_messages<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<MailboxMessagesQuery>,
) -> Result<Json<Vec<MailboxMessageRow>>, ApiError> {
    let limit = match query.limit {
        Some(0) => {
            return Err(refusal(
                StatusCode::BAD_REQUEST,
                "limit must be at least 1".to_owned(),
            ))
        }
        Some(limit) => limit.min(MAX_MAILBOX_LIMIT),
        None => DEFAULT_MAILBOX_LIMIT,
    };
    read_mailbox_messages(state.data_dir.clone(), limit)
        .await
        .map(Json)
        .map_err(|e| refusal(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[cfg(test)]
mod tests;
