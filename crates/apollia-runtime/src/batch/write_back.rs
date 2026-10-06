//! The answers of a batch written back into the source it read.
//!
//! A launch asks for it with [`SourceWrite`]: the columns to add, one per
//! field of the answer the agent gives, and the destination. Once the batch
//! has ended, the source is opened again with the same descriptor, its
//! backup is taken through the batch source contract, and the answer of each
//! completed item fills its row. Nothing is written for a failed, canceled or
//! waiting item.
//!
//! The destination is an option of the source, read when it is opened
//! ([`OPTION_DESTINATION`]), so it is fixed at launch:
//!
//! - [`WriteDestination::Copy`], the default: the backup step creates a copy
//!   of the file next to it and the writes go there; the original is never
//!   opened for writing. No grant is needed.
//! - [`WriteDestination::Original`]: the file itself, after a dated backup
//!   copy. The launch must grant [`SOURCE_WRITE_ACCESS`]; governance decides
//!   it and the journal records it. A batch whose grant is missing writes
//!   nothing.
//!
//! An answer that is a JSON object (bare, or in a fenced code block) gives
//! one cell per column it names. Any other answer, or an object that names
//! none of the columns, is written whole in the first column.

use std::collections::BTreeMap;
use std::path::PathBuf;

use apollia_core::batch_source::{
    OriginRef, SourceError, SourceRegistry, SourceSpec, WriteSession,
};
use apollia_tools::BatchItemRow;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tracing::Level;

use super::{BatchError, SourceDescriptor};

/// The access a launch grants for the results to go into the original file.
pub const SOURCE_WRITE_ACCESS: &str = "source:write";

/// The option a source reads its destination from when it is opened.
pub const OPTION_DESTINATION: &str = apollia_sources::OPTION_DESTINATION;

/// The most columns one launch adds to its source.
pub const MAX_COLUMNS: usize = 32;

/// Where the results of a batch are written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum WriteDestination {
    /// A copy of the file next to it; the original is never opened for
    /// writing.
    #[default]
    Copy,
    /// The original file, after a dated backup copy. Needs the grant
    /// `source:write` at launch.
    Original,
}

impl WriteDestination {
    /// The value of the destination option of the source.
    #[must_use]
    pub fn option_value(self) -> &'static str {
        match self {
            Self::Copy => apollia_sources::ResultsDestination::COPY,
            Self::Original => apollia_sources::ResultsDestination::ORIGINAL,
        }
    }
}

/// What a launch writes back into its source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SourceWrite {
    /// `copy` when absent.
    #[serde(default)]
    pub destination: WriteDestination,
    /// The columns added, one per field of the answer, in order. An answer
    /// that is not an object of these fields is written whole in the first.
    pub columns: Vec<String>,
}

impl SourceWrite {
    /// Refuses no column, too many, a blank one, or the same one twice.
    ///
    /// # Errors
    ///
    /// [`BatchError::Invalid`] with the reason.
    pub fn check(&self) -> Result<(), BatchError> {
        if self.columns.is_empty() {
            return Err(BatchError::Invalid(
                "writing the results needs at least one column".into(),
            ));
        }
        if self.columns.len() > MAX_COLUMNS {
            return Err(BatchError::Invalid(format!(
                "writing the results adds at most {MAX_COLUMNS} columns, got {}",
                self.columns.len()
            )));
        }
        let mut seen: Vec<&str> = Vec::with_capacity(self.columns.len());
        for column in &self.columns {
            let name = column.trim();
            if name.is_empty() {
                return Err(BatchError::Invalid(
                    "a column of results has no name".into(),
                ));
            }
            if seen.iter().any(|s| s.eq_ignore_ascii_case(name)) {
                return Err(BatchError::Invalid(format!(
                    "the column of results '{name}' is named twice"
                )));
            }
            seen.push(name);
        }
        Ok(())
    }
}

/// Why the results of a batch were not written.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WriteBackError {
    /// The original was asked for without the grant that allows it.
    #[error("writing into the original file was not granted for this batch")]
    NotGranted,
    /// The source refused: it cannot write, or its backup failed.
    #[error(transparent)]
    Source(#[from] SourceError),
}

/// What was written back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    /// The file the results went into: the copy, or the backup of the
    /// original when the original was written.
    pub backup: PathBuf,
    /// The items whose answer was written.
    pub items: u64,
    /// The items whose row the source refused.
    pub refused: u64,
}

/// The spec a source is opened with: its options, and the destination when
/// the launch writes back into it.
#[must_use]
pub fn source_spec(descriptor: &SourceDescriptor) -> SourceSpec {
    let mut options: BTreeMap<String, String> = descriptor.options.clone();
    if let Some(write) = &descriptor.write {
        options.insert(
            OPTION_DESTINATION.to_owned(),
            write.destination.option_value().to_owned(),
        );
    }
    SourceSpec {
        location: descriptor.location.clone(),
        options,
    }
}

/// The cells the answer `output` fills, as `(column, value)`.
#[must_use]
pub fn answer_cells(columns: &[String], output: &str) -> Vec<(String, String)> {
    let Some(first) = columns.first() else {
        return Vec::new();
    };
    if let Some(fields) = answer_fields(output) {
        let found: Vec<(String, String)> = columns
            .iter()
            .filter_map(|column| {
                field(&fields, column).map(|value| (column.clone(), cell_text(value)))
            })
            .collect();
        if !found.is_empty() {
            return found;
        }
    }
    vec![(first.clone(), output.trim().to_owned())]
}

fn field<'a>(fields: &'a Map<String, Value>, column: &str) -> Option<&'a Value> {
    fields.get(column).or_else(|| {
        fields
            .iter()
            .find(|(name, _)| name.trim().eq_ignore_ascii_case(column.trim()))
            .map(|(_, value)| value)
    })
}

fn answer_fields(output: &str) -> Option<Map<String, Value>> {
    match serde_json::from_str::<Value>(unfenced(output.trim())) {
        Ok(Value::Object(fields)) => Some(fields),
        _ => None,
    }
}

/// The body of a fenced code block, or `text` when it is not one.
fn unfenced(text: &str) -> &str {
    let Some(rest) = text.strip_prefix("```") else {
        return text;
    };
    let body = rest
        .split_once('\n')
        .map_or("", |(_, body)| body)
        .trim_end();
    body.strip_suffix("```").unwrap_or(body).trim()
}

fn cell_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Writes the answers of the completed `items` back into the source of
/// `descriptor`, opened again from `sources`. `original_granted` says whether
/// the launch granted [`SOURCE_WRITE_ACCESS`]. Blocking: it reads and writes
/// files.
///
/// Returns `None` when the launch asked for no write, or when no item
/// completed: no copy is made then.
///
/// # Errors
///
/// [`WriteBackError::NotGranted`] for the original without its grant,
/// [`WriteBackError::Source`] when the source cannot be opened, cannot write,
/// or its backup fails; nothing is written in these cases.
pub fn write_results(
    sources: &SourceRegistry,
    descriptor: &SourceDescriptor,
    original_granted: bool,
    items: &[BatchItemRow],
) -> Result<Option<Written>, WriteBackError> {
    let Some(write) = &descriptor.write else {
        return Ok(None);
    };
    if write.destination == WriteDestination::Original && !original_granted {
        return Err(WriteBackError::NotGranted);
    }
    let completed: Vec<&BatchItemRow> = items
        .iter()
        .filter(|item| item.status == "completed")
        .collect();
    if completed.is_empty() {
        return Ok(None);
    }
    let mut source = sources.open(&descriptor.kind, &source_spec(descriptor))?;
    let mut session = WriteSession::open(source.as_mut(), chrono::Utc::now())?;
    let mut written = 0u64;
    let mut refused = 0u64;
    for item in completed {
        let origin = OriginRef::new(item.origin.as_str());
        let output = item.output.as_deref().unwrap_or_default();
        let outcome = answer_cells(&write.columns, output)
            .iter()
            .try_for_each(|(column, value)| session.write(&origin, column, value));
        match outcome {
            Ok(()) => written += 1,
            Err(error) => {
                refused += 1;
                tracing::event!(
                    Level::WARN,
                    rank = item.rank,
                    error = %error,
                    "batch.results.row_refused"
                );
            }
        }
    }
    Ok(Some(Written {
        backup: session.backup().path().to_path_buf(),
        items: written,
        refused,
    }))
}
