//! Announce the chosen port to the parent (daemon) via stdout.
//!
//! The runner binds to `127.0.0.1:0` (port chosen by the OS), then writes
//! `READY <port>\n` to stdout. The daemon parses this line with a 10 second
//! timeout in `RunnerSupervisor`.
//!
//! Stdout is dedicated to this channel only. All application logs go to
//! stderr (see [observability::logs]).

use std::io::{self, Write};

/// Emits `READY <port>\n` to standard stdout, then flushes.
///
/// Returns `Ok(())` if the write and flush succeeded. On failure, the daemon
/// hits its `RUNNER_HANDSHAKE_TIMEOUT` and kills the runner.
pub fn announce(port: u16) -> io::Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    announce_to(&mut handle, port)
}

/// Testable variant: writes to an arbitrary writer.
///
/// Used by unit tests with a `Vec<u8>` mock.
pub fn announce_to<W: Write>(writer: &mut W, port: u16) -> io::Result<()> {
    writeln!(writer, "READY {port}")?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn announce_writes_correct_format() {
        let mut buf = Vec::new();
        announce_to(&mut buf, 38492).unwrap();
        assert_eq!(buf, b"READY 38492\n");
    }

    #[test]
    fn announce_handles_max_port() {
        let mut buf = Vec::new();
        announce_to(&mut buf, u16::MAX).unwrap();
        assert_eq!(buf, b"READY 65535\n");
    }
}
