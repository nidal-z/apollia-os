//! The journal as a file: CSV or JSON, in summary or with the detail.
//!
//! An export holds, by default, what a line of the journal says: who did what
//! to what, with which result and when, and the position of the entry in its
//! chain. It holds no content. The detail is asked for: it adds the payload
//! of each entry and, for a tool call, its arguments and its output, which is
//! where personal data may be.
//!
//! Both formats are written without a dependency, and both hold the same
//! entries in the same order, newest first.

use std::collections::HashMap;
use std::str::FromStr;

use serde::Serialize;
use serde_json::Value;

use crate::audit_journal::detail::{entry_detail, ToolCallDetail};
use crate::audit_journal::entry::JournalEntry;
use crate::audit_journal::summary::{JournalLine, JournalSummary};
use crate::audit_journal::AuditJournalHandle;
use crate::batch::csv_line;

/// A file format the journal is exported in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum JournalExportFormat {
    /// Comma-separated values, UTF-8, one header row.
    Csv,
    /// One JSON document with the entries.
    Json,
}

impl JournalExportFormat {
    /// The media type the file is served with.
    #[must_use]
    pub fn content_type(self) -> &'static str {
        match self {
            Self::Csv => "text/csv; charset=utf-8",
            Self::Json => "application/json",
        }
    }

    /// The file extension, without its dot.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
        }
    }
}

impl FromStr for JournalExportFormat {
    type Err = JournalExportError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "csv" => Ok(Self::Csv),
            "json" => Ok(Self::Json),
            other => Err(JournalExportError::Format(other.to_owned())),
        }
    }
}

/// Why the journal could not be exported.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum JournalExportError {
    /// The format is neither `csv` nor `json`.
    #[error("unknown export format '{0}': expected csv or json")]
    Format(String),
    /// The JSON document could not be written.
    #[error("the JSON document could not be written: {0}")]
    Json(#[from] serde_json::Error),
}

/// What the detail adds to the summary of an entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportedDetail {
    /// The payload of the entry, as it is chained.
    pub payload: Value,
    /// Arguments and output, when the entry is about a tool call.
    pub tool_call: Option<ToolCallDetail>,
}

/// One entry of an export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportedEntry {
    /// Run the entry belongs to.
    pub run_id: String,
    /// Sequence number of the entry within its run.
    pub seq: u64,
    /// Kind of the entry, under its wire name.
    pub kind: String,
    /// Hash of the entry, which places it in the chain the journal verifies.
    pub hash: String,
    /// The summary of the entry.
    #[serde(flatten)]
    pub summary: JournalSummary,
    /// The detail, in an export that was asked for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<ExportedDetail>,
}

impl ExportedEntry {
    /// The summary of `line`, without any of its content.
    #[must_use]
    pub fn summary_of(line: &JournalLine) -> Self {
        Self {
            run_id: line.entry.run_id.clone(),
            seq: line.entry.seq,
            kind: line.entry.kind.tag().to_owned(),
            hash: line.entry.hash.clone(),
            summary: line.summary.clone(),
            detail: None,
        }
    }

    /// `line` with its detail, joined from `run`, the entries of its run
    /// ordered by ascending `seq`.
    #[must_use]
    pub fn detailed(line: &JournalLine, run: &[JournalEntry]) -> Self {
        let tool_call = entry_detail(run, line.entry.seq).and_then(|detail| detail.tool_call);
        Self {
            detail: Some(ExportedDetail {
                payload: line.entry.payload.clone(),
                tool_call,
            }),
            ..Self::summary_of(line)
        }
    }
}

/// The JSON document of an export.
#[derive(Debug, Serialize)]
struct ExportDocument<'a> {
    /// Whether each entry holds its detail.
    detail: bool,
    /// Number of entries.
    count: usize,
    /// The entries, newest first.
    entries: &'a [ExportedEntry],
}

/// The columns every export has, in order.
const SUMMARY_COLUMNS: [&str; 12] = [
    "ts", "agent", "nature", "family", "what", "tool", "target", "outcome", "run_id", "seq",
    "kind", "hash",
];

/// The columns the detail adds, in order. The first three hold JSON.
const DETAIL_COLUMNS: [&str; 4] = ["payload", "arguments", "output", "status"];

/// The entries the export of `lines` holds: their summaries, or, with
/// `detail`, each one joined to the detail its run captured.
pub async fn exported_entries(
    journal: &AuditJournalHandle,
    lines: &[JournalLine],
    detail: bool,
) -> Vec<ExportedEntry> {
    if !detail {
        return lines.iter().map(ExportedEntry::summary_of).collect();
    }
    let mut runs: HashMap<&str, Vec<JournalEntry>> = HashMap::new();
    let mut entries = Vec::with_capacity(lines.len());
    for line in lines {
        let run_id = line.entry.run_id.as_str();
        if !runs.contains_key(run_id) {
            runs.insert(run_id, journal.query_run(run_id).await);
        }
        let run = runs.get(run_id).map_or(&[][..], Vec::as_slice);
        entries.push(ExportedEntry::detailed(line, run));
    }
    entries
}

/// `entries` written in `format`. `detail` says whether the entries hold
/// their detail, which decides the columns of the CSV file.
pub fn write_export(
    entries: &[ExportedEntry],
    format: JournalExportFormat,
    detail: bool,
) -> Result<Vec<u8>, JournalExportError> {
    match format {
        JournalExportFormat::Csv => Ok(to_csv(entries, detail)),
        JournalExportFormat::Json => Ok(serde_json::to_vec_pretty(&ExportDocument {
            detail,
            count: entries.len(),
            entries,
        })?),
    }
}

/// The entries as CSV (RFC 4180): a header row, then one row per entry.
fn to_csv(entries: &[ExportedEntry], detail: bool) -> Vec<u8> {
    let mut out = String::new();
    let detail_columns: &[&str] = if detail { &DETAIL_COLUMNS } else { &[] };
    csv_line(
        &mut out,
        SUMMARY_COLUMNS.iter().chain(detail_columns).copied(),
    );
    for entry in entries {
        let summary = &entry.summary;
        let seq = entry.seq.to_string();
        let mut row: Vec<&str> = vec![
            &summary.ts,
            summary.agent.as_deref().unwrap_or_default(),
            summary.nature.as_str(),
            summary.family.as_deref().unwrap_or_default(),
            &summary.what,
            summary.tool.as_deref().unwrap_or_default(),
            summary.target.as_deref().unwrap_or_default(),
            summary.outcome.map_or("", |outcome| outcome.as_str()),
            &entry.run_id,
            &seq,
            &entry.kind,
            &entry.hash,
        ];
        let cells = if detail {
            detail_cells(entry.detail.as_ref())
        } else {
            Vec::new()
        };
        row.extend(cells.iter().map(String::as_str));
        csv_line(&mut out, row.into_iter());
    }
    out.into_bytes()
}

/// The four cells of the detail of an entry: its payload, then the arguments,
/// the output and the status of its tool call, empty when it has none.
fn detail_cells(detail: Option<&ExportedDetail>) -> Vec<String> {
    let Some(detail) = detail else {
        return vec![String::new(); DETAIL_COLUMNS.len()];
    };
    let call = detail.tool_call.as_ref();
    let json = |value: Option<&Value>| value.map(Value::to_string).unwrap_or_default();
    vec![
        detail.payload.to_string(),
        json(call.and_then(|call| call.arguments.as_ref())),
        json(call.and_then(|call| call.output.as_ref())),
        call.and_then(|call| call.status.clone())
            .unwrap_or_default(),
    ]
}

#[cfg(test)]
mod tests;
