//! Where the table is: the proposed sheet and the proposed header row.

use crate::reader::{Cell, Sheet};

/// How many rows, from the top of a sheet, the header is searched among.
pub const HEADER_SEARCH_ROWS: usize = 20;

/// The error codes a spreadsheet writes in a cell.
const ERROR_CODES: &[&str] = &[
    "#NULL!",
    "#DIV/0!",
    "#VALUE!",
    "#REF!",
    "#NAME?",
    "#NUM!",
    "#N/A",
    "#GETTING_DATA",
];

/// True when every cell of `row` is blank. Such a row is never an item.
#[must_use]
pub fn is_empty_row(row: &[Cell]) -> bool {
    row.iter().all(|cell| cell.text.trim().is_empty())
}

/// How many rows of `sheet` hold at least one non-blank cell.
pub(crate) fn non_empty_rows(sheet: &Sheet) -> usize {
    sheet.rows.iter().filter(|row| !is_empty_row(row)).count()
}

/// The sheet a table is proposed on, as an index into `sheets`: the only one,
/// otherwise the one holding the most non-empty rows, the first on a tie.
/// `None` when there is no sheet.
#[must_use]
pub fn propose_sheet(sheets: &[Sheet]) -> Option<usize> {
    if sheets.len() == 1 {
        return Some(0);
    }
    let mut best: Option<(usize, usize)> = None;
    for (index, sheet) in sheets.iter().enumerate() {
        let count = non_empty_rows(sheet);
        if best.is_none_or(|(_, best_count)| count > best_count) {
            best = Some((index, count));
        }
    }
    best.map(|(index, _)| index)
}

/// The header row proposed on `sheet`, as an index into its rows.
///
/// The first row, among the first [`HEADER_SEARCH_ROWS`], whose non-blank
/// cells are all text and which is followed by a row of the same width (the
/// next non-empty row ends on the same column); otherwise the first non-empty
/// row. `None` when the sheet holds no non-empty row.
#[must_use]
pub fn propose_header(sheet: &Sheet) -> Option<usize> {
    let searched = sheet.rows.len().min(HEADER_SEARCH_ROWS);
    for index in 0..searched {
        let row = &sheet.rows[index];
        if is_empty_row(row) || !row_is_text(row) {
            continue;
        }
        let next = sheet.rows[index + 1..]
            .iter()
            .find(|candidate| !is_empty_row(candidate));
        if next.is_some_and(|next| width(next) == width(row)) {
            return Some(index);
        }
    }
    sheet.rows.iter().position(|row| !is_empty_row(row))
}

/// The column of the last non-blank cell of `row`, 0 for an empty row.
pub(crate) fn width(row: &[Cell]) -> u32 {
    row.iter()
        .filter(|cell| !cell.text.trim().is_empty())
        .map(|cell| cell.location.column)
        .max()
        .unwrap_or(0)
}

fn row_is_text(row: &[Cell]) -> bool {
    row.iter()
        .map(|cell| cell.text.trim())
        .filter(|text| !text.is_empty())
        .all(is_text)
}

/// True when `text` reads as words rather than as a number, a date, a time, a
/// boolean or an error code. The readers hand every cell over as text, a
/// workbook's dates in ISO 8601, so the test is on the text itself, the same
/// for a workbook and a CSV file.
pub(crate) fn is_text(text: &str) -> bool {
    let text = text.trim();
    if text.eq_ignore_ascii_case("true") || text.eq_ignore_ascii_case("false") {
        return false;
    }
    if ERROR_CODES.contains(&text) {
        return false;
    }
    !(is_number(text) || is_date_or_time(text))
}

fn is_number(text: &str) -> bool {
    let trimmed = text
        .trim_end_matches(['%', '€', '$', '£'])
        .trim_start_matches(['€', '$', '£'])
        .trim();
    let compact: String = trimmed
        .chars()
        .filter(|c| !matches!(c, ' ' | '\u{a0}' | '\u{202f}' | '\''))
        .collect();
    if compact.is_empty() {
        return false;
    }
    if compact.parse::<f64>().is_ok_and(f64::is_finite) {
        return true;
    }
    let decimal_comma = compact.replacen(',', ".", 1);
    decimal_comma.parse::<f64>().is_ok_and(f64::is_finite)
}

/// A date or a time written with digits and separators only: `2024-03-12`,
/// `12/03/2024`, `10:30`, `2024-03-12T10:30:00`.
fn is_date_or_time(text: &str) -> bool {
    let has_digit = text.chars().any(|c| c.is_ascii_digit());
    let has_separator = text.chars().any(|c| matches!(c, '-' | '/' | ':' | '.'));
    has_digit
        && has_separator
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '-' | '/' | ':' | '.' | 'T' | ' ' | 'Z'))
}
