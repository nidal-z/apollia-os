//! What a file of a source gives the index: its sections, by its format.
//!
//! The format follows the extension, in any case: workbooks, CSV files, PDF,
//! Word and PowerPoint documents go through their readers, plain text files
//! are read as text. The pages of a PDF document that hold no text, and the
//! pictures of the source (`.png`, `.jpg`, `.tiff`), go to optical character
//! recognition; a page it does not read is reported with the reason. Any
//! other file is not indexed and is not reported: a folder holds archives and
//! programs no reader is meant for.

use std::path::Path;

use crate::chunk::Section;
use crate::index::UnreadReason;
use crate::ocr::Ocr;
use crate::reader::{
    read_csv, read_pdf_scanned, read_picture, read_presentation, read_word, read_workbook,
    PageText, ReadError, Sheet, PICTURE_EXTENSIONS, PRESENTATION_EXTENSIONS, WORD_EXTENSIONS,
    WORKBOOK_EXTENSIONS,
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
    Picture,
    Text,
}

/// What a file gives the index: the sections that were read, and the pages
/// that were not.
#[derive(Debug, Default)]
pub struct Extracted {
    /// The sections read, in the order of the document.
    pub sections: Vec<Section>,
    /// The pages without text that were not read, each with the reason.
    pub unread: Vec<(u32, UnreadReason)>,
}

impl Extracted {
    fn of(sections: Vec<Section>) -> Self {
        Self {
            sections,
            unread: Vec::new(),
        }
    }

    /// Files one page: its text among the sections, or its number among the
    /// pages not read. A page that holds nothing is neither.
    fn page(&mut self, page: u32, text: PageText) {
        match text {
            PageText::Text(text) => self.sections.push(Section::Page { page, text }),
            PageText::OcrUnavailable => self.unread.push((page, UnreadReason::OcrUnavailable)),
            PageText::NotRead => self.unread.push((page, UnreadReason::NotRead)),
            PageText::WithoutText => {}
        }
    }
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
    } else if PICTURE_EXTENSIONS.contains(&ext) {
        Some(Format::Picture)
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

/// The sections of the file at `path`, read by the reader of its format, its
/// scanned pages and pictures by `ocr`; `None` for a file of a format the
/// index does not read.
///
/// # Errors
/// The [`ReadError`] of the reader; for a text file, [`ReadError::Io`] when
/// it cannot be read. A text file that is not UTF-8 is read with its invalid
/// bytes replaced.
pub fn sections_of(path: &Path, ocr: &Ocr) -> Result<Option<Extracted>, ReadError> {
    let Some(format) = format_of(path) else {
        return Ok(None);
    };
    let sections = match format {
        Format::Pdf => {
            let mut extracted = Extracted::default();
            for page in read_pdf_scanned(path, ocr)?.pages {
                extracted.page(page.number, page.text);
            }
            return Ok(Some(extracted));
        }
        Format::Picture => {
            let mut extracted = Extracted::default();
            extracted.page(1, read_picture(path, ocr)?);
            return Ok(Some(extracted));
        }
        Format::Workbook => read_workbook(path)?
            .sheets
            .iter()
            .map(sheet_section)
            .collect(),
        Format::Delimited => vec![sheet_section(&read_csv(path)?.sheet)],
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
    Ok(Some(Extracted::of(sections)))
}
