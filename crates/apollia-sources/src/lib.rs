#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! The sources an agent may consult once they are checked for it.
//!
//! A source is an indexed folder, a file, a mailbox, the user profile, the
//! agent's memory or a code folder. Their registry is a table of `system.db`
//! ([`apollia_core::source_registry`]); this crate holds the actor that owns
//! it ([`SourceRegistryHandle`]), the reading of an agent's source list
//! against it ([`checked_sources`]), the readers that turn a workbook or a CSV
//! file into cells of text ([`read_workbook`], [`read_csv`]), the reader that
//! turns a PDF document into the text of its pages ([`read_pdf`]), scanned
//! pages and pictures being read by recognition ([`Ocr`]), the readers
//! of Word documents and PowerPoint presentations ([`read_word`],
//! [`read_presentation`]), the table those cells make for a batch: the sheet
//! and header row proposed, the preview shown before launch
//! ([`preview_table`]) and the batch source that yields one item per row
//! ([`TableSource`], [`register_table_sources`]), which writes the results of
//! the batch into a copy of the file or, on request, into the original
//! ([`ResultsDestination`]), and the index of a source
//! ([`index`]): its documents cut into chunks ([`chunk`]), searched by their
//! words and by their meaning, alone or several at once ([`search_sources`],
//! served by the actor to the `source_search` tool), kept in step with the
//! files of each folder and file source by the indexer ([`indexer`]), and the
//! reading of one office document as the text of its pages, slides or sheets
//! ([`read_document`]).

mod actor;
mod checked;
pub mod chunk;
mod document;
mod folder_source;
pub mod index;
pub mod indexer;
mod ocr;
mod reader;
mod search;
mod table;

pub use actor::{IndexStatus, SourceRegistryHandle, SourcesError, COMMAND_CAPACITY};
pub use apollia_core::source_registry::{
    IndexState, NewSource, Source, SourceKind, SourceRegistryError,
};
pub use checked::{checked_sources, searchable_sources, CheckedSource};
pub use document::{
    is_office_document, is_readable_document, read_document, DocumentPart, DocumentText, PartUnit,
};
pub use folder_source::{
    register_folder_source, FolderSource, FILE_FIELD, FOLDER_KIND, OPTION_RECURSIVE, TEXT_FIELD,
};
pub use indexer::{PassOutcome, PassageEmbedder, MAX_INDEXED_FILE_BYTES};
pub use ocr::{Ocr, OcrError};
pub use reader::{
    read_csv, read_pdf, read_pdf_scanned, read_picture, read_presentation, read_word,
    read_workbook, Cell, CellLocation, CsvFile, Delimiter, PageText, PdfDocument, PdfPage,
    Presentation, ReadError, SegmentKind, Sheet, Slide, WordDocument, WordSegment, Workbook,
    OFFICE_MAX_DECOMPRESSED_BYTES, OFFICE_MAX_ENTRIES, PICTURE_EXTENSIONS, PRESENTATION_EXTENSIONS,
    WORD_EXTENSIONS, WORKBOOK_EXTENSIONS,
};
pub use search::{search_sources, SourcesQuery, SourcesSearchOutcome};
pub use table::{
    column_letter, is_empty_row, preview_table, propose_header, propose_sheet, read_table,
    register_table_sources, results_copy_path, Column, PreviewError, PreviewRow,
    ResultsDestination, SheetSummary, Table, TableFormat, TablePreview, TableSource, CSV_KIND,
    HEADER_SEARCH_ROWS, OPTION_COLUMNS, OPTION_DESTINATION, OPTION_HEADER_ROW, OPTION_LANGUAGE,
    OPTION_SHEET, PREVIEW_ROWS, SPREADSHEET_KIND,
};
