//! The preview of a table file, shown before a batch is launched on it.

use std::path::Path;

use crate::reader::{ReadError, Sheet};
use crate::table::detect::{non_empty_rows, propose_header, propose_sheet, width};
use crate::table::layout::{cell_text, layout, row_index, row_number, Column};
use crate::table::{read_table, TableFormat};

/// How many rows, from the top of the shown sheet, a preview carries.
pub const PREVIEW_ROWS: usize = 20;

/// One sheet of the file, as the preview lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetSummary {
    /// Name of the sheet.
    pub name: String,
    /// How many of its rows hold at least one non-blank cell.
    pub non_empty_rows: usize,
}

/// One row of the preview: its number and the text of its cells, column 1
/// first, as wide as the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewRow {
    /// Row number, from 1.
    pub row: u32,
    /// The text of each cell, one per column of the table.
    pub cells: Vec<String>,
}

/// What a batch on a table file would read, before it is launched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TablePreview {
    /// The format of the file.
    pub format: TableFormat,
    /// Every sheet, in the file's order.
    pub sheets: Vec<SheetSummary>,
    /// The sheet Apollia proposes; `None` when the file holds no sheet.
    pub proposed_sheet: Option<String>,
    /// The sheet shown: the one asked for, or the proposed one.
    pub sheet: Option<String>,
    /// The header row Apollia proposes on the shown sheet, from 1; `None` when
    /// that sheet holds no non-empty row.
    pub proposed_header_row: Option<u32>,
    /// The header row used: the one asked for, or the proposed one.
    pub header_row: Option<u32>,
    /// The columns under that header row.
    pub columns: Vec<Column>,
    /// The first [`PREVIEW_ROWS`] rows of the shown sheet, from its top.
    pub rows: Vec<PreviewRow>,
    /// How many items a batch would read: the non-empty rows below the header.
    pub estimated_items: u64,
    /// True when results may be written into the original file itself.
    pub can_modify_original: bool,
    /// True when the copy of results does not carry the original formatting.
    pub formatting_not_kept: bool,
}

/// Why a preview could not be built.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PreviewError {
    /// The file could not be read.
    #[error(transparent)]
    Read(#[from] ReadError),
    /// The sheet asked for is not in the file.
    #[error("the file has no sheet named '{0}'")]
    UnknownSheet(String),
    /// The header row asked for is not a row of the sheet.
    #[error("the sheet '{sheet}' has no row {row}")]
    UnknownHeaderRow {
        /// The sheet.
        sheet: String,
        /// The row asked for.
        row: u32,
    },
}

/// The preview of the table file at `path`, on `sheet` and under `header_row`
/// when given, on the proposed sheet and header row otherwise. A header row
/// asked for on a sheet is a row number of that sheet, from 1.
///
/// # Errors
///
/// [`PreviewError::Read`] when the file cannot be read,
/// [`PreviewError::UnknownSheet`] for a sheet the file does not hold, and
/// [`PreviewError::UnknownHeaderRow`] for a row the sheet does not hold.
pub fn preview_table(
    path: &Path,
    sheet: Option<&str>,
    header_row: Option<u32>,
) -> Result<TablePreview, PreviewError> {
    let table = read_table(path)?;
    let proposed = propose_sheet(&table.sheets);
    let shown = match sheet {
        Some(name) => Some(
            table
                .sheets
                .iter()
                .position(|candidate| candidate.name == name)
                .ok_or_else(|| PreviewError::UnknownSheet(name.to_string()))?,
        ),
        None => proposed,
    };
    let summaries = table
        .sheets
        .iter()
        .map(|sheet| SheetSummary {
            name: sheet.name.clone(),
            non_empty_rows: non_empty_rows(sheet),
        })
        .collect();
    let proposed_sheet = proposed
        .and_then(|index| table.sheets.get(index))
        .map(|sheet| sheet.name.clone());
    let mut preview = TablePreview {
        format: table.format,
        sheets: summaries,
        proposed_sheet,
        sheet: None,
        proposed_header_row: None,
        header_row: None,
        columns: Vec::new(),
        rows: Vec::new(),
        estimated_items: 0,
        can_modify_original: table.format.can_modify_original(),
        formatting_not_kept: table.format.formatting_not_kept(),
    };
    if let Some(shown) = shown.and_then(|index| table.sheets.get(index)) {
        fill(&mut preview, shown, header_row)?;
    }
    Ok(preview)
}

fn fill(
    preview: &mut TablePreview,
    sheet: &Sheet,
    header_row: Option<u32>,
) -> Result<(), PreviewError> {
    let proposed = propose_header(sheet);
    let header = match header_row {
        Some(number) => {
            Some(
                row_index(sheet, number).ok_or_else(|| PreviewError::UnknownHeaderRow {
                    sheet: sheet.name.clone(),
                    row: number,
                })?,
            )
        }
        None => proposed,
    };
    let (columns, data) = layout(sheet, header);
    let shown_rows = &sheet.rows[..sheet.rows.len().min(PREVIEW_ROWS)];
    let last_column = shown_rows
        .iter()
        .map(|row| width(row))
        .chain(columns.last().map(|column| column.number))
        .max()
        .unwrap_or(0);
    preview.sheet = Some(sheet.name.clone());
    preview.proposed_header_row = proposed.map(|index| row_number(sheet, index));
    preview.header_row = header.map(|index| row_number(sheet, index));
    preview.rows = shown_rows
        .iter()
        .enumerate()
        .map(|(index, row)| PreviewRow {
            row: row_number(sheet, index),
            cells: (1..=last_column)
                .map(|column| cell_text(row, column).to_string())
                .collect(),
        })
        .collect();
    preview.estimated_items = u64::try_from(data.len()).unwrap_or(u64::MAX);
    preview.columns = columns;
    Ok(())
}
