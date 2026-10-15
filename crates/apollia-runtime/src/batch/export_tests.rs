//! The exports of a batch over a fixture batch: CSV and JSON read back as
//! text, the workbook read back from its zip archive.

use std::collections::BTreeMap;
use std::io::Read;

use apollia_tools::{BatchItemRow, BatchRow};
use serde_json::{json, Value};

use super::*;

fn item(rank: u64, input: Value, status: &str, output: Option<&str>) -> BatchItemRow {
    BatchItemRow {
        rank,
        origin: format!("row-{}", rank + 2),
        input,
        status: status.into(),
        output: output.map(str::to_owned),
        sources: None,
        task_id: None,
        updated_at: String::new(),
    }
}

/// A batch of three items: one answered with two cited sources, one failed
/// with a message that holds a comma, a quote and a line break, one still
/// pending whose source row has no `request` field.
fn fixture() -> BatchDetail {
    let mut answered = item(
        0,
        json!({"client": "Ada", "request": "Refund"}),
        "completed",
        Some("Refund granted."),
    );
    answered.sources = Some(json!([
        "contract.pdf",
        {"title": "Refund policy", "url": "file:///policy.md"}
    ]));
    BatchDetail {
        batch: BatchRow {
            batch_id: "b-1".into(),
            agent: "clerk".into(),
            source: json!({"kind": "memory", "location": "/data/requests.xlsx"}),
            grants: vec![],
            concurrency: 1,
            max_failures: 10,
            state: "running".into(),
            total: 3,
            completed: 1,
            failed: 1,
            canceled: 0,
            awaiting: 0,
            created_at: "2026-10-02 10:00:00".into(),
            updated_at: "2026-10-02 10:00:00".into(),
            ended_at: None,
        },
        items: vec![
            answered,
            item(
                1,
                json!({"client": "Bob", "request": "Invoice"}),
                "failed",
                Some("tool \"mail\" failed, twice\nthen gave up"),
            ),
            item(2, json!({"client": "Cleo"}), "pending", None),
        ],
    }
}

fn text(export: &BatchExport) -> String {
    String::from_utf8(export.bytes.clone()).unwrap()
}

// GIVEN a batch of three items
// WHEN it is exported as CSV
// THEN one header row names the input columns, then output, status and
//      sources, and each item is one record, quoted where it must be
#[test]
fn test_the_csv_export_is_one_record_per_item() {
    // GIVEN
    let detail = fixture();

    // WHEN
    let export = export(&detail, ExportFormat::Csv).unwrap();

    // THEN
    assert_eq!(export.file_name, "batch-b-1.csv");
    assert_eq!(
        text(&export),
        "client,request,output,status,sources\r\n\
         Ada,Refund,Refund granted.,completed,\"contract.pdf\nRefund policy\"\r\n\
         Bob,Invoice,\"tool \"\"mail\"\" failed, twice\nthen gave up\",failed,\r\n\
         Cleo,,,pending,\r\n"
    );
}

// GIVEN a batch of three items
// WHEN it is exported as JSON
// THEN the document names the batch and its input columns, and carries one
//      object per item with its input, output, status and cited sources
#[test]
fn test_the_json_export_is_one_object_per_item() {
    // GIVEN
    let detail = fixture();

    // WHEN
    let export = export(&detail, ExportFormat::Json).unwrap();

    // THEN
    assert_eq!(export.file_name, "batch-b-1.json");
    let doc: Value = serde_json::from_slice(&export.bytes).unwrap();
    assert_eq!(doc["batchId"], "b-1");
    assert_eq!(doc["agent"], "clerk");
    assert_eq!(doc["columns"], json!(["client", "request"]));
    assert_eq!(
        doc["items"],
        json!([
            {
                "input": {"client": "Ada", "request": "Refund"},
                "output": "Refund granted.",
                "status": "completed",
                "sources": ["contract.pdf", {"title": "Refund policy", "url": "file:///policy.md"}]
            },
            {
                "input": {"client": "Bob", "request": "Invoice"},
                "output": "tool \"mail\" failed, twice\nthen gave up",
                "status": "failed",
                "sources": []
            },
            {
                "input": {"client": "Cleo"},
                "output": null,
                "status": "pending",
                "sources": []
            }
        ])
    );
}

// GIVEN a batch whose source has a column named `status`
// WHEN it is exported as CSV
// THEN the column the batch adds is `batch.status`, and both are kept
#[test]
fn test_a_source_column_never_shares_a_header_with_an_added_one() {
    // GIVEN
    let mut detail = fixture();
    detail.items = vec![item(
        0,
        json!({"client": "Ada", "status": "VIP"}),
        "completed",
        Some("Done."),
    )];

    // WHEN
    let export = export(&detail, ExportFormat::Csv).unwrap();

    // THEN
    assert_eq!(
        text(&export),
        "client,status,output,batch.status,sources\r\nAda,VIP,Done.,completed,\r\n"
    );
}

/// The entries of a zip archive, by name.
fn unzip(bytes: &[u8]) -> BTreeMap<String, String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut entries = BTreeMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let mut body = String::new();
        entry.read_to_string(&mut body).unwrap();
        entries.insert(entry.name().to_owned(), body);
    }
    entries
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The text of every `<tag ...>text</tag>` in `xml`, in order.
fn texts_of(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        let after = &rest[start + open.len()..];
        if !(after.starts_with('>') || after.starts_with(' ')) {
            rest = after;
            continue;
        }
        let body_start = after.find('>').unwrap() + 1;
        let body_end = after.find(&close).unwrap();
        out.push(unescape(&after[body_start..body_end]));
        rest = &after[body_end + close.len()..];
    }
    out
}

/// The cells of a sheet, by reference (`A1`), resolved through the shared
/// strings.
fn cells(sheet: &str, shared: &[String]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for chunk in sheet.split("<c r=\"").skip(1) {
        let reference = &chunk[..chunk.find('"').unwrap()];
        let value = texts_of(chunk, "v").into_iter().next().unwrap_or_default();
        let resolved = if chunk[..chunk.find('>').unwrap()].contains("t=\"s\"") {
            shared[value.parse::<usize>().unwrap()].clone()
        } else {
            value
        };
        out.insert(reference.to_owned(), resolved);
    }
    out
}

// GIVEN a batch of three items
// WHEN it is exported as Excel and the archive is read back
// THEN it is a workbook of one sheet named `Batch`, whose cells hold the
//      same table as the CSV export, row by row
#[test]
fn test_the_xlsx_export_is_one_sheet_holding_the_same_table() {
    // GIVEN
    let detail = fixture();

    // WHEN
    let export = export(&detail, ExportFormat::Xlsx).unwrap();

    // THEN
    assert_eq!(export.file_name, "batch-b-1.xlsx");
    assert!(export.bytes.starts_with(b"PK\x03\x04"), "not a zip archive");
    let entries = unzip(&export.bytes);
    for part in [
        "[Content_Types].xml",
        "_rels/.rels",
        "xl/workbook.xml",
        "xl/_rels/workbook.xml.rels",
        "xl/worksheets/sheet1.xml",
        "xl/sharedStrings.xml",
        "xl/styles.xml",
    ] {
        assert!(
            entries.contains_key(part),
            "no {part} in {:?}",
            entries.keys()
        );
    }
    assert!(!entries.contains_key("xl/worksheets/sheet2.xml"));
    let workbook = &entries["xl/workbook.xml"];
    assert_eq!(workbook.matches("<sheet ").count(), 1, "{workbook}");
    assert!(workbook.contains("name=\"Batch\""), "{workbook}");
    let shared = texts_of(&entries["xl/sharedStrings.xml"], "t");
    let cells = cells(&entries["xl/worksheets/sheet1.xml"], &shared);
    let expected: BTreeMap<String, String> = [
        ("A1", "client"),
        ("B1", "request"),
        ("C1", "output"),
        ("D1", "status"),
        ("E1", "sources"),
        ("A2", "Ada"),
        ("B2", "Refund"),
        ("C2", "Refund granted."),
        ("D2", "completed"),
        ("E2", "contract.pdf\nRefund policy"),
        ("A3", "Bob"),
        ("B3", "Invoice"),
        ("C3", "tool \"mail\" failed, twice\nthen gave up"),
        ("D3", "failed"),
        ("A4", "Cleo"),
        ("D4", "pending"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect();
    assert_eq!(cells, expected);
}

// GIVEN an item whose output is longer than an Excel cell holds
// WHEN the batch is exported as Excel
// THEN the export succeeds and the cell is cut at the limit
#[test]
fn test_an_output_longer_than_a_cell_is_cut_at_the_limit() {
    // GIVEN
    let mut detail = fixture();
    let long = "\u{e9}".repeat(XLSX_CELL_MAX + 10);
    detail.items = vec![item(0, json!({"client": "Ada"}), "completed", Some(&long))];

    // WHEN
    let export = export(&detail, ExportFormat::Xlsx).unwrap();

    // THEN
    let entries = unzip(&export.bytes);
    let shared = texts_of(&entries["xl/sharedStrings.xml"], "t");
    let cells = cells(&entries["xl/worksheets/sheet1.xml"], &shared);
    assert_eq!(cells["B2"].chars().count(), XLSX_CELL_MAX);
}

// GIVEN the three format names and an unknown one
// WHEN they are parsed
// THEN the three are formats and the unknown one is refused by name
#[test]
fn test_only_xlsx_csv_and_json_are_formats() {
    // GIVEN
    let names = ["xlsx", "csv", "json", "pdf"];

    // WHEN
    let parsed: Vec<_> = names.iter().map(|n| n.parse::<ExportFormat>()).collect();

    // THEN
    assert_eq!(parsed[0].as_ref().unwrap(), &ExportFormat::Xlsx);
    assert_eq!(parsed[1].as_ref().unwrap(), &ExportFormat::Csv);
    assert_eq!(parsed[2].as_ref().unwrap(), &ExportFormat::Json);
    let refused = parsed[3].as_ref().unwrap_err().to_string();
    assert!(refused.contains("'pdf'"), "{refused}");
}
