//! The text of an office document, as `file_read` hands it over.
//!
//! The readers are synchronous and read the whole file, so the extraction runs
//! on the blocking pool. The whole document comes back with each page, slide
//! or sheet under its heading; one part asked for comes back alone.

use std::path::Path;

use apollia_sources::{DocumentText, PartUnit, ReadError};

use super::FileReadError;

/// The one part of a document a call asks for, if any.
#[derive(Debug, Clone, Copy)]
pub(super) struct PartRequest<'a> {
    /// A page or a slide, from 1.
    pub page: Option<u32>,
    /// A sheet, by its name.
    pub sheet: Option<&'a str>,
}

impl PartRequest<'_> {
    /// Whether one part is asked for.
    pub(super) fn is_some(&self) -> bool {
        self.page.is_some() || self.sheet.is_some()
    }
}

/// What a document holds, as the model reads it: `3 pages`, `12 slides`,
/// `sheets: "Sales", "Totals"`, or `one document`.
fn parts_summary(document: &DocumentText) -> String {
    let count = document.parts.len();
    match document.unit {
        PartUnit::Sheet => {
            let names: Vec<String> = document
                .parts
                .iter()
                .map(|part| format!("\"{}\"", part.name.as_deref().unwrap_or_default()))
                .collect();
            format!("sheets: {}", names.join(", "))
        }
        PartUnit::Page if count == 1 => "1 page".to_string(),
        PartUnit::Page => format!("{count} pages"),
        PartUnit::Slide if count == 1 => "1 slide".to_string(),
        PartUnit::Slide => format!("{count} slides"),
        _ => "one document, without pages".to_string(),
    }
}

/// The text of the part `request` asks for, or of the whole document.
fn select(
    document: &DocumentText,
    shown_path: &str,
    request: &PartRequest<'_>,
) -> Result<String, FileReadError> {
    let not_found = |part: String| FileReadError::PartNotFound {
        path: shown_path.to_string(),
        part,
        available: parts_summary(document),
    };
    if let Some(name) = request.sheet {
        if document.unit != PartUnit::Sheet {
            return Err(not_found(format!("sheet \"{name}\"")));
        }
        let wanted = name.trim();
        return document
            .parts
            .iter()
            .find(|part| {
                part.name
                    .as_deref()
                    .is_some_and(|n| n.trim().eq_ignore_ascii_case(wanted))
            })
            .map(|part| part.text.clone())
            .ok_or_else(|| not_found(format!("sheet \"{name}\"")));
    }
    if let Some(number) = request.page {
        let unit = match document.unit {
            PartUnit::Slide => "slide",
            _ => "page",
        };
        if !matches!(document.unit, PartUnit::Page | PartUnit::Slide) {
            return Err(not_found(format!("{unit} {number}")));
        }
        return document
            .parts
            .iter()
            .find(|part| part.number == number)
            .map(|part| part.text.clone())
            .ok_or_else(|| not_found(format!("{unit} {number}")));
    }
    Ok(document.full_text())
}

fn read_error(shown_path: &str, error: ReadError) -> FileReadError {
    match error {
        ReadError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
            FileReadError::NotFound {
                path: shown_path.to_string(),
            }
        }
        ReadError::Io { source, .. } => FileReadError::IoError {
            path: shown_path.to_string(),
            cause: source.to_string(),
        },
        ReadError::PasswordProtected { .. } => FileReadError::UnreadableDocument {
            path: shown_path.to_string(),
            cause: "it is protected by a password".to_string(),
        },
        ReadError::Corrupt { detail, .. } => FileReadError::UnreadableDocument {
            path: shown_path.to_string(),
            cause: format!("it is damaged or is not the format its name says ({detail})"),
        },
        other => FileReadError::UnreadableDocument {
            path: shown_path.to_string(),
            cause: other.to_string(),
        },
    }
}

/// Extract the text of the document at `resolved`, named `shown_path` in any
/// error, and say what the document holds.
pub(super) async fn extract(
    resolved: &Path,
    shown_path: &str,
    request: &PartRequest<'_>,
) -> Result<(String, String), FileReadError> {
    let path = resolved.to_path_buf();
    let read = tokio::task::spawn_blocking(move || apollia_sources::read_document(&path))
        .await
        .map_err(|e| FileReadError::IoError {
            path: shown_path.to_string(),
            cause: e.to_string(),
        })?;
    let document = read.map_err(|e| read_error(shown_path, e))?;
    let text = select(&document, shown_path, request)?;
    Ok((text, parts_summary(&document)))
}

#[cfg(test)]
mod tests;
