//! The results of a batch written back to the table it read.
//!
//! The destination is fixed when the source is opened ([`OPTION_DESTINATION`])
//! and the batch source contract does the rest: [`SourceWriter::back_up`] runs
//! once before the first write, and every write goes through it.
//!
//! - By default ([`ResultsDestination::Copy`]), `back_up` creates the copy of
//!   results, `<name> (results YYYY-MM-DD).<ext>` next to the original, named
//!   in the language of the interface ([`OPTION_LANGUAGE`]), and answers its
//!   path. The original is read and never opened for writing.
//! - On explicit request ([`ResultsDestination::Original`]), and for `.xlsx`
//!   and `.xlsm` only, `back_up` takes the dated backup copy of the original,
//!   and the writes then go into the original itself.
//!
//! Each write rebuilds the file written from what was read before the first
//! one (the original for a copy, the backup for the original) plus every
//! value written so far, and replaces it in one rename, so that an
//! interrupted batch leaves a whole file. An `.xlsx` or `.xlsm` workbook keeps
//! everything but the cells added ([`ooxml`]); a CSV file is copied with its
//! columns added; a `.xls`, `.xlsb` or `.ods` workbook gives a new `.xlsx`
//! copy holding its values only ([`flat`]). Each column written is added after
//! the last column of the sheet, its name in the header row.

mod flat;
mod ooxml;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use apollia_core::batch_source::{
    dated_backup_path, OriginRef, SourceError, SourceWriter, WriteGuard,
};
use chrono::{DateTime, Local, NaiveDate, Utc};

use crate::reader::{read_csv, read_workbook};
use crate::table::TableFormat;

use flat::{CsvBase, ValuesBase};
use ooxml::OoxmlWorkbook;

/// Option choosing where the results go: [`ResultsDestination::COPY`] (the
/// default) or [`ResultsDestination::ORIGINAL`].
pub const OPTION_DESTINATION: &str = "destination";
/// Option giving the language of the interface, which names the copy of
/// results: `fr` or `en` (the default).
pub const OPTION_LANGUAGE: &str = "language";

/// The most characters a cell of a workbook holds.
const CELL_MAX_CHARS: usize = 32_767;
/// How many names of copy are tried before giving up, when the first ones
/// are taken by earlier runs of the same day.
const COPY_NAME_ATTEMPTS: u32 = 100;

/// Where the results of a batch are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResultsDestination {
    /// A copy of the table next to the original, which is never modified.
    Copy,
    /// The original itself, after a dated backup copy. `.xlsx` and `.xlsm`
    /// only.
    Original,
}

impl ResultsDestination {
    /// The option value naming [`ResultsDestination::Copy`].
    pub const COPY: &'static str = "copy";
    /// The option value naming [`ResultsDestination::Original`].
    pub const ORIGINAL: &'static str = "original";

    /// The destination an option value names; `None` for an unknown value.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            Self::COPY => Some(Self::Copy),
            Self::ORIGINAL => Some(Self::Original),
            _ => None,
        }
    }
}

/// The name of the copy of results of `original`, in `format`, dated `date`
/// and named in `language`: `clients (results 2026-10-04).xlsx`, or
/// `clients (résultats 2026-10-04).xlsx` in French. A `rank` above 1 tells it
/// from an earlier copy of the same day: `clients (results 2026-10-04) (2).xlsx`.
#[must_use]
pub fn results_copy_path(
    original: &Path,
    format: TableFormat,
    date: NaiveDate,
    language: &str,
    rank: u32,
) -> PathBuf {
    let stem = original
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let label = if language.trim().eq_ignore_ascii_case("fr") {
        "résultats"
    } else {
        "results"
    };
    let extension = if format.formatting_not_kept() {
        TableFormat::Xlsx.extension()
    } else {
        format.extension()
    };
    let date = date.format("%Y-%m-%d");
    let name = if rank > 1 {
        format!("{stem} ({label} {date}) ({rank}).{extension}")
    } else {
        format!("{stem} ({label} {date}).{extension}")
    };
    original.with_file_name(name)
}

/// Where the items of a table source are: its sheet, its header row and the
/// numbers of its data rows.
#[derive(Debug, Clone)]
pub(crate) struct TablePlace {
    pub(crate) sheet: String,
    pub(crate) header_row: Option<u32>,
    pub(crate) rows: BTreeSet<u32>,
}

/// The rows a table source may write to, and where.
#[derive(Debug, Clone)]
pub(crate) struct ResultsWriter {
    original: PathBuf,
    format: TableFormat,
    sheet: String,
    header_row: Option<u32>,
    rows: BTreeSet<u32>,
    destination: ResultsDestination,
    language: String,
    session: Option<Session>,
}

/// What a successful `back_up` set up for the writes that follow.
#[derive(Debug, Clone)]
struct Session {
    backup: PathBuf,
    target: PathBuf,
    base: Base,
    last_column: u32,
    columns: Vec<String>,
    values: BTreeMap<u32, BTreeMap<usize, String>>,
}

/// The file as read before the first write.
#[derive(Debug, Clone)]
enum Base {
    Ooxml(OoxmlWorkbook),
    Csv(CsvBase),
    Values(ValuesBase),
}

impl Base {
    fn last_column(&self) -> u32 {
        match self {
            Self::Ooxml(workbook) => workbook.last_column(),
            Self::Csv(csv) => csv.last_column(),
            Self::Values(values) => values.last_column(),
        }
    }

    fn write(&self, grid: &Grid) -> Result<Vec<u8>, String> {
        match self {
            Self::Ooxml(workbook) => workbook.write(grid),
            Self::Csv(csv) => csv.write(grid),
            Self::Values(values) => values.write(grid),
        }
    }
}

/// The cells to add, by row number, each row's cells by column number.
pub(crate) type Grid = BTreeMap<u32, Vec<(u32, String)>>;

impl ResultsWriter {
    /// The writer of the table at `original`, whose items are the rows of
    /// `place`, with the options of the source.
    pub(crate) fn new(
        original: &Path,
        format: TableFormat,
        place: TablePlace,
        options: &BTreeMap<String, String>,
    ) -> Result<Self, String> {
        let destination = match options.get(OPTION_DESTINATION) {
            None => ResultsDestination::Copy,
            Some(value) => ResultsDestination::parse(value).ok_or_else(|| {
                format!(
                    "the destination '{value}' is neither '{}' nor '{}'",
                    ResultsDestination::COPY,
                    ResultsDestination::ORIGINAL
                )
            })?,
        };
        if destination == ResultsDestination::Original && !format.can_modify_original() {
            return Err(format!(
                "results are written into the original of a .xlsx or .xlsm workbook only, never of a .{} file",
                format.extension()
            ));
        }
        Ok(Self {
            original: original.to_path_buf(),
            format,
            sheet: place.sheet,
            header_row: place.header_row,
            rows: place.rows,
            destination,
            language: options.get(OPTION_LANGUAGE).cloned().unwrap_or_default(),
            session: None,
        })
    }

    fn read_base(&self, bytes: Vec<u8>) -> Result<Base, SourceError> {
        let path = &self.original;
        match self.format {
            TableFormat::Xlsx | TableFormat::Xlsm => OoxmlWorkbook::prepare(bytes, &self.sheet)
                .map(Base::Ooxml)
                .map_err(|reason| invalid_data(path, reason)),
            TableFormat::Csv => read_csv(path)
                .map(|csv| Base::Csv(CsvBase::new(csv, &bytes)))
                .map_err(|error| invalid_data(path, error.to_string())),
            _ => {
                let sheets = read_workbook(path)
                    .map_err(|error| invalid_data(path, error.to_string()))?
                    .sheets;
                let target = sheets
                    .iter()
                    .position(|sheet| sheet.name == self.sheet)
                    .ok_or_else(|| {
                        invalid_data(path, format!("the file has no sheet '{}'", self.sheet))
                    })?;
                Ok(Base::Values(ValuesBase::new(sheets, target)))
            }
        }
    }

    fn grid(&self, session: &Session) -> Grid {
        let column = |index: usize| {
            session
                .last_column
                .saturating_add(u32::try_from(index).unwrap_or(u32::MAX))
                .saturating_add(1)
        };
        let mut grid = Grid::new();
        if let Some(header) = self.header_row {
            grid.insert(
                header,
                session
                    .columns
                    .iter()
                    .enumerate()
                    .map(|(index, name)| (column(index), name.clone()))
                    .collect(),
            );
        }
        for (row, values) in &session.values {
            grid.entry(*row).or_default().extend(
                values
                    .iter()
                    .map(|(index, value)| (column(*index), value.clone())),
            );
        }
        grid
    }

    fn row_of(&self, origin: &OriginRef) -> Option<u32> {
        let (sheet, row) = origin.as_str().rsplit_once('!')?;
        let row: u32 = row.parse().ok()?;
        (sheet == self.sheet && self.rows.contains(&row)).then_some(row)
    }
}

impl SourceWriter for ResultsWriter {
    /// For a copy, creates the copy of results next to the original and
    /// answers its path; for the original, takes its dated backup copy and
    /// answers the path of the backup.
    fn back_up(&mut self, taken_at: DateTime<Utc>) -> Result<PathBuf, SourceError> {
        self.session = None;
        let bytes = fs::read(&self.original).map_err(|source| SourceError::Io {
            path: self.original.clone(),
            source,
        })?;
        let base = self.read_base(bytes)?;
        let (backup, target) = match self.destination {
            ResultsDestination::Copy => {
                let date = taken_at.with_timezone(&Local).date_naive();
                let first = match &base {
                    Base::Ooxml(workbook) => workbook.bytes().to_vec(),
                    other => other
                        .write(&Grid::new())
                        .map_err(|reason| invalid_data(&self.original, reason))?,
                };
                let copy = self.create_copy(date, &first)?;
                (copy.clone(), copy)
            }
            ResultsDestination::Original => {
                let backup = dated_backup_path(&self.original, taken_at)?;
                let Base::Ooxml(workbook) = &base else {
                    return Err(SourceError::NotWritable);
                };
                create_new(&backup, workbook.bytes())?;
                (backup, self.original.clone())
            }
        };
        tracing::event!(
            tracing::Level::INFO,
            path = %backup.display(),
            destination = ?self.destination,
            "sources.table.results_opened"
        );
        self.session = Some(Session {
            last_column: base.last_column(),
            backup: backup.clone(),
            target,
            base,
            columns: Vec::new(),
            values: BTreeMap::new(),
        });
        Ok(backup)
    }

    fn write_value(
        &mut self,
        guard: &WriteGuard,
        origin: &OriginRef,
        column: &str,
        value: &str,
    ) -> Result<(), SourceError> {
        let foreign = || SourceError::ForeignBackup {
            path: guard.backup().path().to_path_buf(),
        };
        let session = self.session.as_ref().ok_or_else(foreign)?;
        if session.backup != guard.backup().path() {
            return Err(foreign());
        }
        let row = self
            .row_of(origin)
            .ok_or_else(|| SourceError::UnknownOrigin {
                origin: origin.clone(),
            })?;
        let session = self.session.as_mut().ok_or_else(foreign)?;
        let index = match session.columns.iter().position(|name| name == column) {
            Some(index) => index,
            None => {
                session.columns.push(column.to_string());
                session.columns.len() - 1
            }
        };
        session
            .values
            .entry(row)
            .or_default()
            .insert(index, value.to_string());
        let Some(session) = self.session.as_ref() else {
            return Err(foreign());
        };
        let grid = self.grid(session);
        let bytes = session
            .base
            .write(&grid)
            .map_err(|reason| invalid_data(&session.target, reason))?;
        replace(&session.target, &bytes)
    }
}

impl ResultsWriter {
    fn create_copy(&self, date: NaiveDate, bytes: &[u8]) -> Result<PathBuf, SourceError> {
        for rank in 1..=COPY_NAME_ATTEMPTS {
            let path = results_copy_path(&self.original, self.format, date, &self.language, rank);
            match create_new(&path, bytes) {
                Ok(()) => return Ok(path),
                Err(SourceError::Io { source, .. })
                    if source.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        Err(SourceError::Io {
            path: self.original.clone(),
            source: io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{COPY_NAME_ATTEMPTS} copies of results already exist for {date}"),
            ),
        })
    }
}

/// The text a cell receives: the characters XML refuses removed, and cut to
/// what a workbook cell holds.
pub(crate) fn cell_value(text: &str) -> String {
    text.chars()
        .filter(|c| {
            matches!(c, '\t' | '\n' | '\r')
                || (!c.is_control() && !matches!(c, '\u{FFFE}' | '\u{FFFF}'))
        })
        .take(CELL_MAX_CHARS)
        .collect()
}

/// Writes `bytes` to `path`, which must not exist yet, and flushes them to
/// the disk.
fn create_new(path: &Path, bytes: &[u8]) -> Result<(), SourceError> {
    let io_error = |source| SourceError::Io {
        path: path.to_path_buf(),
        source,
    };
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)?;
    file.write_all(bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)
}

/// Replaces the content of `path` with `bytes` in one rename, through a
/// temporary file in the same folder.
fn replace(path: &Path, bytes: &[u8]) -> Result<(), SourceError> {
    let io_error = |source| SourceError::Io {
        path: path.to_path_buf(),
        source,
    };
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temporary = path.with_file_name(format!(".{name}.{}.writing", std::process::id()));
    let written = File::create(&temporary)
        .and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        })
        .and_then(|()| fs::rename(&temporary, path));
    if let Err(source) = written {
        let _ = fs::remove_file(&temporary);
        return Err(io_error(source));
    }
    Ok(())
}

fn invalid_data(path: &Path, reason: String) -> SourceError {
    tracing::event!(
        tracing::Level::WARN,
        path = %path.display(),
        reason = %reason,
        "sources.table.results_failed"
    );
    SourceError::Io {
        path: path.to_path_buf(),
        source: io::Error::new(io::ErrorKind::InvalidData, reason),
    }
}
