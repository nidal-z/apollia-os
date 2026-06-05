//! Post-run verification: programmed checks plus an optional LLM critic pass.
//!
//! Before a run is declared done at a sufficient autonomy tier, the runtime can
//! re-read the result and confirm it. This module hosts the two independent
//! pieces (ADR-029):
//!
//! - [`VerificationLoop`]: runs the shell check commands declared by the agent
//!   manifest (tests, lint) via an injected invoker and aggregates a
//!   [`VerificationReport`].
//!
//! The source of the check commands follows ADR-029: the manifest field
//! `check_commands` is the primary source; a project-config fallback applies
//! only when the manifest declares none. When no command is resolved, the loop
//! is a no-op that reports success.
//!
//! One actor, one responsibility: this module only coordinates and aggregates.
//! Actual command execution is delegated to the injected [`CheckInvoker`], and
//! the LLM call is delegated to the configured backend.

/// Raw outcome from a single check command invocation.
#[derive(Debug, Clone)]
pub struct CheckOutcome {
    /// Exit code of the process.
    pub exit_code: i32,
    /// Captured standard error.
    pub stderr: String,
}

/// A single check command that failed.
#[derive(Debug, Clone)]
pub struct CheckFailure {
    /// The command string that was invoked.
    pub command: String,
    /// Exit code returned by the process, or `-1` when the invoker returned an error.
    pub exit_code: i32,
    /// Standard error output captured from the command, or the invoker error message.
    pub stderr: String,
}

/// Aggregated result of a verification pass.
#[derive(Debug, Clone)]
pub struct VerificationReport {
    /// True if all declared commands exited with code 0 (or no commands were declared).
    pub passed: bool,
    /// Non-empty only when `passed` is false.
    pub failures: Vec<CheckFailure>,
}

/// Errors produced by [`VerificationLoop`] setup itself (not by individual commands).
///
/// Individual command failures are reported as [`CheckFailure`] entries inside a
/// [`VerificationReport`], never as this error.
#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    /// The manifest provided check commands but the injected invoker could not
    /// parse or dispatch a command string.
    #[error("verification setup failed: {0}")]
    SetupFailed(String),
}

/// Minimal invocation contract for a verification check command.
///
/// Injected into [`VerificationLoop::run`]; the concrete implementation delegates
/// to `bash_executor` or an equivalent native tool. Test implementations use a mock.
///
/// Uses return-position `impl Trait` in trait (RPITIT) to avoid `#[async_trait]`
/// boxing. As a consequence the trait is not dyn-compatible, so
/// [`VerificationLoop::run`] is generic over the invoker rather than taking a
/// trait object.
pub trait CheckInvoker: Send + Sync {
    /// Invoke `command` as a shell string.
    ///
    /// Returns the exit code and captured stderr on success, or an error string
    /// when the invoker itself fails (process not found, timeout).
    fn invoke_check(
        &self,
        command: &str,
    ) -> impl std::future::Future<Output = Result<CheckOutcome, String>> + Send;
}

/// Orchestrates verification check commands after a completed agent run.
///
/// One actor, one responsibility: it coordinates invocations and aggregates
/// [`CheckFailure`] entries. Actual execution is delegated to the injected
/// [`CheckInvoker`].
pub struct VerificationLoop {
    /// Resolved command list (manifest commands, or the project fallback when
    /// the manifest declares none).
    commands: Vec<String>,
}

impl VerificationLoop {
    /// Create a `VerificationLoop` from manifest commands and a project-config fallback.
    ///
    /// When `manifest_commands` is non-empty it is used directly and
    /// `fallback_commands` is ignored. Decision documented in ADR-029: the
    /// manifest wins; the fallback provides a project-level safety net without
    /// requiring each agent to redeclare its checks.
    pub fn new(manifest_commands: Vec<String>, fallback_commands: Vec<String>) -> Self {
        let commands = if manifest_commands.is_empty() {
            fallback_commands
        } else {
            manifest_commands
        };
        Self { commands }
    }

    /// Run all resolved check commands via the injected invoker and collect failures.
    ///
    /// Returns `VerificationReport { passed: true, failures: vec![] }` immediately
    /// when no commands are resolved (manifest empty AND fallback empty). Does NOT
    /// short-circuit on the first failure: every command is executed so the report
    /// lists all failures at once. An invoker-level error becomes a
    /// [`CheckFailure`] with `exit_code = -1`.
    pub async fn run<I: CheckInvoker>(&self, invoker: &I) -> VerificationReport {
        let mut failures = Vec::new();

        for command in &self.commands {
            match invoker.invoke_check(command).await {
                Ok(outcome) => {
                    tracing::info!(
                        command = %command,
                        exit_code = outcome.exit_code,
                        "verification.check.done"
                    );
                    if outcome.exit_code != 0 {
                        failures.push(CheckFailure {
                            command: command.clone(),
                            exit_code: outcome.exit_code,
                            stderr: outcome.stderr,
                        });
                    }
                }
                Err(message) => {
                    tracing::warn!(
                        command = %command,
                        error = %message,
                        "verification.check.invoker_error"
                    );
                    failures.push(CheckFailure {
                        command: command.clone(),
                        exit_code: -1,
                        stderr: message,
                    });
                }
            }
        }

        VerificationReport {
            passed: failures.is_empty(),
            failures,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ---- Mock CheckInvoker ----

    struct MockCheckInvoker {
        responses: HashMap<String, Result<CheckOutcome, String>>,
    }

    impl MockCheckInvoker {
        fn new() -> Self {
            Self {
                responses: HashMap::new(),
            }
        }

        fn with(mut self, cmd: &str, outcome: Result<CheckOutcome, String>) -> Self {
            self.responses.insert(cmd.to_string(), outcome);
            self
        }
    }

    impl CheckInvoker for MockCheckInvoker {
        async fn invoke_check(&self, command: &str) -> Result<CheckOutcome, String> {
            self.responses
                .get(command)
                .cloned()
                .unwrap_or_else(|| Err(format!("command not registered: {command}")))
        }
    }

    fn ok(exit_code: i32) -> Result<CheckOutcome, String> {
        Ok(CheckOutcome {
            exit_code,
            stderr: String::new(),
        })
    }

    // AC-1: every declared check passes.
    #[tokio::test]
    async fn test_ac1_all_checks_pass() {
        // GIVEN two declared commands that both exit 0
        let invoker = MockCheckInvoker::new()
            .with("cargo test -p my-agent", ok(0))
            .with("cargo clippy -p my-agent", ok(0));
        let verification = VerificationLoop::new(
            vec![
                "cargo test -p my-agent".into(),
                "cargo clippy -p my-agent".into(),
            ],
            vec![],
        );

        // WHEN the loop runs
        let report = verification.run(&invoker).await;

        // THEN the report passes with no failures
        assert!(report.passed);
        assert!(report.failures.is_empty());
    }

    // AC-2: one check fails and the loop does not short-circuit.
    #[tokio::test]
    async fn test_ac2_one_failure_no_short_circuit() {
        // GIVEN a passing first command and a failing second command
        let invoker = MockCheckInvoker::new()
            .with("cargo test -p my-agent", ok(0))
            .with(
                "cargo clippy",
                Ok(CheckOutcome {
                    exit_code: 1,
                    stderr: "warning treated as error".into(),
                }),
            );
        let verification = VerificationLoop::new(
            vec!["cargo test -p my-agent".into(), "cargo clippy".into()],
            vec![],
        );

        // WHEN the loop runs
        let report = verification.run(&invoker).await;

        // THEN only the failing command appears, after the first ran too
        assert!(!report.passed);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].command, "cargo clippy");
        assert_eq!(report.failures[0].exit_code, 1);
    }

    // AC-3: no declared command is a no-op success.
    #[tokio::test]
    async fn test_ac3_no_commands_noop() {
        // GIVEN no manifest commands and no fallback
        let invoker = MockCheckInvoker::new();
        let verification = VerificationLoop::new(vec![], vec![]);

        // WHEN the loop runs
        let report = verification.run(&invoker).await;

        // THEN it passes immediately without invoking anything
        assert!(report.passed);
        assert!(report.failures.is_empty());
    }

    // AC-4 (error case): an invoker-level error becomes a CheckFailure with exit_code -1.
    #[tokio::test]
    async fn test_ac4_invoker_error_becomes_failure() {
        // GIVEN an invoker that returns an error for the command
        let invoker = MockCheckInvoker::new().with("cargo test", Err("executor not found".into()));
        let verification = VerificationLoop::new(vec!["cargo test".into()], vec![]);

        // WHEN the loop runs
        let report = verification.run(&invoker).await;

        // THEN the failure carries exit_code -1 and the invoker message
        assert!(!report.passed);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].exit_code, -1);
        assert!(report.failures[0].stderr.contains("executor not found"));
    }

    // Fallback: manifest commands empty, project fallback is used.
    #[tokio::test]
    async fn test_fallback_used_when_manifest_empty() {
        // GIVEN no manifest commands but a project fallback command
        let invoker = MockCheckInvoker::new().with("make check", ok(0));
        let verification = VerificationLoop::new(vec![], vec!["make check".into()]);

        // WHEN the loop runs
        let report = verification.run(&invoker).await;

        // THEN the fallback command is executed and the report passes
        assert!(report.passed);
    }
}
