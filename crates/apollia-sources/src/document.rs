//! An office document read as the text of its parts.
//!
//! A PDF document, a Word document, a PowerPoint presentation, a workbook or a
//! CSV file is read by the reader of its format and handed over as
//! [`DocumentPart`]s: the pages of a PDF, the slides of a presentation, the
//! sheets of a workbook, and a Word document or a CSV file as one part. The
//! format follows the extension of the file name, in any case.
//!
//! This is what a reader of one file wants: the text, and a way to ask for
//! one page or one sheet alone. Like the readers it is synchronous and reads
//! the whole file, so an async caller runs it under `spawn_blocking`.

use std::path::Path;

use crate::reader::{
    read_csv, read_pdf, read_presentation, read_word, read_workbook, ReadError, Sheet,
    PRESENTATION_EXTENSIONS, WORD_EXTENSIONS, WORKBOOK_EXTENSIONS,
};

/// What the parts of a document are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PartUnit {
    /// The pages of a PDF document.
    Page,
    /// The slides of a presentation.
    Slide,
    /// The sheets of a workbook.
    Sheet,
    /// The document is one part: a Word document, a CSV file.
    Whole,
}

impl PartUnit {
    /// The unit as a lowercase word: `page`, `slide`, `sheet` or `document`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Page => "page",
            Self::Slide => "slide",
            Self::Sheet => "sheet",
            Self::Whole => "document",
        }
    }
}

/// One part of a document: a page, a slide, a sheet, or the whole of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentPart {
    /// Number of the part, from 1, in the order of the document.
    pub number: u32,
    /// Name of the part when it has one: the name of a sheet.
    pub name: Option<String>,
    /// Text of the part; empty for a page that draws no text.
    pub text: String,
}

/// A document read as text, part by part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentText {
    /// What the parts are.
    pub unit: PartUnit,
    /// Every part, in the order of the document.
    pub parts: Vec<DocumentPart>,
}

impl DocumentText {
    /// Characters of text over every part.
    #[must_use]
    pub fn characters(&self) -> usize {
        self.parts
            .iter()
            .map(|part| part.text.chars().count())
            .sum()
    }

    /// The heading a part is introduced by when several are shown together:
    /// `page 2`, `slide 3`, `sheet "Totals"`. `None` for a document that is
    /// one part.
    #[must_use]
    pub fn heading(&self, part: &DocumentPart) -> Option<String> {
        match (self.unit, part.name.as_deref()) {
            (PartUnit::Whole, _) => None,
            (unit, Some(name)) => Some(format!("{} \"{name}\"", unit.as_str())),
            (unit, None) => Some(format!("{} {}", unit.as_str(), part.number)),
        }
    }

    /// The whole text: each part under its heading between `---` marks, or the
    /// text alone for a document that is one part.
    #[must_use]
    pub fn full_text(&self) -> String {
        let mut out = String::new();
        for part in &self.parts {
            if !out.is_empty() {
                out.push('\n');
            }
            if let Some(heading) = self.heading(part) {
                out.push_str("--- ");
                out.push_str(&heading);
                out.push_str(" ---\n");
            }
            out.push_str(&part.text);
        }
        out
    }
}

/// The format of a document, from the extension of its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Pdf,
    Word,
    Presentation,
    Workbook,
    Csv,
}

fn format_of(path: &Path) -> Option<Format> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let extension = extension.as_str();
    if extension == "pdf" {
        Some(Format::Pdf)
    } else if WORD_EXTENSIONS.contains(&extension) {
        Some(Format::Word)
    } else if PRESENTATION_EXTENSIONS.contains(&extension) {
        Some(Format::Presentation)
    } else if WORKBOOK_EXTENSIONS.contains(&extension) {
        Some(Format::Workbook)
    } else if extension == "csv" {
        Some(Format::Csv)
    } else {
        None
    }
}

/// Whether the name of `path` ends with the extension of an office format
/// [`read_document`] opens: PDF, Word, PowerPoint or a workbook. A CSV file is
/// not one: it is text, and only read here when it is not valid UTF-8.
#[must_use]
pub fn is_office_document(path: &Path) -> bool {
    matches!(format_of(path), Some(format) if format != Format::Csv)
}

/// Whether [`read_document`] opens the file at `path`, by its extension: an
/// office format, or a CSV file.
#[must_use]
pub fn is_readable_document(path: &Path) -> bool {
    format_of(path).is_some()
}

/// A sheet as lines of text: each row that holds text as its cells separated
/// by tabs, trailing empty cells dropped.
fn sheet_text(sheet: &Sheet) -> String {
    let mut lines = Vec::with_capacity(sheet.rows.len());
    for row in &sheet.rows {
        let mut cells: Vec<&str> = row.iter().map(|cell| cell.text.as_str()).collect();
        while cells.last().is_some_and(|text| text.trim().is_empty()) {
            cells.pop();
        }
        if !cells.is_empty() {
            lines.push(cells.join("\t").replace('\n', " "));
        }
    }
    lines.join("\n")
}

fn numbered<T>(items: Vec<T>) -> impl Iterator<Item = (u32, T)> {
    (1u32..).zip(items)
}

/// Reads the document at `path` as the text of its parts.
///
/// A page of a PDF that draws no text (a scanned page) is a part with an empty
/// text. The notes of a slide follow its text.
///
/// # Errors
///
/// [`ReadError::UnsupportedFormat`] when the extension is none this function
/// opens, and the [`ReadError`] of the reader of the format otherwise: a file
/// that cannot be read, is protected by a password, is damaged, or goes over a
/// cap of its archive.
pub fn read_document(path: &Path) -> Result<DocumentText, ReadError> {
    let format = format_of(path).ok_or_else(|| ReadError::UnsupportedFormat {
        path: path.to_path_buf(),
        supported: "pdf, docx, docm, pptx, pptm, xlsx, xlsm, xlsb, xls, ods, csv",
    })?;
    let document = match format {
        Format::Pdf => DocumentText {
            unit: PartUnit::Page,
            parts: read_pdf(path)?
                .pages
                .into_iter()
                .map(|page| DocumentPart {
                    number: page.number,
                    name: None,
                    text: page.text.as_text().unwrap_or_default().to_owned(),
                })
                .collect(),
        },
        Format::Word => {
            let segments: Vec<String> = read_word(path)?
                .segments
                .into_iter()
                .map(|segment| segment.text)
                .filter(|text| !text.trim().is_empty())
                .collect();
            DocumentText {
                unit: PartUnit::Whole,
                parts: vec![DocumentPart {
                    number: 1,
                    name: None,
                    text: segments.join("\n"),
                }],
            }
        }
        Format::Presentation => DocumentText {
            unit: PartUnit::Slide,
            parts: read_presentation(path)?
                .slides
                .into_iter()
                .map(|slide| DocumentPart {
                    number: slide.number,
                    name: None,
                    text: if slide.notes.trim().is_empty() {
                        slide.text
                    } else {
                        format!("{}\n{}", slide.text, slide.notes)
                    },
                })
                .collect(),
        },
        Format::Workbook => DocumentText {
            unit: PartUnit::Sheet,
            parts: numbered(read_workbook(path)?.sheets)
                .map(|(number, sheet)| DocumentPart {
                    number,
                    text: sheet_text(&sheet),
                    name: Some(sheet.name),
                })
                .collect(),
        },
        Format::Csv => DocumentText {
            unit: PartUnit::Whole,
            parts: vec![DocumentPart {
                number: 1,
                name: None,
                text: sheet_text(&read_csv(path)?.sheet),
            }],
        },
    };
    Ok(document)
}
