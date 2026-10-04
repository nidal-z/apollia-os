//! The workbook reader, over `calamine`.

use std::fs::File;
use std::io::BufReader;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use calamine::{
    Data, ExcelDateTime, Ods, OdsError, Reader, SheetType, Sheets, Xls, XlsError, Xlsb, XlsbError,
    Xlsx, XlsxError,
};

use crate::reader::{Cell, CellLocation, ReadError, Sheet};

/// The extensions [`read_workbook`] opens, lower case.
pub const WORKBOOK_EXTENSIONS: &[&str] = &["xlsx", "xlsm", "xlsb", "xls", "ods"];

/// A workbook read as text: its worksheets, in the workbook's order.
///
/// Chart sheets, dialog sheets, macro sheets and VBA modules hold no cells and
/// are left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workbook {
    /// The worksheets, in order.
    pub sheets: Vec<Sheet>,
}

#[derive(Debug, Clone, Copy)]
enum Format {
    OfficeOpenXml,
    Binary,
    Legacy,
    OpenDocument,
}

impl Format {
    fn of(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "xlsx" | "xlsm" => Some(Self::OfficeOpenXml),
            "xlsb" => Some(Self::Binary),
            "xls" => Some(Self::Legacy),
            "ods" => Some(Self::OpenDocument),
            _ => None,
        }
    }
}

/// Reads the workbook at `path`: every worksheet, every cell as text.
///
/// The format follows the extension, in any case. A number is written without
/// a trailing `.0`, a date in ISO 8601 (`2021-01-01`, `2021-01-01T10:10:10`,
/// `10:10:10` for a time alone), a duration in ISO 8601 (`PT255H10M10S`), a
/// boolean as `TRUE` or `FALSE`, a formula by the value the file last computed
/// for it, and an error by its code (`#DIV/0!`, `#N/A`...).
///
/// # Errors
///
/// [`ReadError::UnsupportedFormat`] when the extension is none of
/// [`WORKBOOK_EXTENSIONS`], [`ReadError::Io`] when the file cannot be opened,
/// [`ReadError::PasswordProtected`] when it is encrypted, and
/// [`ReadError::Corrupt`] when its content cannot be read, including when the
/// underlying reader stops on a structure it does not handle.
pub fn read_workbook(path: &Path) -> Result<Workbook, ReadError> {
    let format = Format::of(path).ok_or_else(|| ReadError::UnsupportedFormat {
        path: path.to_path_buf(),
        supported: "xlsx, xlsm, xlsb, xls, ods",
    })?;
    let file = File::open(path).map_err(|source| ReadError::io(path, source))?;
    match catch_unwind(AssertUnwindSafe(|| parse(path, format, file))) {
        Ok(result) => result,
        Err(_) => {
            tracing::event!(
                tracing::Level::WARN,
                path = %path.display(),
                "sources.workbook.reader_panicked"
            );
            Err(ReadError::corrupt(
                path,
                "the reader stopped on a structure it does not handle",
            ))
        }
    }
}

fn parse(path: &Path, format: Format, file: File) -> Result<Workbook, ReadError> {
    let reader = BufReader::new(file);
    let opened = match format {
        Format::OfficeOpenXml => Xlsx::new(reader)
            .map(Sheets::Xlsx)
            .map_err(calamine::Error::Xlsx),
        Format::Binary => Xlsb::new(reader)
            .map(Sheets::Xlsb)
            .map_err(calamine::Error::Xlsb),
        Format::Legacy => Xls::new(reader)
            .map(Sheets::Xls)
            .map_err(calamine::Error::Xls),
        Format::OpenDocument => Ods::new(reader)
            .map(Sheets::Ods)
            .map_err(calamine::Error::Ods),
    };
    let mut sheets = opened.map_err(|error| read_error(path, &error))?;
    let names: Vec<String> = sheets
        .sheets_metadata()
        .iter()
        .filter(|sheet| sheet.typ == SheetType::WorkSheet)
        .map(|sheet| sheet.name.clone())
        .collect();
    let mut read = Vec::with_capacity(names.len());
    for name in names {
        let range = sheets
            .worksheet_range(&name)
            .map_err(|error| read_error(path, &error))?;
        let (first_row, first_column) = range.start().unwrap_or((0, 0));
        let rows = range
            .rows()
            .zip(first_row..)
            .map(|(cells, row)| {
                cells
                    .iter()
                    .zip(first_column..)
                    .map(|(data, column)| Cell {
                        location: CellLocation {
                            sheet: name.clone(),
                            row: row.saturating_add(1),
                            column: column.saturating_add(1),
                        },
                        text: cell_text(data),
                    })
                    .collect()
            })
            .collect();
        read.push(Sheet { name, rows });
    }
    Ok(Workbook { sheets: read })
}

fn read_error(path: &Path, error: &calamine::Error) -> ReadError {
    let protected = matches!(
        error,
        calamine::Error::Xlsx(XlsxError::Password)
            | calamine::Error::Xlsb(XlsbError::Password)
            | calamine::Error::Xls(XlsError::Password)
            | calamine::Error::Ods(OdsError::Password)
    );
    if protected {
        ReadError::PasswordProtected {
            path: path.to_path_buf(),
        }
    } else {
        ReadError::corrupt(path, error.to_string())
    }
}

/// The text of one cell.
pub(crate) fn cell_text(data: &Data) -> String {
    match data {
        Data::Empty => String::new(),
        Data::String(text) | Data::DurationIso(text) => text.clone(),
        Data::DateTimeIso(text) => text.strip_suffix("T00:00:00").unwrap_or(text).to_string(),
        Data::Int(value) => value.to_string(),
        Data::Float(value) => number_text(*value),
        Data::Bool(true) => "TRUE".to_string(),
        Data::Bool(false) => "FALSE".to_string(),
        Data::DateTime(value) if value.is_duration() => duration_text(value.as_f64()),
        Data::DateTime(value) => datetime_text(value),
        Data::Error(code) => code.to_string(),
    }
}

/// A number as a spreadsheet shows it unformatted: `3`, not `3.0`.
///
/// `f64`'s `Display` already writes the shortest text that reads back to the
/// same value and drops a zero fraction; only the negative zero is folded.
fn number_text(value: f64) -> String {
    if value == 0.0 {
        "0".to_string()
    } else {
        value.to_string()
    }
}

/// A date, a date and time, or a time alone, in ISO 8601.
///
/// A serial value under one day carries no date: it is a time of day.
fn datetime_text(value: &ExcelDateTime) -> String {
    let (year, month, day, hour, minute, second, milli) = value.to_ymd_hms_milli();
    let time = if milli == 0 {
        format!("{hour:02}:{minute:02}:{second:02}")
    } else {
        format!("{hour:02}:{minute:02}:{second:02}.{milli:03}")
    };
    let serial = value.as_f64();
    if (0.0..1.0).contains(&serial) {
        time
    } else if hour == 0 && minute == 0 && second == 0 && milli == 0 {
        format!("{year:04}-{month:02}-{day:02}")
    } else {
        format!("{year:04}-{month:02}-{day:02}T{time}")
    }
}

/// A duration given in days, in ISO 8601: `PT255H10M10S`.
fn duration_text(days: f64) -> String {
    let millis = (days * 86_400_000.0).round();
    let sign = if millis < 0.0 { "-" } else { "" };
    let millis = millis.abs();
    let hours = (millis / 3_600_000.0).floor();
    let minutes = ((millis - hours * 3_600_000.0) / 60_000.0).floor();
    let rest = millis - hours * 3_600_000.0 - minutes * 60_000.0;
    let seconds = (rest / 1000.0).floor();
    let fraction = rest - seconds * 1000.0;
    if fraction == 0.0 {
        format!("{sign}PT{hours}H{minutes}M{seconds}S")
    } else {
        format!("{sign}PT{hours}H{minutes}M{seconds}.{fraction:03}S")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calamine::{CellErrorType, ExcelDateTimeType};

    #[test]
    fn test_a_whole_number_is_written_without_a_fraction() {
        // GIVEN whole and fractional floats, a negative zero and an integer
        let cells = [
            Data::Float(3.0),
            Data::Float(1.5),
            Data::Float(-0.0),
            Data::Float(1234.56),
            Data::Int(-42),
        ];

        // WHEN each is turned into text
        let texts: Vec<String> = cells.iter().map(cell_text).collect();

        // THEN no text carries a superfluous `.0`
        assert_eq!(texts, ["3", "1.5", "0", "1234.56", "-42"]);
    }

    #[test]
    fn test_dates_times_and_durations_are_iso_8601() {
        // GIVEN a date, a date with a time, a time alone and a duration
        let cells = [
            Data::DateTime(ExcelDateTime::new(
                44197.0,
                ExcelDateTimeType::DateTime,
                false,
            )),
            Data::DateTime(ExcelDateTime::new(
                44_197.423_726_851_85,
                ExcelDateTimeType::DateTime,
                false,
            )),
            Data::DateTime(ExcelDateTime::new(0.5, ExcelDateTimeType::DateTime, false)),
            Data::DateTime(ExcelDateTime::new(
                10.632_060_185_185_2,
                ExcelDateTimeType::TimeDelta,
                false,
            )),
        ];

        // WHEN each is turned into text
        let texts: Vec<String> = cells.iter().map(cell_text).collect();

        // THEN each is written in ISO 8601
        assert_eq!(
            texts,
            [
                "2021-01-01",
                "2021-01-01T10:10:10",
                "12:00:00",
                "PT255H10M10S"
            ]
        );
    }

    #[test]
    fn test_an_error_a_boolean_and_an_empty_cell_have_their_text() {
        // GIVEN an error, the two booleans and an empty cell
        let cells = [
            Data::Error(CellErrorType::Div0),
            Data::Bool(true),
            Data::Bool(false),
            Data::Empty,
        ];

        // WHEN each is turned into text
        let texts: Vec<String> = cells.iter().map(cell_text).collect();

        // THEN the error reads as its code and the empty cell as nothing
        assert_eq!(texts, ["#DIV/0!", "TRUE", "FALSE", ""]);
    }

    #[test]
    fn test_the_format_follows_the_extension_in_any_case() {
        // GIVEN paths with each supported extension and one unsupported
        let paths = ["a.XLSX", "a.xlsm", "a.xlsb", "a.Xls", "a.ods", "a.csv", "a"];

        // WHEN the format is read off each
        let formats: Vec<Option<&str>> = paths
            .iter()
            .map(|path| {
                Format::of(Path::new(path)).map(|format| match format {
                    Format::OfficeOpenXml => "ooxml",
                    Format::Binary => "binary",
                    Format::Legacy => "legacy",
                    Format::OpenDocument => "odf",
                })
            })
            .collect();

        // THEN the five workbook extensions are recognised, the rest refused
        assert_eq!(
            formats,
            [
                Some("ooxml"),
                Some("ooxml"),
                Some("binary"),
                Some("legacy"),
                Some("odf"),
                None,
                None
            ]
        );
    }
}
