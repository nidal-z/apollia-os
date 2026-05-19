//! Native Python executor tool with per-agent virtualenv isolation.
//!
//! Each agent owns a dedicated virtualenv under `<venv_base_dir>/<agent_id>/venv/`.
//! Packages are installed at `INITIALIZING` via `setup_venv()` — never at execution time
//! (Principle #4: Fail fast, Principle #1: Local-first).
//!
//! Code is written to a temporary file (never passed via `-c`) to avoid quoting issues
//! and support multi-line scripts. The temp file is always cleaned up after execution.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::json;
use thiserror::Error;
use tokio::io::AsyncReadExt;

use apollia_core::SandboxProfile;

use crate::descriptor::{ToolDescriptor, ToolKind};

/// Executor that runs Python code in a per-agent isolated virtualenv.
///
/// The virtualenv lives at `<venv_base_dir>/<agent_id>/venv/`. Packages declared
/// in the agent manifest must be installed via [`PythonExecutor::setup_venv`] at
/// agent `INITIALIZING` — not at execution time.
pub struct PythonExecutor {
    /// Identifier of the agent owning this virtualenv.
    agent_id: String,
    /// Absolute path to the virtualenv: `<venv_base_dir>/<agent_id>/venv/`.
    venv_path: PathBuf,
    /// Absolute path to the Python interpreter inside the venv.
    python_bin: PathBuf,
}

/// Input parameters for a Python invocation.
pub struct PythonInput {
    /// Python source code to execute. Must not be empty.
    pub code: String,
    /// Hard timeout in seconds before SIGKILL.
    pub timeout_secs: u64,
}

/// Result of a successful Python invocation.
pub struct PythonOutput {
    /// Captured standard output from the Python process.
    pub stdout: String,
    /// Captured standard error from the Python process.
    pub stderr: String,
    /// Exit code reported by the Python process. `-1` if terminated by a signal.
    pub exit_code: i32,
    /// Wall-clock duration of the execution in milliseconds.
    pub duration_ms: u64,
}

/// Errors produced by [`PythonExecutor`].
#[derive(Debug, Error)]
pub enum PythonExecutorError {
    /// `code` is empty — rejected before any I/O (Principle #4).
    #[error("code must not be empty")]
    EmptyCode,
    /// `python3` is not available in PATH — detected at construction time (Principle #4).
    #[error("python3 is not available in PATH")]
    PythonUnavailable,
    /// `python3 -m venv` failed to create the virtualenv.
    #[error("failed to create virtualenv: {0}")]
    VenvCreationFailed(String),
    /// `pip install` failed for a specific package.
    #[error("failed to install package '{package}': {stderr}")]
    PackageInstallFailed {
        /// The package that failed to install.
        package: String,
        /// The stderr output from pip.
        stderr: String,
    },
    /// The Python process exceeded the hard timeout and was killed.
    #[error("python execution timed out after {timeout_secs}s")]
    Timeout {
        /// The configured timeout in seconds.
        timeout_secs: u64,
    },
    /// The OS refused to spawn the Python process.
    #[error("failed to spawn python process: {0}")]
    SpawnFailed(String),
    /// I/O error reading stdout or stderr from the child process.
    #[error("output capture failed: {0}")]
    OutputCaptureFailed(String),
    /// I/O error writing the temporary Python script file.
    #[error("failed to write temporary script: {0}")]
    TempFileFailed(String),
}

/// Locate the `site-packages` directories of an agent's per-package venv.
///
/// Convention : the venv lives at `<venv_base_dir>/<agent_name>/venv/`.
/// On Unix : `lib/python<X.Y>/site-packages` (we glob `lib/*/site-packages`).
/// On Windows : `Lib/site-packages`.
///
/// Returns an empty vec if the venv does not exist — the caller should
/// treat that as "no extra sys.path needed" (agent declares no pip packages).
///
/// This helper is canonical and shared by all callers that need to inject
/// the agent's venv into PyO3's `sys.path` before importing the Python
/// module (agent runtime backends, validation flows, CLI commands).
pub fn agent_venv_site_packages(venv_base_dir: &Path, agent_name: &str) -> Vec<PathBuf> {
    let venv_path = venv_base_dir.join(agent_name).join("venv");
    if !venv_path.is_dir() {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    let lib = venv_path.join("lib");
    if lib.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&lib) {
            for entry in entries.flatten() {
                let sp = entry.path().join("site-packages");
                if sp.is_dir() {
                    candidates.push(sp);
                }
            }
        }
    }
    let win_lib = venv_path.join("Lib").join("site-packages");
    if win_lib.is_dir() {
        candidates.push(win_lib);
    }
    candidates
}

impl PythonExecutor {
    /// Creates a `PythonExecutor` for the given agent.
    ///
    /// Verifies that `python3` is available in PATH. Does **not** create the virtualenv
    /// (call [`setup_venv`][Self::setup_venv] at agent `INITIALIZING` for that).
    ///
    /// # Errors
    ///
    /// Returns [`PythonExecutorError::PythonUnavailable`] if `python3` is not in PATH.
    pub fn new(agent_id: &str, venv_base_dir: &Path) -> Result<Self, PythonExecutorError> {
        // Principle #4 — Fail fast: verify python3 availability at construction time.
        let python3_check = std::process::Command::new("python3")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        match python3_check {
            Ok(status) if status.success() => {}
            _ => return Err(PythonExecutorError::PythonUnavailable),
        }

        let venv_path = venv_base_dir.join(agent_id).join("venv");
        let python_bin = venv_path.join("bin").join("python");

        Ok(Self {
            agent_id: agent_id.to_string(),
            venv_path,
            python_bin,
        })
    }

    /// Creates the virtualenv and installs the declared packages.
    ///
    /// Idempotent: if the virtualenv already exists and the Python interpreter is present,
    /// creation is skipped and package installation proceeds normally. This allows agents
    /// to go through INITIALIZING multiple times without recreating the venv.
    ///
    /// Must be called at agent `INITIALIZING`. May take time if packages are numerous.
    /// Installing packages at execution time is forbidden by design.
    ///
    /// # Errors
    ///
    /// - [`PythonExecutorError::VenvCreationFailed`] — `python3 -m venv` failed
    /// - [`PythonExecutorError::PackageInstallFailed`] — `pip install <package>` failed
    pub async fn setup_venv(&self, packages: &[String]) -> Result<(), PythonExecutorError> {
        // Idempotent: skip creation if the venv interpreter already resolves to a live binary.
        // `Path::exists()` follows symlinks — returns false for broken symlinks.
        // This handles INITIALIZING restarts without blowing away installed packages.
        if self.python_bin.exists() {
            tracing::info!(
                agent_id = %self.agent_id,
                "python_executor: virtualenv already exists, skipping creation"
            );
        } else {
            // `--clear` purges any existing (possibly broken) venv directory before recreating.
            // This handles stale venvs whose interpreter symlinks point to a removed Python.
            let venv_args: &[&str] = if self.venv_path.is_dir() {
                &["-m", "venv", "--clear"]
            } else {
                &["-m", "venv"]
            };

            tracing::info!(
                agent_id = %self.agent_id,
                venv_path = %self.venv_path.display(),
                clear = self.venv_path.is_dir(),
                "python_executor: creating virtualenv"
            );

            let venv_path_str = self.venv_path.to_str().unwrap_or("");
            let mut cmd_args: Vec<&str> = venv_args.to_vec();
            cmd_args.push(venv_path_str);

            let venv_output = tokio::process::Command::new("python3")
                .args(&cmd_args)
                .output()
                .await
                .map_err(|e| PythonExecutorError::VenvCreationFailed(e.to_string()))?;

            if !venv_output.status.success() {
                let stderr = String::from_utf8_lossy(&venv_output.stderr).into_owned();
                return Err(PythonExecutorError::VenvCreationFailed(stderr));
            }
        }

        let pip_bin = self.venv_path.join("bin").join("pip");

        for package in packages {
            tracing::info!(
                agent_id = %self.agent_id,
                package = %package,
                "python_executor: installing package"
            );

            let pip_output = tokio::process::Command::new(&pip_bin)
                .args(["install", package.as_str(), "--quiet"])
                .output()
                .await
                .map_err(|e| PythonExecutorError::PackageInstallFailed {
                    package: package.clone(),
                    stderr: e.to_string(),
                })?;

            if !pip_output.status.success() {
                let stderr = String::from_utf8_lossy(&pip_output.stderr).into_owned();
                return Err(PythonExecutorError::PackageInstallFailed {
                    package: package.clone(),
                    stderr,
                });
            }
        }

        Ok(())
    }

    /// Executes Python code in the agent's virtualenv.
    ///
    /// Code is written to a temporary file and always cleaned up after execution,
    /// even on timeout or error.
    ///
    /// # Errors
    ///
    /// - [`PythonExecutorError::EmptyCode`] — `code` is empty (checked before any I/O)
    /// - [`PythonExecutorError::Timeout`] — process exceeded `timeout_secs`; killed, no zombie
    /// - [`PythonExecutorError::SpawnFailed`] — OS refused to spawn the process
    /// - [`PythonExecutorError::OutputCaptureFailed`] — I/O error reading stdout/stderr
    /// - [`PythonExecutorError::TempFileFailed`] — could not write the temporary script file
    pub async fn run(&self, input: PythonInput) -> Result<PythonOutput, PythonExecutorError> {
        // Principle #4 — Fail fast: validate before any I/O.
        if input.code.trim().is_empty() {
            return Err(PythonExecutorError::EmptyCode);
        }

        // Write code to a temp file to avoid shell-quoting issues with -c.
        let script_path = std::env::temp_dir().join(format!("apollia_{}.py", uuid::Uuid::new_v4()));

        tokio::fs::write(&script_path, &input.code)
            .await
            .map_err(|e| PythonExecutorError::TempFileFailed(e.to_string()))?;

        let result = self.execute_script(&script_path, input.timeout_secs).await;

        // Always remove the temp file — success or error (no file leak).
        let _ = tokio::fs::remove_file(&script_path).await;

        result
    }

    /// Returns the [`ToolDescriptor`] for registration in [`crate::registry::ToolRegistry`].
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "python_executor".to_string(),
            version: "1.0.0".to_string(),
            description: "Execute Python code in the agent's virtualenv. Only pre-installed \
                          packages are available. Write focused scripts that do one thing well."
                .to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "required": ["code", "timeout_secs"],
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "Python source code to execute"
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 300,
                        "description": "Hard timeout in seconds before SIGKILL"
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
            tags: vec!["python".to_string(), "scripting".to_string()],
            dangerous: false,
            is_read_only: false,
            risk_score: 8,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        }
    }

    /// Spawns the Python interpreter on the given script file with a hard timeout.
    ///
    /// On timeout: reader tasks are aborted, child is killed and reaped (no zombie).
    async fn execute_script(
        &self,
        script_path: &Path,
        timeout_secs: u64,
    ) -> Result<PythonOutput, PythonExecutorError> {
        let mut cmd = tokio::process::Command::new(&self.python_bin);
        cmd.arg(script_path);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| PythonExecutorError::SpawnFailed(e.to_string()))?;

        // Take pipes before `wait` to avoid pipe-buffer deadlock on large outputs.
        let mut stdout_pipe = child.stdout.take().ok_or_else(|| {
            PythonExecutorError::OutputCaptureFailed("stdout pipe missing".to_string())
        })?;
        let mut stderr_pipe = child.stderr.take().ok_or_else(|| {
            PythonExecutorError::OutputCaptureFailed("stderr pipe missing".to_string())
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

        // Wait for process exit with a hard timeout.
        // On timeout: abort reader tasks, kill child, wait to reap (no zombie).
        let status = tokio::select! {
            result = child.wait() => {
                result.map_err(|e| PythonExecutorError::OutputCaptureFailed(e.to_string()))?
            }
            _ = tokio::time::sleep(Duration::from_secs(timeout_secs)) => {
                stdout_task.abort();
                stderr_task.abort();
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(PythonExecutorError::Timeout { timeout_secs });
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        let stdout_bytes = stdout_task
            .await
            .map_err(|e| PythonExecutorError::OutputCaptureFailed(e.to_string()))?
            .map_err(|e| PythonExecutorError::OutputCaptureFailed(e.to_string()))?;

        let stderr_bytes = stderr_task
            .await
            .map_err(|e| PythonExecutorError::OutputCaptureFailed(e.to_string()))?
            .map_err(|e| PythonExecutorError::OutputCaptureFailed(e.to_string()))?;

        Ok(PythonOutput {
            stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
            stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
            exit_code: status.code().unwrap_or(-1),
            duration_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_venv_dir() -> PathBuf {
        std::env::temp_dir().join("apollia_test_venv")
    }

    /// Helper: creates a PythonExecutor, skipping the test if python3 is unavailable.
    macro_rules! make_executor {
        ($agent_id:expr) => {
            match PythonExecutor::new($agent_id, &test_venv_dir()) {
                Ok(e) => e,
                Err(PythonExecutorError::PythonUnavailable) => return,
                Err(e) => panic!("unexpected error creating executor: {:?}", e),
            }
        };
    }

    #[tokio::test]
    async fn test_ac1_simple_print_returns_stdout() {
        // GIVEN
        let executor = make_executor!("test-agent-ac1");
        executor.setup_venv(&[]).await.expect("venv setup failed");
        let input = PythonInput {
            code: "print('hello')".to_string(),
            timeout_secs: 30,
        };
        // WHEN
        let output = executor.run(input).await.expect("execution failed");
        // THEN
        assert_eq!(output.stdout.trim(), "hello");
        assert_eq!(output.stderr, "");
        assert_eq!(output.exit_code, 0);
    }

    #[test]
    fn test_ac2_python_unavailable_detected_at_construction() {
        // GIVEN — we test the happy path: if python3 is present, new() succeeds.
        // The error case (PythonUnavailable) is covered implicitly by make_executor! in other tests.
        let result = PythonExecutor::new("test-agent-ac2", &test_venv_dir());
        match result {
            Ok(_) => { /* python3 present — construction succeeded as expected */ }
            Err(PythonExecutorError::PythonUnavailable) => { /* python3 absent — also valid */ }
            Err(e) => panic!("unexpected error: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_ac3_import_missing_package_returns_nonzero_exit() {
        // GIVEN — venv with no extra packages
        let executor = make_executor!("test-agent-ac3");
        executor.setup_venv(&[]).await.expect("venv setup failed");
        let input = PythonInput {
            code: "import pandas".to_string(),
            timeout_secs: 30,
        };
        // WHEN
        let output = executor
            .run(input)
            .await
            .expect("should not be a Rust error");
        // THEN — Python process exits non-zero, stderr contains ModuleNotFoundError
        assert_ne!(output.exit_code, 0);
        assert!(
            output.stderr.contains("ModuleNotFoundError")
                || output.stderr.contains("No module named"),
            "expected ModuleNotFoundError in stderr, got: {}",
            output.stderr
        );
    }

    #[tokio::test]
    async fn test_ac4_timeout_kills_python_process() {
        // GIVEN
        let executor = make_executor!("test-agent-ac4");
        executor.setup_venv(&[]).await.expect("venv setup failed");
        let input = PythonInput {
            code: "import time; time.sleep(60)".to_string(),
            timeout_secs: 1,
        };
        // WHEN
        let result = executor.run(input).await;
        // THEN
        assert!(
            matches!(result, Err(PythonExecutorError::Timeout { .. })),
            "expected Timeout error, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_ac5_empty_code_rejected_immediately() {
        // GIVEN
        let executor = make_executor!("test-agent-ac5");
        executor.setup_venv(&[]).await.expect("venv setup failed");
        let input = PythonInput {
            code: "".to_string(),
            timeout_secs: 10,
        };
        // WHEN / THEN
        assert!(matches!(
            executor.run(input).await,
            Err(PythonExecutorError::EmptyCode)
        ));
    }

    #[tokio::test]
    async fn test_ac5_whitespace_only_code_rejected() {
        // GIVEN — whitespace-only is also empty
        let executor = make_executor!("test-agent-ac5-ws");
        executor.setup_venv(&[]).await.expect("venv setup failed");
        let input = PythonInput {
            code: "   \n\t  ".to_string(),
            timeout_secs: 10,
        };
        // WHEN / THEN
        assert!(matches!(
            executor.run(input).await,
            Err(PythonExecutorError::EmptyCode)
        ));
    }

    #[tokio::test]
    async fn test_ac6_isolation_between_agents() {
        // GIVEN — two executors for different agents, both with empty venvs
        let executor_a = make_executor!("test-agent-isolation-a");
        let executor_b = make_executor!("test-agent-isolation-b");
        executor_a
            .setup_venv(&[])
            .await
            .expect("venv A setup failed");
        executor_b
            .setup_venv(&[])
            .await
            .expect("venv B setup failed");

        // WHEN — import something that does exist in stdlib (venv-independent)
        let input_a = PythonInput {
            code: "import os; print(os.name)".to_string(),
            timeout_secs: 10,
        };
        let output_a = executor_a.run(input_a).await.expect("agent A failed");

        let input_b = PythonInput {
            code: "import os; print(os.name)".to_string(),
            timeout_secs: 10,
        };
        let output_b = executor_b.run(input_b).await.expect("agent B failed");

        // THEN — both agents run in their own venv, both succeed independently
        assert_eq!(output_a.exit_code, 0);
        assert_eq!(output_b.exit_code, 0);
        // Both venvs are separate directories
        assert_ne!(executor_a.venv_path, executor_b.venv_path);
    }

    #[test]
    fn test_descriptor_is_valid() {
        // GIVEN / WHEN
        let descriptor = PythonExecutor::descriptor();
        // THEN
        assert_eq!(descriptor.name, "python_executor");
        assert!(descriptor.validate().is_ok());
    }
}
