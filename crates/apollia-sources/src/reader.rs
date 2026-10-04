//! Readers that turn a workbook, a CSV file, a PDF document, a Word document
//! or a PowerPoint presentation into text.
//!
//! A workbook (`.xlsx`, `.xlsm`, `.xlsb`, `.xls`, `.ods`) is read by
//! [`read_workbook`], a CSV file by [`read_csv`]. Both answer sheets of rows of
//! [`Cell`]s, each cell carrying its text and where it was read
//! ([`CellLocation`]). A PDF document is read by [`read_pdf`], which answers
//! the text of each page with its number ([`PdfPage`]). A Word document is read
//! by [`read_word`], which answers its paragraphs and table rows in order
//! ([`WordSegment`]), and a PowerPoint presentation by [`read_presentation`],
//! which answers the text and the notes of each slide ([`Slide`]). The readers
//! only read:
//! nothing is written, nothing leaves the machine. They are synchronous and read the whole file, so an async
//! caller runs them under `spawn_blocking`.

use std::path::{Path, PathBuf};

mod delimited;
mod office;
mod pdf;
mod presentation;
mod word;
mod workbook;

pub use delimited::{read_csv, CsvFile, Delimiter};
pub use office::{OFFICE_MAX_DECOMPRESSED_BYTES, OFFICE_MAX_ENTRIES};
pub use pdf::{read_pdf, PageText, PdfDocument, PdfPage};
pub use presentation::{read_presentation, Presentation, Slide, PRESENTATION_EXTENSIONS};
pub use word::{read_word, SegmentKind, WordDocument, WordSegment, WORD_EXTENSIONS};
pub use workbook::{read_workbook, Workbook, WORKBOOK_EXTENSIONS};

/// Where a cell was read: its sheet, its row and its column.
///
/// Rows and columns count from 1, as a spreadsheet shows them: the cell `B3`
/// is row 3, column 2. A CSV file has one sheet, named after the file.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CellLocation {
    /// Name of the sheet.
    pub sheet: String,
    /// Row number, from 1.
    pub row: u32,
    /// Column number, from 1.
    pub column: u32,
}

/// One cell read as text, with its location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// Where the cell was read.
    pub location: CellLocation,
    /// The value as text; an empty cell is the empty string.
    pub text: String,
}

/// One sheet: its name and its rows of cells, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sheet {
    /// Name of the sheet.
    pub name: String,
    /// Rows of the sheet's used range, top to bottom, each left to right.
    pub rows: Vec<Vec<Cell>>,
}

/// Why a file could not be read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReadError {
    /// The file could not be opened or read.
    #[error("failed to read {}", .path.display())]
    Io {
        /// The file.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The file name does not end with an extension the reader opens, as a
    /// legacy binary Word (`.doc`) or PowerPoint (`.ppt`) file does not.
    #[error("{} is not a format this reader opens ({supported})", .path.display())]
    UnsupportedFormat {
        /// The file.
        path: PathBuf,
        /// The extensions the reader opens, for the message.
        supported: &'static str,
    },
    /// The file is protected by a password.
    #[error("{} is protected by a password", .path.display())]
    PasswordProtected {
        /// The file.
        path: PathBuf,
    },
    /// The file is damaged, or is not the format its name says.
    #[error("{} is damaged or is not the format its name says: {detail}", .path.display())]
    Corrupt {
        /// The file.
        path: PathBuf,
        /// What the reader found wrong.
        detail: String,
    },
    /// The archive a Word or PowerPoint file is goes over a cap: more entries
    /// than [`OFFICE_MAX_ENTRIES`], or more bytes once decompressed than
    /// [`OFFICE_MAX_DECOMPRESSED_BYTES`]. It is not read.
    #[error("{} is over the cap of {limit} {cap} and is not read", .path.display())]
    ArchiveOverCap {
        /// The file.
        path: PathBuf,
        /// What the cap counts: entries, or bytes decompressed.
        cap: &'static str,
        /// The cap.
        limit: u64,
    },
}

impl ReadError {
    fn io(path: &Path, source: std::io::Error) -> Self {
        Self::Io {
            path: path.to_path_buf(),
            source,
        }
    }

    fn corrupt(path: &Path, detail: impl Into<String>) -> Self {
        Self::Corrupt {
            path: path.to_path_buf(),
            detail: detail.into(),
        }
    }
}
