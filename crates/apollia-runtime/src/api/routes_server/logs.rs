//! The logs of the compatible server, from the native API:
//! `GET /api/v1/server/logs` streams them, `DELETE /api/v1/server/logs`
//! empties them.
//!
//! The stream sends what the ring holds, oldest first, then each new line as
//! it is recorded, every line at or above the asked level. Emptying the ring
//! touches nothing else: the audit journal keeps every entry it holds.

use std::convert::Infallible;

use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::{Stream, StreamExt};

use super::{handle, ApiError};
use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::compat_server::{ServerLogLevel, ServerLogLine, ServerLogsError};
use crate::coordinator::ExecutionBackend;

/// SSE event name of a frame that carries one line of the logs.
pub const LOG_FRAME: &str = "log";

/// The query of `GET /api/v1/server/logs`.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ServerLogsQuery {
    /// The lowest level sent: `debug` (every line, the default), `info`,
    /// `warn` or `error`.
    pub level: Option<ServerLogLevel>,
}

/// What `DELETE /api/v1/server/logs` answers.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClearedServerLogs {
    /// How many lines the ring held.
    pub cleared: usize,
}

fn gone(error: ServerLogsError) -> ApiError {
    (
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        Json(ApiErrorBody {
            error: error.to_string(),
        }),
    )
}

/// The SSE frame for one line, `None` when it cannot be serialized.
fn frame_of(line: &ServerLogLine) -> Option<Result<Event, Infallible>> {
    match serde_json::to_string(line) {
        Ok(json) => Some(Ok(Event::default().event(LOG_FRAME).data(json))),
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                "api.server_logs.serialize_failed"
            );
            None
        }
    }
}

/// `GET /api/v1/server/logs`: the logs of the compatible server as SSE.
///
/// Each `log` frame carries one line. The ring comes first, oldest line
/// first, then each line as it is recorded; the stream stays open until the
/// client disconnects or the runtime stops.
#[utoipa::path(
    get,
    path = "/api/v1/server/logs",
    tag = "server",
    params(ServerLogsQuery),
    responses(
        (
            status = 200,
            description = "SSE stream of the server's logs: each `log` frame carries one ServerLogLine, the lines held first, then the live ones",
            content_type = "text/event-stream",
            body = ServerLogLine
        ),
        (status = 400, description = "Unknown level"),
        (status = 503, description = "No compatible server in this runtime", body = ApiErrorBody),
    )
)]
pub(crate) async fn stream_server_logs<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<ServerLogsQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let subscription = handle(&state)?
        .logs()
        .subscribe(query.level.unwrap_or_default())
        .await
        .map_err(gone)?;
    let backlog = tokio_stream::iter(subscription.backlog);
    let live = ReceiverStream::new(subscription.live);
    let frames = backlog.chain(live).filter_map(|line| frame_of(&line));
    Ok(Sse::new(frames).keep_alive(KeepAlive::default()))
}

/// `DELETE /api/v1/server/logs`: empties the logs of the compatible server.
/// The audit journal is not touched.
#[utoipa::path(
    delete,
    path = "/api/v1/server/logs",
    tag = "server",
    responses(
        (status = 200, description = "The logs are empty; how many lines they held", body = ClearedServerLogs),
        (status = 503, description = "No compatible server in this runtime", body = ApiErrorBody),
    )
)]
pub(crate) async fn clear_server_logs<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<ClearedServerLogs>, ApiError> {
    let cleared = handle(&state)?.logs().clear().await.map_err(gone)?;
    Ok(Json(ClearedServerLogs { cleared }))
}
