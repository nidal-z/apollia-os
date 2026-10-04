#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! The sources an agent may consult once they are checked for it.
//!
//! A source is an indexed folder, a file, a mailbox, the user profile, the
//! agent's memory or a code folder. Their registry is a table of `system.db`
//! ([`apollia_core::source_registry`]); this crate holds the actor that owns
//! it ([`SourceRegistryHandle`]), the reading of an agent's source list
//! against it ([`checked_sources`]), the readers that turn a workbook or a CSV
//! file into cells of text ([`read_workbook`], [`read_csv`]), the reader that
//! turns a PDF document into the text of its pages ([`read_pdf`]), the readers
//! of Word documents and PowerPoint presentations ([`read_word`],
//! [`read_presentation`]), the table those cells make for a batch: the sheet
//! and header row proposed, the preview shown before launch
//! ([`preview_table`]) and the batch source that yields one item per row
//! ([`TableSource`], [`register_table_sources`]), and the index of a source
//! ([`index`]): its documents cut into chunks ([`chunk`]), searched by their
//! words and by their meaning.

mod actor;
mod checked;
pub mod chunk;
pub mod index;
mod reader;
mod table;

pub use actor::{SourceRegistryHandle, SourcesError, COMMAND_CAPACITY};
pub use apollia_core::source_registry::{
    IndexState, NewSource, Source, SourceKind, SourceRegistryError,
};
pub use checked::{checked_sources, CheckedSource};
pub use reader::{
    read_csv, read_pdf, read_presentation, read_word, read_workbook, Cell, CellLocation, CsvFile,
    Delimiter, PageText, PdfDocument, PdfPage, Presentation, ReadError, SegmentKind, Sheet, Slide,
    WordDocument, WordSegment, Workbook, OFFICE_MAX_DECOMPRESSED_BYTES, OFFICE_MAX_ENTRIES,
    PRESENTATION_EXTENSIONS, WORD_EXTENSIONS, WORKBOOK_EXTENSIONS,
};
pub use table::{
    column_letter, is_empty_row, preview_table, propose_header, propose_sheet, read_table,
    register_table_sources, Column, PreviewError, PreviewRow, SheetSummary, Table, TableFormat,
    TablePreview, TableSource, CSV_KIND, HEADER_SEARCH_ROWS, OPTION_COLUMNS, OPTION_HEADER_ROW,
    OPTION_SHEET, PREVIEW_ROWS, SPREADSHEET_KIND,
};
