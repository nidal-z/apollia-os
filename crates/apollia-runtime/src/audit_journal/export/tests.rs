use super::*;
use crate::audit_journal::JournalEntryKind;
use serde_json::json;

fn entry(seq: u64, kind: JournalEntryKind, payload: Value) -> JournalEntry {
    JournalEntry {
        seq,
        run_id: "run-1".into(),
        ts: format!("2026-10-04T09:00:0{seq}Z"),
        kind,
        payload,
        prev_hash: String::new(),
        hash: format!("h{seq}"),
        signature: None,
        signing_key_id: None,
    }
}

/// A run where the model asked for a write, which ran and was captured. The
/// argument and the output hold content; the target holds a comma, a quote
/// and a line break.
fn run() -> Vec<JournalEntry> {
    let target = "/home/u/a, \"b\"\nc.md";
    vec![
        entry(
            0,
            JournalEntryKind::LlmCompletion,
            json!({
                "content": "", "model_id": "m",
                "tool_calls": [{ "id": "c0", "name": "file_write",
                    "arguments": { "path": target, "content": "SECRET-ARGUMENT" } }],
            }),
        ),
        entry(
            1,
            JournalEntryKind::ToolOutput,
            json!({
                "tool_call_id": "c0", "tool_name": "file_write",
                "output": "SECRET-OUTPUT", "status": "success",
            }),
        ),
        entry(
            2,
            JournalEntryKind::ToolCallCompleted,
            json!({
                "tool_name": "file_write", "agent_id": "quotes", "tool_call_id": "c0",
                "family": "write_or_delete", "target": target, "success": true,
            }),
        ),
    ]
}

/// The entries of [`run`], newest first, as an export holds them.
fn exported(detail: bool) -> Vec<ExportedEntry> {
    let run = run();
    run.iter()
        .rev()
        .map(|entry| {
            let line = JournalLine::of(entry.clone());
            if detail {
                ExportedEntry::detailed(&line, &run)
            } else {
                ExportedEntry::summary_of(&line)
            }
        })
        .collect()
}

fn text(entries: &[ExportedEntry], format: JournalExportFormat, detail: bool) -> String {
    String::from_utf8(write_export(entries, format, detail).unwrap()).unwrap()
}

/// Reads CSV records back (RFC 4180): quoted fields, doubled quotes, and line
/// breaks inside a quoted field.
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            (true, '"') => quoted = false,
            (true, other) => field.push(other),
            (false, '"') => quoted = true,
            (false, ',') => record.push(std::mem::take(&mut field)),
            (false, '\r') => {}
            (false, '\n') => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            (false, other) => field.push(other),
        }
    }
    records
}

#[test]
fn the_csv_summary_is_one_record_per_entry_and_holds_no_content() {
    // GIVEN the summaries of a run whose call carried content
    let entries = exported(false);

    // WHEN they are written as CSV
    let csv = text(&entries, JournalExportFormat::Csv, false);

    // THEN there is a header row and one record per entry, newest first
    let records = parse_csv(&csv);
    assert_eq!(records.len(), 4);
    assert_eq!(records[0], SUMMARY_COLUMNS);
    assert_eq!(
        records[1][..8],
        [
            "2026-10-04T09:00:02Z",
            "quotes",
            "action",
            "write_or_delete",
            "Tool call finished",
            "file_write",
            "/home/u/a, \"b\"\nc.md",
            "success",
        ]
    );
    assert_eq!(records[1][8..], ["run-1", "2", "tool_call_completed", "h2"]);
    // AND neither the argument nor the output of the call is in the file
    assert!(!csv.contains("SECRET"), "{csv}");
}

#[test]
fn a_csv_field_with_a_comma_a_quote_or_a_line_break_is_quoted() {
    // GIVEN an entry whose target holds a comma, a quote and a line break
    let entries = exported(false);

    // WHEN it is written as CSV
    let csv = text(&entries, JournalExportFormat::Csv, false);

    // THEN the field is quoted, its quotes doubled, its line break kept
    assert!(csv.contains("\"/home/u/a, \"\"b\"\"\nc.md\""), "{csv}");
    // AND every record ends on a carriage return and a line feed
    assert!(csv.ends_with("\r\n"));
    // AND a reader gets the value back, in a record of twelve fields
    let records = parse_csv(&csv);
    assert!(records.iter().all(|record| record.len() == 12));
    assert_eq!(records[1][6], "/home/u/a, \"b\"\nc.md");
}

#[test]
fn the_csv_detail_adds_the_payload_the_arguments_and_the_output() {
    // GIVEN the entries of a run with their detail
    let entries = exported(true);

    // WHEN they are written as CSV
    let csv = text(&entries, JournalExportFormat::Csv, true);

    // THEN four columns follow the twelve of the summary
    let records = parse_csv(&csv);
    assert_eq!(records.len(), 4);
    assert!(records.iter().all(|record| record.len() == 16));
    assert_eq!(records[0][12..], DETAIL_COLUMNS);
    // AND the end of the call holds its payload, its arguments and its output
    let payload: Value = serde_json::from_str(&records[1][12]).unwrap();
    assert_eq!(payload["tool_call_id"], "c0");
    let arguments: Value = serde_json::from_str(&records[1][13]).unwrap();
    assert_eq!(arguments["content"], "SECRET-ARGUMENT");
    assert_eq!(records[1][14], "\"SECRET-OUTPUT\"");
    assert_eq!(records[1][15], "success");
    // AND the model response, which is about no call, has its payload alone
    assert!(records[3][12].contains("SECRET-ARGUMENT"));
    assert_eq!(records[3][13..], ["", "", ""]);
}

#[test]
fn the_json_summary_holds_the_summary_of_each_entry_and_no_content() {
    // GIVEN the summaries of a run whose call carried content
    let entries = exported(false);

    // WHEN they are written as JSON
    let json = text(&entries, JournalExportFormat::Json, false);

    // THEN the document counts its entries and says it holds no detail
    let document: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(document["detail"], false);
    assert_eq!(document["count"], 3);
    // AND each entry is its summary and its place in the chain, newest first
    let first = &document["entries"][0];
    assert_eq!(first["run_id"], "run-1");
    assert_eq!(first["seq"], 2);
    assert_eq!(first["kind"], "tool_call_completed");
    assert_eq!(first["hash"], "h2");
    assert_eq!(first["agent"], "quotes");
    assert_eq!(first["nature"], "action");
    assert_eq!(first["target"], "/home/u/a, \"b\"\nc.md");
    assert_eq!(first["outcome"], "success");
    assert!(first.get("detail").is_none());
    assert!(first.get("payload").is_none());
    // AND neither the argument nor the output of the call is in the file
    assert!(!json.contains("SECRET"), "{json}");
}

#[test]
fn the_json_detail_holds_the_payload_and_the_call_of_each_entry() {
    // GIVEN the entries of a run with their detail
    let entries = exported(true);

    // WHEN they are written as JSON
    let json = text(&entries, JournalExportFormat::Json, true);

    // THEN the document says it holds the detail
    let document: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(document["detail"], true);
    assert_eq!(document["count"], 3);
    // AND the end of the call holds its payload, its arguments and its output
    let detail = &document["entries"][0]["detail"];
    assert_eq!(detail["payload"]["tool_call_id"], "c0");
    assert_eq!(
        detail["tool_call"]["arguments"]["content"],
        "SECRET-ARGUMENT"
    );
    assert_eq!(detail["tool_call"]["output"], "SECRET-OUTPUT");
    assert_eq!(detail["tool_call"]["status"], "success");
    // AND an entry about no call holds its payload and no call
    let response = &document["entries"][2]["detail"];
    assert_eq!(response["payload"]["model_id"], "m");
    assert!(response["tool_call"].is_null());
}

#[test]
fn only_csv_and_json_are_formats() {
    // GIVEN the two formats and one the journal is not written in
    let names = ["csv", "json", "pdf"];

    // WHEN each name is read
    let parsed: Vec<_> = names
        .iter()
        .map(|name| name.parse::<JournalExportFormat>())
        .collect();

    // THEN the two formats are read, and the third is refused by name
    assert_eq!(parsed[0].as_ref().unwrap(), &JournalExportFormat::Csv);
    assert_eq!(parsed[1].as_ref().unwrap(), &JournalExportFormat::Json);
    assert_eq!(
        parsed[2].as_ref().unwrap_err().to_string(),
        "unknown export format 'pdf': expected csv or json"
    );
}
