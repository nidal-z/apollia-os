//! Write files inside the agent's sandbox.

use crate::descriptor::{ToolDescriptor, ToolKind};
use crate::journal::{JournalEntry, JournalError, JournalWriterHandle};
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
///
/// When a [`JournalWriterHandle`] is attached (via [`FileWrite::with_journal`]),
/// the previous state of the file is persisted to the reversible journal
/// before the write takes place. The mutation is aborted if the journal
/// write fails.
#[derive(Debug, Clone)]
pub struct FileWrite {
    sandbox: SandboxRoot,
    journal: Option<JournalWriterHandle>,
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

    /// Journal write failed, mutation aborted to preserve safety invariant.
    #[error("journal write failed before mutation: {0}")]
    JournalFailed(#[from] JournalError),
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

        Ok(Self {
            sandbox,
            journal: None,
        })
    }

    /// Attach a reversible journal to this tool instance.
    ///
    /// When set, the previous state of every file is persisted to the journal
    /// before each write. The write is aborted if the journal entry cannot be
    /// durably written.
    pub fn with_journal(mut self, handle: JournalWriterHandle) -> Self {
        self.journal = Some(handle);
        self
    }

    /// Execute a file write operation.
    ///
    /// Creates the file and all intermediate directories if they don't exist.
    /// Overwrites the file completely if it already exists.
    ///
    /// If a journal handle is set, the previous file state is recorded before
    /// the write. The mutation is aborted on journal failure.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is invalid, the journal fails, or the
    /// file cannot be written.
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

        // Journal the previous state before any mutation
        if let Some(handle) = &self.journal {
            let previous_content = match fs::read(&resolved_path).await {
                Ok(bytes) => Some(bytes),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => {
                    return Err(FileWriteError::IoError {
                        path: input.path.clone(),
                        cause: e.to_string(),
                    })
                }
            };

            let (previous_mode, previous_mtime) = match fs::metadata(&resolved_path).await {
                Ok(meta) => (
                    crate::journal::mode_from_metadata(&meta),
                    crate::journal::mtime_from_metadata(&meta),
                ),
                Err(_) => (None, None),
            };

            let entry = JournalEntry::Write {
                path: resolved_path.clone(),
                previous_content,
                previous_mode,
                previous_mtime,
            };

            // Abort if journal write fails: do not proceed with mutation
            handle.record(entry).await?;
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
            is_read_only: false,
            risk_score: 5,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::{rollback_session, JournalWriter};
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

    // ── Journal integration ──────────────────────────────────────────────────

    #[tokio::test]
    async fn write_with_journal_records_and_rollback_restores() {
        // GIVEN a file "data.txt" containing "old content"
        let sandbox = TempDir::new().expect("sandbox dir");
        let journal_root = TempDir::new().expect("journal dir");

        let file_path = sandbox.path().join("data.txt");
        tokio::fs::write(&file_path, b"old content")
            .await
            .expect("seed file");

        let journal_handle = JournalWriter::spawn(
            "sess-fw1".to_string(),
            journal_root.path().to_path_buf(),
            50,
        );

        let tool = FileWrite::new(sandbox.path().to_path_buf())
            .expect("tool")
            .with_journal(journal_handle.clone());

        // WHEN writing "new content"
        tool.run(FileWriteInput {
            path: "data.txt".to_string(),
            content: "new content".to_string(),
        })
        .await
        .expect("write ok");

        journal_handle.shutdown().await;

        // THEN the file contains "new content"
        let on_disk = tokio::fs::read_to_string(&file_path).await.expect("read");
        assert_eq!(on_disk, "new content");

        // AND rollback restores "old content"
        rollback_session(journal_root.path(), "sess-fw1", false)
            .await
            .expect("rollback ok");

        let restored = tokio::fs::read_to_string(&file_path)
            .await
            .expect("read restored");
        assert_eq!(restored, "old content");
    }

    #[tokio::test]
    async fn write_with_journal_new_file_rollback_removes_it() {
        // GIVEN no pre-existing file
        let sandbox = TempDir::new().expect("sandbox dir");
        let journal_root = TempDir::new().expect("journal dir");

        let file_path = sandbox.path().join("new.txt");

        let journal_handle = JournalWriter::spawn(
            "sess-fw2".to_string(),
            journal_root.path().to_path_buf(),
            50,
        );

        let tool = FileWrite::new(sandbox.path().to_path_buf())
            .expect("tool")
            .with_journal(journal_handle.clone());

        // WHEN creating a new file
        tool.run(FileWriteInput {
            path: "new.txt".to_string(),
            content: "hello".to_string(),
        })
        .await
        .expect("write ok");

        journal_handle.shutdown().await;
        assert!(file_path.exists());

        // THEN rolling back removes the newly created file
        rollback_session(journal_root.path(), "sess-fw2", false)
            .await
            .expect("rollback ok");

        assert!(!file_path.exists(), "file should be removed on rollback");
    }
}
