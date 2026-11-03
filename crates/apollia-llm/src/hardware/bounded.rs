//! A hardware probe that cannot hang the caller.
//!
//! Every probe of this module used to run through `Command::output()`, which
//! waits for as long as the child does. A `nvidia-smi` stuck on a driver in a
//! bad state, or on a card handed to a virtual machine, then held `detect()`
//! and every API route that reads the hardware profile. [`output_within`]
//! waits a bounded time and kills the child past it.

use std::io::Read;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use tracing::{event, Level};

/// What one external probe may take before it is given up on.
pub const PROBE_LIMIT: Duration = Duration::from_secs(5);

/// How often the waiter of a running probe checks it for its exit, between
/// two looks at a request to kill it.
const POLL: Duration = Duration::from_millis(20);

/// What is left to collect the output of a child that has exited, when a
/// process it started keeps the pipes open after it.
const DRAIN_GRACE: Duration = Duration::from_millis(500);

/// Why a probe gave no output.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProbeRunError {
    /// The program could not be started: absent, or not executable.
    #[error("could not start {program}")]
    Spawn {
        /// The program asked for.
        program: String,
        /// The error of the operating system.
        #[source]
        source: std::io::Error,
    },
    /// The program did not finish within the limit and was killed.
    #[error("{program} did not answer within {limit_ms} ms")]
    TimedOut {
        /// The program asked for.
        program: String,
        /// The limit it was given, in milliseconds.
        limit_ms: u128,
    },
    /// The program could not be waited on.
    #[error("could not wait on {program}")]
    Wait {
        /// The program asked for.
        program: String,
        /// The error of the operating system.
        #[source]
        source: std::io::Error,
    },
}

impl ProbeRunError {
    /// Whether the program is simply not installed, which is the ordinary
    /// answer of a machine without the card the probe asks about.
    #[must_use]
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::Spawn { source, .. } if source.kind() == std::io::ErrorKind::NotFound)
    }
}

/// Read a pipe to its end on a thread of its own, so a child that fills one
/// pipe while the caller waits on it cannot block either side.
fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        let _ = tx.send(bytes);
    });
    rx
}

/// How the waiter of a child ended.
enum Waited {
    Exited(ExitStatus),
    Killed,
    Failed(std::io::Error),
}

/// Wait on `child` on a thread of its own, which owns it and kills it as
/// soon as a request comes through the returned sender, or when that sender
/// is dropped.
fn wait_on(mut child: Child) -> (mpsc::Sender<()>, mpsc::Receiver<Waited>) {
    let (kill_tx, kill_rx) = mpsc::channel::<()>();
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let waited = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Waited::Exited(status),
                Ok(None) => match kill_rx.recv_timeout(POLL) {
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break Waited::Killed;
                    }
                },
                Err(source) => {
                    let _ = child.kill();
                    break Waited::Failed(source);
                }
            }
        };
        let _ = done_tx.send(waited);
    });
    (kill_tx, done_rx)
}

/// Run `cmd` to completion and collect its output, or kill it once `limit`
/// has passed.
///
/// The caller configures the command, `hide_console` included; this function
/// sets the three standard streams.
pub(crate) fn output_within(cmd: &mut Command, limit: Duration) -> Result<Output, ProbeRunError> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|source| ProbeRunError::Spawn {
        program: program.clone(),
        source,
    })?;
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());

    let deadline = Instant::now() + limit;
    let timed_out = || {
        event!(
            Level::WARN,
            program = %program,
            limit_ms = limit.as_millis(),
            "hardware.probe_timed_out"
        );
        ProbeRunError::TimedOut {
            program: program.clone(),
            limit_ms: limit.as_millis(),
        }
    };
    let (kill, done) = wait_on(child);
    let status = match done.recv_timeout(limit) {
        Ok(Waited::Exited(status)) => status,
        Ok(Waited::Failed(source)) => {
            return Err(ProbeRunError::Wait {
                program: program.clone(),
                source,
            })
        }
        Ok(Waited::Killed) | Err(_) => {
            let _ = kill.send(());
            let _ = done.recv_timeout(DRAIN_GRACE);
            return Err(timed_out());
        }
    };

    let collect = |rx: mpsc::Receiver<Vec<u8>>| {
        let left = deadline.saturating_duration_since(Instant::now());
        rx.recv_timeout(left.max(DRAIN_GRACE)).ok()
    };
    let (Some(stdout), Some(stderr)) = (collect(stdout), collect(stderr)) else {
        return Err(timed_out());
    };
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

#[cfg(all(test, unix))]
pub(crate) mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    /// The limit of a test whose fake tool is expected to answer. macOS can
    /// hold the first launch of a freshly written executable for seconds while
    /// it assesses it, so such a test does not race the five seconds of
    /// [`PROBE_LIMIT`]; only the tests of a hang use a short limit.
    pub(crate) const ANSWER_LIMIT: Duration = Duration::from_secs(60);

    /// A shell script written into `dir` under `name`, executable.
    pub(crate) fn fake_tool(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write the fake tool");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("make the fake tool executable");
        path
    }

    #[test]
    fn a_probe_that_answers_gives_its_output() {
        // GIVEN a tool that prints a line and exits
        let dir = tempfile::tempdir().expect("tempdir");
        let tool = fake_tool(dir.path(), "tool", "echo hello");

        // WHEN it is run within a generous limit
        let out = output_within(&mut Command::new(&tool), ANSWER_LIMIT).expect("it answers");

        // THEN its output and its status come back
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hello");
    }

    #[test]
    fn a_probe_that_hangs_is_killed_at_the_limit() {
        // GIVEN a tool that sleeps far longer than the limit
        let dir = tempfile::tempdir().expect("tempdir");
        let tool = fake_tool(dir.path(), "tool", "exec sleep 30");
        let started = Instant::now();

        // WHEN it is run with a limit of a fraction of a second
        let result = output_within(&mut Command::new(&tool), Duration::from_millis(300));

        // THEN the call returns a timeout, long before the tool would have ended
        assert!(matches!(result, Err(ProbeRunError::TimedOut { .. })));
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn an_absent_program_reads_as_not_found() {
        // GIVEN a path where no program is
        let dir = tempfile::tempdir().expect("tempdir");

        // WHEN it is run
        let result = output_within(&mut Command::new(dir.path().join("absent")), PROBE_LIMIT);

        // THEN the error says the program is not installed, not that it failed
        assert!(result.is_err_and(|e| e.is_not_found()));
    }
}
