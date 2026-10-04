//! The batch source of a table file: one item per non-empty row.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use apollia_core::batch_source::{
    BatchSource, OriginRef, SourceError, SourceItem, SourceItems, SourceRegistry, SourceSpec,
};

use crate::reader::ReadError;
use crate::table::detect::{propose_header, propose_sheet};
use crate::table::layout::{cell_text, layout, row_index, row_number, Column};
use crate::table::{read_table, Table, TableFormat};

/// The kind under which a workbook source is registered.
pub const SPREADSHEET_KIND: &str = "spreadsheet";
/// The kind under which a CSV source is registered.
pub const CSV_KIND: &str = "csv";

/// Option naming the sheet; the proposed sheet when absent.
pub const OPTION_SHEET: &str = "sheet";
/// Option giving the header row number, from 1; the proposed row when absent.
pub const OPTION_HEADER_ROW: &str = "header_row";
/// Option listing the checked columns by letter, comma separated (`A,C,F`);
/// every column when absent.
pub const OPTION_COLUMNS: &str = "columns";

/// A workbook or a CSV file read as the items of a batch.
///
/// The fields are the checked columns, named by their header cells. An item
/// is a non-empty row below the header row; its origin reads `Sheet!12`, the
/// sheet and the row number. The file is read once, when the source is opened.
#[derive(Debug, Clone)]
pub struct TableSource {
    fields: Vec<String>,
    items: Vec<SourceItem>,
}

impl TableSource {
    /// Opens the table file at `path` with the options of `spec`
    /// ([`OPTION_SHEET`], [`OPTION_HEADER_ROW`], [`OPTION_COLUMNS`]).
    ///
    /// # Errors
    ///
    /// [`SourceError::Io`] when the file cannot be read, or when an option
    /// names a sheet, a row or a column the file does not hold; the reason is
    /// the error's source.
    pub fn open(path: &Path, options: &BTreeMap<String, String>) -> Result<Self, SourceError> {
        let table = read_table(path).map_err(|error| read_failure(path, error))?;
        Self::from_table(path, &table, options)
    }

    pub(crate) fn from_table(
        path: &Path,
        table: &Table,
        options: &BTreeMap<String, String>,
    ) -> Result<Self, SourceError> {
        let sheet_index = match options.get(OPTION_SHEET) {
            Some(name) => table.sheets.iter().position(|sheet| &sheet.name == name),
            None => propose_sheet(&table.sheets),
        }
        .ok_or_else(|| {
            invalid(
                path,
                format!(
                    "the file has no sheet named '{}'",
                    options.get(OPTION_SHEET).map_or("", String::as_str)
                ),
            )
        })?;
        let sheet = table
            .sheets
            .get(sheet_index)
            .ok_or_else(|| invalid(path, "the file has no sheet".to_string()))?;
        let header = match options.get(OPTION_HEADER_ROW) {
            Some(raw) => {
                let number: u32 = raw.trim().parse().map_err(|_| {
                    invalid(path, format!("the header row '{raw}' is not a row number"))
                })?;
                Some(row_index(sheet, number).ok_or_else(|| {
                    invalid(
                        path,
                        format!("the sheet '{}' has no row {number}", sheet.name),
                    )
                })?)
            }
            None => propose_header(sheet),
        };
        let (columns, data) = layout(sheet, header);
        let checked = checked_columns(path, &columns, options.get(OPTION_COLUMNS))?;
        let fields = checked.iter().map(|column| column.name.clone()).collect();
        let items = data
            .into_iter()
            .map(|index| {
                let row = sheet.rows.get(index).map_or(&[][..], Vec::as_slice);
                SourceItem {
                    origin: OriginRef::new(format!("{}!{}", sheet.name, row_number(sheet, index))),
                    fields: checked
                        .iter()
                        .map(|column| {
                            (
                                column.name.clone(),
                                cell_text(row, column.number).to_string(),
                            )
                        })
                        .collect(),
                }
            })
            .collect();
        Ok(Self { fields, items })
    }
}

impl BatchSource for TableSource {
    fn fields(&self) -> &[String] {
        &self.fields
    }

    fn items(&self) -> Result<SourceItems<'_>, SourceError> {
        Ok(SourceItems {
            estimated_count: u64::try_from(self.items.len()).unwrap_or(u64::MAX),
            items: Box::new(self.items.iter().cloned().map(Ok)),
        })
    }
}

/// Registers the two table kinds: [`SPREADSHEET_KIND`] opens a workbook
/// (`.xlsx`, `.xlsm`, `.xlsb`, `.xls`, `.ods`), [`CSV_KIND`] a CSV file. The
/// location of the spec is the path of the file.
///
/// # Errors
///
/// [`SourceError::DuplicateKind`] when either kind is already registered.
pub fn register_table_sources(registry: &mut SourceRegistry) -> Result<(), SourceError> {
    registry.register(
        SPREADSHEET_KIND,
        Box::new(|spec: &SourceSpec| open_kind(spec, false)),
    )?;
    registry.register(
        CSV_KIND,
        Box::new(|spec: &SourceSpec| open_kind(spec, true)),
    )
}

fn open_kind(spec: &SourceSpec, csv: bool) -> Result<Box<dyn BatchSource>, SourceError> {
    let path = PathBuf::from(&spec.location);
    let is_csv = TableFormat::of(&path) == Some(TableFormat::Csv);
    if is_csv != csv || TableFormat::of(&path).is_none() {
        return Err(invalid(
            &path,
            if csv {
                "a CSV source opens a .csv file".to_string()
            } else {
                "a spreadsheet source opens a .xlsx, .xlsm, .xlsb, .xls or .ods file".to_string()
            },
        ));
    }
    Ok(Box::new(TableSource::open(&path, &spec.options)?))
}

fn checked_columns(
    path: &Path,
    columns: &[Column],
    option: Option<&String>,
) -> Result<Vec<Column>, SourceError> {
    let Some(option) = option else {
        return Ok(columns.to_vec());
    };
    option
        .split(',')
        .map(str::trim)
        .filter(|letter| !letter.is_empty())
        .map(|letter| {
            columns
                .iter()
                .find(|column| column.letter.eq_ignore_ascii_case(letter))
                .cloned()
                .ok_or_else(|| invalid(path, format!("the table has no column {letter}")))
        })
        .collect()
}

fn read_failure(path: &Path, error: ReadError) -> SourceError {
    let kind = match &error {
        ReadError::Io { source, .. } => source.kind(),
        _ => io::ErrorKind::InvalidData,
    };
    tracing::event!(
        tracing::Level::WARN,
        path = %path.display(),
        error = %error,
        "sources.table.read_failed"
    );
    SourceError::Io {
        path: path.to_path_buf(),
        source: io::Error::new(kind, error),
    }
}

fn invalid(path: &Path, reason: String) -> SourceError {
    SourceError::Io {
        path: path.to_path_buf(),
        source: io::Error::new(io::ErrorKind::InvalidInput, reason),
    }
}
