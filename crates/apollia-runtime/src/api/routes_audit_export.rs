//! `GET /api/v1/audit/export`: the filtered journal as a CSV or JSON file.
//!
//! The route answers the content of the file and writes nothing itself: the
//! caller, on the same machine, decides where the file goes. Every export is
//! itself recorded in the journal, with its format, its filters and whether
//! the detail went with it.

use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use crate::api::routes_audit::{resolve_filter, ErrorResponse};
use crate::api::server::AppState;
use crate::audit_journal::{
    exported_entries, write_export, JournalCriteria, JournalExportError, JournalExportFormat,
    JournalFilter,
};
use crate::coordinator::ExecutionBackend;
use crate::governance::{record_export, ExportFilters};

/// Query parameters of `GET /api/v1/audit/export`.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct JournalExportQuery {
    /// `csv` or `json`.
    #[param(value_type = JournalExportFormat)]
    pub format: String,
    /// `true` to add, to each entry, its payload and the arguments and output
    /// of its tool call. Those may hold personal data. Defaults to `false`.
    #[serde(default)]
    pub detail: bool,
    /// Comma-separated agent names. When set, the export holds the entries
    /// that name one of those agents and the runs of their tasks.
    pub agents: Option<String>,
    /// Comma-separated action families (`read`, `write_or_delete`, ...).
    pub family: Option<String>,
    /// Comma-separated natures: `action`, `access_change`, `policy`, `system`.
    pub nature: Option<String>,
    /// Comma-separated results: `success`, `failure`, `refused`.
    pub result: Option<String>,
    /// Start of the period, included: a day (`YYYY-MM-DD`, UTC) or an RFC 3339
    /// instant.
    pub since: Option<String>,
    /// End of the period, excluded: a day or an RFC 3339 instant.
    pub until: Option<String>,
}

fn error(status: StatusCode, message: impl ToString) -> (StatusCode, Json<ErrorResponse>) {
    (
        status,
        Json(ErrorResponse {
            error: message.to_string(),
        }),
    )
}

/// What the journal keeps of the filters of an export.
fn recorded_filters(filter: &JournalFilter) -> ExportFilters {
    let instant =
        |at: &chrono::DateTime<chrono::Utc>| at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    ExportFilters {
        agents: filter.agents.clone(),
        families: filter.families.clone(),
        natures: filter
            .natures
            .iter()
            .map(|nature| nature.as_str().to_owned())
            .collect(),
        results: filter
            .outcomes
            .iter()
            .map(|outcome| outcome.as_str().to_owned())
            .collect(),
        since: filter.since.as_ref().map(instant),
        until: filter.until.as_ref().map(instant),
    }
}

/// `GET /api/v1/audit/export`: the journal as a file, narrowed by the filters
/// of `GET /api/v1/audit/journal`.
///
/// Every entry that passes the filters is in the file, newest first: the
/// export is not paged. Each entry is its summary (when, who, what, on what,
/// with which result) and its place in the chain (run, sequence, kind, hash),
/// without content. With `detail=true` each entry also holds its payload and,
/// for a tool call, its arguments and its output. `csv` is UTF-8 with one
/// header row, `json` is one document with the entries. The file is offered
/// as `journal.<format>`, and the export is recorded in the journal. 400 on a
/// format or a filter value that cannot be read, 503 when the journal is not
/// configured.
#[utoipa::path(
    get,
    path = "/api/v1/audit/export",
    tag = "audit",
    params(JournalExportQuery),
    responses(
        (status = 200, description = "The filtered journal as a CSV or JSON file", content(
            (String = "text/csv"),
            (Object = "application/json"),
        )),
        (status = 400, description = "A format or a filter value that cannot be read", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Audit journal not configured, or `agents` given with no task repository to resolve them", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn export_audit_journal<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<JournalExportQuery>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let format: JournalExportFormat = query
        .format
        .parse()
        .map_err(|e: JournalExportError| error(StatusCode::BAD_REQUEST, e))?;
    let journal = state.audit_journal.as_ref().ok_or_else(|| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "audit journal not available",
        )
    })?;
    let filter = resolve_filter(
        &state,
        JournalCriteria {
            agents: query.agents.as_deref(),
            families: query.family.as_deref(),
            natures: query.nature.as_deref(),
            outcomes: query.result.as_deref(),
            since: query.since.as_deref(),
            until: query.until.as_deref(),
        },
    )
    .await?;
    let filters = recorded_filters(&filter);

    // One read with no ceiling: an export that stopped at a page would be a
    // file that looks whole and is not.
    let lines = journal.query_lines(filter, usize::MAX, 0).await;
    let entries = exported_entries(journal, &lines, query.detail).await;
    let bytes = write_export(&entries, format, query.detail)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    // Recorded once the file is written, so an export never lists itself.
    record_export(
        journal,
        format.extension(),
        entries.len() as u64,
        filters,
        query.detail,
    );
    tracing::event!(
        tracing::Level::INFO,
        format = %format.extension(),
        detail = query.detail,
        entries = entries.len(),
        bytes = bytes.len(),
        "audit.journal.exported"
    );
    Ok((
        [
            (header::CONTENT_TYPE, format.content_type().to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"journal.{}\"", format.extension()),
            ),
        ],
        bytes,
    )
        .into_response())
}

#[cfg(test)]
mod tests;
