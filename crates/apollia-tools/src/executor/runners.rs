//! `ToolExecutor` adapters for the network and code-execution tools.
//!
//! Split out of `executor.rs`: the trait and the dispatcher stay in the
//! parent, the per-tool JSON-in / JSON-out adapters that reach the network or
//! spawn a process live here.

use std::future::Future;
use std::pin::Pin;

use serde_json::Value;

use crate::executor::{ToolExecutionError, ToolExecutor};
use crate::tools::bash_executor::{BashExecutor, BashExecutorError, BashInput};
use crate::tools::python_executor::{PythonExecutor, PythonExecutorError, PythonInput};

#[cfg(feature = "http")]
use crate::tools::http_fetch::{HttpFetch, HttpFetchError, HttpFetchInput};
#[cfg(feature = "memory-search")]
use crate::tools::memory_search::{MemorySearchInput, MemorySearchTool, MemorySearchToolError};
#[cfg(feature = "web-read")]
use crate::tools::web_read::{WebRead, WebReadError, WebReadInput};
#[cfg(feature = "web-search")]
use crate::tools::web_search::{WebSearch, WebSearchError, WebSearchInput};

#[cfg(feature = "http")]
impl ToolExecutor for HttpFetch {
    fn name(&self) -> &str {
        "http_fetch"
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
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
                    HttpFetchError::Ssrf(_) => "ssrf_blocked",
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
        })
    }
}
#[cfg(feature = "web-search")]
impl ToolExecutor for WebSearch {
    fn name(&self) -> &str {
        "web_search"
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
            let typed: WebSearchInput =
                serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                    message: e.to_string(),
                })?;

            let output = self.run(typed).await.map_err(|e| {
                let code = match &e {
                    WebSearchError::InvalidQuery { .. } => "invalid_query",
                    WebSearchError::BackendNotAvailable { .. } => "backend_not_available",
                    WebSearchError::AllBackendsFailed { .. } => "all_backends_failed",
                    WebSearchError::NoBackends => "no_backends_available",
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
        })
    }
}
#[cfg(feature = "web-read")]
impl ToolExecutor for WebRead {
    fn name(&self) -> &str {
        "web_read"
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
            let typed: WebReadInput =
                serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                    message: e.to_string(),
                })?;

            let output = self.run(typed).await.map_err(|e| {
                let code = match &e {
                    WebReadError::InvalidUrl(_) => "invalid_url",
                    WebReadError::PrivateAddress(_) => "private_address",
                    WebReadError::UnsupportedContentType { .. } => "unsupported_content_type",
                    WebReadError::RequestFailed(_) => "request_failed",
                    WebReadError::BadStatus(_) => "bad_status",
                    WebReadError::ResponseTooLarge { .. } => "response_too_large",
                    WebReadError::Timeout(_) => "timeout",
                    WebReadError::ExtractionFailed(_) => "extraction_failed",
                    WebReadError::EmptyContent(_) => "empty_content",
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
        })
    }
}
#[cfg(feature = "memory-search")]
impl ToolExecutor for MemorySearchTool {
    fn name(&self) -> &str {
        "memory_search"
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
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
        })
    }
}
impl ToolExecutor for BashExecutor {
    fn name(&self) -> &str {
        "bash_executor"
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
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
                    BashExecutorError::SyntaxError { .. } => "syntax_error",
                    BashExecutorError::RiskyCommand { .. } => "risky_command",
                    BashExecutorError::SyntaxValidationTimeout => "syntax_validation_timeout",
                    BashExecutorError::ShellUnavailable(_) => "shell_unavailable",
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
        })
    }
}
impl ToolExecutor for PythonExecutor {
    fn name(&self) -> &str {
        "python_executor"
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
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
                    PythonExecutorError::PythonUnavailable { .. } => "python_unavailable",
                    PythonExecutorError::VenvCreationFailed(_) => "venv_creation_failed",
                    PythonExecutorError::PackageInstallFailed { .. } => "package_install_failed",
                    PythonExecutorError::InvalidPackageSpec { .. } => "invalid_package_spec",
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
        })
    }
}
