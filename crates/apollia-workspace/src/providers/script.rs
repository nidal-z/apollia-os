//! [`ScriptProvider`], a context provider backed by a shell script or binary.
//!
//! The script runs in `cwd` and must write its JSON result to stdout.
//! Expected format:
//! ```json
//! { "sections": [{"title": "...", "content": "..."}], "errors": [] }
//! ```

use std::path::{Path, PathBuf};
use std::time::Duration;

use apollia_core::workspace::{WorkspaceProvider, WorkspaceSection, WorkspaceSlice};
use serde::Deserialize;

/// Context provider that runs a script and parses its JSON output.
///
/// Fail-silent: if the script is missing, fails, or produces invalid JSON, it
/// returns a [`WorkspaceSlice::with_error`] without panicking.
pub struct ScriptProvider {
    /// Unique provider name.
    name: String,
    /// Path to the script or binary to execute.
    path: PathBuf,
    /// Execution timeout in milliseconds.
    timeout_ms: u64,
    /// Display priority in the system prompt.
    priority: u8,
}

/// JSON output expected from the script.
#[derive(Deserialize)]
struct ScriptOutput {
    #[serde(default)]
    sections: Vec<ScriptSection>,
    #[serde(default)]
    errors: Vec<String>,
}

/// JSON section produced by a script.
#[derive(Deserialize)]
struct ScriptSection {
    title: String,
    content: String,
}

impl ScriptProvider {
    /// Builds a script provider from the given parameters.
    ///
    /// `timeout_ms` is the script execution timeout. `0` disables the timeout.
    pub fn new(
        name: impl Into<String>,
        path: impl Into<PathBuf>,
        timeout_ms: u64,
        priority: u8,
    ) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            timeout_ms,
            priority,
        }
    }
}

#[async_trait::async_trait]
impl WorkspaceProvider for ScriptProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> u8 {
        self.priority
    }

    fn description(&self) -> &str {
        "Script shell ou binaire produisant du JSON sur stdout"
    }

    /// Runs the script and parses its JSON output.
    ///
    /// Returns [`WorkspaceSlice::with_error`] if the script fails, exceeds the
    /// timeout, or produces invalid JSON.
    async fn collect(&self, cwd: &Path) -> WorkspaceSlice {
        let source = self.name.clone();
        let timeout = if self.timeout_ms > 0 {
            Duration::from_millis(self.timeout_ms)
        } else {
            Duration::from_secs(30)
        };

        let output_result = tokio::time::timeout(
            timeout,
            {
                let mut script = tokio::process::Command::new(&self.path);
                apollia_core::subprocess_env::scrub_bundled_python_async(&mut script);
                script.current_dir(cwd).output()
            },
        )
        .await;

        let output = match output_result {
            Err(_elapsed) => {
                return WorkspaceSlice::with_error(
                    &source,
                    format!(
                        "script '{}' timed out after {}ms",
                        self.path.display(),
                        self.timeout_ms
                    ),
                );
            }
            Ok(Err(e)) => {
                return WorkspaceSlice::with_error(
                    &source,
                    format!("script '{}' failed to start: {}", self.path.display(), e),
                );
            }
            Ok(Ok(o)) => o,
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return WorkspaceSlice::with_error(
                &source,
                format!(
                    "script '{}' exited with {}: {}",
                    self.path.display(),
                    output.status,
                    stderr.trim()
                ),
            );
        }

        let stdout = match String::from_utf8(output.stdout) {
            Ok(s) => s,
            Err(e) => {
                return WorkspaceSlice::with_error(
                    &source,
                    format!(
                        "script '{}' output is not valid UTF-8: {}",
                        self.path.display(),
                        e
                    ),
                );
            }
        };

        let parsed: ScriptOutput = match serde_json::from_str(&stdout) {
            Ok(p) => p,
            Err(e) => {
                return WorkspaceSlice::with_error(
                    &source,
                    format!("script '{}' JSON parse error: {}", self.path.display(), e),
                );
            }
        };

        let sections: Vec<WorkspaceSection> = parsed
            .sections
            .into_iter()
            .map(|s| WorkspaceSection {
                title: s.title,
                content: s.content,
                source: source.clone(),
            })
            .collect();

        WorkspaceSlice {
            source: source.clone(),
            sections,
            errors: parsed.errors,
            collected_at: std::time::Instant::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn script_provider_missing_binary_returns_error_slice() {
        // GIVEN a provider pointing at a non-existent script path
        let tmp = tempfile::tempdir().expect("tempdir");
        let provider = ScriptProvider::new("test", "/nonexistent/script.sh", 500, 50);
        // WHEN
        let slice = provider.collect(tmp.path()).await;
        // THEN fail-silent: no panic, error recorded in slice.errors
        assert!(slice.is_empty());
        assert!(!slice.errors.is_empty());
    }
}
