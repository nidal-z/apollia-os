//! Write files inside the agent's sandbox.

use crate::descriptor::{ToolDescriptor, ToolKind};
use crate::sandbox_path::SandboxRoot;
use apollia_core::SandboxProfile;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use thiserror::Error;
use tokio::fs;

/// Write content to a file inside the agent's sandbox.
///
/// Creates the file and intermediate directories if they don't exist.
/// Overwrites the file if it already exists.
#[derive(Debug, Clone)]
pub struct FileWrite {
    sandbox: SandboxRoot,
}

/// Errors produced by [`FileWrite`].
#[derive(Debug, Error)]
pub enum FileWriteError {
    /// Path attempts to escape the sandbox root.
    #[error("sandbox violation: path '{path}' escapes the sandbox root")]
    SandboxViolation { path: String },

    /// I/O error while writing the file.
    #[error("I/O error on '{path}': {cause}")]
    IoError { path: String, cause: String },
}

/// Input for a file write operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWriteInput {
    /// Relative path inside the sandbox.
    pub path: String,
    /// Content to write to the file.
    pub content: String,
}

impl FileWrite {
    /// Create a new FileWrite tool instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the sandbox root cannot be initialized.
    pub fn new(sandbox_root: PathBuf) -> Result<Self, FileWriteError> {
        let sandbox = SandboxRoot::new(sandbox_root).map_err(|e| FileWriteError::IoError {
            path: "sandbox_root".to_string(),
            cause: e.to_string(),
        })?;

        Ok(Self { sandbox })
    }

    /// Execute a file write operation.
    ///
    /// Creates the file and all intermediate directories if they don't exist.
    /// Overwrites the file completely if it already exists.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is invalid or the file cannot be written.
    pub async fn run(&self, input: FileWriteInput) -> Result<(), FileWriteError> {
        let resolved_path =
            self.sandbox
                .resolve(&input.path)
                .map_err(|_| FileWriteError::SandboxViolation {
                    path: input.path.clone(),
                })?;

        // Create parent directories if they don't exist
        if let Some(parent) = resolved_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| FileWriteError::IoError {
                    path: input.path.clone(),
                    cause: e.to_string(),
                })?;
        }

        // Write the file (overwrites if exists)
        fs::write(&resolved_path, input.content.as_bytes())
            .await
            .map_err(|e| FileWriteError::IoError {
                path: input.path.clone(),
                cause: e.to_string(),
            })?;

        Ok(())
    }

    /// Return the tool descriptor for registration in the ToolRegistry.
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "file_write".to_string(),
            version: "1.0.0".to_string(),
            description: "Write content to a file inside the agent's sandbox. Creates the file and intermediate directories if they don't exist. Overwrites the file if it already exists.".to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path inside the sandbox"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write to the file"
                    }
                },
                "required": ["path", "content"]
            }),
            output_schema: None,
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec!["file".to_string(), "write".to_string()],
            dangerous: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn write_creates_file_and_directories() {
        // GIVEN: sandbox temp vide
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_write =
            FileWrite::new(temp_dir.path().to_path_buf()).expect("Failed to create FileWrite");

        // WHEN: file_write(path="deep/nested/file.txt", content="hello")
        let input = FileWriteInput {
            path: "deep/nested/file.txt".to_string(),
            content: "hello".to_string(),
        };
        let result = file_write.run(input).await;

        // THEN: le fichier existe et contient "hello"
        assert!(result.is_ok());
        let file_path = temp_dir.path().join("deep/nested/file.txt");
        assert!(file_path.exists());
        let content = tokio::fs::read_to_string(&file_path)
            .await
            .expect("Failed to read file");
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn write_overwrites_existing_file() {
        // GIVEN: fichier "test.txt" contenant "old"
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("test.txt");
        tokio::fs::write(&file_path, "old")
            .await
            .expect("Failed to create initial file");

        let file_write =
            FileWrite::new(temp_dir.path().to_path_buf()).expect("Failed to create FileWrite");

        // WHEN: file_write(path="test.txt", content="new")
        let input = FileWriteInput {
            path: "test.txt".to_string(),
            content: "new".to_string(),
        };
        let result = file_write.run(input).await;

        // THEN: le fichier contient "new" (et non "oldnew")
        assert!(result.is_ok());
        let content = tokio::fs::read_to_string(&file_path)
            .await
            .expect("Failed to read file");
        assert_eq!(content, "new");
    }

    #[tokio::test]
    async fn write_outside_sandbox_returns_violation() {
        // GIVEN: sandbox valide
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_write =
            FileWrite::new(temp_dir.path().to_path_buf()).expect("Failed to create FileWrite");

        // WHEN: file_write(path="../escape.txt", content="hack")
        let input = FileWriteInput {
            path: "../escape.txt".to_string(),
            content: "hack".to_string(),
        };
        let result = file_write.run(input).await;

        // THEN: Err(FileWriteError::SandboxViolation)
        assert!(result.is_err());
        match result.err() {
            Some(FileWriteError::SandboxViolation { path }) => {
                assert_eq!(path, "../escape.txt");
            }
            _ => panic!("Expected SandboxViolation error"),
        }
    }

    #[test]
    fn descriptor_is_valid() {
        // GIVEN: FileWrite::descriptor()
        let descriptor = FileWrite::descriptor();

        // WHEN: descriptor.validate()
        let result = descriptor.validate();

        // THEN: Ok(())
        assert!(result.is_ok());
    }
}
