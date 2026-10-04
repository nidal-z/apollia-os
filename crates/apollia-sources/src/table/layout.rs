//! The columns of a table and its data rows, once its header row is known.

use std::collections::BTreeSet;

use crate::reader::{Cell, Sheet};
use crate::table::detect::{is_empty_row, width};

/// One column of a table: its letter, as a spreadsheet shows it, and the name
/// its header cell gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    /// Column number, from 1.
    pub number: u32,
    /// Column letter: `A` for 1, `AA` for 27.
    pub letter: String,
    /// The header cell's text, trimmed. A blank header cell is named after
    /// its letter, and a name already taken gets ` (2)`, ` (3)`...
    pub name: String,
}

/// The letters a spreadsheet gives column `number` (from 1): `A`, `Z`, `AA`.
/// Column 0 has no letter and gives the empty string.
#[must_use]
pub fn column_letter(number: u32) -> String {
    let mut letters = Vec::new();
    let mut rest = number;
    while rest > 0 {
        let digit = (rest - 1) % 26;
        letters.push(char::from(b'A' + u8::try_from(digit).unwrap_or(0)));
        rest = (rest - 1) / 26;
    }
    letters.iter().rev().collect()
}

/// The number of the row at `index` in `sheet`, as the spreadsheet shows it.
pub(crate) fn row_number(sheet: &Sheet, index: usize) -> u32 {
    sheet
        .rows
        .get(index)
        .and_then(|row| row.first())
        .map(|cell| cell.location.row)
        .unwrap_or_else(|| u32::try_from(index).map_or(u32::MAX, |i| i.saturating_add(1)))
}

/// The index of the row numbered `number` in `sheet`.
pub(crate) fn row_index(sheet: &Sheet, number: u32) -> Option<usize> {
    (0..sheet.rows.len()).find(|&index| row_number(sheet, index) == number)
}

/// The text of the cell of `row` in column `number`, empty when absent.
pub(crate) fn cell_text(row: &[Cell], number: u32) -> &str {
    row.iter()
        .find(|cell| cell.location.column == number)
        .map_or("", |cell| cell.text.as_str())
}

/// The table of `sheet` under the header row at `header`: its columns and the
/// indices of its data rows, the non-empty rows below the header. Without a
/// header row (an empty sheet) there is no column and no data row.
pub(crate) fn layout(sheet: &Sheet, header: Option<usize>) -> (Vec<Column>, Vec<usize>) {
    let Some(header) = header else {
        return (Vec::new(), Vec::new());
    };
    let data: Vec<usize> = (header + 1..sheet.rows.len())
        .filter(|&index| !is_empty_row(&sheet.rows[index]))
        .collect();
    let header_row = sheet.rows.get(header).map_or(&[][..], Vec::as_slice);
    let last = data
        .iter()
        .map(|&index| width(&sheet.rows[index]))
        .chain(std::iter::once(width(header_row)))
        .max()
        .unwrap_or(0);
    let mut taken = BTreeSet::new();
    let columns = (1..=last)
        .map(|number| {
            let letter = column_letter(number);
            let text = cell_text(header_row, number).trim();
            let base = if text.is_empty() {
                letter.clone()
            } else {
                text.to_string()
            };
            let mut name = base.clone();
            let mut rank = 2;
            while !taken.insert(name.clone()) {
                name = format!("{base} ({rank})");
                rank += 1;
            }
            Column {
                number,
                letter,
                name,
            }
        })
        .collect();
    (columns, data)
}
