//! The log file of the desktop application, under `~/.apollia/logs/`.
//!
//! The application writes its traces to the terminal and to `desktop.log`,
//! rotated by size: when a write would take the file past
//! [`MAX_FILE_BYTES`], the file becomes `desktop.log.1`, the older ones shift
//! by one, and the one past [`KEPT_ROTATIONS`] is deleted. The folder thus
//! holds at most `KEPT_ROTATIONS + 1` files of bounded size.
//!
//! "Clear the logs" removes the whole folder while the application runs. The
//! writer notices that its file is gone at the next write and opens a new one,
//! so the folder fills again instead of the traces going to an unlinked file.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Name of the live log file in the logs folder.
pub(crate) const LOG_FILE_NAME: &str = "desktop.log";

/// Size past which the live file is rotated, 10 MiB.
pub(crate) const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;

/// Rotated files kept beside the live one.
pub(crate) const KEPT_ROTATIONS: usize = 4;

/// The logs folder under a data directory.
pub(crate) fn logs_dir_under(data_dir: &Path) -> PathBuf {
    data_dir.join("logs")
}

/// A log file rotated by size, the writer of the file layer.
#[derive(Debug)]
pub(crate) struct RotatingFile {
    dir: PathBuf,
    max_bytes: u64,
    kept: usize,
    file: File,
    written: u64,
}

impl RotatingFile {
    /// Opens (or creates) `desktop.log` in `dir`, appending to it.
    ///
    /// # Errors
    ///
    /// The folder cannot be created or the file cannot be opened.
    pub(crate) fn open(dir: &Path) -> io::Result<Self> {
        Self::with_bounds(dir, MAX_FILE_BYTES, KEPT_ROTATIONS)
    }

    /// Same as [`open`](Self::open) with explicit bounds.
    pub(crate) fn with_bounds(dir: &Path, max_bytes: u64, kept: usize) -> io::Result<Self> {
        let (file, written) = open_live(dir)?;
        Ok(Self {
            dir: dir.to_path_buf(),
            max_bytes,
            kept,
            file,
            written,
        })
    }

    fn live_path(&self) -> PathBuf {
        self.dir.join(LOG_FILE_NAME)
    }

    fn rotated_path(&self, index: usize) -> PathBuf {
        self.dir.join(format!("{LOG_FILE_NAME}.{index}"))
    }

    /// Shifts every file by one, drops the one past the bound, and opens a
    /// fresh live file.
    fn rotate(&mut self) -> io::Result<()> {
        self.file.flush()?;
        if self.kept == 0 {
            remove_if_present(&self.live_path())?;
        } else {
            remove_if_present(&self.rotated_path(self.kept))?;
            for index in (1..self.kept).rev() {
                rename_if_present(&self.rotated_path(index), &self.rotated_path(index + 1))?;
            }
            rename_if_present(&self.live_path(), &self.rotated_path(1))?;
        }
        self.reopen()
    }

    fn reopen(&mut self) -> io::Result<()> {
        let (file, written) = open_live(&self.dir)?;
        self.file = file;
        self.written = written;
        Ok(())
    }
}

impl Write for RotatingFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if !self.live_path().exists() {
            self.reopen()?;
        }
        let incoming = buf.len() as u64;
        if self.written > 0 && self.written.saturating_add(incoming) > self.max_bytes {
            self.rotate()?;
        }
        let n = self.file.write(buf)?;
        self.written = self.written.saturating_add(n as u64);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// Opens the live file of `dir` for appending, creating the folder, and
/// answers its current size.
fn open_live(dir: &Path) -> io::Result<(File, u64)> {
    std::fs::create_dir_all(dir)?;
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(LOG_FILE_NAME))?;
    let written = file.metadata().map(|m| m.len()).unwrap_or(0);
    Ok((file, written))
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

fn rename_if_present(from: &Path, to: &Path) -> io::Result<()> {
    match std::fs::rename(from, to) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort_unstable();
        names
    }

    #[test]
    fn test_a_file_at_the_threshold_rotates_and_the_oldest_past_the_bound_is_deleted() {
        // GIVEN a logs folder holding a live file at the threshold and the
        // two rotated files the bound allows, the oldest marked
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("logs");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("desktop.log"), b"0123456789").unwrap();
        std::fs::write(dir.join("desktop.log.1"), b"newer").unwrap();
        std::fs::write(dir.join("desktop.log.2"), b"oldest").unwrap();
        let mut log = RotatingFile::with_bounds(&dir, 10, 2).unwrap();

        // WHEN one more line is written
        log.write_all(b"next line\n").unwrap();
        log.flush().unwrap();

        // THEN the live file holds the new line alone, each older file moved
        // by one, and the oldest is gone: still three files
        assert_eq!(
            names_in(&dir),
            vec!["desktop.log", "desktop.log.1", "desktop.log.2"]
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("desktop.log")).unwrap(),
            "next line\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("desktop.log.1")).unwrap(),
            "0123456789"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("desktop.log.2")).unwrap(),
            "newer"
        );
    }

    #[test]
    fn test_writes_under_the_threshold_stay_in_one_file() {
        // GIVEN an empty logs folder and a generous bound
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("logs");
        let mut log = RotatingFile::with_bounds(&dir, 1024, 2).unwrap();

        // WHEN two lines are written
        log.write_all(b"first\n").unwrap();
        log.write_all(b"second\n").unwrap();
        log.flush().unwrap();

        // THEN the folder holds the live file alone, with both lines
        assert_eq!(names_in(&dir), vec!["desktop.log"]);
        assert_eq!(
            std::fs::read_to_string(dir.join("desktop.log")).unwrap(),
            "first\nsecond\n"
        );
    }

    #[tokio::test]
    async fn test_clearing_the_logs_empties_the_folder_the_application_fills() {
        // GIVEN a data directory whose logs folder the application wrote to
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join(".apollia");
        let dir = logs_dir_under(&data_dir);
        let mut log = RotatingFile::open(&dir).unwrap();
        log.write_all(b"desktop.started\n").unwrap();
        log.flush().unwrap();
        assert_eq!(names_in(&dir), vec!["desktop.log"]);

        // WHEN the logs are cleared, as the danger zone does
        let removed = apollia_runtime::app_lifecycle::clear_logs(&data_dir)
            .await
            .unwrap();

        // THEN no log file is left
        assert!(removed);
        assert!(names_in(&dir).is_empty());

        // AND the next trace opens a new file rather than going nowhere
        log.write_all(b"after the clear\n").unwrap();
        log.flush().unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("desktop.log")).unwrap(),
            "after the clear\n"
        );
    }
}
