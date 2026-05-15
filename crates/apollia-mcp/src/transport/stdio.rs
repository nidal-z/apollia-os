//! Stdio-based MCP transport: spawn a subprocess and communicate over its stdin/stdout.

use std::collections::{HashMap, VecDeque};
use std::process::Stdio;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter, Lines};
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
///   can include them in `TransportError` messages on handshake timeouts —
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
    /// Line reader protecting the child's stdout pipe.
    stdout: Mutex<Lines<BufReader<ChildStdout>>>,
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
    pub fn spawn(
        server_name: &str,
        command: &str,
        args: &[String],
        envs: HashMap<String, String>,
    ) -> Result<Self, TransportError> {
        let mut child = Command::new(command)
            .args(args)
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

        // Background drainer — required so that a chatty server cannot stall
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
            stdout: Mutex::new(BufReader::new(raw_stdout).lines()),
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

    /// Read the next newline-terminated line from the child's stdout.
    async fn recv(&self) -> Result<String, TransportError> {
        self.stdout
            .lock()
            .await
            .next_line()
            .await
            .map_err(|e| TransportError::Io(e.to_string()))?
            .ok_or(TransportError::Closed)
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

    fn cat_transport() -> StdioTransport {
        StdioTransport::spawn("test", "cat", &[], HashMap::new())
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
        let result =
            StdioTransport::spawn("test", "nonexistent-binary-xyz-12345", &[], HashMap::new());
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
}
