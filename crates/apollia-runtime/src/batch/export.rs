//! The result of a batch as a file: Excel, CSV or JSON.
//!
//! Every format carries one row per item, in source order: the input columns
//! of the item as read from the source, then its output, its status and the
//! sources its answer cites. The input columns are the union of the fields of
//! every item, in the order they first appear; an item without one of them
//! leaves its cell empty. An input that is not an object of fields is a single
//! `input` column.
//!
//! When a field of the source is itself named `output`, `status` or
//! `sources`, the column the batch adds is named `batch.output`,
//! `batch.status` or `batch.sources`, so that no two columns share a header.
//!
//! CSV and JSON are written without a dependency. Excel is written by
//! `rust_xlsxwriter`, in one sheet; Excel refuses a cell longer than
//! [`XLSX_CELL_MAX`] characters, so a longer one is cut there.

use std::str::FromStr;

use apollia_tools::BatchItemRow;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::BatchDetail;

/// The longest text an Excel cell holds, in characters.
pub const XLSX_CELL_MAX: usize = 32_767;

/// The column an input that is not an object of fields is written to.
const BARE_INPUT: &str = "input";

/// The columns the batch adds after those of the source.
const ADDED: [&str; 3] = ["output", "status", "sources"];

/// The format a batch is exported in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    /// An Excel workbook of one sheet.
    Xlsx,
    /// Comma-separated values, UTF-8, one header row.
    Csv,
    /// One JSON document with the batch and its items.
    Json,
}

impl ExportFormat {
    /// The media type the file is served with.
    #[must_use]
    pub fn content_type(self) -> &'static str {
        match self {
            Self::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            Self::Csv => "text/csv; charset=utf-8",
            Self::Json => "application/json",
        }
    }

    /// The file extension, without its dot.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Xlsx => "xlsx",
            Self::Csv => "csv",
            Self::Json => "json",
        }
    }
}

impl FromStr for ExportFormat {
    type Err = ExportError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "xlsx" => Ok(Self::Xlsx),
            "csv" => Ok(Self::Csv),
            "json" => Ok(Self::Json),
            other => Err(ExportError::Format(other.to_owned())),
        }
    }
}

/// Why a batch could not be exported.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ExportError {
    /// The format is none of `xlsx`, `csv` or `json`.
    #[error("unknown export format '{0}': expected xlsx, csv or json")]
    Format(String),
    /// The workbook could not be written.
    #[error("the workbook could not be written: {0}")]
    Xlsx(#[from] rust_xlsxwriter::XlsxError),
    /// The JSON document could not be written.
    #[error("the JSON document could not be written: {0}")]
    Json(#[from] serde_json::Error),
}

/// A batch written in one format.
#[derive(Debug, Clone)]
pub struct BatchExport {
    /// What the bytes are.
    pub format: ExportFormat,
    /// The name the file is offered under: `batch-<id>.<extension>`.
    pub file_name: String,
    /// The file.
    pub bytes: Vec<u8>,
}

/// The table every format is written from: headers, then one row per item.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Table {
    /// The input columns, in the order they first appear.
    inputs: Vec<String>,
    /// The headers of the three added columns.
    added: [String; 3],
    rows: Vec<Vec<String>>,
}

impl Table {
    fn headers(&self) -> impl Iterator<Item = &str> {
        self.inputs
            .iter()
            .map(String::as_str)
            .chain(self.added.iter().map(String::as_str))
    }
}

/// The input columns of `items`, in the order they first appear.
fn input_columns(items: &[BatchItemRow]) -> Vec<String> {
    let mut columns: Vec<String> = Vec::new();
    for item in items {
        match item.input.as_object() {
            Some(fields) => {
                for key in fields.keys() {
                    if !columns.iter().any(|c| c == key) {
                        columns.push(key.clone());
                    }
                }
            }
            None => {
                if !columns.iter().any(|c| c == BARE_INPUT) {
                    columns.push(BARE_INPUT.to_owned());
                }
            }
        }
    }
    columns
}

/// A JSON value as the text of a cell: a string as itself, nothing as empty.
fn cell(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// The sources an answer cites, one per line: a string as itself, an object
/// by its title, location or path when it has one.
fn sources_text(sources: Option<&Value>) -> String {
    let named = |v: &Value| -> String {
        match v {
            Value::Object(o) => ["title", "url", "uri", "path", "location", "name"]
                .iter()
                .find_map(|k| o.get(*k).and_then(Value::as_str).map(str::to_owned))
                .unwrap_or_else(|| v.to_string()),
            other => cell(other),
        }
    };
    match sources {
        None | Some(Value::Null) => String::new(),
        Some(Value::Array(list)) => list.iter().map(named).collect::<Vec<_>>().join("\n"),
        Some(other) => named(other),
    }
}

fn input_cell(item: &BatchItemRow, column: &str) -> String {
    match item.input.as_object() {
        Some(fields) => fields.get(column).map(cell).unwrap_or_default(),
        None if column == BARE_INPUT => cell(&item.input),
        None => String::new(),
    }
}

fn table(items: &[BatchItemRow]) -> Table {
    let inputs = input_columns(items);
    let added = ADDED.map(|name| {
        if inputs.iter().any(|c| c == name) {
            format!("batch.{name}")
        } else {
            name.to_owned()
        }
    });
    let rows = items
        .iter()
        .map(|item| {
            let mut row: Vec<String> = inputs.iter().map(|c| input_cell(item, c)).collect();
            row.push(item.output.clone().unwrap_or_default());
            row.push(item.status.clone());
            row.push(sources_text(item.sources.as_ref()));
            row
        })
        .collect();
    Table {
        inputs,
        added,
        rows,
    }
}

/// One CSV field, quoted when it holds a comma, a quote or a line break.
fn csv_field(out: &mut String, field: &str) {
    if field.contains([',', '"', '\n', '\r']) {
        out.push('"');
        out.push_str(&field.replace('"', "\"\""));
        out.push('"');
    } else {
        out.push_str(field);
    }
}

fn csv_line<'a>(out: &mut String, fields: impl Iterator<Item = &'a str>) {
    for (i, field) in fields.enumerate() {
        if i > 0 {
            out.push(',');
        }
        csv_field(out, field);
    }
    out.push_str("\r\n");
}

/// The batch as CSV (RFC 4180): a header row, then one row per item.
fn to_csv(table: &Table) -> Vec<u8> {
    let mut out = String::new();
    csv_line(&mut out, table.headers());
    for row in &table.rows {
        csv_line(&mut out, row.iter().map(String::as_str));
    }
    out.into_bytes()
}

/// The batch as one JSON document: the batch, its input columns, and one
/// object per item with its input, output, status and sources.
fn to_json(detail: &BatchDetail, table: &Table) -> Result<Vec<u8>, ExportError> {
    let items: Vec<Value> = detail
        .items
        .iter()
        .map(|item| {
            let mut row = Map::new();
            row.insert("input".into(), item.input.clone());
            row.insert(
                "output".into(),
                item.output.clone().map_or(Value::Null, Value::String),
            );
            row.insert("status".into(), Value::String(item.status.clone()));
            row.insert(
                "sources".into(),
                item.sources.clone().unwrap_or(Value::Array(Vec::new())),
            );
            Value::Object(row)
        })
        .collect();
    let document = serde_json::json!({
        "batchId": detail.batch.batch_id,
        "agent": detail.batch.agent,
        "state": detail.batch.state,
        "columns": table.inputs,
        "items": items,
    });
    Ok(serde_json::to_vec_pretty(&document)?)
}

/// `text` cut to the length an Excel cell holds.
fn xlsx_text(text: &str) -> &str {
    match text.char_indices().nth(XLSX_CELL_MAX) {
        Some((end, _)) => &text[..end],
        None => text,
    }
}

/// The batch as an Excel workbook of one sheet, named `Batch`.
fn to_xlsx(table: &Table) -> Result<Vec<u8>, ExportError> {
    let mut workbook = rust_xlsxwriter::Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.set_name("Batch")?;
    let bold = rust_xlsxwriter::Format::new().set_bold();
    for (col, header) in table.headers().enumerate() {
        let col =
            u16::try_from(col).map_err(|_| rust_xlsxwriter::XlsxError::RowColumnLimitError)?;
        sheet.write_string_with_format(0, col, xlsx_text(header), &bold)?;
    }
    for (r, row) in table.rows.iter().enumerate() {
        let r =
            u32::try_from(r + 1).map_err(|_| rust_xlsxwriter::XlsxError::RowColumnLimitError)?;
        for (col, value) in row.iter().enumerate() {
            let col =
                u16::try_from(col).map_err(|_| rust_xlsxwriter::XlsxError::RowColumnLimitError)?;
            if !value.is_empty() {
                sheet.write_string(r, col, xlsx_text(value))?;
            }
        }
    }
    Ok(workbook.save_to_buffer()?)
}

/// `detail` written in `format`.
///
/// # Errors
///
/// [`ExportError::Xlsx`] when the workbook cannot be written (more rows or
/// columns than a sheet holds), [`ExportError::Json`] when the document
/// cannot be.
pub fn export(detail: &BatchDetail, format: ExportFormat) -> Result<BatchExport, ExportError> {
    let table = table(&detail.items);
    let bytes = match format {
        ExportFormat::Csv => to_csv(&table),
        ExportFormat::Json => to_json(detail, &table)?,
        ExportFormat::Xlsx => to_xlsx(&table)?,
    };
    Ok(BatchExport {
        format,
        file_name: format!("batch-{}.{}", detail.batch.batch_id, format.extension()),
        bytes,
    })
}
