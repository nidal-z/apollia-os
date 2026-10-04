//! The CSV reader, over `csv`.

use std::borrow::Cow;
use std::path::Path;

use crate::reader::{Cell, CellLocation, ReadError, Sheet};

/// How many records the delimiter detection reads.
const SAMPLE_RECORDS: usize = 20;

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// The field separator of a CSV file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Delimiter {
    /// `,`
    Comma,
    /// `;`, the usual separator where the comma is the decimal mark.
    Semicolon,
    /// A tabulation.
    Tab,
}

impl Delimiter {
    /// The candidates, in the order a tie is settled.
    const ALL: [Self; 3] = [Self::Comma, Self::Semicolon, Self::Tab];

    /// The separator byte.
    #[must_use]
    pub fn byte(self) -> u8 {
        match self {
            Self::Comma => b',',
            Self::Semicolon => b';',
            Self::Tab => b'\t',
        }
    }
}

/// A CSV file read as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvFile {
    /// The one sheet of the file, named after the file without its extension.
    pub sheet: Sheet,
    /// The separator detected.
    pub delimiter: Delimiter,
    /// True when the file was not valid UTF-8 and its invalid bytes were each
    /// replaced by `U+FFFD`. The text is then approximate: a file saved in
    /// another encoding (Windows-1252, for instance) loses its accented
    /// letters.
    pub invalid_bytes_replaced: bool,
}

/// Reads the CSV file at `path`: every record, every field as text.
///
/// The separator is detected among `,`, `;` and the tabulation, from the first
/// records. Fields may be quoted, and a quoted field may hold the separator,
/// a doubled quote or a line break. A leading UTF-8 byte order mark is dropped.
/// A row is a record, so a field spanning two lines does not shift the rows
/// after it. Records may have different numbers of fields.
///
/// # Errors
///
/// [`ReadError::Io`] when the file cannot be read, and [`ReadError::Corrupt`]
/// when a record cannot be parsed.
pub fn read_csv(path: &Path) -> Result<CsvFile, ReadError> {
    let bytes = std::fs::read(path).map_err(|source| ReadError::io(path, source))?;
    let bytes = bytes.strip_prefix(UTF8_BOM).unwrap_or(&bytes);
    let text = String::from_utf8_lossy(bytes);
    let invalid_bytes_replaced = matches!(text, Cow::Owned(_));
    let delimiter = detect_delimiter(&text);
    let name = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let rows = parse(&text, delimiter, &name).map_err(|error| ReadError::corrupt(path, error))?;
    Ok(CsvFile {
        sheet: Sheet { name, rows },
        delimiter,
        invalid_bytes_replaced,
    })
}

fn reader(text: &str, delimiter: Delimiter) -> csv::Reader<&[u8]> {
    csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter.byte())
        .from_reader(text.as_bytes())
}

fn parse(text: &str, delimiter: Delimiter, sheet: &str) -> Result<Vec<Vec<Cell>>, String> {
    let mut rows = Vec::new();
    for (record, row) in reader(text, delimiter).records().zip(1u32..) {
        let record = record.map_err(|error| error.to_string())?;
        let cells = record
            .iter()
            .zip(1u32..)
            .map(|(field, column)| Cell {
                location: CellLocation {
                    sheet: sheet.to_string(),
                    row,
                    column,
                },
                text: field.to_string(),
            })
            .collect();
        rows.push(cells);
    }
    Ok(rows)
}

/// The separator that splits the first records most regularly.
///
/// A candidate counts only if it splits the first record in two fields or
/// more. Among those, the one whose records most often have as many fields as
/// the first wins, then the one giving the most fields, then the first in
/// [`Delimiter::ALL`]. With no candidate, the comma.
fn detect_delimiter(text: &str) -> Delimiter {
    let mut best: Option<(usize, usize, Delimiter)> = None;
    for delimiter in Delimiter::ALL {
        let counts: Vec<usize> = reader(text, delimiter)
            .records()
            .take(SAMPLE_RECORDS)
            .map_while(Result::ok)
            .map(|record| record.len())
            .collect();
        let Some(&first) = counts.first() else {
            continue;
        };
        if first < 2 {
            continue;
        }
        let regular = counts.iter().filter(|&&count| count == first).count();
        let better = match best {
            None => true,
            Some((best_regular, best_fields, _)) => (regular, first) > (best_regular, best_fields),
        };
        if better {
            best = Some((regular, first, delimiter));
        }
    }
    best.map_or(Delimiter::Comma, |(_, _, delimiter)| delimiter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_delimiter_is_the_one_that_splits_regularly() {
        // GIVEN a comma file, a semicolon file with decimal commas, a tab file
        // and a file of one column
        let comma = "a,b,c\n1,2,3\n";
        let semicolon = "name;price\nbread;1,5\nmilk;0,95\n";
        let tab = "a\tb\n1\t2\n";
        let single = "a\nb\n";

        // WHEN the delimiter of each is detected
        let detected = [comma, semicolon, tab, single].map(detect_delimiter);

        // THEN each gets its own, and the column falls back to the comma
        assert_eq!(
            detected,
            [
                Delimiter::Comma,
                Delimiter::Semicolon,
                Delimiter::Tab,
                Delimiter::Comma
            ]
        );
    }

    #[test]
    fn test_a_comma_inside_names_does_not_outvote_the_semicolon() {
        // GIVEN a semicolon file where only some names hold a comma
        let text = "name;city\nSmith, John;Leeds\nJones;York\nBrown;Bath\n";

        // WHEN its delimiter is detected
        let delimiter = detect_delimiter(text);

        // THEN the semicolon, regular on every record, wins
        assert_eq!(delimiter, Delimiter::Semicolon);
    }
}
