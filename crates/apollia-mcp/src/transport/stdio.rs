//! Stdio-based MCP transport: spawn a subprocess and communicate over its stdin/stdout.

use std::collections::HashMap;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use std::process::Stdio;

use super::{McpTransport, TransportError};

// ─── StdioTransport ──────────────────────────────────────────────────────────

/// MCP transport that communicates with a server subprocess over stdin/stdout.
///
/// Each call to [`send`] writes one newline-terminated JSON-RPC line to the
/// child's stdin. Each call to [`recv`] reads the next newline-terminated line
/// from the child's stdout.
///
/// The child process is killed on [`shutdown`]. Because `send` and `recv` use
/// separate mutexes (stdin vs. stdout), the two directions are fully independent
/// and can proceed concurrently.
pub struct StdioTransport {
    /// PID captured at spawn time; stable for the lifetime of the transport.
    pid: Option<u32>,
    /// Buffered writer protecting the child's stdin pipe.
    stdin: Mutex<BufWriter<ChildStdin>>,
    /// Line reader protecting the child's stdout pipe.
    stdout: Mutex<Lines<BufReader<ChildStdout>>>,
    /// Child process handle, kept for `shutdown`.
    child: Mutex<Child>,
}

impl StdioTransport {
    /// Spawn `command` with `args` and `envs`, returning a connected transport.
    ///
    /// Both stdin and stdout are piped. The child's stderr is inherited so that
    /// server-side error output is visible in the parent process logs.
    pub fn spawn(
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

        Ok(Self {
            pid,
            stdin: Mutex::new(BufWriter::new(raw_stdin)),
            stdout: Mutex::new(BufReader::new(raw_stdout).lines()),
            child: Mutex::new(child),
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
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn cat_transport() -> StdioTransport {
        StdioTransport::spawn("cat", &[], HashMap::new())
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
        let result = StdioTransport::spawn("nonexistent-binary-xyz-12345", &[], HashMap::new());
        // THEN spawn returns SpawnFailed
        assert!(matches!(result, Err(TransportError::SpawnFailed(_))));
    }
}
