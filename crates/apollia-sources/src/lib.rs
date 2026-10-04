#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! The sources an agent may consult once they are checked for it.
//!
//! A source is an indexed folder, a file, a mailbox, the user profile, the
//! agent's memory or a code folder. Their registry is a table of `system.db`
//! ([`apollia_core::source_registry`]); this crate holds the actor that owns
//! it ([`SourceRegistryHandle`]), the reading of an agent's source list
//! against it ([`checked_sources`]), the readers that turn a workbook or a
//! CSV file into cells of text ([`read_workbook`], [`read_csv`]), and the
//! index of a source ([`index`]): its documents cut into chunks ([`chunk`]),
//! searched by their words and by their meaning.

mod actor;
mod checked;
pub mod chunk;
pub mod index;
mod reader;

pub use actor::{SourceRegistryHandle, SourcesError, COMMAND_CAPACITY};
pub use apollia_core::source_registry::{
    IndexState, NewSource, Source, SourceKind, SourceRegistryError,
};
pub use checked::{checked_sources, CheckedSource};
pub use reader::{
    read_csv, read_workbook, Cell, CellLocation, CsvFile, Delimiter, ReadError, Sheet, Workbook,
    WORKBOOK_EXTENSIONS,
};
