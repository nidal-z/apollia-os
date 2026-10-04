//! What a file of a source gives the index: its sections, by its format.
//!
//! The format follows the extension, in any case: workbooks, CSV files, PDF,
//! Word and PowerPoint documents go through their readers, plain text files
//! are read as text. Any other file is not indexed and is not reported: a
//! folder holds pictures, archives and programs no reader is meant for.

use std::path::Path;

use crate::chunk::Section;
use crate::reader::{
    read_csv, read_pdf, read_presentation, read_word, read_workbook, ReadError, Sheet,
    PRESENTATION_EXTENSIONS, WORD_EXTENSIONS, WORKBOOK_EXTENSIONS,
};

/// Extensions read as plain text, lower case.
pub const TEXT_EXTENSIONS: [&str; 12] = [
    "txt", "md", "markdown", "rst", "log", "json", "yaml", "yml", "toml", "xml", "html", "htm",
];

/// Extensions read as delimited text, lower case.
const DELIMITED_EXTENSIONS: [&str; 2] = ["csv", "tsv"];

/// The format of a file, from its extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Workbook,
    Delimited,
    Pdf,
    Word,
    Presentation,
    Text,
}

fn format_of(path: &Path) -> Option<Format> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let ext = extension.as_str();
    if WORKBOOK_EXTENSIONS.contains(&ext) {
        Some(Format::Workbook)
    } else if DELIMITED_EXTENSIONS.contains(&ext) {
        Some(Format::Delimited)
    } else if ext == "pdf" {
        Some(Format::Pdf)
    } else if WORD_EXTENSIONS.contains(&ext) {
        Some(Format::Word)
    } else if PRESENTATION_EXTENSIONS.contains(&ext) {
        Some(Format::Presentation)
    } else if TEXT_EXTENSIONS.contains(&ext) {
        Some(Format::Text)
    } else {
        None
    }
}

/// Whether the file at `path` is of a format the index reads.
#[must_use]
pub fn is_indexed_format(path: &Path) -> bool {
    format_of(path).is_some()
}

/// A sheet as the rows of text the chunker cuts: each non-empty row as its
/// number and its non-empty cells joined by ` | `.
fn sheet_section(sheet: &Sheet) -> Section {
    let rows = sheet
        .rows
        .iter()
        .filter_map(|row| {
            let number = row.first()?.location.row;
            let cells: Vec<&str> = row
                .iter()
                .map(|cell| cell.text.trim())
                .filter(|text| !text.is_empty())
                .collect();
            (!cells.is_empty()).then(|| (number, cells.join(" | ")))
        })
        .collect();
    Section::Sheet {
        name: sheet.name.clone(),
        rows,
    }
}

/// The sections of the file at `path`, read by the reader of its format;
/// `None` for a file of a format the index does not read.
///
/// # Errors
/// The [`ReadError`] of the reader; for a text file, [`ReadError::Io`] when
/// it cannot be read. A text file that is not UTF-8 is read with its invalid
/// bytes replaced.
pub fn sections_of(path: &Path) -> Result<Option<Vec<Section>>, ReadError> {
    let Some(format) = format_of(path) else {
        return Ok(None);
    };
    let sections = match format {
        Format::Workbook => read_workbook(path)?
            .sheets
            .iter()
            .map(sheet_section)
            .collect(),
        Format::Delimited => vec![sheet_section(&read_csv(path)?.sheet)],
        Format::Pdf => read_pdf(path)?
            .pages
            .into_iter()
            .filter_map(|page| {
                let text = page.text.as_text()?.to_owned();
                Some(Section::Page {
                    page: page.number,
                    text,
                })
            })
            .collect(),
        Format::Word => {
            let segments: Vec<String> = read_word(path)?
                .segments
                .into_iter()
                .map(|segment| segment.text)
                .filter(|text| !text.trim().is_empty())
                .collect();
            vec![Section::Text {
                text: segments.join("\n\n"),
            }]
        }
        Format::Presentation => read_presentation(path)?
            .slides
            .into_iter()
            .map(|slide| {
                let text = if slide.notes.trim().is_empty() {
                    slide.text
                } else {
                    format!("{}\n\n{}", slide.text, slide.notes)
                };
                Section::Slide {
                    slide: slide.number,
                    text,
                }
            })
            .collect(),
        Format::Text => {
            let bytes = std::fs::read(path).map_err(|source| ReadError::Io {
                path: path.to_path_buf(),
                source,
            })?;
            vec![Section::Text {
                text: String::from_utf8_lossy(&bytes).into_owned(),
            }]
        }
    };
    Ok(Some(sections))
}
