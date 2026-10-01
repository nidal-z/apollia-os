//! The contract a batch consumes to read its items, whatever their origin.
//!
//! Decision in force: the batch knows this contract and nothing else. A
//! spreadsheet, a folder or a mailbox becomes a batch input by implementing
//! [`BatchSource`]; the readers themselves live with the data sources, never
//! inside the batch engine. A source:
//!
//! - describes its fields ([`BatchSource::fields`]);
//! - yields a sequence of items, each `{origin reference, fields}`, together
//!   with an estimated count ([`BatchSource::items`], [`SourceItems`]);
//! - when it allows it, writes `(reference, column, value)` back to its
//!   origin ([`SourceWriter`]), and every such write is preceded by a dated
//!   backup copy placed next to the original file.
//!
//! The backup rule is held by the types rather than by convention:
//! [`SourceWriter::write_value`] takes a [`Backup`], which only
//! [`WriteSession::open`] can build, and only after
//! [`SourceWriter::back_up`] has succeeded. A write without a successful
//! backup cannot be expressed.
//!
//! The contract is synchronous: readers parse files and archives, and the
//! batch engine drives a source from a blocking task rather than from an
//! actor's event loop.

mod memory;
mod registry;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use tracing::Level;

pub use memory::MemorySource;
pub use registry::{SourceFactory, SourceRegistry, SourceSpec};

/// Where an item comes from inside its source: a sheet and row, a file path,
/// a message identifier. Opaque to the batch, meaningful to the source.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OriginRef(String);

impl OriginRef {
    pub fn new(reference: impl Into<String>) -> Self {
        Self(reference.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for OriginRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One element of a batch: its origin and its fields, by field name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceItem {
    pub origin: OriginRef,
    pub fields: BTreeMap<String, String>,
}

/// The item sequence of a source, with the count the source expects to
/// yield. The estimate drives progress display; the sequence is the truth.
pub struct SourceItems<'a> {
    pub estimated_count: u64,
    pub items: Box<dyn Iterator<Item = Result<SourceItem, SourceError>> + Send + 'a>,
}

/// A source a batch can read, and possibly write back to.
pub trait BatchSource: Send {
    /// The field names every item carries, in display order.
    fn fields(&self) -> &[String];

    /// The items of the source, with an estimated count.
    fn items(&self) -> Result<SourceItems<'_>, SourceError>;

    /// The writer of this source, when it can write back to its origin.
    /// `None` means read-only, which is the default.
    fn writer(&mut self) -> Option<&mut dyn SourceWriter> {
        None
    }
}

/// The write half of a source. Reached only through a [`WriteSession`].
pub trait SourceWriter: Send {
    /// Copies the original next to itself under a dated name (see
    /// [`dated_backup_path`]) and returns the location of the copy.
    fn back_up(&mut self, taken_at: DateTime<Utc>) -> Result<PathBuf, SourceError>;

    /// Writes `value` into `column` of the item at `origin`. The `backup`
    /// argument is the proof that the copy was taken first.
    fn write_value(
        &mut self,
        backup: &Backup,
        origin: &OriginRef,
        column: &str,
        value: &str,
    ) -> Result<(), SourceError>;
}

/// A backup copy that was taken successfully. It has no public constructor:
/// [`WriteSession::open`] is the only place one is built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backup {
    path: PathBuf,
    taken_at: DateTime<Utc>,
}

impl Backup {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn taken_at(&self) -> DateTime<Utc> {
        self.taken_at
    }
}

/// A run of writes into one source, opened by a successful backup.
pub struct WriteSession<'a> {
    writer: &'a mut dyn SourceWriter,
    backup: Backup,
}

impl std::fmt::Debug for WriteSession<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WriteSession")
            .field("backup", &self.backup)
            .finish_non_exhaustive()
    }
}

impl<'a> WriteSession<'a> {
    /// Takes the dated backup of `source`, then opens the session. Refuses a
    /// read-only source, and refuses to open when the backup fails: no write
    /// is possible in either case.
    pub fn open(
        source: &'a mut dyn BatchSource,
        taken_at: DateTime<Utc>,
    ) -> Result<Self, SourceError> {
        let writer = source.writer().ok_or(SourceError::NotWritable)?;
        let path = writer.back_up(taken_at).map_err(|err| {
            tracing::event!(Level::WARN, error = %err, "batch_source.backup_failed");
            SourceError::BackupFailed {
                reason: err.to_string(),
            }
        })?;
        tracing::event!(Level::INFO, path = %path.display(), "batch_source.backup_taken");
        Ok(Self {
            writer,
            backup: Backup { path, taken_at },
        })
    }

    /// The backup this session was opened on.
    pub fn backup(&self) -> &Backup {
        &self.backup
    }

    /// Writes `value` into `column` of the item at `origin`.
    pub fn write(
        &mut self,
        origin: &OriginRef,
        column: &str,
        value: &str,
    ) -> Result<(), SourceError> {
        self.writer.write_value(&self.backup, origin, column, value)
    }
}

/// The dated sibling path of `original`: `name.ext` becomes
/// `name.backup-YYYYMMDDTHHMMSSZ.ext` in the same directory.
pub fn dated_backup_path(original: &Path, taken_at: DateTime<Utc>) -> Result<PathBuf, SourceError> {
    let stem = original
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| SourceError::InvalidLocation {
            location: original.display().to_string(),
        })?;
    let stamp = taken_at.format("%Y%m%dT%H%M%SZ");
    let name = match original.extension().and_then(|e| e.to_str()) {
        Some(ext) => format!("{stem}.backup-{stamp}.{ext}"),
        None => format!("{stem}.backup-{stamp}"),
    };
    Ok(original.with_file_name(name))
}

/// Errors of the batch source contract.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SourceError {
    #[error("this source is read-only")]
    NotWritable,
    #[error("backup copy failed, nothing was written: {reason}")]
    BackupFailed { reason: String },
    #[error("no item at origin {origin}")]
    UnknownOrigin { origin: OriginRef },
    #[error("no source kind named {kind}")]
    UnknownKind { kind: String },
    #[error("source kind {kind} is already registered")]
    DuplicateKind { kind: String },
    #[error("not a file location: {location}")]
    InvalidLocation { location: String },
    #[error("i/o failure on {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
