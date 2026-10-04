//! The PDF reader, over `pdf_oxide`.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use pdf_oxide::document::PdfDocument as OxideDocument;
use pdf_oxide::error::Error as OxideError;

use crate::reader::ReadError;

/// A PDF document read as text, page by page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfDocument {
    /// Every page of the document, in order.
    pub pages: Vec<PdfPage>,
}

/// One page of a PDF document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfPage {
    /// Page number, from 1, as a PDF viewer shows it.
    pub number: u32,
    /// What the page holds as text.
    pub text: PageText,
}

/// The text of one page.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PageText {
    /// The text the page draws, in reading order.
    Text(String),
    /// The page draws no letter and no digit: a scanned page, a picture or a
    /// blank page. Its text can only come from optical character recognition.
    WithoutText,
}

impl PageText {
    /// The text of the page, or `None` for a page without text.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::WithoutText => None,
        }
    }
}

/// Reads the PDF document at `path`: the text of every page, with its number.
///
/// A page that draws no letter and no digit is [`PageText::WithoutText`]. A
/// document locked with an owner password only opens, since reading it needs
/// no password; a document that needs a password to be opened does not.
///
/// # Errors
///
/// [`ReadError::Io`] when the file cannot be read,
/// [`ReadError::PasswordProtected`] when the document needs a password to be
/// opened, and [`ReadError::Corrupt`] when it is damaged or is not a PDF. A
/// defect of the parser on a malformed file is reported as
/// [`ReadError::Corrupt`] too, never as a crash.
pub fn read_pdf(path: &Path) -> Result<PdfDocument, ReadError> {
    let bytes = std::fs::read(path).map_err(|source| ReadError::io(path, source))?;
    match catch_unwind(AssertUnwindSafe(|| parse(path, bytes))) {
        Ok(read) => read,
        Err(_) => {
            tracing::event!(
                tracing::Level::WARN,
                path = %path.display(),
                "sources.pdf.parser_panicked"
            );
            Err(ReadError::corrupt(
                path,
                "the PDF parser failed on this file",
            ))
        }
    }
}

fn parse(path: &Path, bytes: Vec<u8>) -> Result<PdfDocument, ReadError> {
    let document = OxideDocument::from_bytes(bytes).map_err(|error| read_error(path, &error))?;
    if document.is_encrypted() {
        let opened = document
            .authenticate(b"")
            .map_err(|error| read_error(path, &error))?;
        if !opened {
            return Err(ReadError::PasswordProtected {
                path: path.to_path_buf(),
            });
        }
    }
    let count = document
        .page_count()
        .map_err(|error| read_error(path, &error))?;
    let mut pages = Vec::with_capacity(count);
    for (index, number) in (0..count).zip(1u32..) {
        let text = document
            .extract_text(index)
            .map_err(|error| read_error(path, &error))?;
        let text = if text.chars().any(char::is_alphanumeric) {
            PageText::Text(text)
        } else {
            PageText::WithoutText
        };
        pages.push(PdfPage { number, text });
    }
    Ok(PdfDocument { pages })
}

fn read_error(path: &Path, error: &OxideError) -> ReadError {
    match error {
        OxideError::EncryptedPdf => ReadError::PasswordProtected {
            path: path.to_path_buf(),
        },
        other => ReadError::corrupt(path, other.to_string()),
    }
}
