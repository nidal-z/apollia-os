//! `GET /api/v1/audit/activity`: what the agents did with the tools of one
//! connection over a period, read from the journal.
//!
//! The connection is named by the prefix its tools share (`mcp:notion/`,
//! `conn:google:ada/`). The journal is read with the filters of
//! `GET /api/v1/audit/journal` and the ends of tool calls are counted, so the
//! answer holds an agent, a count and a date, never a content.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::api::routes_audit::{resolve_filter, ErrorResponse};
use crate::api::server::AppState;
use crate::audit_journal::{tally, AgentActivity, JournalCriteria, JournalNature};
use crate::coordinator::ExecutionBackend;

/// The period read when the request names no start.
const DEFAULT_PERIOD_DAYS: i64 = 7;

/// Query parameters of `GET /api/v1/audit/activity`.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ConnectionActivityQuery {
    /// The prefix the tools of the connection share, such as `mcp:notion/`.
    pub tool_prefix: Option<String>,
    /// Start of the period, included: a day (`YYYY-MM-DD`, UTC) or an RFC 3339
    /// instant. Defaults to seven days before the request.
    pub since: Option<String>,
    /// End of the period, excluded: a day or an RFC 3339 instant. Defaults to
    /// no end.
    pub until: Option<String>,
}

/// Response body of `GET /api/v1/audit/activity`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionActivityResponse {
    /// The prefix the activity was read for.
    pub tool_prefix: String,
    /// Start of the period read, RFC 3339 UTC.
    pub since: String,
    /// End of the period read, RFC 3339 UTC; `None` when it has no end.
    pub until: Option<String>,
    /// Tool calls of the connection that ended over the period.
    pub actions: u64,
    /// When the last of them ended, RFC 3339 UTC; `None` when none did.
    pub last_used_at: Option<String>,
    /// One line per agent, the most active first.
    pub agents: Vec<AgentActivity>,
}

fn bad_request(message: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: message.to_owned(),
        }),
    )
}

fn instant(at: &DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// `GET /api/v1/audit/activity`: the last use of the tools of a connection
/// and the number of actions each agent took with them over a period.
///
/// An action is a tool call of the connection that ended, successfully or
/// not. The journal is read whole for the period, without an index. 400 when
/// the prefix is missing or a bound cannot be read, 503 when the journal is
/// not configured.
#[utoipa::path(
    get,
    path = "/api/v1/audit/activity",
    tag = "audit",
    params(ConnectionActivityQuery),
    responses(
        (status = 200, description = "The last use and the actions per agent of the connection over the period", body = ConnectionActivityResponse),
        (status = 400, description = "No tool prefix, or a bound of the period that cannot be read", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Audit journal not configured", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_connection_activity<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<ConnectionActivityQuery>,
) -> Result<Json<ConnectionActivityResponse>, (StatusCode, Json<ErrorResponse>)> {
    let tool_prefix = query
        .tool_prefix
        .as_deref()
        .map(str::trim)
        .filter(|prefix| !prefix.is_empty())
        .ok_or_else(|| bad_request("tool_prefix is required, such as mcp:notion/"))?
        .to_owned();
    let journal = state.audit_journal.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "audit journal not available".to_owned(),
            }),
        )
    })?;
    let mut filter = resolve_filter(
        &state,
        JournalCriteria {
            since: query.since.as_deref(),
            until: query.until.as_deref(),
            ..JournalCriteria::default()
        },
    )
    .await?;
    filter.natures = vec![JournalNature::Action];
    filter.tool_prefixes = vec![tool_prefix.clone()];
    let since = *filter
        .since
        .get_or_insert_with(|| Utc::now() - Duration::days(DEFAULT_PERIOD_DAYS));
    let until = filter.until;

    let lines = journal.query_lines(filter, usize::MAX, 0).await;
    let activity = tally(&lines);
    tracing::event!(
        tracing::Level::DEBUG,
        tool_prefix = %tool_prefix,
        actions = activity.actions,
        "audit.activity.read"
    );
    Ok(Json(ConnectionActivityResponse {
        tool_prefix,
        since: instant(&since),
        until: until.as_ref().map(instant),
        actions: activity.actions,
        last_used_at: activity.last_used_at,
        agents: activity.agents,
    }))
}

#[cfg(test)]
mod tests;
