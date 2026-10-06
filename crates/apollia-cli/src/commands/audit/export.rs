//! `audit export`: write the filtered journal to a file, as CSV or JSON.

use std::path::Path;

use crate::client::{ClientError, RuntimeClient};
use crate::exit_codes;
use crate::note;

/// A file format the journal is exported in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum JournalExportFormat {
    /// Comma-separated values, UTF-8, one header row.
    Csv,
    /// One JSON document with the entries.
    Json,
}

impl JournalExportFormat {
    /// The name the format carries on the wire.
    fn as_str(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
        }
    }
}

/// What an export is asked with: its format, whether the detail goes with
/// it, and its filters.
pub(super) struct JournalExport<'a> {
    pub(super) format: JournalExportFormat,
    pub(super) detail: bool,
    pub(super) agents: &'a [String],
    pub(super) families: &'a [String],
    pub(super) natures: &'a [String],
    pub(super) results: &'a [String],
    pub(super) since: Option<&'a str>,
    pub(super) until: Option<&'a str>,
}

/// The export URI: the detail and each filter appear only when they are
/// given, the values encoded and joined by commas as the route reads them.
fn export_uri(export: &JournalExport<'_>) -> String {
    let mut uri = format!("/api/v1/audit/export?format={}", export.format.as_str());
    if export.detail {
        uri.push_str("&detail=true");
    }
    let encode = crate::commands::model::urlencode;
    for (name, values) in [
        ("agents", export.agents),
        ("family", export.families),
        ("nature", export.natures),
        ("result", export.results),
    ] {
        if !values.is_empty() {
            let encoded: Vec<String> = values.iter().map(|v| encode(v)).collect();
            uri.push_str(&format!("&{name}={}", encoded.join(",")));
        }
    }
    for (name, value) in [("since", export.since), ("until", export.until)] {
        if let Some(value) = value {
            uri.push_str(&format!("&{name}={}", encode(value)));
        }
    }
    uri
}

/// `apollia-os audit export`: ask the runtime for the filtered journal and
/// write it to `output`.
///
/// The runtime answers the content and this command writes the file, so the
/// export stays on the machine. Nothing is written when the runtime refuses
/// or cannot be reached. With the detail, a warning says the file may hold
/// personal data.
pub(super) async fn run_export(
    client: &RuntimeClient,
    export: &JournalExport<'_>,
    output: &Path,
    json: bool,
) -> i32 {
    let bytes = match client.export_journal(&export_uri(export)).await {
        Ok(bytes) => bytes,
        Err(e) => return handle_error(e, json),
    };
    if let Err(e) = std::fs::write(output, &bytes) {
        return crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("could not write {}: {e}", output.display()),
        );
    }
    let format = export.format.as_str();
    if json {
        println!(
            "{}",
            serde_json::json!({
                "format": format,
                "detail": export.detail,
                "path": output.display().to_string(),
                "bytes": bytes.len(),
            })
        );
    } else {
        println!("{}", output.display());
        note!("Journal exported as {format}, {} bytes.", bytes.len());
        if export.detail {
            eprintln!("! this file holds the detail of each entry: it may contain personal data.");
        }
    }
    exit_codes::SUCCESS
}

/// An unreachable runtime, or one without a journal, is a runtime error (2);
/// a refused format or filter is a general error (1).
fn handle_error(err: ClientError, json: bool) -> i32 {
    match err {
        ClientError::ConnectionRefused => crate::output::emit_error(
            json,
            exit_codes::RUNTIME_ERROR,
            "runtime not started (connection refused)",
        ),
        ClientError::ServerError { status: 503, body } => {
            crate::output::emit_error(json, exit_codes::RUNTIME_ERROR, &body)
        }
        ClientError::ServerError { body, .. } => {
            crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &body)
        }
        other => crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn export<'a>(format: JournalExportFormat, detail: bool) -> JournalExport<'a> {
        JournalExport {
            format,
            detail,
            agents: &[],
            families: &[],
            natures: &[],
            results: &[],
            since: None,
            until: None,
        }
    }

    #[test]
    fn test_the_export_uri_names_the_format_and_nothing_it_was_not_given() {
        // GIVEN an export in each format, the detail and the filters left out
        let csv = export(JournalExportFormat::Csv, false);
        let json = export(JournalExportFormat::Json, false);

        // WHEN the URI of each is built
        // THEN it names the format alone: a summary of the whole journal
        assert_eq!(export_uri(&csv), "/api/v1/audit/export?format=csv");
        assert_eq!(export_uri(&json), "/api/v1/audit/export?format=json");
    }

    #[test]
    fn test_the_export_uri_carries_the_detail_and_every_filter() {
        // GIVEN an export with the detail and every filter, one agent name
        // holding a space
        let agents = ["quotes".to_owned(), "flux invoices".to_owned()];
        let families = ["read".to_owned(), "send_external".to_owned()];
        let natures = ["action".to_owned()];
        let results = ["refused".to_owned()];
        let export = JournalExport {
            agents: &agents,
            families: &families,
            natures: &natures,
            results: &results,
            since: Some("2026-10-01"),
            until: Some("2026-10-04T00:00:00Z"),
            ..export(JournalExportFormat::Csv, true)
        };

        // WHEN the URI is built
        let uri = export_uri(&export);

        // THEN the detail is asked for, and each filter is joined by commas
        // under the name the route reads, its values encoded
        assert_eq!(
            uri,
            "/api/v1/audit/export?format=csv&detail=true\
             &agents=quotes,flux%20invoices&family=read,send_external\
             &nature=action&result=refused\
             &since=2026-10-01&until=2026-10-04T00%3A00%3A00Z"
        );
    }

    #[tokio::test]
    async fn test_an_export_without_runtime_is_a_runtime_error_and_writes_nothing() {
        // GIVEN a socket no runtime listens on, and a file that does not exist
        let dir = tempfile::tempdir().unwrap();
        let client = RuntimeClient::new(dir.path().join("off.sock"));
        let output = dir.path().join("journal.csv");

        // WHEN the journal is exported to that file
        let code = run_export(
            &client,
            &export(JournalExportFormat::Csv, false),
            &output,
            true,
        )
        .await;

        // THEN the command exits 2 and the file was not created
        assert_eq!(code, exit_codes::RUNTIME_ERROR);
        assert!(!output.exists());
    }
}
