//! The PDF reader, over `pdf_oxide`.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use pdf_oxide::document::PdfDocument as OxideDocument;
use pdf_oxide::error::Error as OxideError;

use crate::ocr::Ocr;
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
    /// blank page. Its text can only come from optical character recognition,
    /// which was not asked for, or found nothing to read on it.
    WithoutText,
    /// A scanned page, a single picture, that recognition would read, on a
    /// machine where recognition is not available yet.
    OcrUnavailable,
    /// A page without text that recognition could not read: it is not a
    /// single picture, its picture cannot be decoded, or the engine failed.
    NotRead,
}

impl PageText {
    /// The text of the page, or `None` for a page without text.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::WithoutText | Self::OcrUnavailable | Self::NotRead => None,
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
    read(path, None)
}

/// Reads the PDF document at `path` as [`read_pdf`] does, and hands every
/// page without text to `ocr`.
///
/// A scanned page is one picture: it is taken as the document embeds it,
/// without rendering the page, and recognised. Its text is
/// [`PageText::Text`]; a picture nothing is recognised on stays
/// [`PageText::WithoutText`]. When `ocr` is not available the page is
/// [`PageText::OcrUnavailable`]. A page without text that is not a single
/// picture is [`PageText::NotRead`], as is one whose picture cannot be
/// decoded.
///
/// # Errors
///
/// As [`read_pdf`].
pub fn read_pdf_scanned(path: &Path, ocr: &Ocr) -> Result<PdfDocument, ReadError> {
    read(path, Some(ocr))
}

fn read(path: &Path, ocr: Option<&Ocr>) -> Result<PdfDocument, ReadError> {
    let bytes = std::fs::read(path).map_err(|source| ReadError::io(path, source))?;
    match catch_unwind(AssertUnwindSafe(|| parse(path, bytes, ocr))) {
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

fn parse(path: &Path, bytes: Vec<u8>, ocr: Option<&Ocr>) -> Result<PdfDocument, ReadError> {
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
        } else if let Some(ocr) = ocr {
            recognise(&document, index, ocr)
        } else {
            PageText::WithoutText
        };
        pages.push(PdfPage { number, text });
    }
    Ok(PdfDocument { pages })
}

/// The text of the page `index`, which draws none, from the single picture
/// it embeds.
fn recognise(document: &OxideDocument, index: usize, ocr: &Ocr) -> PageText {
    let Ok(pictures) = document.extract_images(index) else {
        return PageText::NotRead;
    };
    let [picture] = pictures.as_slice() else {
        return PageText::NotRead;
    };
    if !ocr.is_available() {
        return PageText::OcrUnavailable;
    }
    let Ok(picture) = picture.to_dynamic_image() else {
        return PageText::NotRead;
    };
    match ocr.read(&picture.to_luma8()) {
        Some(Ok(text)) if text.chars().any(char::is_alphanumeric) => PageText::Text(text),
        Some(Ok(_)) => PageText::WithoutText,
        Some(Err(error)) => {
            tracing::event!(
                tracing::Level::WARN,
                page = index + 1,
                error = %error,
                "sources.pdf.ocr_failed"
            );
            PageText::NotRead
        }
        None => PageText::OcrUnavailable,
    }
}

fn read_error(path: &Path, error: &OxideError) -> ReadError {
    match error {
        OxideError::EncryptedPdf => ReadError::PasswordProtected {
            path: path.to_path_buf(),
        },
        other => ReadError::corrupt(path, other.to_string()),
    }
}
