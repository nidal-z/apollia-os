//! Persistent login-shell session.
//!
//! One `/bin/sh -l` process per [`SessionId`]. Commands run directly in the parent
//! shell so that `cd` and `export` persist across successive [`ShellSession::exec`]
//! calls. On timeout, cancellation, or an `exit` command the underlying process is
//! transparently restarted at the last-known working directory.

use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Unique identifier for a shell session — one UUID per agent.
pub type SessionId = Uuid;

/// Output produced by a single command executed inside a [`ShellSession`].
#[derive(Debug, Clone)]
pub struct ShellOutput {
    /// Lines written to stdout by the command, joined with `\n`.
    pub stdout: String,
    /// Lines written to stderr since the last command, joined with `\n`.
    pub stderr: String,
    /// Exit code reported by the command. `-1` if the process was signalled.
    pub exit_code: i32,
    /// Working directory of the shell after the command completed.
    pub cwd: String,
}

/// Errors produced by [`ShellSession::exec`].
#[derive(Debug, Error)]
pub enum PersistentBashError {
    /// `bash -n -c` rejected the command's syntax.
    #[error("syntax error: {0}")]
    SyntaxError(String),

    /// The command exceeded the caller-supplied timeout.
    #[error("command timed out")]
    Timeout,

    /// The caller's [`CancellationToken`] was triggered before the command finished.
    #[error("command was cancelled")]
    Cancelled,

    /// `/bin/sh -l` could not be spawned.
    #[error("failed to spawn shell: {0}")]
    SpawnFailed(String),

    /// An I/O error occurred on the shell's stdin/stdout pipes.
    #[error("I/O error: {0}")]
    Io(String),

    /// The `bash -n` syntax-check subprocess did not respond within 1 second.
    #[error("syntax check timed out")]
    SyntaxCheckTimeout,
}

// ---------------------------------------------------------------------------
// Internal result types
// ---------------------------------------------------------------------------

/// Outcome of a single [`read_with_timeout`] drive.
enum ReadOutcome {
    Done(String, String, i32, String),
    ShellExited,
    Timeout,
    Cancelled,
    Io(String),
}

/// Low-level error from [`do_read_until_marker`].
enum ReadIoError {
    /// Stdout hit EOF before the marker (the shell exited).
    Eof,
    Io(String),
}

// ---------------------------------------------------------------------------
// ShellSession
// ---------------------------------------------------------------------------

/// Persistent login shell — one `/bin/sh -l` process kept alive between steps.
///
/// Commands are sent via stdin and output is read until a unique UUID marker
/// appears on stdout. The session is transparently restarted at the last-known
/// working directory if a timeout, cancellation, or `exit` command kills the
/// underlying process.
pub struct ShellSession {
    /// Identifier of this session; remains stable across transparent restarts.
    pub session_id: SessionId,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    stderr_buf: Arc<Mutex<String>>,
    stderr_drainer: JoinHandle<()>,
    /// Last confirmed working directory — used to restart the shell at the right place.
    cwd: String,
    /// Set when a previous exec left the shell in an undefined state.
    /// Cleared by the restart at the start of the next [`exec`] call.
    needs_restart: bool,
}

impl ShellSession {
    /// Spawns a new `/bin/sh -l` session rooted at `cwd`.
    ///
    /// # Errors
    ///
    /// Returns [`PersistentBashError::SpawnFailed`] if the shell process cannot be started.
    pub async fn new(session_id: SessionId, cwd: &str) -> Result<Self, PersistentBashError> {
        let stderr_buf = Arc::new(Mutex::new(String::new()));
        let (child, stdin, stdout, stderr_drainer) =
            Self::spawn_shell_process(cwd, stderr_buf.clone()).await?;
        Ok(Self {
            session_id,
            child,
            stdin,
            stdout,
            stderr_buf,
            stderr_drainer,
            cwd: cwd.to_string(),
            needs_restart: false,
        })
    }

    /// Executes `command` in this session and returns the captured output.
    ///
    /// The working directory and exported variables from previous calls are preserved.
    /// Syntax is validated with `bash -n -c` before sending the command to the shell,
    /// so malformed commands never corrupt the session state.
    ///
    /// If `timeout` elapses or `cancel` is triggered the session is silently restarted
    /// at the last known working directory, ready for the next call.
    ///
    /// # Errors
    ///
    /// - [`PersistentBashError::SyntaxError`] — invalid shell syntax
    /// - [`PersistentBashError::Timeout`] — command exceeded `timeout`
    /// - [`PersistentBashError::Cancelled`] — `cancel` token was triggered
    /// - [`PersistentBashError::SpawnFailed`] — shell restart after error failed
    /// - [`PersistentBashError::Io`] — pipe I/O failure
    pub async fn exec(
        &mut self,
        command: &str,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> Result<ShellOutput, PersistentBashError> {
        if self.needs_restart {
            self.restart_session().await?;
            self.needs_restart = false;
        }

        if command.trim().is_empty() {
            return Err(PersistentBashError::Io("command must not be empty".into()));
        }

        Self::validate_syntax(command).await?;

        // Unique per-call marker guarantees no collision with command output.
        let id = Uuid::new_v4();
        let marker = format!("__APOLLIA_DONE_{}__", id.simple());
        let escaped = shell_single_quote(command);

        // Run the command directly in the parent shell (NOT a subshell) so that
        // `cd` and `export` affect the persistent session.
        // `</dev/null` prevents the command from consuming input from the shared stdin.
        // `pwd` after the command captures the updated working directory.
        // NOTE: `exit N` will cause the shell to exit (EOF on stdout) — handled below.
        let script = format!(
            "eval {escaped} </dev/null; EXEC_CODE=$?; pwd; printf '%s\\n' {marker}:$EXEC_CODE\n"
        );

        self.stdin
            .write_all(script.as_bytes())
            .await
            .map_err(|e| PersistentBashError::Io(e.to_string()))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| PersistentBashError::Io(e.to_string()))?;

        let outcome = self.read_with_timeout(&marker, timeout, cancel).await;

        match outcome {
            ReadOutcome::Done(stdout_text, stderr_text, exit_code, cwd) => {
                self.cwd = cwd.clone();
                Ok(ShellOutput {
                    stdout: stdout_text,
                    stderr: stderr_text,
                    exit_code,
                    cwd,
                })
            }

            ReadOutcome::ShellExited => {
                // `exit N` or crash: retrieve exit code from the child status.
                let exit_code = self
                    .child
                    .wait()
                    .await
                    .map(|s| s.code().unwrap_or(-1))
                    .unwrap_or(-1);
                let cwd = self.cwd.clone();
                // Mark for restart so the next exec() begins on a fresh shell.
                self.needs_restart = true;
                Ok(ShellOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code,
                    cwd,
                })
            }

            ReadOutcome::Timeout => {
                self.needs_restart = true;
                Err(PersistentBashError::Timeout)
            }

            ReadOutcome::Cancelled => {
                self.needs_restart = true;
                Err(PersistentBashError::Cancelled)
            }

            ReadOutcome::Io(msg) => Err(PersistentBashError::Io(msg)),
        }
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Drives `do_read_until_marker` inside a `select!` with `timeout` and `cancel`.
    async fn read_with_timeout(
        &mut self,
        marker: &str,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> ReadOutcome {
        let fallback_cwd = self.cwd.clone();
        tokio::select! {
            r = Self::do_read_until_marker(
                &mut self.stdout,
                &self.stderr_buf,
                marker,
                &fallback_cwd,
            ) => {
                match r {
                    Ok((s, e, c, cwd)) => ReadOutcome::Done(s, e, c, cwd),
                    Err(ReadIoError::Eof) => ReadOutcome::ShellExited,
                    Err(ReadIoError::Io(m)) => ReadOutcome::Io(m),
                }
            }
            _ = tokio::time::sleep(timeout) => ReadOutcome::Timeout,
            _ = cancel.cancelled() => ReadOutcome::Cancelled,
        }
    }

    /// Reads stdout line-by-line until `marker:EXIT_CODE` appears.
    ///
    /// Returns `(stdout, stderr, exit_code, cwd)`.
    /// The line immediately before the marker is treated as the current working
    /// directory (output of `pwd`).
    async fn do_read_until_marker(
        stdout: &mut BufReader<ChildStdout>,
        stderr_buf: &Arc<Mutex<String>>,
        marker: &str,
        fallback_cwd: &str,
    ) -> Result<(String, String, i32, String), ReadIoError> {
        let marker_prefix = format!("{marker}:");
        let mut output_lines: Vec<String> = Vec::new();

        loop {
            let mut line = String::new();
            let n = stdout
                .read_line(&mut line)
                .await
                .map_err(|e| ReadIoError::Io(e.to_string()))?;

            if n == 0 {
                // EOF — the shell exited before printing the marker.
                return Err(ReadIoError::Eof);
            }

            // Trim trailing CR/LF produced on different platforms.
            let trimmed = line
                .trim_end_matches('\n')
                .trim_end_matches('\r')
                .to_string();

            if let Some(rest) = trimmed.strip_prefix(&marker_prefix) {
                let exit_code = rest.trim().parse::<i32>().unwrap_or(-1);
                // Last line before the marker is `pwd` output (= new CWD).
                let cwd = output_lines
                    .pop()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| fallback_cwd.to_string());
                let stdout_text = output_lines.join("\n");
                let stderr_text = {
                    let mut buf = stderr_buf.lock().await;
                    std::mem::take(&mut *buf)
                };
                return Ok((stdout_text, stderr_text, exit_code, cwd));
            }

            output_lines.push(trimmed);
        }
    }

    /// Validates `command` syntax via `bash -n -c` with a 1-second hard timeout.
    async fn validate_syntax(command: &str) -> Result<(), PersistentBashError> {
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            tokio::process::Command::new("bash")
                .args(["-n", "-c", command])
                .output(),
        )
        .await
        .map_err(|_| PersistentBashError::SyntaxCheckTimeout)?
        .map_err(|e| PersistentBashError::Io(e.to_string()))?;

        if !result.status.success() {
            let msg = String::from_utf8_lossy(&result.stderr).into_owned();
            return Err(PersistentBashError::SyntaxError(msg));
        }
        Ok(())
    }

    /// Kills the current shell process and spawns a fresh one at `self.cwd`.
    async fn restart_session(&mut self) -> Result<(), PersistentBashError> {
        self.stderr_drainer.abort();
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
        self.stderr_buf.lock().await.clear();

        let cwd = self.cwd.clone();
        let stderr_buf = self.stderr_buf.clone();
        let (child, stdin, stdout, drainer) = Self::spawn_shell_process(&cwd, stderr_buf).await?;

        self.child = child;
        self.stdin = stdin;
        self.stdout = stdout;
        self.stderr_drainer = drainer;

        Ok(())
    }

    /// Spawns `/bin/sh -l`, takes its I/O pipes, and starts the stderr drain task.
    async fn spawn_shell_process(
        cwd: &str,
        stderr_buf: Arc<Mutex<String>>,
    ) -> Result<(Child, ChildStdin, BufReader<ChildStdout>, JoinHandle<()>), PersistentBashError>
    {
        let mut child = tokio::process::Command::new("/bin/sh")
            .arg("-l")
            .current_dir(cwd)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| PersistentBashError::SpawnFailed(e.to_string()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| PersistentBashError::SpawnFailed("stdin pipe unavailable".into()))?;
        let stdout =
            BufReader::new(child.stdout.take().ok_or_else(|| {
                PersistentBashError::SpawnFailed("stdout pipe unavailable".into())
            })?);
        let stderr_pipe = child
            .stderr
            .take()
            .ok_or_else(|| PersistentBashError::SpawnFailed("stderr pipe unavailable".into()))?;

        // Drain stderr asynchronously so a verbose command never fills the pipe buffer
        // and deadlocks the shell. Accumulated bytes are flushed per-command in
        // `do_read_until_marker`.
        let drain_target = stderr_buf;
        let stderr_drainer = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr_pipe);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => drain_target.lock().await.push_str(&line),
                }
            }
        });

        Ok((child, stdin, stdout, stderr_drainer))
    }

    /// Kills the underlying child process and waits for it to exit.
    ///
    /// Called by [`super::registry::ShellSessionRegistry::close`] to cleanly
    /// release the session's resources when it is removed from the registry.
    pub(super) async fn child_kill(&mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}

impl Drop for ShellSession {
    fn drop(&mut self) {
        // Abort the background stderr drainer. The child process is killed via
        // `kill_on_drop(true)` when `self.child` is dropped.
        self.stderr_drainer.abort();
    }
}

// ---------------------------------------------------------------------------
// Shell escaping
// ---------------------------------------------------------------------------

/// Wraps `command` in POSIX single quotes, escaping any embedded single quotes.
///
/// Single quotes cannot themselves appear inside a single-quoted string in POSIX sh.
/// The standard workaround is to end the quoted string, insert a double-quoted `'`,
/// then restart the single-quoted string: `'it'\''s'` → `it's`.
fn shell_single_quote(command: &str) -> String {
    format!("'{}'", command.replace('\'', "'\"'\"'"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;

    // ---------------------------------------------------------------------------
    // Unit tests — shell_single_quote
    // ---------------------------------------------------------------------------

    #[test]
    fn test_shell_single_quote_no_special_chars() {
        // GIVEN: command without single quotes
        let result = shell_single_quote("echo hello");
        // THEN: wrapped as-is
        assert_eq!(result, "'echo hello'");
    }

    #[test]
    fn test_shell_single_quote_with_apostrophe() {
        // GIVEN: command containing a single quote
        let result = shell_single_quote("echo it's");
        // THEN: escaped correctly (no literal single-quote inside single-quoted string)
        assert!(
            result.contains("'\"'\"'"),
            "apostrophe not properly escaped: {result}"
        );
        // AND: the full result is parseable by sh (structural check)
        assert!(result.starts_with('\'') && result.ends_with('\''));
    }

    #[test]
    fn test_shell_single_quote_empty_command() {
        // GIVEN
        let result = shell_single_quote("");
        // THEN: still wrapped — eval '' is a valid no-op
        assert_eq!(result, "''");
    }

    // ---------------------------------------------------------------------------
    // Integration tests — require /bin/sh
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_cwd_persists_between_steps() {
        // GIVEN: session in /tmp
        let mut session = ShellSession::new(Uuid::new_v4(), "/tmp")
            .await
            .expect("spawn failed");

        // WHEN: cd to /var
        session
            .exec("cd /var", Duration::from_secs(5), CancellationToken::new())
            .await
            .expect("cd failed");

        // AND: check pwd in the same session
        let output = session
            .exec("pwd", Duration::from_secs(5), CancellationToken::new())
            .await
            .expect("pwd failed");

        // THEN: /var is reflected in stdout or cwd
        assert!(
            output.stdout.contains("/var") || output.cwd.contains("/var"),
            "expected /var — stdout='{}' cwd='{}'",
            output.stdout,
            output.cwd
        );
    }

    #[tokio::test]
    async fn test_env_var_persists_between_steps() {
        // GIVEN
        let mut session = ShellSession::new(Uuid::new_v4(), "/tmp")
            .await
            .expect("spawn failed");

        // WHEN: export then read
        session
            .exec(
                "export APOLLIA_TEST_VAR=hello42",
                Duration::from_secs(5),
                CancellationToken::new(),
            )
            .await
            .expect("export failed");

        let output = session
            .exec(
                "echo $APOLLIA_TEST_VAR",
                Duration::from_secs(5),
                CancellationToken::new(),
            )
            .await
            .expect("echo failed");

        // THEN
        assert!(
            output.stdout.contains("hello42"),
            "expected 'hello42' — stdout='{}'",
            output.stdout
        );
    }

    #[tokio::test]
    async fn test_syntax_error_does_not_corrupt_session() {
        // GIVEN
        let mut session = ShellSession::new(Uuid::new_v4(), "/tmp")
            .await
            .expect("spawn failed");

        // WHEN: invalid syntax submitted
        let result = session
            .exec(
                "if then fi",
                Duration::from_secs(5),
                CancellationToken::new(),
            )
            .await;

        // THEN: SyntaxError returned
        assert!(
            matches!(result, Err(PersistentBashError::SyntaxError(_))),
            "expected SyntaxError — got: {result:?}"
        );

        // AND: session still functional
        let ok = session
            .exec(
                "echo ok_after_syntax",
                Duration::from_secs(5),
                CancellationToken::new(),
            )
            .await
            .expect("session unusable after syntax error");
        assert!(
            ok.stdout.contains("ok_after_syntax"),
            "stdout after syntax error: '{}'",
            ok.stdout
        );
    }

    #[tokio::test]
    async fn test_cancel_returns_cancelled_and_session_recovers() {
        // GIVEN
        let mut session = ShellSession::new(Uuid::new_v4(), "/tmp")
            .await
            .expect("spawn failed");

        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            cancel_clone.cancel();
        });

        // WHEN: long-running command cancelled at 100 ms
        let result = session
            .exec("sleep 30", Duration::from_secs(60), cancel)
            .await;

        // THEN: Cancelled error
        assert!(
            matches!(result, Err(PersistentBashError::Cancelled)),
            "expected Cancelled — got: {result:?}"
        );

        // AND: session usable after implicit restart
        let ok = session
            .exec(
                "echo after_cancel",
                Duration::from_secs(5),
                CancellationToken::new(),
            )
            .await
            .expect("session unusable after cancel");
        assert!(
            ok.stdout.contains("after_cancel"),
            "stdout after cancel: '{}'",
            ok.stdout
        );
    }

    #[tokio::test]
    async fn test_two_sessions_have_independent_cwd() {
        // GIVEN: two separate sessions
        let mut session_a = ShellSession::new(Uuid::new_v4(), "/tmp")
            .await
            .expect("spawn A failed");
        let mut session_b = ShellSession::new(Uuid::new_v4(), "/tmp")
            .await
            .expect("spawn B failed");

        // WHEN: each navigates to a different directory
        session_a
            .exec("cd /var", Duration::from_secs(5), CancellationToken::new())
            .await
            .expect("cd /var failed");
        session_b
            .exec("cd /tmp", Duration::from_secs(5), CancellationToken::new())
            .await
            .expect("cd /tmp failed");

        // THEN: each reports its own directory
        let out_a = session_a
            .exec("pwd", Duration::from_secs(5), CancellationToken::new())
            .await
            .expect("pwd A failed");
        let out_b = session_b
            .exec("pwd", Duration::from_secs(5), CancellationToken::new())
            .await
            .expect("pwd B failed");

        assert!(
            out_a.stdout.contains("/var") || out_a.cwd.contains("/var"),
            "session_a expected /var — stdout='{}' cwd='{}'",
            out_a.stdout,
            out_a.cwd
        );
        assert!(
            out_b.stdout.contains("/tmp") || out_b.cwd.contains("/tmp"),
            "session_b expected /tmp — stdout='{}' cwd='{}'",
            out_b.stdout,
            out_b.cwd
        );
    }

    #[tokio::test]
    async fn test_exit_command_returns_exit_code_not_tool_error() {
        // GIVEN
        let mut session = ShellSession::new(Uuid::new_v4(), "/tmp")
            .await
            .expect("spawn failed");

        // WHEN: exit 42 terminates the shell process
        let output = session
            .exec("exit 42", Duration::from_secs(5), CancellationToken::new())
            .await
            .expect("should return Ok, not PersistentBashError");

        // THEN: exit code captured, no error
        assert_eq!(
            output.exit_code, 42,
            "expected exit_code=42 — got {}",
            output.exit_code
        );

        // AND: session transparently restarted
        let ok = session
            .exec(
                "echo recovered",
                Duration::from_secs(5),
                CancellationToken::new(),
            )
            .await
            .expect("session unusable after exit 42");
        assert!(
            ok.stdout.contains("recovered"),
            "stdout after restart: '{}'",
            ok.stdout
        );
    }

    #[tokio::test]
    async fn test_nonzero_exit_code_is_not_tool_error() {
        // GIVEN: command that exits non-zero via false(1)
        let mut session = ShellSession::new(Uuid::new_v4(), "/tmp")
            .await
            .expect("spawn failed");

        // WHEN
        let output = session
            .exec("false", Duration::from_secs(5), CancellationToken::new())
            .await
            .expect("false should not produce PersistentBashError");

        // THEN: exit code 1, no error
        assert_ne!(output.exit_code, 0, "false must exit non-zero");
    }

    #[tokio::test]
    async fn test_empty_command_returns_io_error() {
        // GIVEN
        let mut session = ShellSession::new(Uuid::new_v4(), "/tmp")
            .await
            .expect("spawn failed");

        // WHEN
        let result = session
            .exec("   ", Duration::from_secs(5), CancellationToken::new())
            .await;

        // THEN
        assert!(
            matches!(result, Err(PersistentBashError::Io(_))),
            "expected Io error for blank command — got: {result:?}"
        );
    }
}
