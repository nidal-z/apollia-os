//! `apollia-os source <verb>`: the registry of sources, what an agent may
//! consult once it is checked for it.
//!
//! Every leaf goes through the runtime (`GET`, `POST /api/v1/sources` and
//! `DELETE /api/v1/sources/{id}`), whose actor owns the registry in
//! `system.db`. Daemon off, each refuses with exit 2. An agent checks a source
//! with `agent set <agent> sources <id>...`.
//!
//! `inspect` previews a workbook or a CSV file before a batch is launched on
//! it (`POST /api/v1/sources/preview`): the sheet and header row proposed, the
//! first rows, and whether results may be written into the original. A file
//! that is not there is refused with exit 1 before the runtime is reached.

use std::path::PathBuf;

use clap::Subcommand;
use serde_json::Value;

use crate::client::{RawResponse, RuntimeClient};
use crate::exit_codes;

/// Route of the registry.
const SOURCES_ROUTE: &str = "/api/v1/sources";
/// Route of the preview of a workbook or a CSV file.
const PREVIEW_ROUTE: &str = "/api/v1/sources/preview";

/// `apollia-os source <verb>`.
#[derive(Debug, Subcommand)]
pub enum SourceCommand {
    /// List the sources, oldest first.
    List,
    /// Add a source.
    Create {
        /// What the source is: folder, file, mail, profile, memory or code_folder.
        #[arg(long)]
        kind: String,
        /// The name the user sees.
        #[arg(long)]
        label: String,
        /// What the source points at: a folder connection, a file path, a mail
        /// connection, a memory space. Required for folder, file and mail.
        #[arg(long)]
        reference: Option<String>,
        /// A group the source belongs to, such as the files of one project.
        #[arg(long)]
        group: Option<String>,
    },
    /// Remove a source. Agents that list it stop seeing it.
    Delete {
        /// Source identifier, as `source list` shows it.
        id: String,
        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        confirm: bool,
    },
    /// Preview a workbook or a CSV file as a batch would read it: the sheet
    /// and header row proposed, the first twenty rows, the columns.
    Inspect {
        /// The .xlsx, .xlsm, .xlsb, .xls, .ods or .csv file.
        file: PathBuf,
        /// Show this sheet instead of the proposed one.
        #[arg(long)]
        sheet: Option<String>,
        /// Read the header on this row (from 1) instead of the proposed one.
        #[arg(long)]
        header_row: Option<u32>,
    },
}

/// Run one leaf; returns the process exit code.
pub async fn run(cmd: &SourceCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let socket = socket.unwrap_or_else(crate::client::default_socket_path);
    let client = RuntimeClient::new(socket);
    let response = match cmd {
        SourceCommand::List => client.get(SOURCES_ROUTE).await,
        SourceCommand::Create {
            kind,
            label,
            reference,
            group,
        } => {
            let body = serde_json::json!({
                "kind": kind,
                "label": label,
                "reference": reference,
                "group": group,
            });
            client.post(SOURCES_ROUTE, Some(&body)).await
        }
        SourceCommand::Delete { id, confirm } => {
            let action = format!("delete source '{id}'");
            if let Some(code) = crate::output::require_confirmation(*confirm, json, &action) {
                return code;
            }
            client.delete(&format!("{SOURCES_ROUTE}/{id}")).await
        }
        SourceCommand::Inspect {
            file,
            sheet,
            header_row,
        } => {
            let path = match file.canonicalize() {
                Ok(path) => path,
                Err(e) => {
                    return crate::output::emit_error(
                        json,
                        exit_codes::GENERAL_ERROR,
                        &format!("cannot read {}: {e}", file.display()),
                    );
                }
            };
            let body = serde_json::json!({
                "path": path.display().to_string(),
                "sheet": sheet,
                "headerRow": header_row,
            });
            client.post(PREVIEW_ROUTE, Some(&body)).await
        }
    };
    match response {
        Ok(resp) if resp.status < 400 => {
            if json {
                println!("{}", resp.body);
            } else {
                render(cmd, &resp);
            }
            exit_codes::SUCCESS
        }
        Ok(resp) => {
            let message = serde_json::from_str::<Value>(&resp.body)
                .ok()
                .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_owned))
                .unwrap_or(resp.body);
            let code = if resp.status < 500 {
                exit_codes::GENERAL_ERROR
            } else {
                exit_codes::RUNTIME_ERROR
            };
            crate::output::emit_error(json, code, &message)
        }
        Err(e) => crate::output::emit_client_error(json, &e),
    }
}

/// The human rendering of a successful answer.
fn render(cmd: &SourceCommand, resp: &RawResponse) {
    let Ok(value) = serde_json::from_str::<Value>(&resp.body) else {
        println!("{}", resp.body);
        return;
    };
    match cmd {
        SourceCommand::List => {
            let rows = list_lines(&value);
            if rows.is_empty() {
                crate::note!("No sources.");
                return;
            }
            crate::note!("{}", row("ID", "KIND", "INDEX", "LABEL", "REFERENCE"));
            for line in rows {
                println!("{line}");
            }
        }
        SourceCommand::Create { .. } => {
            println!("{}", field(&value, "id"));
            crate::note!(
                "Source '{}' added. Check it for an agent with `agent set <agent> sources {}`.",
                field(&value, "label"),
                field(&value, "id")
            );
        }
        SourceCommand::Delete { .. } => {
            crate::note!(
                "Source '{}' ({}) removed.",
                field(&value, "label"),
                field(&value, "id")
            );
        }
        SourceCommand::Inspect { .. } => {
            for line in preview_lines(&value) {
                println!("{line}");
            }
        }
    }
}

/// The human rendering of a `POST /api/v1/sources/preview` answer.
fn preview_lines(value: &Value) -> Vec<String> {
    let number = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_u64)
            .map_or_else(|| "-".to_owned(), |n| n.to_string())
    };
    let sheets: Vec<String> = value
        .get("sheets")
        .and_then(Value::as_array)
        .map(|sheets| {
            sheets
                .iter()
                .map(|s| {
                    format!(
                        "{} ({} rows)",
                        field(s, "name"),
                        s.get("nonEmptyRows").and_then(Value::as_u64).unwrap_or(0)
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let columns: Vec<String> = value
        .get("columns")
        .and_then(Value::as_array)
        .map(|columns| {
            columns
                .iter()
                .map(|c| format!("{}={}", field(c, "letter"), field(c, "name")))
                .collect()
        })
        .unwrap_or_default();
    let flag = |key: &str| value.get(key).and_then(Value::as_bool).unwrap_or(false);
    let mut lines = vec![
        format!(
            "Format:          {} (source type {})",
            field(value, "format"),
            field(value, "sourceType")
        ),
        format!("Sheets:          {}", sheets.join(", ")),
        format!(
            "Sheet:           {} (proposed: {})",
            field(value, "sheet"),
            field(value, "proposedSheet")
        ),
        format!(
            "Header row:      {} (proposed: {})",
            number("headerRow"),
            number("proposedHeaderRow")
        ),
        format!("Columns:         {}", columns.join(", ")),
        format!("Items:           {}", number("estimatedItems")),
        format!(
            "Modify original: {}",
            if flag("canModifyOriginal") {
                "possible"
            } else {
                "no, results go to a copy"
            }
        ),
    ];
    if flag("formattingNotKept") {
        lines.push("Formatting:      not kept in the copy of results".to_owned());
    }
    lines.push(String::new());
    if let Some(rows) = value.get("rows").and_then(Value::as_array) {
        for row in rows {
            let cells: Vec<&str> = row
                .get("cells")
                .and_then(Value::as_array)
                .map(|cells| cells.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let number = row.get("row").and_then(Value::as_u64).unwrap_or(0);
            lines.push(
                format!("{number:>4} | {}", cells.join(" | "))
                    .trim_end()
                    .to_owned(),
            );
        }
    }
    lines
}

/// A string field of `value`, empty when absent.
fn field<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn row(id: &str, kind: &str, index: &str, label: &str, reference: &str) -> String {
    format!("{id:<36}  {kind:<11}  {index:<13}  {label:<24}  {reference}")
        .trim_end()
        .to_owned()
}

/// One line per source of a `GET /api/v1/sources` answer.
fn list_lines(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|sources| {
            sources
                .iter()
                .map(|s| {
                    row(
                        field(s, "id"),
                        field(s, "kind"),
                        field(s, "indexState"),
                        field(s, "label"),
                        field(s, "reference"),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    use crate::{Cli, Commands};

    /// The source leaf the real top-level parser selects for `args`.
    fn parse(args: &[&str]) -> SourceCommand {
        match Cli::parse_from(args).command {
            Commands::Source { command } => command,
            other => panic!("expected source, got {other:?}"),
        }
    }

    #[test]
    fn test_cli_parses_source_list() {
        // GIVEN "apollia-os source list"
        // WHEN the top-level parser reads it
        let leaf = parse(&["apollia-os", "source", "list"]);
        // THEN the list leaf is selected
        assert!(matches!(leaf, SourceCommand::List));
    }

    #[test]
    fn test_cli_parses_source_create() {
        // GIVEN a folder source with its reference and group
        // WHEN the top-level parser reads it
        let leaf = parse(&[
            "apollia-os",
            "source",
            "create",
            "--kind",
            "folder",
            "--label",
            "Contracts",
            "--reference",
            "conn-contracts",
            "--group",
            "Office",
        ]);
        // THEN every field is captured
        assert!(
            matches!(
                &leaf,
                SourceCommand::Create { kind, label, reference, group }
                    if kind == "folder"
                        && label == "Contracts"
                        && reference.as_deref() == Some("conn-contracts")
                        && group.as_deref() == Some("Office")
            ),
            "{leaf:?}"
        );
    }

    #[test]
    fn test_cli_parses_source_delete() {
        // GIVEN "apollia-os source delete src-1 --confirm"
        // WHEN the top-level parser reads it
        let leaf = parse(&["apollia-os", "source", "delete", "src-1", "--confirm"]);
        // THEN the identifier and the confirmation are captured
        assert!(
            matches!(&leaf, SourceCommand::Delete { id, confirm: true } if id == "src-1"),
            "{leaf:?}"
        );
    }

    #[test]
    fn test_cli_parses_source_inspect() {
        // GIVEN "apollia-os source inspect clients.xlsx --sheet Clients --header-row 3"
        // WHEN the top-level parser reads it
        let leaf = parse(&[
            "apollia-os",
            "source",
            "inspect",
            "clients.xlsx",
            "--sheet",
            "Clients",
            "--header-row",
            "3",
        ]);
        // THEN the file, the sheet and the header row are captured
        assert!(
            matches!(
                &leaf,
                SourceCommand::Inspect { file, sheet, header_row: Some(3) }
                    if file == &PathBuf::from("clients.xlsx") && sheet.as_deref() == Some("Clients")
            ),
            "{leaf:?}"
        );
    }

    #[test]
    fn test_the_preview_lines_show_the_proposal_the_flags_and_the_rows() {
        // GIVEN a preview answer for an .ods file
        let value = serde_json::json!({
            "format": "ods", "sourceType": "spreadsheet",
            "sheets": [{ "name": "Garde", "nonEmptyRows": 0 }, { "name": "Clients", "nonEmptyRows": 3 }],
            "proposedSheet": "Clients", "sheet": "Clients",
            "proposedHeaderRow": 2, "headerRow": 2,
            "columns": [{ "letter": "A", "name": "Nom" }, { "letter": "B", "name": "Ville" }],
            "rows": [
                { "row": 1, "cells": ["Clients", ""] },
                { "row": 2, "cells": ["Nom", "Ville"] }
            ],
            "estimatedItems": 1, "canModifyOriginal": false, "formattingNotKept": true
        });

        // WHEN it is rendered
        let lines = preview_lines(&value);

        // THEN the proposal, both flags and each row with its number are shown
        assert!(
            lines.contains(&"Sheet:           Clients (proposed: Clients)".to_owned()),
            "{lines:?}"
        );
        assert!(
            lines.contains(&"Header row:      2 (proposed: 2)".to_owned()),
            "{lines:?}"
        );
        assert!(
            lines.contains(&"Columns:         A=Nom, B=Ville".to_owned()),
            "{lines:?}"
        );
        assert!(
            lines.contains(&"Modify original: no, results go to a copy".to_owned()),
            "{lines:?}"
        );
        assert!(
            lines.contains(&"Formatting:      not kept in the copy of results".to_owned()),
            "{lines:?}"
        );
        assert!(lines.contains(&"   1 | Clients |".to_owned()), "{lines:?}");
        assert!(
            lines.contains(&"   2 | Nom | Ville".to_owned()),
            "{lines:?}"
        );
    }

    #[test]
    fn test_the_list_lines_name_each_source_and_nothing_for_none() {
        // GIVEN an empty answer and an answer with one folder
        let none = serde_json::json!([]);
        let one = serde_json::json!([{
            "id": "src-1", "kind": "folder", "label": "Contracts", "group": null,
            "reference": "conn", "indexState": "absent", "addedAt": "2026-10-03T00:00:00.000Z"
        }]);

        // WHEN they are rendered
        let none = list_lines(&none);
        let one = list_lines(&one);

        // THEN the first has no line, the second one line with id, kind, state, label, reference
        assert!(none.is_empty());
        assert_eq!(one.len(), 1);
        let cells: Vec<&str> = one[0].split_whitespace().collect();
        assert_eq!(
            cells,
            vec!["src-1", "folder", "absent", "Contracts", "conn"]
        );
    }
}
