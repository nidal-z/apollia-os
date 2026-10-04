//! The picture reader: a `.png`, `.jpg` or `.tiff` file, read by recognition.

use std::path::Path;

use crate::ocr::Ocr;
use crate::reader::{PageText, ReadError};

/// Extensions of the pictures read, lower case.
pub const PICTURE_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "tif", "tiff"];

/// Reads the picture at `path` as one page: its text as `ocr` recognises it.
///
/// The answer is [`PageText::Text`], [`PageText::WithoutText`] for a picture
/// nothing is recognised on, [`PageText::OcrUnavailable`] when `ocr` is not
/// available (the file is then not even decoded), and [`PageText::NotRead`]
/// when the engine failed on it.
///
/// # Errors
///
/// [`ReadError::Io`] when the file cannot be read and [`ReadError::Corrupt`]
/// when it is damaged or is not the format its name says.
pub fn read_picture(path: &Path, ocr: &Ocr) -> Result<PageText, ReadError> {
    if !ocr.is_available() {
        return Ok(PageText::OcrUnavailable);
    }
    let picture = image::ImageReader::open(path)
        .map_err(|source| ReadError::io(path, source))?
        .decode()
        .map_err(|error| ReadError::corrupt(path, error.to_string()))?;
    Ok(match ocr.read(&picture.to_luma8()) {
        Some(Ok(text)) if text.chars().any(char::is_alphanumeric) => PageText::Text(text),
        Some(Ok(_)) => PageText::WithoutText,
        Some(Err(error)) => {
            tracing::event!(
                tracing::Level::WARN,
                path = %path.display(),
                error = %error,
                "sources.picture.ocr_failed"
            );
            PageText::NotRead
        }
        None => PageText::OcrUnavailable,
    })
}
