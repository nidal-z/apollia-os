//! What the Word and PowerPoint readers share: the zip archive an Office
//! document is, opened and read within caps, and the text of its XML parts.
//!
//! A `.docx` or a `.pptx` file comes from elsewhere, so it is read as hostile:
//! the number of entries is capped before anything is decompressed, and every
//! part is read through a budget of decompressed bytes that counts the bytes
//! actually inflated, never the sizes the archive declares.

use std::io::{Cursor, Read};
use std::path::Path;

use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesRef, BytesText};
use zip::result::ZipError;
use zip::ZipArchive;

use crate::reader::ReadError;

/// Most entries a Word or PowerPoint archive may hold. A presentation of a few
/// hundred slides, with their notes, relationships and pictures, holds a few
/// thousand.
pub const OFFICE_MAX_ENTRIES: usize = 10_000;

/// Most bytes the parts read from one Word or PowerPoint archive may weigh once
/// decompressed, all parts together. The text of a document of several
/// thousand pages fits well under it.
pub const OFFICE_MAX_DECOMPRESSED_BYTES: u64 = 64 * 1024 * 1024;

/// An Office document opened as an archive, with what is left of its budget.
pub(super) struct OfficeArchive<'p> {
    path: &'p Path,
    zip: ZipArchive<Cursor<Vec<u8>>>,
    remaining: u64,
}

impl<'p> OfficeArchive<'p> {
    /// Opens `path` when its extension is one of `extensions`, and checks the
    /// entry cap.
    pub(super) fn open(
        path: &'p Path,
        extensions: &[&str],
        supported: &'static str,
    ) -> Result<Self, ReadError> {
        let known = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extensions
                    .iter()
                    .any(|known| extension.eq_ignore_ascii_case(known))
            });
        if !known {
            return Err(ReadError::UnsupportedFormat {
                path: path.to_path_buf(),
                supported,
            });
        }
        let bytes = std::fs::read(path).map_err(|source| ReadError::io(path, source))?;
        let zip = ZipArchive::new(Cursor::new(bytes))
            .map_err(|error| ReadError::corrupt(path, format!("not a zip archive: {error}")))?;
        if zip.len() > OFFICE_MAX_ENTRIES {
            return Err(over_cap(path, "entries", OFFICE_MAX_ENTRIES as u64));
        }
        Ok(Self {
            path,
            zip,
            remaining: OFFICE_MAX_DECOMPRESSED_BYTES,
        })
    }

    /// The file, for the errors of a caller.
    pub(super) fn path(&self) -> &'p Path {
        self.path
    }

    /// The names of every entry.
    pub(super) fn names(&self) -> Vec<String> {
        self.zip.file_names().map(str::to_owned).collect()
    }

    /// The bytes of the part `name` once decompressed, or `None` when the
    /// archive has no such part.
    pub(super) fn read_part(&mut self, name: &str) -> Result<Option<Vec<u8>>, ReadError> {
        let entry = match self.zip.by_name(name) {
            Ok(entry) => entry,
            Err(ZipError::FileNotFound) => return Ok(None),
            Err(error) => return Err(ReadError::corrupt(self.path, format!("{name}: {error}"))),
        };
        let mut bytes = Vec::new();
        entry
            .take(self.remaining.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| ReadError::corrupt(self.path, format!("{name}: {error}")))?;
        let read = bytes.len() as u64;
        if read > self.remaining {
            tracing::event!(
                tracing::Level::WARN,
                path = %self.path.display(),
                part = %name,
                "sources.office.decompressed_cap_reached"
            );
            return Err(over_cap(
                self.path,
                "bytes decompressed",
                OFFICE_MAX_DECOMPRESSED_BYTES,
            ));
        }
        self.remaining -= read;
        Ok(Some(bytes))
    }
}

fn over_cap(path: &Path, cap: &'static str, limit: u64) -> ReadError {
    ReadError::ArchiveOverCap {
        path: path.to_path_buf(),
        cap,
        limit,
    }
}

/// Appends the content of a text event to `out`, line ends normalised.
pub(super) fn push_text(out: &mut String, text: &BytesText<'_>) -> Result<(), String> {
    let content = text.xml10_content().map_err(|error| error.to_string())?;
    out.push_str(&content);
    Ok(())
}

/// Appends what a character or entity reference stands for to `out`. A
/// reference to an entity XML does not predefine stands for nothing: an
/// Office part declares none.
pub(super) fn push_reference(out: &mut String, reference: &BytesRef<'_>) -> Result<(), String> {
    if let Some(character) = reference
        .resolve_char_ref()
        .map_err(|error| error.to_string())?
    {
        out.push(character);
        return Ok(());
    }
    let name = reference.decode().map_err(|error| error.to_string())?;
    if let Some(text) = resolve_predefined_entity(&name) {
        out.push_str(text);
    }
    Ok(())
}
