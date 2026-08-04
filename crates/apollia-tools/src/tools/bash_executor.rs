//! Native shell executor tool. Namespace isolation is Linux-only.
//!
//! Every command runs through one resolved POSIX shell (`/bin/sh` on Unix, a
//! `PATH`-discovered bash/sh off Unix; see [`crate::tools::shell_discovery`]).
//! On Linux the command is additionally wrapped with
//! `unshare --pid --mount --fork`. On macOS and Windows there is no OS
//! sandbox and a per-invocation `tracing::warn!` says so.
//!
//! Before spawning a process, the validation steps applied in order are:
//! 1. Risk classification (sync): blocked if a risky pattern is matched.
//! 2. Shell resolution: fails with an actionable message when the host has
//!    no POSIX shell (Windows without Git Bash, MSYS2 or WSL).
//! 3. Syntax validation (async, `<shell> -n -c`): blocked if syntax is
//!    invalid, using the same shell that will execute the command.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use thiserror::Error;
use tokio::io::AsyncReadExt;

use apollia_core::{BashValidatorConfig, EventBusSender, SandboxProfile};

use crate::descriptor::{ToolDescriptor, ToolKind};
use crate::file_path_extractor::FilePathExtractor;
use crate::tools::bash_validator::BashValidator;
use crate::tools::risk_classifier::RiskCategory;
// Only referenced from the Unix spawn paths; the non-Unix build has no rlimits.
#[cfg(unix)]
use crate::tools::rlimits::ResourceLimits;

/// Native shell executor. Namespace isolation (PID + mount) is Linux-only.
///
/// On Linux: wraps commands with `unshare --pid --mount --fork <shell> -c` for
/// PID and mount namespace isolation, where `<shell>` is the resolved POSIX
/// shell (`/bin/sh`).
///
/// On non-Linux (macOS, Windows): executes directly via `<shell> -c` with a
/// per-invocation `tracing::warn!` to make the absence of an OS sandbox
/// impossible to miss.
///
/// Before any process is spawned, [`BashValidator`] applies risk classification
/// and syntax validation (fail fast).
///
/// An optional [`FilePathExtractor`] can be wired in via
/// [`with_file_path_extractor`](Self::with_file_path_extractor). When set, paths found
/// in the stdout of every successful command are extracted asynchronously and emitted
/// as [`apollia_core::RuntimeEvent::BashFilePathsExtracted`] (non-blocking).
pub struct BashExecutor {
    validator: BashValidator,
    file_path_extractor: Option<Arc<FilePathExtractor>>,
    event_tx: Option<EventBusSender>,
}

/// Input parameters for a bash invocation.
pub struct BashInput {
    /// Shell command interpreted by the resolved POSIX shell (`<shell> -c`).
    /// Must not be empty or whitespace-only.
    pub command: String,
    /// Hard timeout in seconds before SIGKILL. Max 300s recommended.
    pub timeout_secs: u64,
    /// Optional working directory. `None` uses the process's current directory.
    pub working_dir: Option<PathBuf>,
}

/// Result of a successful bash invocation.
#[derive(Debug)]
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
    /// `command` is empty or whitespace-only, rejected before any I/O (fail fast).
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
    /// `<shell> -n -c` reported a syntax error; the command was never executed.
    #[error("shell syntax error in `{cmd}`: {detail}")]
    SyntaxError {
        /// The command that failed syntax validation.
        cmd: String,
        /// stderr output from `<shell> -n -c`.
        detail: String,
    },
    /// A risk pattern was matched; the command was blocked before spawning.
    #[error("risky command blocked (category: {category:?}): {command}")]
    RiskyCommand {
        /// The command that triggered the risk classifier.
        command: String,
        /// The first risk category detected.
        category: RiskCategory,
    },
    /// `<shell> -n -c` did not complete within the configured timeout.
    #[error("shell syntax validation timed out")]
    SyntaxValidationTimeout,
    /// No POSIX shell is available on this host; nothing was validated or run.
    #[error(transparent)]
    ShellUnavailable(#[from] crate::tools::shell_discovery::ShellUnavailable),
}

impl BashExecutor {
    /// Creates a `BashExecutor` with default validation configuration.
    ///
    /// All risk categories are enabled but pattern lists are empty; no command is blocked
    /// without explicit operator configuration in `apollia.toml` (opt-in behaviour).
    pub fn new() -> Self {
        Self {
            validator: BashValidator::new(BashValidatorConfig::default()),
            file_path_extractor: None,
            event_tx: None,
        }
    }

    /// Creates a `BashExecutor` with a custom [`BashValidatorConfig`].
    ///
    /// Used when the operator has configured explicit risk patterns or adjusted
    /// the syntax check timeout in `apollia.toml`.
    pub fn with_config(config: BashValidatorConfig) -> Self {
        Self {
            validator: BashValidator::new(config),
            file_path_extractor: None,
            event_tx: None,
        }
    }

    /// Wires a [`FilePathExtractor`] into this executor.
    ///
    /// After every successful bash execution, paths found in stdout are extracted
    /// asynchronously via `extractor` and emitted on `event_tx` as
    /// [`apollia_core::RuntimeEvent::BashFilePathsExtracted`].
    /// The extraction never blocks `run`; it runs in a detached [`tokio::spawn`] task.
    pub fn with_file_path_extractor(
        mut self,
        extractor: Arc<FilePathExtractor>,
        event_tx: EventBusSender,
    ) -> Self {
        self.file_path_extractor = Some(extractor);
        self.event_tx = Some(event_tx);
        self
    }

    /// Returns the [`ToolDescriptor`] for registration in [`crate::registry::ToolRegistry`].
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "bash_executor".to_string(),
            version: "1.0.0".to_string(),
            description:
                "Execute a shell command. Prefer targeted, fast commands over broad scans. \
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
                                        for the task - avoid scanning entire filesystems when a \
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
            is_read_only: false,
            risk_score: 8,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        }
    }

    /// Executes a shell command through the resolved POSIX shell.
    ///
    /// # Validation order (fail fast)
    ///
    /// 1. Empty-command check (sync).
    /// 2. Working-directory existence check (sync).
    /// 3. Risk classification via [`BashValidator::classify_risks`] (sync).
    /// 4. Shell resolution via [`crate::tools::shell_discovery::resolve_posix_shell`].
    /// 5. Syntax validation via `<shell> -n -c` (async, timeout-bounded).
    /// 6. Process spawn and execution with the same shell.
    ///
    /// # Errors
    ///
    /// - [`BashExecutorError::EmptyCommand`]: `command` is blank.
    /// - [`BashExecutorError::WorkingDirNotFound`]: `working_dir` path does not exist.
    /// - [`BashExecutorError::RiskyCommand`]: a risk pattern was matched; process never spawned.
    /// - [`BashExecutorError::ShellUnavailable`]: no POSIX shell on this host; the
    ///   message names what to install (Git Bash, MSYS2 or WSL).
    /// - [`BashExecutorError::SyntaxError`]: `<shell> -n` reported a parse error.
    /// - [`BashExecutorError::SyntaxValidationTimeout`]: syntax check exceeded its timeout.
    /// - [`BashExecutorError::Timeout`]: command exceeded `timeout_secs`; child is killed.
    /// - [`BashExecutorError::SpawnFailed`]: OS refused to spawn the child.
    /// - [`BashExecutorError::OutputCaptureFailed`]: I/O error reading stdout/stderr.
    pub async fn run(&self, input: BashInput) -> Result<BashOutput, BashExecutorError> {
        // Filesystem mutations executed via shell (rm, mv, dd, …) are NOT journaled.
        // Apollia has no control over what a shell command mutates; capturing the inverse
        // is therefore impossible. Safety for bash remains upstream (RiskClassifier,
        // BashValidator, HITL).

        // Pre-capture the command string for extraction before any conditional moves.
        // The clone is skipped when no extractor is configured (common path).
        let command_for_extraction: Option<String> = self
            .file_path_extractor
            .as_ref()
            .map(|_| input.command.clone());

        // Step 1, fail fast: empty command.
        if input.command.trim().is_empty() {
            return Err(BashExecutorError::EmptyCommand);
        }

        // Step 2, fail fast: working directory existence.
        if let Some(ref dir) = input.working_dir {
            if !dir.is_dir() {
                return Err(BashExecutorError::WorkingDirNotFound(dir.clone()));
            }
        }

        // Step 3, risk classification (sync, no I/O).
        let risks = self.validator.classify_risks(&input.command);
        if let Some(category) = risks.into_iter().next() {
            tracing::warn!(
                command = %input.command,
                category = ?category,
                "bash_executor: command blocked by risk classifier"
            );
            return Err(BashExecutorError::RiskyCommand {
                command: input.command,
                category,
            });
        }

        // Step 4, resolve the single shell that validates AND executes. On a
        // host without one (Windows before Git Bash, MSYS2 or WSL is
        // installed) this fails pre-spawn with a message naming the missing
        // prerequisite, instead of the bare NotFound a hardcoded shell name
        // used to produce.
        let shell = crate::tools::shell_discovery::resolve_posix_shell()?;

        // Step 5, syntax validation (async, `<shell> -n -c`).
        self.validator
            .validate_syntax(&shell, &input.command)
            .await?;

        // Step 6, execution.
        let mut cmd = Self::build_command(&input, &shell);
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

        let output = BashOutput {
            stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
            stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
            exit_code: status.code().unwrap_or(-1),
            duration_ms,
        };

        // Non-blocking path extraction: spawned in a detached task after a successful run.
        if let (Some(extractor), Some(event_tx), Some(cmd)) = (
            &self.file_path_extractor,
            &self.event_tx,
            command_for_extraction,
        ) {
            extractor.extract_detached(cmd, output.stdout.clone(), event_tx.clone());
        }

        Ok(output)
    }

    /// Builds the OS-appropriate [`tokio::process::Command`] for the given input.
    ///
    /// `shell` is the POSIX shell already resolved by
    /// [`crate::tools::shell_discovery::resolve_posix_shell`], the same one the
    /// syntax validation ran under.
    ///
    /// On Linux: wraps with `unshare --pid --mount --fork` for namespace isolation.
    /// On non-Linux: direct `<shell> -c` with a per-invocation dev-mode warning.
    ///
    /// On every Unix platform, per-process resource limits ([`ResourceLimits`])
    /// are attached via a `pre_exec` hook. They are inherited across
    /// `unshare --fork`, so the limits reach the shell.
    #[cfg(target_os = "linux")]
    fn build_command(input: &BashInput, shell: &std::path::Path) -> tokio::process::Command {
        let mut cmd = std::process::Command::new("/usr/bin/unshare");
        cmd.args(["--pid", "--mount", "--fork"])
            .arg(shell)
            .args(["-c", &input.command]);
        crate::tools::rlimits::apply_rlimits(&mut cmd, ResourceLimits::v0_defaults());
        tokio::process::Command::from(cmd)
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    fn build_command(input: &BashInput, shell: &std::path::Path) -> tokio::process::Command {
        tracing::warn!(
            command = %input.command,
            "bash_executor: running in Dev mode - no sandbox active. \
             Linux namespaces are not available on this platform. \
             Production deployments require Linux."
        );
        let mut cmd = std::process::Command::new(shell);
        cmd.args(["-c", &input.command]);
        // The agent's shell must not inherit the desktop's embedded-Python
        // environment: a script that calls python3 would otherwise load the
        // bundle's standard library instead of the interpreter's own.
        apollia_core::subprocess_env::scrub_bundled_python(&mut cmd);
        crate::tools::rlimits::apply_rlimits(&mut cmd, ResourceLimits::v0_defaults());
        tokio::process::Command::from(cmd)
    }

    /// Off Unix, run the command through the POSIX shell found on `PATH`.
    ///
    /// Windows ships no POSIX shell, so one has to come from Git Bash, WSL or
    /// MSYS2. That is a deliberate requirement rather than a fallback to
    /// `cmd.exe` or PowerShell: the command validators guarding this tool
    /// encode POSIX quoting and chaining rules, and a different shell has a
    /// different injection surface. Hosts without one are rejected earlier in
    /// `run`, at shell resolution, with a message naming the requirement.
    #[cfg(not(unix))]
    fn build_command(input: &BashInput, shell: &std::path::Path) -> tokio::process::Command {
        tracing::warn!(
            command = %input.command,
            shell = %shell.display(),
            "bash_executor: no OS sandbox on this platform and no resource limits. \
             Production deployments require Linux."
        );
        let mut cmd = std::process::Command::new(shell);
        cmd.args(["-c", &input.command]);
        apollia_core::subprocess_env::scrub_bundled_python(&mut cmd);
        tokio::process::Command::from(cmd)
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

    #[test]
    fn code_executor_descriptor_names_stay_in_sync() {
        // GIVEN the permission layer's arbitrary-code-executor guard
        // WHEN checked against the native executor descriptor names
        // THEN both stay members, so the "no blanket allow" invariant keeps
        // covering them even if a descriptor name is later renamed.
        assert!(apollia_permissions::is_code_executor(
            &BashExecutor::descriptor().name
        ));
        assert!(apollia_permissions::is_code_executor(
            &crate::tools::python_executor::PythonExecutor::descriptor().name
        ));
    }

    /// Returns `true` if the platform can actually execute shell commands
    /// through our `build_command` path. On Linux without `CAP_SYS_ADMIN`
    /// (e.g. GitHub Actions runners), `unshare --pid --mount` fails with
    /// EPERM: these tests must be skipped gracefully.
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
    async fn test_echo_returns_stdout() {
        if !can_run_shell() {
            tracing::warn!("skipped: unshare requires CAP_SYS_ADMIN (not available on CI)");
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
    async fn test_failed_command_returns_nonzero_exit_code() {
        if !can_run_shell() {
            tracing::warn!("skipped: unshare requires CAP_SYS_ADMIN (not available on CI)");
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
    async fn test_timeout_kills_process() {
        if !can_run_shell() {
            tracing::warn!("skipped: unshare requires CAP_SYS_ADMIN (not available on CI)");
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
    async fn test_empty_command_rejected_immediately() {
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
    async fn test_whitespace_only_command_rejected() {
        // GIVEN: whitespace is not a valid command either
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
    async fn test_stderr_captured_separately() {
        if !can_run_shell() {
            tracing::warn!("skipped: unshare requires CAP_SYS_ADMIN (not available on CI)");
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
    async fn test_invalid_working_dir_returns_error() {
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

    #[tokio::test]
    async fn test_risk_classification_blocks_before_exec() {
        // GIVEN an executor with a network pattern configured
        let config = BashValidatorConfig {
            block_network_egress: true,
            network_egress_patterns: vec!["wget".to_owned()],
            ..BashValidatorConfig::default()
        };
        let executor = BashExecutor::with_config(config);
        let input = BashInput {
            command: "wget http://evil.com".to_string(),
            timeout_secs: 10,
            working_dir: None,
        };
        // WHEN
        let result = executor.run(input).await;
        // THEN: blocked before any spawn
        assert!(
            matches!(
                result,
                Err(BashExecutorError::RiskyCommand {
                    category: RiskCategory::NetworkEgress,
                    ..
                })
            ),
            "expected RiskyCommand(NetworkEgress), got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_syntax_error_blocks_before_exec() {
        // GIVEN an executor with default config (no risk patterns)
        let executor = BashExecutor::new();
        let input = BashInput {
            command: "if [ -z $VAR; then echo ok".to_string(),
            timeout_secs: 10,
            working_dir: None,
        };
        // WHEN
        let result = executor.run(input).await;
        // THEN
        assert!(
            matches!(result, Err(BashExecutorError::SyntaxError { .. })),
            "expected SyntaxError, got: {result:?}"
        );
    }

    #[test]
    fn test_descriptor_is_valid() {
        // GIVEN / WHEN
        let descriptor = BashExecutor::descriptor();
        // THEN
        assert_eq!(descriptor.name, "bash_executor");
        assert!(descriptor.validate().is_ok());
    }

    // ── FilePathExtractor integration ─────────────────────────────────────────

    #[tokio::test]
    async fn bash_executor_emits_paths_extracted_event() {
        if !can_run_shell() {
            tracing::warn!("skipped: unshare requires CAP_SYS_ADMIN (not available on CI)");
            return;
        }

        // GIVEN: executor wired with a mock extractor
        use std::collections::HashMap;
        use std::pin::Pin;
        use std::sync::Arc;

        use apollia_llm::types::{
            CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError,
            StreamChunk, TokenUsage,
        };
        use apollia_llm::LlmRouter;
        use async_trait::async_trait;
        use futures::Stream;
        use tokio::sync::broadcast;

        use apollia_core::RuntimeEvent;

        struct EchoBackend;
        #[async_trait]
        impl CompletionModel for EchoBackend {
            async fn complete(
                &self,
                _req: CompletionRequest,
            ) -> Result<CompletionResponse, LlmError> {
                Ok(CompletionResponse {
                    engine_timings: None,
                    content: "/tmp/test.txt".into(),
                    tool_calls: vec![],
                    usage: TokenUsage::default(),
                    finish_reason: FinishReason::Stop,
                    latency_ms: 1,
                    ttft_ms: None,
                })
            }

            async fn stream(
                &self,
                _req: CompletionRequest,
            ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
            {
                unimplemented!()
            }

            fn is_available(&self) -> bool {
                true
            }

            fn backend_name(&self) -> &str {
                "echo"
            }

            fn model_id(&self) -> &str {
                "echo-v1"
            }
        }

        let mut backends = HashMap::new();
        backends.insert(
            "echo".to_owned(),
            Arc::new(EchoBackend) as Arc<dyn CompletionModel>,
        );
        let router = Arc::new(LlmRouter::with_backends(backends, "echo"));
        let extractor = Arc::new(crate::file_path_extractor::FilePathExtractor::new(router));
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(16);

        let executor = BashExecutor::new().with_file_path_extractor(extractor, tx);

        let input = BashInput {
            command: "echo test.txt".to_string(),
            timeout_secs: 10,
            working_dir: None,
        };

        // WHEN
        let _output = executor.run(input).await.expect("execution must succeed");

        // THEN: BashFilePathsExtracted must arrive within 2 seconds (non-blocking)
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                match rx.recv().await {
                    Ok(RuntimeEvent::BashFilePathsExtracted { paths }) => {
                        assert!(!paths.is_empty(), "extracted paths must not be empty");
                        break;
                    }
                    Ok(_) => continue,
                    Err(e) => panic!("channel error before BashFilePathsExtracted: {e}"),
                }
            }
        })
        .await
        .expect("BashFilePathsExtracted must be emitted within 2 seconds");
    }
}
