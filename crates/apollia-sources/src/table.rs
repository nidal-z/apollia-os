//! A workbook or a CSV file seen as a table: one sheet, one header row, one
//! item per non-empty row below it.
//!
//! [`read_table`] reads the file whatever its format. [`propose_sheet`] and
//! [`propose_header`] guess where the table is: the only sheet, or the one
//! holding the most non-empty rows; the first row among the first twenty whose
//! non-empty cells are all text and which is followed by a row of the same
//! width, or the first non-empty row. [`preview_table`] shows that guess with
//! the first rows, and follows a sheet or a header row the user picks instead.
//! [`TableSource`] is the batch source that yields the rows as items, and
//! [`register_table_sources`] adds its two kinds to a batch source registry.

mod detect;
mod layout;
mod preview;
mod source;

#[cfg(test)]
mod tests;

use std::path::Path;

use crate::reader::{read_csv, read_workbook, ReadError, Sheet};

pub use detect::{is_empty_row, propose_header, propose_sheet, HEADER_SEARCH_ROWS};
pub use layout::{column_letter, Column};
pub use preview::{
    preview_table, PreviewError, PreviewRow, SheetSummary, TablePreview, PREVIEW_ROWS,
};
pub use source::{
    register_table_sources, TableSource, CSV_KIND, OPTION_COLUMNS, OPTION_HEADER_ROW, OPTION_SHEET,
    SPREADSHEET_KIND,
};

/// The format of a table file, read off its extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TableFormat {
    /// `.xlsx`
    Xlsx,
    /// `.xlsm`, a workbook with macros.
    Xlsm,
    /// `.xlsb`, the binary workbook.
    Xlsb,
    /// `.xls`, the workbook of Excel 97 to 2003.
    Xls,
    /// `.ods`, the OpenDocument spreadsheet.
    Ods,
    /// `.csv`
    Csv,
}

impl TableFormat {
    /// The format of `path`, from its extension in any case; `None` for a
    /// file no table reader opens.
    #[must_use]
    pub fn of(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "xlsx" => Some(Self::Xlsx),
            "xlsm" => Some(Self::Xlsm),
            "xlsb" => Some(Self::Xlsb),
            "xls" => Some(Self::Xls),
            "ods" => Some(Self::Ods),
            "csv" => Some(Self::Csv),
            _ => None,
        }
    }

    /// The extension, lower case.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Xlsx => "xlsx",
            Self::Xlsm => "xlsm",
            Self::Xlsb => "xlsb",
            Self::Xls => "xls",
            Self::Ods => "ods",
            Self::Csv => "csv",
        }
    }

    /// True when results may be written into the original file itself: only
    /// an Office Open XML workbook keeps its formatting when edited in place.
    #[must_use]
    pub fn can_modify_original(self) -> bool {
        matches!(self, Self::Xlsx | Self::Xlsm)
    }

    /// True when the copy of results is a new workbook that does not carry
    /// the formatting of the original.
    #[must_use]
    pub fn formatting_not_kept(self) -> bool {
        matches!(self, Self::Xlsb | Self::Xls | Self::Ods)
    }
}

/// A table file read as text: its format and its sheets, in order. A CSV file
/// has one sheet, named after the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    /// The format of the file.
    pub format: TableFormat,
    /// The sheets, in the file's order.
    pub sheets: Vec<Sheet>,
}

/// Reads the workbook or CSV file at `path`, choosing the reader by extension.
///
/// # Errors
///
/// [`ReadError::UnsupportedFormat`] for an extension no reader opens, and the
/// errors of [`read_workbook`] and [`read_csv`].
pub fn read_table(path: &Path) -> Result<Table, ReadError> {
    let format = TableFormat::of(path).ok_or_else(|| ReadError::UnsupportedFormat {
        path: path.to_path_buf(),
        supported: "xlsx, xlsm, xlsb, xls, ods, csv",
    })?;
    let sheets = if format == TableFormat::Csv {
        vec![read_csv(path)?.sheet]
    } else {
        read_workbook(path)?.sheets
    };
    Ok(Table { format, sheets })
}
