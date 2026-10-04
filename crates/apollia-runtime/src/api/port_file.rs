//! The port a daemon listens on, recorded in the data folder.
//!
//! `apollia-os start` resolves its TCP port from `--port`, then `[api].port`,
//! then the default. The desktop application only knows the file, so a daemon
//! started with `--port` would be invisible to it. The daemon writes the port
//! it bound to `<data_dir>/api-port` once it serves, and removes the file when
//! it stops; the application reads it first and falls back to `[api].port`.
//!
//! A file left by a daemon that died is harmless: the reader checks that
//! something listens on the recorded port before using it.

use std::path::{Path, PathBuf};

const PORT_FILE: &str = "api-port";

fn port_file(data_dir: &Path) -> PathBuf {
    data_dir.join(PORT_FILE)
}

/// Records `port` as the port the daemon serves.
///
/// # Errors
///
/// The data folder cannot be created or the file cannot be written.
pub fn write_port_file(data_dir: &Path, port: u16) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    std::fs::write(port_file(data_dir), format!("{port}\n"))
}

/// The port a daemon recorded, or `None` when no file exists or it does not
/// hold a port.
pub fn read_port_file(data_dir: &Path) -> Option<u16> {
    std::fs::read_to_string(port_file(data_dir))
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// Removes the record of the daemon's port. A missing file is not an error.
pub fn remove_port_file(data_dir: &Path) {
    match std::fs::remove_file(port_file(data_dir)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::event!(
            tracing::Level::WARN,
            error = %e,
            "api.port_file.remove_failed"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_written_port_reads_back_and_is_gone_once_removed() {
        // GIVEN an empty data folder
        let dir = tempfile::tempdir().unwrap();

        // WHEN the daemon records port 7781
        write_port_file(dir.path(), 7781).unwrap();
        let recorded = read_port_file(dir.path());
        remove_port_file(dir.path());

        // THEN the port reads back, and nothing is left after removal
        assert_eq!(recorded, Some(7781));
        assert_eq!(read_port_file(dir.path()), None);
    }

    #[test]
    fn test_a_file_that_holds_no_port_reads_as_none() {
        // GIVEN a record that is not a port
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(PORT_FILE), "70000\n").unwrap();

        // WHEN it is read
        let recorded = read_port_file(dir.path());

        // THEN it is ignored
        assert_eq!(recorded, None);
    }
}
