//! Native bash executor tool with Linux namespace isolation.
//!
//! On Linux: wraps every command with `unshare --pid --mount --fork` (ADR-005).
//! On macOS/non-Linux: executes directly with a per-invocation `tracing::warn!` (ADR-012).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::json;
use thiserror::Error;
use tokio::io::AsyncReadExt;

use apollia_core::SandboxProfile;

use crate::descriptor::{ToolDescriptor, ToolKind};

/// Native shell executor with Linux namespace isolation (PID + mount).
///
/// On Linux: wraps commands with `unshare --pid --mount --fork /bin/sh -c` for PID
/// and mount namespace isolation (see ADR-005).
///
/// On non-Linux (macOS, dev): executes directly via `/bin/sh -c` with a per-invocation
/// `tracing::warn!` to make the absence of sandbox impossible to miss (see ADR-012).
pub struct BashExecutor;

/// Input parameters for a bash invocation.
pub struct BashInput {
    /// Shell command interpreted by `/bin/sh -c`. Must not be empty or whitespace-only.
    pub command: String,
    /// Hard timeout in seconds before SIGKILL. Max 300s recommended.
    pub timeout_secs: u64,
    /// Optional working directory. `None` uses the process's current directory.
    pub working_dir: Option<PathBuf>,
}

/// Result of a successful bash invocation.
pub struct BashOutput {
    /// Captured standard output from the child process.
    pub stdout: String,
    /// Captured standard error from the child process.
    pub stderr: String,
    /// Exit code reported by the child process. `-1` if terminated by a signal.
    pub exit_code: i32,
    /// Wall-clock duration of the execution in milliseconds.
    pub duration_ms: u64,
}

/// Errors produced by [`BashExecutor::run`].
#[derive(Debug, Error)]
pub enum BashExecutorError {
    /// `command` is empty or whitespace-only — rejected before any I/O (Principe #4).
    #[error("command must not be empty")]
    EmptyCommand,
    /// The specified working directory does not exist or is not a directory.
    #[error("working directory not found: {0}")]
    WorkingDirNotFound(PathBuf),
    /// Command exceeded the hard timeout and the child process was killed.
    #[error("command timed out after {timeout_secs}s: {command}")]
    Timeout {
        /// The command string that was killed.
        command: String,
        /// The configured timeout in seconds.
        timeout_secs: u64,
    },
    /// The OS refused to spawn the child process (e.g. `unshare` not found, EPERM).
    #[error("failed to spawn process: {0}")]
    SpawnFailed(String),
    /// I/O error reading stdout or stderr from the child process.
    #[error("output capture failed: {0}")]
    OutputCaptureFailed(String),
}

impl BashExecutor {
    /// Creates a `BashExecutor`.
    ///
    /// The sandbox mode is resolved at compile time:
    /// - Linux → Linux namespaces via `unshare` (production path)
    /// - Other → Dev mode, no sandbox, warning at each invocation
    pub fn new() -> Self {
        Self
    }

    /// Returns the [`ToolDescriptor`] for registration in [`crate::registry::ToolRegistry`].
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "bash_executor".to_string(),
            version: "1.0.0".to_string(),
            description: "Execute a shell command. Prefer targeted, fast commands over broad scans. \
                          Set timeout_secs proportional to expected duration."
                .to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "required": ["command", "timeout_secs"],
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Shell command to execute. Use the most efficient approach \
                                        for the task — avoid scanning entire filesystems when a \
                                        scoped command suffices."
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 300,
                        "description": "Timeout in seconds. Match to expected command duration \
                                        (e.g. simple lookups: 5-10s, builds: 60-120s)."
                    },
                    "working_dir": {
                        "type": "string",
                        "description": "Optional working directory. Use to avoid unnecessary \
                                        path navigation in the command itself."
                    }
                }
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "stdout":      { "type": "string" },
                    "stderr":      { "type": "string" },
                    "exit_code":   { "type": "integer" },
                    "duration_ms": { "type": "integer" }
                }
            })),
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec!["shell".to_string(), "system".to_string()],
            dangerous: false,
        }
    }

    /// Executes a shell command with sandbox isolation.
    ///
    /// # Errors
    ///
    /// - [`BashExecutorError::EmptyCommand`] — `command` is blank (checked before any I/O)
    /// - [`BashExecutorError::WorkingDirNotFound`] — `working_dir` path does not exist
    /// - [`BashExecutorError::Timeout`] — command exceeded `timeout_secs`; child is killed (no zombie)
    /// - [`BashExecutorError::SpawnFailed`] — OS refused to spawn the child
    /// - [`BashExecutorError::OutputCaptureFailed`] — I/O error reading stdout/stderr
    pub async fn run(&self, input: BashInput) -> Result<BashOutput, BashExecutorError> {
        // Principle #4 — Fail fast: validate before any I/O.
        if input.command.trim().is_empty() {
            return Err(BashExecutorError::EmptyCommand);
        }

        if let Some(ref dir) = input.working_dir {
            if !dir.is_dir() {
                return Err(BashExecutorError::WorkingDirNotFound(dir.clone()));
            }
        }

        let mut cmd = Self::build_command(&input);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        if let Some(ref dir) = input.working_dir {
            cmd.current_dir(dir);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| BashExecutorError::SpawnFailed(e.to_string()))?;

        // Take pipes before `wait` to avoid pipe-buffer deadlock on large outputs.
        let mut stdout_pipe = child.stdout.take().ok_or_else(|| {
            BashExecutorError::OutputCaptureFailed("stdout pipe missing".to_string())
        })?;
        let mut stderr_pipe = child.stderr.take().ok_or_else(|| {
            BashExecutorError::OutputCaptureFailed("stderr pipe missing".to_string())
        })?;

        // Drain stdout/stderr concurrently in background tasks.
        // Without this, large outputs would fill the pipe buffer and deadlock `wait`.
        let stdout_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            stdout_pipe.read_to_end(&mut buf).await.map(|_| buf)
        });
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            stderr_pipe.read_to_end(&mut buf).await.map(|_| buf)
        });

        let start = Instant::now();
        let timeout_secs = input.timeout_secs;

        // Wait for process exit with a hard timeout.
        // On timeout: abort reader tasks, kill child, wait for reap (no zombie).
        let status = tokio::select! {
            result = child.wait() => {
                result.map_err(|e| BashExecutorError::OutputCaptureFailed(e.to_string()))?
            }
            _ = tokio::time::sleep(Duration::from_secs(timeout_secs)) => {
                stdout_task.abort();
                stderr_task.abort();
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(BashExecutorError::Timeout {
                    command: input.command,
                    timeout_secs,
                });
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        let stdout_bytes = stdout_task
            .await
            .map_err(|e| BashExecutorError::OutputCaptureFailed(e.to_string()))?
            .map_err(|e| BashExecutorError::OutputCaptureFailed(e.to_string()))?;

        let stderr_bytes = stderr_task
            .await
            .map_err(|e| BashExecutorError::OutputCaptureFailed(e.to_string()))?
            .map_err(|e| BashExecutorError::OutputCaptureFailed(e.to_string()))?;

        Ok(BashOutput {
            stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
            stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
            exit_code: status.code().unwrap_or(-1),
            duration_ms,
        })
    }

    /// Builds the OS-appropriate [`tokio::process::Command`] for the given input.
    ///
    /// On Linux: wraps with `unshare --pid --mount --fork` for namespace isolation.
    /// On non-Linux: direct `/bin/sh -c` with a per-invocation dev-mode warning (ADR-012).
    #[cfg(target_os = "linux")]
    fn build_command(input: &BashInput) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new("/usr/bin/unshare");
        cmd.args([
            "--pid",
            "--mount",
            "--fork",
            "/bin/sh",
            "-c",
            &input.command,
        ]);
        cmd
    }

    #[cfg(not(target_os = "linux"))]
    fn build_command(input: &BashInput) -> tokio::process::Command {
        tracing::warn!(
            command = %input.command,
            "bash_executor: running in Dev mode — no sandbox active. \
             Linux namespaces are not available on this platform. \
             Production deployments require Linux."
        );
        let mut cmd = tokio::process::Command::new("/bin/sh");
        cmd.args(["-c", &input.command]);
        cmd
    }
}

impl Default for BashExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns `true` if the platform can actually execute shell commands
    /// through our `build_command` path. On Linux without `CAP_SYS_ADMIN`
    /// (e.g. GitHub Actions runners), `unshare --pid --mount` fails with
    /// EPERM — these tests must be skipped gracefully.
    fn can_run_shell() -> bool {
        #[cfg(target_os = "linux")]
        {
            let result = std::process::Command::new("/usr/bin/unshare")
                .args(["--pid", "--mount", "--fork", "/bin/true"])
                .output();
            matches!(result, Ok(output) if output.status.success())
        }
        #[cfg(not(target_os = "linux"))]
        {
            true
        }
    }

    #[tokio::test]
    async fn test_ac1_echo_returns_stdout() {
        if !can_run_shell() {
            eprintln!("skipped: unshare requires CAP_SYS_ADMIN (not available on CI)");
            return;
        }
        // GIVEN
        let executor = BashExecutor::new();
        let input = BashInput {
            command: "echo hello".to_string(),
            timeout_secs: 10,
            working_dir: None,
        };
        // WHEN
        let output = executor.run(input).await.expect("execution failed");
        // THEN
        assert_eq!(output.stdout.trim(), "hello");
        assert_eq!(output.exit_code, 0);
    }

    #[tokio::test]
    async fn test_ac2_failed_command_returns_nonzero_exit_code() {
        if !can_run_shell() {
            eprintln!("skipped: unshare requires CAP_SYS_ADMIN (not available on CI)");
            return;
        }
        // GIVEN
        let executor = BashExecutor::new();
        let input = BashInput {
            command: "exit 42".to_string(),
            timeout_secs: 10,
            working_dir: None,
        };
        // WHEN
        let output = executor
            .run(input)
            .await
            .expect("should not be a Rust error");
        // THEN
        assert_eq!(output.exit_code, 42);
    }

    #[tokio::test]
    async fn test_ac3_timeout_kills_process() {
        if !can_run_shell() {
            eprintln!("skipped: unshare requires CAP_SYS_ADMIN (not available on CI)");
            return;
        }
        // GIVEN
        let executor = BashExecutor::new();
        let input = BashInput {
            command: "sleep 60".to_string(),
            timeout_secs: 1,
            working_dir: None,
        };
        // WHEN
        let result = executor.run(input).await;
        // THEN
        assert!(matches!(result, Err(BashExecutorError::Timeout { .. })));
    }

    #[tokio::test]
    async fn test_ac4_empty_command_rejected_immediately() {
        // GIVEN
        let executor = BashExecutor::new();
        let input = BashInput {
            command: "".to_string(),
            timeout_secs: 10,
            working_dir: None,
        };
        // WHEN
        let result = executor.run(input).await;
        // THEN
        assert!(matches!(result, Err(BashExecutorError::EmptyCommand)));
    }

    #[tokio::test]
    async fn test_ac4_whitespace_only_command_rejected() {
        // GIVEN — whitespace is not a valid command either
        let executor = BashExecutor::new();
        let input = BashInput {
            command: "   ".to_string(),
            timeout_secs: 10,
            working_dir: None,
        };
        // WHEN
        let result = executor.run(input).await;
        // THEN
        assert!(matches!(result, Err(BashExecutorError::EmptyCommand)));
    }

    #[tokio::test]
    async fn test_ac5_stderr_captured_separately() {
        if !can_run_shell() {
            eprintln!("skipped: unshare requires CAP_SYS_ADMIN (not available on CI)");
            return;
        }
        // GIVEN
        let executor = BashExecutor::new();
        let input = BashInput {
            command: "echo error >&2".to_string(),
            timeout_secs: 10,
            working_dir: None,
        };
        // WHEN
        let output = executor.run(input).await.expect("execution failed");
        // THEN
        assert!(output.stdout.trim().is_empty());
        assert!(output.stderr.contains("error"));
    }

    #[tokio::test]
    async fn test_ac6_invalid_working_dir_returns_error() {
        // GIVEN
        let executor = BashExecutor::new();
        let input = BashInput {
            command: "echo ok".to_string(),
            timeout_secs: 10,
            working_dir: Some(std::path::PathBuf::from("/nonexistent_apollia_test_dir")),
        };
        // WHEN
        let result = executor.run(input).await;
        // THEN
        assert!(matches!(
            result,
            Err(BashExecutorError::WorkingDirNotFound(_))
        ));
    }

    #[test]
    fn test_descriptor_is_valid() {
        // GIVEN / WHEN
        let descriptor = BashExecutor::descriptor();
        // THEN
        assert_eq!(descriptor.name, "bash_executor");
        assert!(descriptor.validate().is_ok());
    }
}
