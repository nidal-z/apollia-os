//! Unified tool execution interface for native tools.
//!
//! Provides [`ToolExecutor`] — a JSON-in / JSON-out async trait — and [`ToolDispatcher`],
//! which routes calls to the correct executor by tool name.
//!
//! Each native tool struct implements [`ToolExecutor`] by deserialising the input
//! [`serde_json::Value`] into its typed input struct, delegating to its `run()` method,
//! and serialising the output back to JSON. Domain errors are mapped to
//! [`ToolExecutionError::ExecutionFailed`] with a stable `code` string.

use serde_json::Value;
use thiserror::Error;

use crate::tools::bash_executor::{BashExecutor, BashExecutorError, BashInput};
use crate::tools::file_edit::{FileEdit, FileEditError, FileEditInput};
use crate::tools::file_glob::{FileGlob, FileGlobError, FileGlobInput};
use crate::tools::file_grep::{FileGrep, FileGrepError, FileGrepInput};
use crate::tools::file_list::{FileList, FileListError, FileListInput};
use crate::tools::file_read::{FileRead, FileReadError, FileReadInput};
use crate::tools::file_write::{FileWrite, FileWriteError, FileWriteInput};
use crate::tools::python_executor::{PythonExecutor, PythonExecutorError, PythonInput};

#[cfg(feature = "http")]
use crate::tools::http_fetch::{HttpFetch, HttpFetchError, HttpFetchInput};

#[cfg(feature = "memory-search")]
use crate::tools::memory_search::{MemorySearchInput, MemorySearchTool, MemorySearchToolError};

/// Unified error type for tool execution via [`ToolExecutor`].
#[derive(Debug, Error)]
pub enum ToolExecutionError {
    /// The input JSON could not be deserialized into the expected schema.
    #[error("invalid input: {message}")]
    InvalidInput {
        /// Human-readable description of the deserialization failure.
        message: String,
    },

    /// The tool execution failed with a domain-specific error.
    #[error("execution failed: {code} — {message}")]
    ExecutionFailed {
        /// Stable snake_case error code (e.g. `"not_found"`, `"sandbox_violation"`).
        code: String,
        /// Human-readable description of the failure.
        message: String,
    },

    /// The requested tool is not registered in the dispatcher.
    #[error("unknown tool: '{name}'")]
    UnknownTool {
        /// The unrecognised tool name.
        name: String,
    },
}

/// Trait for executing a native tool via a JSON-in / JSON-out interface.
///
/// Each native tool implements this trait. The dispatcher routes by tool name
/// and delegates to the corresponding executor instance.
#[async_trait::async_trait]
pub trait ToolExecutor: Send + Sync {
    /// The unique name identifying this tool (must match the descriptor name).
    fn name(&self) -> &str;

    /// Execute the tool with the given JSON input and return the JSON output.
    ///
    /// # Errors
    ///
    /// - [`ToolExecutionError::InvalidInput`] if `input` cannot be deserialized into the
    ///   tool's expected schema.
    /// - [`ToolExecutionError::ExecutionFailed`] if the tool encounters a domain error.
    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError>;
}

/// Routes tool calls to the appropriate executor by name.
///
/// The dispatcher holds a list of pre-built executors and delegates each call to the
/// one whose [`ToolExecutor::name`] matches `tool_name`.
pub struct ToolDispatcher {
    executors: Vec<Box<dyn ToolExecutor>>,
}

impl ToolDispatcher {
    /// Create a new dispatcher from a list of pre-built executors.
    pub fn new(executors: Vec<Box<dyn ToolExecutor>>) -> Self {
        Self { executors }
    }

    /// Dispatch a tool call to the executor registered under `tool_name`.
    ///
    /// # Errors
    ///
    /// - [`ToolExecutionError::UnknownTool`] if no executor is registered for `tool_name`.
    /// - All other errors are forwarded unchanged from the matched executor.
    pub async fn dispatch(
        &self,
        tool_name: &str,
        input: Value,
    ) -> Result<Value, ToolExecutionError> {
        let executor = self
            .executors
            .iter()
            .find(|e| e.name() == tool_name)
            .ok_or_else(|| ToolExecutionError::UnknownTool {
                name: tool_name.to_string(),
            })?;

        executor.execute(input).await
    }
}

// ---------------------------------------------------------------------------
// ToolExecutor implementations — file tools
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl ToolExecutor for FileRead {
    fn name(&self) -> &str {
        "file_read"
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let typed: FileReadInput =
            serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                message: e.to_string(),
            })?;

        let output = self.run(typed).await.map_err(|e| {
            let code = match &e {
                FileReadError::SandboxViolation { .. } => "sandbox_violation",
                FileReadError::NotFound { .. } => "not_found",
                FileReadError::IoError { .. } => "io_error",
                FileReadError::BinaryFile { .. } => "binary_file",
            };
            ToolExecutionError::ExecutionFailed {
                code: code.to_string(),
                message: e.to_string(),
            }
        })?;

        serde_json::to_value(output).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialization_error".to_string(),
            message: e.to_string(),
        })
    }
}

#[async_trait::async_trait]
impl ToolExecutor for FileWrite {
    fn name(&self) -> &str {
        "file_write"
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let typed: FileWriteInput =
            serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                message: e.to_string(),
            })?;

        self.run(typed).await.map_err(|e| {
            let code = match &e {
                FileWriteError::SandboxViolation { .. } => "sandbox_violation",
                FileWriteError::IoError { .. } => "io_error",
            };
            ToolExecutionError::ExecutionFailed {
                code: code.to_string(),
                message: e.to_string(),
            }
        })?;

        Ok(serde_json::json!({}))
    }
}

#[async_trait::async_trait]
impl ToolExecutor for FileEdit {
    fn name(&self) -> &str {
        "file_edit"
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let typed: FileEditInput =
            serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                message: e.to_string(),
            })?;

        let output = self.run(typed).await.map_err(|e| {
            let code = match &e {
                FileEditError::SandboxViolation { .. } => "sandbox_violation",
                FileEditError::NotFound { .. } => "not_found",
                FileEditError::PatternNotFound { .. } => "pattern_not_found",
                FileEditError::AmbiguousMatch { .. } => "ambiguous_match",
                FileEditError::NoChange => "no_change",
                FileEditError::BinaryFile { .. } => "binary_file",
                FileEditError::IoError { .. } => "io_error",
            };
            ToolExecutionError::ExecutionFailed {
                code: code.to_string(),
                message: e.to_string(),
            }
        })?;

        serde_json::to_value(output).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialization_error".to_string(),
            message: e.to_string(),
        })
    }
}

#[async_trait::async_trait]
impl ToolExecutor for FileList {
    fn name(&self) -> &str {
        "file_list"
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let typed: FileListInput =
            serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                message: e.to_string(),
            })?;

        let output = self.run(typed).await.map_err(|e| {
            let code = match &e {
                FileListError::SandboxViolation { .. } => "sandbox_violation",
                FileListError::NotFound { .. } => "not_found",
                FileListError::NotADirectory { .. } => "not_a_directory",
                FileListError::IoError { .. } => "io_error",
            };
            ToolExecutionError::ExecutionFailed {
                code: code.to_string(),
                message: e.to_string(),
            }
        })?;

        serde_json::to_value(output).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialization_error".to_string(),
            message: e.to_string(),
        })
    }
}

#[async_trait::async_trait]
impl ToolExecutor for FileGlob {
    fn name(&self) -> &str {
        "file_glob"
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let typed: FileGlobInput =
            serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                message: e.to_string(),
            })?;

        let output = self.run(typed).await.map_err(|e| {
            let code = match &e {
                FileGlobError::SandboxViolation { .. } => "sandbox_violation",
                FileGlobError::InvalidPattern(_) => "invalid_pattern",
                FileGlobError::IoError(_) => "io_error",
            };
            ToolExecutionError::ExecutionFailed {
                code: code.to_string(),
                message: e.to_string(),
            }
        })?;

        serde_json::to_value(output).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialization_error".to_string(),
            message: e.to_string(),
        })
    }
}

#[async_trait::async_trait]
impl ToolExecutor for FileGrep {
    fn name(&self) -> &str {
        "file_grep"
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let typed: FileGrepInput =
            serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                message: e.to_string(),
            })?;

        let output = self.run(typed).await.map_err(|e| {
            let code = match &e {
                FileGrepError::SandboxViolation { .. } => "sandbox_violation",
                FileGrepError::InvalidRegex(_) => "invalid_regex",
                FileGrepError::IoError(_) => "io_error",
            };
            ToolExecutionError::ExecutionFailed {
                code: code.to_string(),
                message: e.to_string(),
            }
        })?;

        serde_json::to_value(output).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialization_error".to_string(),
            message: e.to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// ToolExecutor implementations — network tool (feature = "http")
// ---------------------------------------------------------------------------

#[cfg(feature = "http")]
#[async_trait::async_trait]
impl ToolExecutor for HttpFetch {
    fn name(&self) -> &str {
        "http_fetch"
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let typed: HttpFetchInput =
            serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                message: e.to_string(),
            })?;

        let output = self.run(typed).await.map_err(|e| {
            let code = match &e {
                HttpFetchError::HostNotAllowed { .. } => "host_not_allowed",
                HttpFetchError::NoAllowlist => "no_allowlist",
                HttpFetchError::InvalidUrl(_) => "invalid_url",
                HttpFetchError::RequestFailed(_) => "request_failed",
                HttpFetchError::ResponseTooLarge { .. } => "response_too_large",
                HttpFetchError::Timeout { .. } => "timeout",
            };
            ToolExecutionError::ExecutionFailed {
                code: code.to_string(),
                message: e.to_string(),
            }
        })?;

        serde_json::to_value(output).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialization_error".to_string(),
            message: e.to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// ToolExecutor implementations — memory tool (feature = "memory-search")
// ---------------------------------------------------------------------------

#[cfg(feature = "memory-search")]
#[async_trait::async_trait]
impl ToolExecutor for MemorySearchTool {
    fn name(&self) -> &str {
        "memory_search"
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let typed: MemorySearchInput =
            serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                message: e.to_string(),
            })?;

        let output = self.run(typed).await.map_err(|e| {
            let code = match &e {
                MemorySearchToolError::EmptyQuery => "empty_query",
                MemorySearchToolError::NamespaceNotAllowed(_) => "namespace_not_allowed",
                MemorySearchToolError::SearchFailed(_) => "search_failed",
            };
            ToolExecutionError::ExecutionFailed {
                code: code.to_string(),
                message: e.to_string(),
            }
        })?;

        serde_json::to_value(output).map_err(|e| ToolExecutionError::ExecutionFailed {
            code: "serialization_error".to_string(),
            message: e.to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// ToolExecutor implementations — process tools (manual JSON I/O)
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl ToolExecutor for BashExecutor {
    fn name(&self) -> &str {
        "bash_executor"
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let command = input["command"]
            .as_str()
            .ok_or_else(|| ToolExecutionError::InvalidInput {
                message: "missing required field 'command'".to_string(),
            })?
            .to_string();

        let timeout_secs =
            input["timeout_secs"]
                .as_u64()
                .ok_or_else(|| ToolExecutionError::InvalidInput {
                    message: "missing required field 'timeout_secs'".to_string(),
                })?;

        let working_dir = input["working_dir"].as_str().map(std::path::PathBuf::from);

        let bash_input = BashInput {
            command,
            timeout_secs,
            working_dir,
        };

        let output = self.run(bash_input).await.map_err(|e| {
            let code = match &e {
                BashExecutorError::EmptyCommand => "empty_command",
                BashExecutorError::WorkingDirNotFound(_) => "working_dir_not_found",
                BashExecutorError::Timeout { .. } => "timeout",
                BashExecutorError::SpawnFailed(_) => "spawn_failed",
                BashExecutorError::OutputCaptureFailed(_) => "output_capture_failed",
            };
            ToolExecutionError::ExecutionFailed {
                code: code.to_string(),
                message: e.to_string(),
            }
        })?;

        Ok(serde_json::json!({
            "stdout": output.stdout,
            "stderr": output.stderr,
            "exit_code": output.exit_code,
            "duration_ms": output.duration_ms,
        }))
    }
}

#[async_trait::async_trait]
impl ToolExecutor for PythonExecutor {
    fn name(&self) -> &str {
        "python_executor"
    }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
        let code = input["code"]
            .as_str()
            .ok_or_else(|| ToolExecutionError::InvalidInput {
                message: "missing required field 'code'".to_string(),
            })?
            .to_string();

        let timeout_secs =
            input["timeout_secs"]
                .as_u64()
                .ok_or_else(|| ToolExecutionError::InvalidInput {
                    message: "missing required field 'timeout_secs'".to_string(),
                })?;

        let python_input = PythonInput { code, timeout_secs };

        let output = self.run(python_input).await.map_err(|e| {
            let code_str = match &e {
                PythonExecutorError::EmptyCode => "empty_code",
                PythonExecutorError::PythonUnavailable => "python_unavailable",
                PythonExecutorError::VenvCreationFailed(_) => "venv_creation_failed",
                PythonExecutorError::PackageInstallFailed { .. } => "package_install_failed",
                PythonExecutorError::Timeout { .. } => "timeout",
                PythonExecutorError::SpawnFailed(_) => "spawn_failed",
                PythonExecutorError::OutputCaptureFailed(_) => "output_capture_failed",
                PythonExecutorError::TempFileFailed(_) => "temp_file_failed",
            };
            ToolExecutionError::ExecutionFailed {
                code: code_str.to_string(),
                message: e.to_string(),
            }
        })?;

        Ok(serde_json::json!({
            "stdout": output.stdout,
            "stderr": output.stderr,
            "exit_code": output.exit_code,
            "duration_ms": output.duration_ms,
        }))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    /// Minimal executor used to verify dispatcher routing without file I/O.
    struct EchoExecutor {
        tool_name: &'static str,
    }

    #[async_trait::async_trait]
    impl ToolExecutor for EchoExecutor {
        fn name(&self) -> &str {
            self.tool_name
        }

        async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError> {
            Ok(input)
        }
    }

    #[tokio::test]
    async fn dispatcher_routes_to_correct_executor() {
        // GIVEN: dispatcher with a single executor named "test_tool"
        let dispatcher = ToolDispatcher::new(vec![Box::new(EchoExecutor {
            tool_name: "test_tool",
        })]);
        let payload = json!({"key": "value"});

        // WHEN: dispatch("test_tool", payload)
        let result = dispatcher.dispatch("test_tool", payload.clone()).await;

        // THEN: the echo executor returns the input unchanged
        assert_eq!(result.expect("dispatch should succeed"), payload);
    }

    #[tokio::test]
    async fn dispatcher_unknown_tool_returns_error() {
        // GIVEN: dispatcher with no registered executors
        let dispatcher = ToolDispatcher::new(vec![]);

        // WHEN: dispatch("unknown", ...)
        let result = dispatcher.dispatch("unknown", json!({})).await;

        // THEN: Err(UnknownTool { name: "unknown" })
        match result {
            Err(ToolExecutionError::UnknownTool { name }) => {
                assert_eq!(name, "unknown");
            }
            other => panic!("expected UnknownTool, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn executor_invalid_input_returns_error() {
        // GIVEN: FileRead executor with a valid sandbox
        let tmp = TempDir::new().expect("tempdir");
        let executor =
            FileRead::new(tmp.path().to_path_buf()).expect("FileRead::new should succeed");

        // WHEN: execute with JSON that is missing the required "path" field
        let result = executor.execute(json!({"invalid": true})).await;

        // THEN: Err(InvalidInput)
        assert!(
            matches!(result, Err(ToolExecutionError::InvalidInput { .. })),
            "expected InvalidInput, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn executor_domain_error_maps_to_execution_failed() {
        // GIVEN: FileRead executor with an empty sandbox (no "nonexistent.txt")
        let tmp = TempDir::new().expect("tempdir");
        let executor =
            FileRead::new(tmp.path().to_path_buf()).expect("FileRead::new should succeed");

        // WHEN: execute with a valid input schema but a file that does not exist
        let result = executor.execute(json!({"path": "nonexistent.txt"})).await;

        // THEN: Err(ExecutionFailed { code: "not_found", ... })
        match result {
            Err(ToolExecutionError::ExecutionFailed { code, .. }) => {
                assert_eq!(code, "not_found");
            }
            other => panic!("expected ExecutionFailed(not_found), got: {other:?}"),
        }
    }
}
