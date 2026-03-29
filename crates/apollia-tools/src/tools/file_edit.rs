//! Surgical text replacement in files inside the agent's sandbox.

use crate::descriptor::{ToolDescriptor, ToolKind};
use crate::sandbox_path::SandboxRoot;
use apollia_core::SandboxProfile;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncReadExt;

/// Perform surgical text replacement in a file inside the sandbox.
///
/// Finds `old_text` in the file and replaces it with `new_text`.
/// By default, replaces only the first occurrence. Set `replace_all`
/// to replace every occurrence (useful for variable renaming).
///
/// Fails fast before any write if `old_text` is not found, is ambiguous
/// (multiple matches when `replace_all` is false), or equals `new_text`.
#[derive(Debug, Clone)]
pub struct FileEdit {
    sandbox: SandboxRoot,
}

/// Errors produced by [`FileEdit`].
#[derive(Debug, Error)]
pub enum FileEditError {
    /// Path attempts to escape the sandbox root.
    #[error("sandbox violation: path '{path}' escapes the sandbox root")]
    SandboxViolation { path: String },

    /// File not found at the specified path.
    #[error("not found: '{path}'")]
    NotFound { path: String },

    /// `old_text` does not appear anywhere in the file.
    #[error("old_text not found in file '{path}'")]
    PatternNotFound { path: String },

    /// `old_text` appears more than once and `replace_all` is false.
    #[error("old_text matches {count} times in '{path}' — use replace_all=true or provide more context to make it unique")]
    AmbiguousMatch { path: String, count: usize },

    /// `old_text` and `new_text` are identical — no edit would occur.
    #[error("old_text and new_text are identical")]
    NoChange,

    /// File contains non-UTF-8 data (binary file).
    #[error("file is binary (not valid UTF-8): '{path}'")]
    BinaryFile { path: String },

    /// I/O error while reading or writing the file.
    #[error("I/O error on '{path}': {cause}")]
    IoError { path: String, cause: String },
}

/// Input for a file edit operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEditInput {
    /// Relative path inside the sandbox.
    pub path: String,
    /// Exact text to locate in the file.
    pub old_text: String,
    /// Replacement text.
    pub new_text: String,
    /// When `true`, replace every occurrence. When `false` (default),
    /// fail with [`FileEditError::AmbiguousMatch`] if more than one match is found.
    pub replace_all: bool,
}

/// Output of a file edit operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEditOutput {
    /// Number of replacements performed.
    pub replacements: usize,
}

impl FileEdit {
    /// Create a new `FileEdit` tool instance bound to `sandbox_root`.
    ///
    /// # Errors
    ///
    /// Returns [`FileEditError::IoError`] if the sandbox root cannot be initialized.
    pub fn new(sandbox_root: PathBuf) -> Result<Self, FileEditError> {
        let sandbox = SandboxRoot::new(sandbox_root).map_err(|e| FileEditError::IoError {
            path: "sandbox_root".to_string(),
            cause: e.to_string(),
        })?;
        Ok(Self { sandbox })
    }

    /// Execute a file edit operation.
    ///
    /// Validation order: `NoChange` → `SandboxViolation` → `NotFound` →
    /// `BinaryFile` → `PatternNotFound` / `AmbiguousMatch` → replace → write.
    ///
    /// # Errors
    ///
    /// See [`FileEditError`] for the full list of failure modes.
    pub async fn run(&self, input: FileEditInput) -> Result<FileEditOutput, FileEditError> {
        // 1. NoChange — caught before any I/O
        if input.old_text == input.new_text {
            return Err(FileEditError::NoChange);
        }

        // 2. SandboxViolation
        let resolved =
            self.sandbox
                .resolve(&input.path)
                .map_err(|_| FileEditError::SandboxViolation {
                    path: input.path.clone(),
                })?;

        // 3. NotFound
        let mut file = fs::File::open(&resolved)
            .await
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => FileEditError::NotFound {
                    path: input.path.clone(),
                },
                _ => FileEditError::IoError {
                    path: input.path.clone(),
                    cause: e.to_string(),
                },
            })?;

        let mut raw = Vec::new();
        file.read_to_end(&mut raw)
            .await
            .map_err(|e| FileEditError::IoError {
                path: input.path.clone(),
                cause: e.to_string(),
            })?;

        // 4. BinaryFile
        let content = String::from_utf8(raw).map_err(|_| FileEditError::BinaryFile {
            path: input.path.clone(),
        })?;

        // 5. PatternNotFound / AmbiguousMatch
        let count = content.matches(input.old_text.as_str()).count();
        if count == 0 {
            return Err(FileEditError::PatternNotFound {
                path: input.path.clone(),
            });
        }
        if count > 1 && !input.replace_all {
            return Err(FileEditError::AmbiguousMatch {
                path: input.path.clone(),
                count,
            });
        }

        // 6. Replace
        let (updated, replacements) = if input.replace_all {
            let updated = content.replace(input.old_text.as_str(), input.new_text.as_str());
            (updated, count)
        } else {
            let updated = content.replacen(input.old_text.as_str(), input.new_text.as_str(), 1);
            (updated, 1)
        };

        // 7. Write back
        fs::write(&resolved, updated.as_bytes())
            .await
            .map_err(|e| FileEditError::IoError {
                path: input.path.clone(),
                cause: e.to_string(),
            })?;

        Ok(FileEditOutput { replacements })
    }

    /// Return the tool descriptor for registration in the `ToolRegistry`.
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "file_edit".to_string(),
            version: "1.0.0".to_string(),
            description:
                "Replace exact text in a file inside the agent's sandbox. \
                 Fails if old_text is not found or matches multiple times (unless replace_all=true)."
                    .to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path inside the sandbox"
                    },
                    "old_text": {
                        "type": "string",
                        "description": "Exact text to locate in the file"
                    },
                    "new_text": {
                        "type": "string",
                        "description": "Replacement text"
                    },
                    "replace_all": {
                        "type": "boolean",
                        "description": "Replace every occurrence instead of requiring a unique match",
                        "default": false
                    }
                },
                "required": ["path", "old_text", "new_text"]
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "replacements": {
                        "type": "integer",
                        "description": "Number of replacements performed"
                    }
                },
                "required": ["replacements"]
            })),
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec!["file".to_string(), "edit".to_string()],
            dangerous: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_sandbox_with_file(content: &[u8]) -> (TempDir, FileEdit, String) {
        let temp_dir = TempDir::new().expect("temp dir");
        let file_path = temp_dir.path().join("test.txt");
        let mut f = std::fs::File::create(&file_path).expect("create file");
        f.write_all(content).expect("write file");
        let tool = FileEdit::new(temp_dir.path().to_path_buf()).expect("FileEdit");
        (temp_dir, tool, "test.txt".to_string())
    }

    #[tokio::test]
    async fn edit_unique_match_succeeds() {
        // GIVEN: file containing "hello world"
        let (_dir, tool, path) = create_sandbox_with_file(b"hello world");

        // WHEN: file_edit(old_text="hello", new_text="goodbye")
        let result = tool
            .run(FileEditInput {
                path: path.clone(),
                old_text: "hello".to_string(),
                new_text: "goodbye".to_string(),
                replace_all: false,
            })
            .await;

        // THEN: Ok(replacements=1) and file contains "goodbye world"
        let out = result.expect("should succeed");
        assert_eq!(out.replacements, 1);
        let on_disk = tokio::fs::read_to_string(_dir.path().join(&path))
            .await
            .expect("read back");
        assert_eq!(on_disk, "goodbye world");
    }

    #[tokio::test]
    async fn edit_ambiguous_match_fails() {
        // GIVEN: file containing "foo bar foo baz"
        let (_dir, tool, path) = create_sandbox_with_file(b"foo bar foo baz");

        // WHEN: file_edit(old_text="foo", new_text="qux", replace_all=false)
        let result = tool
            .run(FileEditInput {
                path,
                old_text: "foo".to_string(),
                new_text: "qux".to_string(),
                replace_all: false,
            })
            .await;

        // THEN: Err(AmbiguousMatch { count: 2 })
        match result {
            Err(FileEditError::AmbiguousMatch { count, .. }) => assert_eq!(count, 2),
            other => panic!("expected AmbiguousMatch, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn edit_replace_all_succeeds() {
        // GIVEN: file containing "foo bar foo baz"
        let (_dir, tool, path) = create_sandbox_with_file(b"foo bar foo baz");

        // WHEN: file_edit(old_text="foo", new_text="qux", replace_all=true)
        let result = tool
            .run(FileEditInput {
                path: path.clone(),
                old_text: "foo".to_string(),
                new_text: "qux".to_string(),
                replace_all: true,
            })
            .await;

        // THEN: Ok(replacements=2) and file contains "qux bar qux baz"
        let out = result.expect("should succeed");
        assert_eq!(out.replacements, 2);
        let on_disk = tokio::fs::read_to_string(_dir.path().join(&path))
            .await
            .expect("read back");
        assert_eq!(on_disk, "qux bar qux baz");
    }

    #[tokio::test]
    async fn edit_pattern_not_found_fails() {
        // GIVEN: file containing "hello world"
        let (_dir, tool, path) = create_sandbox_with_file(b"hello world");

        // WHEN: file_edit(old_text="missing")
        let result = tool
            .run(FileEditInput {
                path,
                old_text: "missing".to_string(),
                new_text: "x".to_string(),
                replace_all: false,
            })
            .await;

        // THEN: Err(PatternNotFound)
        assert!(matches!(result, Err(FileEditError::PatternNotFound { .. })));
    }

    #[tokio::test]
    async fn edit_no_change_fails() {
        // GIVEN: any file
        let (_dir, tool, path) = create_sandbox_with_file(b"same content");

        // WHEN: file_edit(old_text="same", new_text="same")
        let result = tool
            .run(FileEditInput {
                path,
                old_text: "same".to_string(),
                new_text: "same".to_string(),
                replace_all: false,
            })
            .await;

        // THEN: Err(NoChange)
        assert!(matches!(result, Err(FileEditError::NoChange)));
    }

    #[tokio::test]
    async fn edit_binary_file_fails() {
        // GIVEN: binary file (non-UTF-8)
        let (_dir, tool, path) = create_sandbox_with_file(&[0xFF, 0xFE, 0xFD, 0xFC]);

        // WHEN: file_edit on this file
        let result = tool
            .run(FileEditInput {
                path,
                old_text: "anything".to_string(),
                new_text: "x".to_string(),
                replace_all: false,
            })
            .await;

        // THEN: Err(BinaryFile)
        assert!(matches!(result, Err(FileEditError::BinaryFile { .. })));
    }

    #[tokio::test]
    async fn edit_verifies_disk_write() {
        // GIVEN: file containing "before"
        let (_dir, tool, path) = create_sandbox_with_file(b"before");

        // WHEN: file_edit(old_text="before", new_text="after")
        tool.run(FileEditInput {
            path: path.clone(),
            old_text: "before".to_string(),
            new_text: "after".to_string(),
            replace_all: false,
        })
        .await
        .expect("should succeed");

        // THEN: reading the file confirms "after"
        let on_disk = tokio::fs::read_to_string(_dir.path().join(&path))
            .await
            .expect("read back");
        assert_eq!(on_disk, "after");
    }

    #[test]
    fn descriptor_is_valid() {
        // GIVEN: FileEdit::descriptor()
        let descriptor = FileEdit::descriptor();

        // WHEN: descriptor.validate()
        let result = descriptor.validate();

        // THEN: Ok(())
        assert!(result.is_ok());
    }
}
