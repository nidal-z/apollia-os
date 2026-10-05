//! The daily export the organisation policy asks for: one JSON file per day,
//! in summary, with the anchor of the chain, in a folder of this machine.
//!
//! A file holds what the lines of one UTC day say (who did what to what, with
//! which result and when) and the place of each entry in its chain. It never
//! holds content. Beside the entries it carries the head anchor of the global
//! chain at the time the file was written: kept out of the application's
//! folder, that anchor is what shows a journal cut at its end.
//!
//! The export catches up: every day since the last file that was written, up
//! to the day before `today`, gets its file. A day without an entry gets none.
//! A file that exists is never rewritten.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Days, NaiveDate, Utc};
use serde::Serialize;

use crate::audit_journal::export::ExportedEntry;
use crate::audit_journal::filter::JournalFilter;
use crate::audit_journal::verify::JournalAnchor;
use crate::audit_journal::AuditJournalHandle;

/// Prefix of the name of a daily file, before its day.
const FILE_PREFIX: &str = "journal-";

/// Extension of a daily file, with its dot.
const FILE_SUFFIX: &str = ".json";

/// Why the daily export could not be written.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DailyExportError {
    /// The folder the policy designates is not an absolute path.
    #[error("the export folder '{0}' is not an absolute path")]
    NotAbsolute(String),
    /// The folder cannot be created, listed or written to.
    #[error("the export folder '{directory}' cannot be used: {source}")]
    Directory {
        /// The folder the policy designates.
        directory: String,
        /// What the system answered.
        #[source]
        source: std::io::Error,
    },
    /// The JSON document could not be written.
    #[error("the JSON document could not be written: {0}")]
    Json(#[from] serde_json::Error),
}

/// One daily file that was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyExport {
    /// The UTC day the file holds.
    pub day: NaiveDate,
    /// Where it was written.
    pub path: PathBuf,
    /// How many entries it holds.
    pub entries: u64,
}

/// The JSON document of a daily file.
#[derive(Debug, Serialize)]
struct DailyDocument<'a> {
    /// The UTC day the entries were written on.
    day: String,
    /// Always `false`: a daily file is a summary.
    detail: bool,
    /// Number of entries.
    count: usize,
    /// Head anchor of the global chain when the file was written.
    anchor: Option<&'a JournalAnchor>,
    /// The entries of the day, newest first.
    entries: &'a [ExportedEntry],
}

/// The name of the file of `day`.
#[must_use]
pub fn daily_file_name(day: NaiveDate) -> String {
    format!("{FILE_PREFIX}{}{FILE_SUFFIX}", day.format("%Y-%m-%d"))
}

/// The day a file name stands for, `None` for any other file.
fn day_of(name: &str) -> Option<NaiveDate> {
    let day = name.strip_prefix(FILE_PREFIX)?.strip_suffix(FILE_SUFFIX)?;
    NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()
}

fn start_of(day: NaiveDate) -> Option<DateTime<Utc>> {
    day.and_hms_opt(0, 0, 0).map(|start| start.and_utc())
}

/// Writes, in `dir`, the file of every day that has entries and no file yet,
/// from the day after the last file written up to the day before `today`.
/// Returns the files written, oldest day first.
///
/// `today` is given by the caller, so the catch-up of a day that has passed is
/// tested without waiting for it.
pub async fn export_missing_days(
    journal: &AuditJournalHandle,
    dir: &Path,
    today: NaiveDate,
) -> Result<Vec<DailyExport>, DailyExportError> {
    if !dir.is_absolute() {
        return Err(DailyExportError::NotAbsolute(dir.display().to_string()));
    }
    let unusable = |source: std::io::Error| DailyExportError::Directory {
        directory: dir.display().to_string(),
        source,
    };
    tokio::fs::create_dir_all(dir).await.map_err(unusable)?;

    let mut last_written: Option<NaiveDate> = None;
    let mut listing = tokio::fs::read_dir(dir).await.map_err(unusable)?;
    while let Some(file) = listing.next_entry().await.map_err(unusable)? {
        if let Some(day) = file.file_name().to_str().and_then(day_of) {
            last_written = last_written.max(Some(day));
        }
    }

    let since = last_written
        .and_then(|day| day.checked_add_days(Days::new(1)))
        .and_then(start_of);
    let Some(until) = start_of(today) else {
        return Ok(Vec::new());
    };
    if since.is_some_and(|since| since >= until) {
        return Ok(Vec::new());
    }

    let filter = JournalFilter {
        since,
        until: Some(until),
        ..JournalFilter::default()
    };
    let lines = journal.query_lines(filter, usize::MAX, 0).await;
    let mut days: BTreeMap<NaiveDate, Vec<ExportedEntry>> = BTreeMap::new();
    for line in &lines {
        let Ok(at) = DateTime::parse_from_rfc3339(&line.entry.ts) else {
            continue;
        };
        days.entry(at.with_timezone(&Utc).date_naive())
            .or_default()
            .push(ExportedEntry::summary_of(line));
    }
    if days.is_empty() {
        return Ok(Vec::new());
    }

    let anchor = journal.journal_anchor().await.ok().flatten();
    let mut written = Vec::with_capacity(days.len());
    for (day, entries) in days {
        let path = dir.join(daily_file_name(day));
        let bytes = serde_json::to_vec_pretty(&DailyDocument {
            day: day.format("%Y-%m-%d").to_string(),
            detail: false,
            count: entries.len(),
            anchor: anchor.as_ref(),
            entries: &entries,
        })?;
        // Written beside its place then moved, so a reader never opens half
        // a file and a file that exists is whole.
        let partial = path.with_extension("json.partial");
        tokio::fs::write(&partial, &bytes).await.map_err(unusable)?;
        tokio::fs::rename(&partial, &path).await.map_err(unusable)?;
        written.push(DailyExport {
            day,
            path,
            entries: entries.len() as u64,
        });
    }
    Ok(written)
}

#[cfg(test)]
mod tests;
