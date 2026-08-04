//! Pre-execution validator for `BashExecutor`.
//!
//! `BashValidator` groups the two checks applied before any process spawn:
//!
//! 1. **Risk classification** (synchronous, no I/O), delegated to [`RiskClassifier`].
//!    Returns immediately if a risk pattern is detected.
//!
//! 2. **Syntax validation** (asynchronous, `<shell> -n -c`), invokes the
//!    resolved POSIX shell in dry-run mode to detect syntax errors without
//!    running the command. The shell is the caller-resolved one that will also
//!    execute the command (see `tools::shell_discovery`), so a command can
//!    never pass validation under one parser and run under another. Subject to
//!    a configurable timeout (default: 1000 ms).
//!
//! The order is imposed by the architecture: classification, then syntax, then
//! execution. This guarantees that risky commands never consume a StepBudget
//! invocation (fail fast).

use std::path::Path;
use std::time::Duration;

use apollia_core::BashValidatorConfig;
use tokio::process::Command;

use crate::tools::bash_executor::BashExecutorError;
use crate::tools::risk_classifier::{RiskCategory, RiskClassifier};

/// Pre-execution validator combining risk classification and syntax checking.
///
/// Instantiated from [`BashValidatorConfig`]; the configuration is immutable
/// after construction. `BashValidator` is a field of `BashExecutor` and must
/// not be shared between actors.
pub struct BashValidator {
    config: BashValidatorConfig,
}

impl BashValidator {
    /// Builds a `BashValidator` from its configuration.
    pub fn new(config: BashValidatorConfig) -> Self {
        Self { config }
    }

    /// Returns the risk categories detected for `cmd`.
    ///
    /// Delegates to [`RiskClassifier::classify`]. The list is empty if no risk is detected.
    pub fn classify_risks(&self, cmd: &str) -> Vec<RiskCategory> {
        RiskClassifier::classify(cmd, &self.config)
    }

    /// Validates the syntax of `cmd` via `<shell> -n -c`.
    ///
    /// Runs `<shell> -n -c <cmd>` in an isolated subprocess (dry-run mode, no
    /// command is actually executed). `shell` must be the same resolved POSIX
    /// shell that will execute the command, so validation and execution share
    /// one parser. If the shell reports a syntax error (exit code != 0),
    /// returns [`BashExecutorError::SyntaxError`] with the stderr of
    /// `<shell> -n`.
    ///
    /// The subprocess is subject to the configured `syntax_check_timeout_ms`
    /// timeout. Beyond that, the process is killed and
    /// [`BashExecutorError::SyntaxValidationTimeout`] is returned.
    ///
    /// # Errors
    ///
    /// - [`BashExecutorError::SyntaxError`]: `<shell> -n` reported a syntax error.
    /// - [`BashExecutorError::SyntaxValidationTimeout`]: timeout exceeded.
    /// - [`BashExecutorError::SpawnFailed`]: the shell could not be spawned.
    pub async fn validate_syntax(&self, shell: &Path, cmd: &str) -> Result<(), BashExecutorError> {
        let timeout = Duration::from_millis(self.config.syntax_check_timeout_ms);

        let mut validator = Command::new(shell);
        apollia_core::subprocess_env::scrub_bundled_python_async(&mut validator);
        let mut child = validator
            .args(["-n", "-c", cmd])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| BashExecutorError::SpawnFailed(e.to_string()))?;

        let mut stderr_pipe = child.stderr.take().ok_or_else(|| {
            BashExecutorError::SpawnFailed("syntax check: stderr pipe missing".to_string())
        })?;

        // Drain stderr in a background task to avoid deadlock on a full pipe.
        let stderr_task = tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            stderr_pipe.read_to_end(&mut buf).await.map(|_| buf)
        });

        let status = tokio::select! {
            result = child.wait() => {
                result.map_err(|e| BashExecutorError::SpawnFailed(e.to_string()))?
            }
            _ = tokio::time::sleep(timeout) => {
                stderr_task.abort();
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(BashExecutorError::SyntaxValidationTimeout);
            }
        };

        let stderr_bytes = stderr_task
            .await
            .map_err(|e| BashExecutorError::SpawnFailed(e.to_string()))?
            .map_err(|e| BashExecutorError::SpawnFailed(e.to_string()))?;

        if !status.success() {
            let detail = String::from_utf8_lossy(&stderr_bytes).trim().to_owned();
            return Err(BashExecutorError::SyntaxError {
                cmd: cmd.to_owned(),
                detail,
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_validator() -> BashValidator {
        BashValidator::new(BashValidatorConfig::default())
    }

    fn resolved_shell() -> std::path::PathBuf {
        crate::tools::shell_discovery::resolve_posix_shell().expect("test host has a POSIX shell")
    }

    #[tokio::test]
    async fn bash_validator_rejects_invalid_syntax() {
        // GIVEN a BashValidator with default config and the resolved shell
        let validator = default_validator();
        // WHEN a syntactically invalid shell command
        let result = validator
            .validate_syntax(&resolved_shell(), "if [ -z $VAR; then echo ok")
            .await;
        // THEN a syntax error is returned
        assert!(
            matches!(result, Err(BashExecutorError::SyntaxError { .. })),
            "expected SyntaxError, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn bash_validator_accepts_valid_complex_command() {
        // GIVEN a BashValidator with default config and the resolved shell
        let validator = default_validator();
        // WHEN a complex but syntactically correct POSIX command
        let result = validator
            .validate_syntax(&resolved_shell(), "if [ -z \"$VAR\" ]; then echo ok; fi")
            .await;
        // THEN no error
        assert!(result.is_ok(), "expected Ok, got: {result:?}");
    }

    #[tokio::test]
    async fn bash_validator_accepts_simple_command() {
        // GIVEN
        let validator = default_validator();
        // WHEN
        let result = validator
            .validate_syntax(&resolved_shell(), "echo hello world")
            .await;
        // THEN
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn bash_validator_rejects_unclosed_quote() {
        // GIVEN
        let validator = default_validator();
        // WHEN an unclosed string
        let result = validator
            .validate_syntax(&resolved_shell(), "echo \"unclosed")
            .await;
        // THEN
        assert!(matches!(result, Err(BashExecutorError::SyntaxError { .. })));
    }

    #[tokio::test]
    async fn bash_validator_uses_resolved_shell() {
        // GIVEN a validator and the shell resolved by shell_discovery, the
        // same one build_command executes with
        let validator = default_validator();
        let shell = resolved_shell();
        // WHEN validating one valid and one invalid POSIX command through it
        let valid = validator.validate_syntax(&shell, "printf '%s\\n' ok").await;
        let invalid = validator.validate_syntax(&shell, "echo \"unclosed").await;
        // THEN the resolved shell accepts the former and rejects the latter
        assert!(valid.is_ok(), "expected Ok, got: {valid:?}");
        assert!(matches!(
            invalid,
            Err(BashExecutorError::SyntaxError { .. })
        ));
    }

    #[test]
    fn bash_validator_classify_risks_delegates_to_classifier() {
        // GIVEN a validator with a network pattern configured
        let config = BashValidatorConfig {
            block_network_egress: true,
            network_egress_patterns: vec!["wget".to_owned()],
            ..BashValidatorConfig::default()
        };
        let validator = BashValidator::new(config);
        // WHEN
        let risks = validator.classify_risks("wget http://evil.com");
        // THEN
        use crate::tools::risk_classifier::RiskCategory;
        assert!(risks.contains(&RiskCategory::NetworkEgress));
    }
}
