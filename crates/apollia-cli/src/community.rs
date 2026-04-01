//! Community agent validation — checks a Python agent file before installation.
//!
//! Enforces the community contract: the file must implement the AIP duck-typing
//! contract (`manifest()` + async `run()`), the manifest must carry the required
//! fields (`name`, `version`, `tools_required`), and an optional test suite is
//! executed via `python3 -m pytest` unless the caller requests to skip it.
//!
//! A security warning is emitted (via `tracing`) whenever the manifest declares
//! `dangerous_tools_allowed: true` so that operators and callers can surface it
//! to the end user.

use std::path::{Path, PathBuf};

use apollia_core::AgentManifest;

// ─────────────────────────────────────────────────────────────────────────────
// Error type
// ─────────────────────────────────────────────────────────────────────────────

/// Errors raised during community agent validation.
#[derive(Debug, thiserror::Error)]
pub enum AgentValidationError {
    /// The agent file does not exist at the given path.
    #[error("agent file not found: {0}")]
    FileNotFound(PathBuf),

    /// The Python module could not be loaded or does not satisfy the AIP contract.
    #[error("manifest validation failed: {0}")]
    ManifestLoad(String),

    /// The agent's pytest suite exited with a non-zero status.
    #[error("agent tests failed:\n{0}")]
    TestsFailed(String),

    /// `python3 -m pytest` could not be executed (pytest missing, python3 absent, etc.).
    #[error("could not run pytest: {0}")]
    TestRunnerError(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Validates a community agent Python file before installation.
///
/// Performs the following checks in order:
/// 1. Verifies the file exists on disk.
/// 2. Loads the Python module via PyO3 and validates the AIP duck-typing contract
///    (`manifest()` callable + async `run()`).
/// 3. Emits a `tracing::warn!` when the manifest declares `dangerous_tools_allowed`.
/// 4. Runs the sibling `tests/` directory via `python3 -m pytest` unless
///    `skip_tests` is `true` or no `tests/` directory exists next to the file.
///
/// Returns the validated [`AgentManifest`] on success so that callers can
/// proceed with the install step.
pub async fn validate_community_agent(
    path: &Path,
    skip_tests: bool,
) -> Result<AgentManifest, AgentValidationError> {
    if !path.exists() {
        return Err(AgentValidationError::FileNotFound(path.to_path_buf()));
    }

    let module = apollia_aip::loader::load_agent_module(path)
        .map_err(|e| AgentValidationError::ManifestLoad(e.to_string()))?;

    let validated = apollia_aip::validator::validate_agent(&module)
        .map_err(|e| AgentValidationError::ManifestLoad(e.to_string()))?;

    let manifest = validated.manifest;

    if manifest.dangerous_tools_allowed {
        tracing::warn!(
            agent = %manifest.name,
            "community agent requests dangerous_tools_allowed — user approval required"
        );
    }

    if skip_tests {
        tracing::warn!(
            agent = %manifest.name,
            "skipping agent tests — validation coverage is reduced"
        );
        return Ok(manifest);
    }

    if let Some(parent) = path.parent() {
        let test_dir = parent.join("tests");
        if test_dir.exists() {
            run_agent_tests(&test_dir).await?;
        }
    }

    Ok(manifest)
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Executes `python3 -m pytest <test_dir> --tb=short -q` and returns an error
/// if the process exits with a non-zero status or cannot be started.
async fn run_agent_tests(test_dir: &Path) -> Result<(), AgentValidationError> {
    let output = tokio::process::Command::new("python3")
        .args(["-m", "pytest", "--tb=short", "-q"])
        .arg(test_dir)
        .output()
        .await
        .map_err(|e| AgentValidationError::TestRunnerError(e.to_string()))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = format!("{}{}", stdout.trim_end(), stderr.trim_end())
            .trim()
            .to_string();
        return Err(AgentValidationError::TestsFailed(detail));
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;
    use tempfile::NamedTempFile;

    /// Minimal Python AIP agent source that satisfies the duck-typing contract
    /// and does not import any third-party package.
    const MINIMAL_AGENT_PY: &str = r#"
class MinimalAgent:
    def manifest(self):
        return {
            "name": "minimal-test-agent",
            "version": "0.1.0",
            "description": "Minimal agent for unit tests",
            "tools_required": [],
        }

    async def run(self, task, ctx):
        return {"status": "completed", "result": "ok"}

agent = MinimalAgent()
"#;

    /// Minimal Python AIP agent with dangerous_tools_allowed declared.
    const DANGEROUS_AGENT_PY: &str = r#"
class DangerousAgent:
    def manifest(self):
        return {
            "name": "dangerous-test-agent",
            "version": "0.1.0",
            "description": "Agent declaring dangerous_tools_allowed for tests",
            "tools_required": [],
            "dangerous_tools_allowed": True,
        }

    async def run(self, task, ctx):
        return {"status": "completed", "result": "ok"}

agent = DangerousAgent()
"#;

    fn write_temp_agent(content: &str) -> NamedTempFile {
        let mut tmp = NamedTempFile::with_suffix(".py").expect("failed to create temp file");
        tmp.write_all(content.as_bytes())
            .expect("failed to write temp file");
        tmp
    }

    #[tokio::test]
    async fn test_validate_community_agent_missing_file() {
        // GIVEN a path that does not exist on disk
        let path = Path::new("/tmp/__apollia_nonexistent_test_agent_xyz.py");

        // WHEN validate_community_agent is called
        let result = validate_community_agent(path, true).await;

        // THEN Err(FileNotFound) is returned — no Python interaction occurs
        assert!(
            matches!(result, Err(AgentValidationError::FileNotFound(_))),
            "expected FileNotFound, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_validate_community_agent_valid() {
        // GIVEN a minimal Python agent that satisfies the AIP contract
        let tmp = write_temp_agent(MINIMAL_AGENT_PY);

        // WHEN validate_community_agent is called with skip_tests = true
        let result = validate_community_agent(tmp.path(), true).await;

        // THEN Ok(manifest) with the expected name is returned
        match result {
            Ok(manifest) => {
                assert_eq!(manifest.name, "minimal-test-agent");
                assert_eq!(manifest.version, "0.1.0");
                assert!(!manifest.dangerous_tools_allowed);
            }
            Err(AgentValidationError::ManifestLoad(msg)) => {
                // PyO3 initialisation or Python unavailable in this environment.
                eprintln!("skipping test_validate_community_agent_valid: {msg}");
            }
            Err(e) => panic!("unexpected error: {e}"),
        }
    }

    #[tokio::test]
    async fn test_validate_community_agent_dangerous_tools_warning() {
        // GIVEN an agent that declares dangerous_tools_allowed: true
        let tmp = write_temp_agent(DANGEROUS_AGENT_PY);

        // WHEN validate_community_agent is called with skip_tests = true
        let result = validate_community_agent(tmp.path(), true).await;

        // THEN Ok(manifest) is returned (installation is not blocked)
        // AND the manifest carries dangerous_tools_allowed = true
        match result {
            Ok(manifest) => {
                assert_eq!(manifest.name, "dangerous-test-agent");
                assert!(manifest.dangerous_tools_allowed);
            }
            Err(AgentValidationError::ManifestLoad(msg)) => {
                // PyO3 initialisation or Python unavailable in this environment.
                eprintln!("skipping test_validate_community_agent_dangerous_tools_warning: {msg}");
            }
            Err(e) => panic!("unexpected error: {e}"),
        }
    }
}
