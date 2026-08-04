//! Stdio-based MCP transport: spawn a subprocess and communicate over its stdin/stdout.

use std::collections::{HashMap, VecDeque};
use std::process::Stdio;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use super::{McpTransport, TransportError};

// ─── StdioTransport ──────────────────────────────────────────────────────────

/// Maximum number of stderr lines retained per spawned server. Stderr beyond
/// this rolling window is still emitted to tracing, just not kept in memory
/// for inclusion in error messages.
const STDERR_TAIL_CAPACITY: usize = 50;

/// MCP transport that communicates with a server subprocess over stdin/stdout.
///
/// Each call to [`send`](StdioTransport::send) writes one newline-terminated
/// JSON-RPC line to the child's stdin. Each call to
/// [`recv`](StdioTransport::recv) reads the next newline-terminated line from
/// the child's stdout.
///
/// The child's stderr is piped and drained by a background task that:
/// - tees every line into `tracing` (tagged with the server name) so spawn
///   failures, npm/uvx install logs, and runtime warnings appear in the
///   parent process logs;
/// - retains the last [`STDERR_TAIL_CAPACITY`] lines so the session layer
///   can include them in `TransportError` messages on handshake timeouts,
///   the single most useful piece of evidence when a subprocess starts but
///   never responds (PATH issue, wrong runtime, registry auth failure …).
///
/// The child process is killed on [`shutdown`](StdioTransport::shutdown).
/// Because `send` and `recv` use separate mutexes (stdin vs. stdout), the two
/// directions are fully independent and can proceed concurrently.
pub struct StdioTransport {
    /// PID captured at spawn time; stable for the lifetime of the transport.
    pid: Option<u32>,
    /// Buffered writer protecting the child's stdin pipe.
    stdin: Mutex<BufWriter<ChildStdin>>,
    /// Buffered reader protecting the child's stdout pipe. A single JSON-RPC
    /// line is read at a time by [`recv`](StdioTransport::recv), bounded by
    /// `max_response_bytes`.
    stdout: Mutex<BufReader<ChildStdout>>,
    /// Maximum bytes accepted for a single stdout line before the read aborts
    /// with [`TransportError::ResponseTooLarge`]. Bounds memory against a server
    /// that never emits a newline.
    max_response_bytes: u64,
    /// Child process handle, kept for `shutdown`.
    child: Mutex<Child>,
    /// Rolling window of the most recent stderr lines, populated by the
    /// background drainer task. Cloned into error messages by the session
    /// layer when a handshake or tool call fails.
    stderr_tail: Arc<std::sync::Mutex<VecDeque<String>>>,
}

impl StdioTransport {
    /// Spawn `command` with `args` and `envs`, returning a connected transport.
    ///
    /// `server_name` is used purely as a tracing tag so multi-server logs stay
    /// disambiguable; pass an empty string when no name is yet known.
    ///
    /// `max_response_bytes` caps a single stdout line; a line that grows past it
    /// (a server that never emits a newline) aborts with
    /// [`TransportError::ResponseTooLarge`] instead of exhausting memory.
    pub fn spawn(
        server_name: &str,
        command: &str,
        args: &[String],
        envs: HashMap<String, String>,
        max_response_bytes: u64,
    ) -> Result<Self, TransportError> {
        let mut command_builder = Command::new(command);
        command_builder.args(args);
        // Before the operator's own environment, so an explicit PYTHONHOME in
        // the server config still wins. What is stripped here is the ambient
        // one the desktop exports for its embedded interpreter, which would
        // otherwise make a Python MCP server load the wrong standard library.
        apollia_core::subprocess_env::scrub_bundled_python_async(&mut command_builder);
        // One console window per configured server otherwise, on Windows, for
        // the whole life of the session.
        apollia_core::subprocess_window::hide_console_async(&mut command_builder);
        let mut child = command_builder
            .envs(envs)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| TransportError::SpawnFailed(e.to_string()))?;

        // Both pipes are guaranteed by the Stdio::piped() calls above.
        let pid = child.id();
        let raw_stdin = child.stdin.take().expect("stdin was piped");
        let raw_stdout = child.stdout.take().expect("stdout was piped");
        let raw_stderr = child.stderr.take().expect("stderr was piped");

        let stderr_tail = Arc::new(std::sync::Mutex::new(VecDeque::with_capacity(
            STDERR_TAIL_CAPACITY,
        )));

        // Background drainer: required so that a chatty server cannot stall
        // by filling the OS pipe buffer (typically 64 KiB on Linux/macOS) and
        // never getting drained. We tee into tracing for observability and
        // into `stderr_tail` so the session layer can surface the last few
        // lines in error messages.
        {
            let tail = Arc::clone(&stderr_tail);
            let server = server_name.to_string();
            tokio::spawn(async move {
                let mut lines = BufReader::new(raw_stderr).lines();
                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => {
                            tracing::info!(
                                server = %server,
                                line = %line,
                                "mcp.stdio.stderr"
                            );
                            if let Ok(mut guard) = tail.lock() {
                                if guard.len() == STDERR_TAIL_CAPACITY {
                                    guard.pop_front();
                                }
                                guard.push_back(line);
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            tracing::warn!(
                                server = %server,
                                error = %e,
                                "mcp.stdio.stderr.read_failed"
                            );
                            break;
                        }
                    }
                }
            });
        }

        Ok(Self {
            pid,
            stdin: Mutex::new(BufWriter::new(raw_stdin)),
            stdout: Mutex::new(BufReader::new(raw_stdout)),
            max_response_bytes,
            child: Mutex::new(child),
            stderr_tail,
        })
    }
}

#[async_trait::async_trait]
impl McpTransport for StdioTransport {
    /// Write `message` followed by a newline to the child's stdin.
    async fn send(&self, message: &str) -> Result<(), TransportError> {
        let mut writer = self.stdin.lock().await;
        let line = format!("{message}\n");
        writer
            .write_all(line.as_bytes())
            .await
            .map_err(|e| TransportError::Io(e.to_string()))?;
        writer
            .flush()
            .await
            .map_err(|e| TransportError::Io(e.to_string()))
    }

    /// Read the next newline-terminated line from the child's stdout, bounded
    /// by `max_response_bytes`.
    ///
    /// Reads byte ranges from the buffered reader until a `\n` is found,
    /// returning the line without the terminator (a trailing `\r` is stripped,
    /// mirroring `tokio`'s `Lines`). Returns [`TransportError::Closed`] on EOF
    /// with no pending bytes, and [`TransportError::ResponseTooLarge`] as soon
    /// as the accumulated line exceeds the cap, so a server that never emits a
    /// newline cannot grow memory without bound.
    async fn recv(&self) -> Result<String, TransportError> {
        let mut reader = self.stdout.lock().await;
        let mut line: Vec<u8> = Vec::new();
        loop {
            let (consumed, done) = {
                let available = reader
                    .fill_buf()
                    .await
                    .map_err(|e| TransportError::Io(e.to_string()))?;
                if available.is_empty() {
                    // EOF: the stream ended before a newline arrived.
                    if line.is_empty() {
                        return Err(TransportError::Closed);
                    }
                    (0, true)
                } else if let Some(pos) = available.iter().position(|&b| b == b'\n') {
                    line.extend_from_slice(&available[..pos]);
                    (pos + 1, true)
                } else {
                    line.extend_from_slice(available);
                    (available.len(), false)
                }
            };
            reader.consume(consumed);
            if line.len() as u64 > self.max_response_bytes {
                return Err(TransportError::ResponseTooLarge {
                    limit: self.max_response_bytes,
                });
            }
            if done {
                break;
            }
        }
        // Mirror tokio `Lines::next_line`: a CRLF line drops the trailing '\r'.
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        String::from_utf8(line).map_err(|e| TransportError::Io(e.to_string()))
    }

    /// Kill the child process and wait for it to exit.
    async fn shutdown(&self) -> Result<(), TransportError> {
        let mut child = self.child.lock().await;
        let _ = child.kill().await;
        let _ = child.wait().await;
        Ok(())
    }

    /// Returns the OS process ID captured at spawn time.
    fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Returns the most recent stderr lines emitted by the subprocess, oldest
    /// first. Used by the session layer to enrich `TransportError` messages
    /// on handshake / tool-call failures.
    fn stderr_tail(&self) -> Vec<String> {
        self.stderr_tail
            .lock()
            .map(|g| g.iter().cloned().collect())
            .unwrap_or_default()
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Generous cap used by the round-trip tests where the payload is small.
    const TEST_CAP: u64 = 8 * 1024 * 1024;

    fn cat_transport() -> StdioTransport {
        StdioTransport::spawn("test", "cat", &[], HashMap::new(), TEST_CAP)
            .expect("cat must be available on the test system")
    }

    #[tokio::test]
    async fn test_stdio_transport_send_recv() {
        // GIVEN a StdioTransport backed by `cat` (echoes stdin to stdout)
        let transport = cat_transport();
        let message = r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#;

        // WHEN a message is sent and then received
        transport.send(message).await.expect("send must succeed");
        let received = transport.recv().await.expect("recv must succeed");

        // THEN the received line matches the sent message
        assert_eq!(received, message);

        transport.shutdown().await.expect("shutdown must succeed");
    }

    #[tokio::test]
    async fn test_stdio_transport_multiple_round_trips() {
        // GIVEN a StdioTransport backed by `cat`
        let transport = cat_transport();

        // WHEN two messages are sent and received in order
        transport.send(r#"{"id":1}"#).await.unwrap();
        transport.send(r#"{"id":2}"#).await.unwrap();

        let first = transport.recv().await.unwrap();
        let second = transport.recv().await.unwrap();

        // THEN messages arrive in the same order they were sent
        assert_eq!(first, r#"{"id":1}"#);
        assert_eq!(second, r#"{"id":2}"#);

        transport.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_stdio_transport_pid_is_some() {
        // GIVEN a spawned StdioTransport
        let transport = cat_transport();
        // THEN the PID is available
        assert!(transport.pid().is_some());
        transport.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_stdio_transport_recv_after_shutdown_returns_closed() {
        // GIVEN a StdioTransport that has been shut down
        let transport = cat_transport();
        transport.shutdown().await.unwrap();

        // WHEN recv is called after shutdown
        let result = transport.recv().await;

        // THEN the transport reports that the channel is closed
        assert!(matches!(
            result,
            Err(TransportError::Closed) | Err(TransportError::Io(_))
        ));
    }

    #[tokio::test]
    async fn test_spawn_invalid_command_returns_error() {
        // GIVEN a command that does not exist on this system
        let result = StdioTransport::spawn(
            "test",
            "nonexistent-binary-xyz-12345",
            &[],
            HashMap::new(),
            TEST_CAP,
        );
        // THEN spawn returns SpawnFailed
        assert!(matches!(result, Err(TransportError::SpawnFailed(_))));
    }

    #[tokio::test]
    async fn test_stdio_transport_stderr_tail_captures_subprocess_writes() {
        // GIVEN a subprocess that emits multiple lines on stderr
        let transport = StdioTransport::spawn(
            "test",
            "sh",
            &[
                "-c".to_string(),
                "echo first 1>&2; echo second 1>&2; cat".to_string(),
            ],
            HashMap::new(),
            TEST_CAP,
        )
        .expect("sh spawn must succeed");

        // Give the drainer task a moment to consume the stderr lines.
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        // THEN both lines are available via stderr_tail()
        let tail = transport.stderr_tail();
        assert!(
            tail.iter().any(|l| l == "first"),
            "expected 'first' in stderr tail, got {tail:?}"
        );
        assert!(
            tail.iter().any(|l| l == "second"),
            "expected 'second' in stderr tail, got {tail:?}"
        );

        transport.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_stdio_transport_rejects_oversized_line_without_newline() {
        // GIVEN a subprocess that streams 100_000 bytes with no newline, under a
        // 1 KiB cap (the DoS shape: a server that never terminates a line)
        let transport = StdioTransport::spawn(
            "test",
            "sh",
            &[
                "-c".to_string(),
                "head -c 100000 /dev/zero | tr '\\0' 'a'".to_string(),
            ],
            HashMap::new(),
            1024,
        )
        .expect("sh spawn must succeed");

        // WHEN the line is read
        let result = transport.recv().await;

        // THEN the read aborts with ResponseTooLarge rather than growing memory
        assert!(
            matches!(
                result,
                Err(TransportError::ResponseTooLarge { limit: 1024 })
            ),
            "expected ResponseTooLarge, got {result:?}"
        );

        transport.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_stdio_transport_legit_line_ok_under_small_cap() {
        // GIVEN a `cat` transport with a small (1 KiB) cap
        let transport = StdioTransport::spawn("test", "cat", &[], HashMap::new(), 1024)
            .expect("cat must be available on the test system");
        let message = r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#;

        // WHEN a message well under the cap is sent and received
        transport.send(message).await.expect("send must succeed");
        let received = transport.recv().await.expect("recv must succeed");

        // THEN the legitimate line round-trips unchanged
        assert_eq!(received, message);

        transport.shutdown().await.expect("shutdown must succeed");
    }
}
