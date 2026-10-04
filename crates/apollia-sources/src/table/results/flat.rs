//! Results written where the original's formatting cannot be kept: a CSV
//! file copied with its columns added, and a new `.xlsx` workbook holding the
//! values of a `.xls`, `.xlsb` or `.ods` workbook with the columns added.

use std::collections::BTreeSet;

use rust_xlsxwriter::Workbook;

use crate::reader::{CsvFile, Delimiter, Sheet};
use crate::table::results::{cell_value, Grid};

/// The UTF-8 byte order mark a spreadsheet looks for at the start of a CSV.
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// A CSV file read once, ready to be written again with added columns.
#[derive(Debug, Clone)]
pub(super) struct CsvBase {
    rows: Vec<Vec<String>>,
    delimiter: Delimiter,
    bom: bool,
    crlf: bool,
}

impl CsvBase {
    /// The CSV file `csv`, whose raw bytes are `bytes`.
    pub(super) fn new(csv: CsvFile, bytes: &[u8]) -> Self {
        Self {
            rows: csv
                .sheet
                .rows
                .into_iter()
                .map(|row| row.into_iter().map(|cell| cell.text).collect())
                .collect(),
            delimiter: csv.delimiter,
            bom: bytes.starts_with(UTF8_BOM),
            crlf: bytes.windows(2).any(|pair| pair == b"\r\n"),
        }
    }

    /// The width of the widest record.
    pub(super) fn last_column(&self) -> u32 {
        self.rows
            .iter()
            .map(|row| u32::try_from(row.len()).unwrap_or(u32::MAX))
            .max()
            .unwrap_or(0)
    }

    /// The file with the cells of `grid` added, with the same separator, and
    /// the byte order mark and the line ends of the original.
    pub(super) fn write(&self, grid: &Grid) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        if self.bom {
            out.extend_from_slice(UTF8_BOM);
        }
        let mut writer = csv::WriterBuilder::new()
            .delimiter(self.delimiter.byte())
            .flexible(true)
            .terminator(if self.crlf {
                csv::Terminator::CRLF
            } else {
                csv::Terminator::Any(b'\n')
            })
            .from_writer(out);
        for (row, number) in self.rows.iter().zip(1u32..) {
            let mut fields = row.clone();
            if let Some(cells) = grid.get(&number) {
                for (column, text) in cells {
                    let index = usize::try_from(*column)
                        .map_err(|error| error.to_string())?
                        .saturating_sub(1);
                    if fields.len() <= index {
                        fields.resize(index + 1, String::new());
                    }
                    fields[index] = cell_value(text);
                }
            }
            writer
                .write_record(&fields)
                .map_err(|error| error.to_string())?;
        }
        writer
            .into_inner()
            .map_err(|error| error.error().to_string())
    }
}

/// A workbook whose formatting cannot be kept, read once as values.
#[derive(Debug, Clone)]
pub(super) struct ValuesBase {
    sheets: Vec<Sheet>,
    target: usize,
}

impl ValuesBase {
    /// The sheets of the workbook, the results going to the one at `target`.
    pub(super) fn new(sheets: Vec<Sheet>, target: usize) -> Self {
        Self { sheets, target }
    }

    /// The last column the target sheet uses.
    pub(super) fn last_column(&self) -> u32 {
        self.sheets
            .get(self.target)
            .into_iter()
            .flat_map(|sheet| sheet.rows.iter().flatten())
            .filter(|cell| !cell.text.is_empty())
            .map(|cell| cell.location.column)
            .max()
            .unwrap_or(0)
    }

    /// A new `.xlsx` workbook with every sheet's values, in order, and the
    /// cells of `grid` added to the target sheet.
    pub(super) fn write(&self, grid: &Grid) -> Result<Vec<u8>, String> {
        let mut workbook = Workbook::new();
        let mut taken = BTreeSet::new();
        for (index, sheet) in self.sheets.iter().enumerate() {
            let worksheet = workbook.add_worksheet();
            let name = sheet_name(&sheet.name, index, &mut taken);
            worksheet
                .set_name(name.as_str())
                .map_err(|error| error.to_string())?;
            for cell in sheet.rows.iter().flatten() {
                if cell.text.is_empty() {
                    continue;
                }
                let (row, column) = position(cell.location.row, cell.location.column)?;
                match number(&cell.text) {
                    Some(value) => worksheet.write_number(row, column, value),
                    None => worksheet.write_string(row, column, cell_value(&cell.text)),
                }
                .map_err(|error| error.to_string())?;
            }
            if index == self.target {
                for (row, cells) in grid {
                    for (column, text) in cells {
                        let (row, column) = position(*row, *column)?;
                        worksheet
                            .write_string(row, column, cell_value(text))
                            .map_err(|error| error.to_string())?;
                    }
                }
            }
        }
        workbook.save_to_buffer().map_err(|error| error.to_string())
    }
}

/// The zero-based position of the cell at `row` and `column`, counted from 1.
fn position(row: u32, column: u32) -> Result<(u32, u16), String> {
    let column = u16::try_from(column.saturating_sub(1))
        .map_err(|_| format!("column {column} is beyond what a workbook holds"))?;
    Ok((row.saturating_sub(1), column))
}

/// The number a cell's text spells exactly, so that it is written back as a
/// number: `12` and `830.5`, not `007` nor `1e3`.
fn number(text: &str) -> Option<f64> {
    let value: f64 = text.parse().ok()?;
    (value.is_finite() && value.to_string() == text).then_some(value)
}

/// A sheet name a `.xlsx` workbook accepts and not taken yet: the original
/// one when it is, otherwise `Sheet` and its rank.
fn sheet_name(name: &str, index: usize, taken: &mut BTreeSet<String>) -> String {
    let valid = !name.is_empty()
        && name.chars().count() <= 31
        && !name.contains(['[', ']', ':', '*', '?', '/', '\\'])
        && !name.starts_with('\'')
        && !name.ends_with('\'');
    let mut candidate = if valid {
        name.to_string()
    } else {
        format!("Sheet{}", index + 1)
    };
    let mut rank = 2;
    while !taken.insert(candidate.to_lowercase()) {
        candidate = format!("Sheet{}-{rank}", index + 1);
        rank += 1;
    }
    candidate
}
