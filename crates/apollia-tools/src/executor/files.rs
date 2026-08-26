//! `ToolExecutor` adapters for the file and notebook tools.
//!
//! Split out of `executor.rs`: the trait and the dispatcher stay in the
//! parent, the per-tool JSON-in / JSON-out adapters for the filesystem tools
//! live here.

use std::future::Future;
use std::pin::Pin;

use serde_json::Value;

use crate::executor::{ToolExecutionError, ToolExecutor};
use crate::tools::file_edit::{FileEdit, FileEditError, FileEditInput};
use crate::tools::file_glob::{FileGlob, FileGlobError, FileGlobInput};
use crate::tools::file_grep::{FileGrep, FileGrepError, FileGrepInput};
use crate::tools::file_list::{FileList, FileListError, FileListInput};
use crate::tools::file_read::{FileRead, FileReadError, FileReadInput};
use crate::tools::file_write::{FileWrite, FileWriteError, FileWriteInput};
use crate::tools::notebook_edit::{NotebookEdit, NotebookEditError, NotebookEditInput};
use crate::tools::notebook_read::{NotebookRead, NotebookReadError, NotebookReadInput};

impl ToolExecutor for FileRead {
    fn name(&self) -> &str {
        "file_read"
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
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
        })
    }
}
impl ToolExecutor for FileWrite {
    fn name(&self) -> &str {
        "file_write"
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
            let typed: FileWriteInput =
                serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                    message: e.to_string(),
                })?;

            self.run(typed).await.map_err(|e| {
                let code = match &e {
                    FileWriteError::SandboxViolation { .. } => "sandbox_violation",
                    FileWriteError::IoError { .. } => "io_error",
                    FileWriteError::JournalFailed(_) => "journal_failed",
                };
                ToolExecutionError::ExecutionFailed {
                    code: code.to_string(),
                    message: e.to_string(),
                }
            })?;

            Ok(serde_json::json!({}))
        })
    }
}
impl ToolExecutor for FileEdit {
    fn name(&self) -> &str {
        "file_edit"
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
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
                    FileEditError::JournalFailed(_) => "journal_failed",
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
impl ToolExecutor for FileList {
    fn name(&self) -> &str {
        "file_list"
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
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
        })
    }
}
impl ToolExecutor for FileGlob {
    fn name(&self) -> &str {
        "file_glob"
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
            let typed: FileGlobInput =
                serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                    message: e.to_string(),
                })?;

            let output = self.run(typed).await.map_err(|e| {
                let code = match &e {
                    FileGlobError::SandboxViolation { .. } => "sandbox_violation",
                    FileGlobError::InvalidPattern(_) => "invalid_pattern",
                    FileGlobError::IoError(_) => "io_error",
                    FileGlobError::GlobLimitExceeded { .. } => "glob_limit_exceeded",
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
impl ToolExecutor for FileGrep {
    fn name(&self) -> &str {
        "file_grep"
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
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
        })
    }
}
impl ToolExecutor for NotebookRead {
    fn name(&self) -> &str {
        "notebook_read"
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
            let typed: NotebookReadInput =
                serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                    message: e.to_string(),
                })?;

            let output = self.run(typed).await.map_err(|e| {
                let code = match &e {
                    NotebookReadError::SandboxViolation { .. } => "sandbox_violation",
                    NotebookReadError::NotFound { .. } => "not_found",
                    NotebookReadError::Io { .. } => "io_error",
                    NotebookReadError::InvalidNotebook => "invalid_input",
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
impl ToolExecutor for NotebookEdit {
    fn name(&self) -> &str {
        "notebook_edit"
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
            let typed: NotebookEditInput =
                serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                    message: e.to_string(),
                })?;

            let output = self.run(typed).await.map_err(|e| {
                let code = match &e {
                    NotebookEditError::SandboxViolation { .. } => "sandbox_violation",
                    NotebookEditError::NotFound { .. } => "not_found",
                    NotebookEditError::Io { .. } => "io_error",
                    NotebookEditError::InvalidNotebook => "invalid_input",
                    NotebookEditError::IndexOutOfBounds { .. } => "index_out_of_bounds",
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
