//! Read files inside the agent's sandbox with offset/limit support.

use crate::descriptor::{ToolDescriptor, ToolKind};
use crate::sandbox_path::SandboxRoot;
use apollia_core::SandboxProfile;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncReadExt;

/// Read the contents of a file inside the agent's sandbox.
///
/// Supports optional offset/limit for reading portions of large files.
/// Returns file content as UTF-8 text with line numbers prepended.
#[derive(Debug, Clone)]
pub struct FileRead {
    sandbox: SandboxRoot,
}

/// Errors produced by [`FileRead`].
#[derive(Debug, Error)]
pub enum FileReadError {
    /// Path attempts to escape the sandbox root.
    #[error("sandbox violation: path '{path}' escapes the sandbox root")]
    SandboxViolation { path: String },

    /// File not found at the specified path.
    #[error("not found: '{path}'")]
    NotFound { path: String },

    /// I/O error while reading the file.
    #[error("I/O error on '{path}': {cause}")]
    IoError { path: String, cause: String },

    /// File contains non-UTF-8 data (binary file).
    #[error("file is binary (not valid UTF-8): '{path}'")]
    BinaryFile { path: String },
}

/// Input for a file read operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileReadInput {
    /// Relative path inside the sandbox.
    pub path: String,
    /// Line offset (1-based). None = start from line 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
    /// Max lines to return. None = all remaining lines.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// Output of a file read operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileReadOutput {
    /// File content with line numbers (e.g., "    1\tline content").
    pub content: String,
    /// Total number of lines in the file.
    pub total_lines: u32,
    /// Whether the output was truncated by the limit parameter.
    pub truncated: bool,
}

impl FileRead {
    /// Create a new FileRead tool instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the sandbox root cannot be initialized.
    pub fn new(sandbox_root: PathBuf) -> Result<Self, FileReadError> {
        let sandbox = SandboxRoot::new(sandbox_root).map_err(|e| FileReadError::IoError {
            path: "sandbox_root".to_string(),
            cause: e.to_string(),
        })?;

        Ok(Self { sandbox })
    }

    /// Execute a file read operation.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is invalid, the file doesn't exist,
    /// cannot be read, or contains non-UTF-8 data.
    pub async fn run(&self, input: FileReadInput) -> Result<FileReadOutput, FileReadError> {
        let resolved_path =
            self.sandbox
                .resolve(&input.path)
                .map_err(|_| FileReadError::SandboxViolation {
                    path: input.path.clone(),
                })?;

        let mut file = fs::File::open(&resolved_path)
            .await
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => FileReadError::NotFound {
                    path: input.path.clone(),
                },
                _ => FileReadError::IoError {
                    path: input.path.clone(),
                    cause: e.to_string(),
                },
            })?;

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .await
            .map_err(|e| FileReadError::IoError {
                path: input.path.clone(),
                cause: e.to_string(),
            })?;

        let content = String::from_utf8(buffer).map_err(|_| FileReadError::BinaryFile {
            path: input.path.clone(),
        })?;

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len() as u32;

        let offset = input.offset.unwrap_or(1).saturating_sub(1) as usize;
        let limit = input.limit.map(|l| l as usize);

        let lines_to_read = if offset >= lines.len() {
            Vec::new()
        } else {
            let end = limit
                .map(|l| (offset + l).min(lines.len()))
                .unwrap_or(lines.len());
            lines[offset..end].to_vec()
        };

        let truncated = limit.map(|l| offset + l < lines.len()).unwrap_or(false);

        let formatted_lines: Vec<String> = lines_to_read
            .iter()
            .enumerate()
            .map(|(idx, line)| {
                let line_number = offset + idx + 1;
                format!("{:>5}\t{}", line_number, line)
            })
            .collect();

        let output_content = if formatted_lines.is_empty() {
            String::new()
        } else {
            formatted_lines.join("\n")
        };

        Ok(FileReadOutput {
            content: output_content,
            total_lines,
            truncated,
        })
    }

    /// Return the tool descriptor for registration in the ToolRegistry.
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "file_read".to_string(),
            version: "1.0.0".to_string(),
            description: "Read the contents of a file inside the agent's sandbox. Supports offset/limit for reading portions of large files. Returns file content as UTF-8 text with line numbers prepended.".to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path inside the sandbox"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Line offset (1-based). None = start from line 1.",
                        "minimum": 1
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max lines to return. None = all remaining lines.",
                        "minimum": 1
                    }
                },
                "required": ["path"]
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "File content with line numbers (e.g., '    1\\tline content')"
                    },
                    "total_lines": {
                        "type": "integer",
                        "description": "Total number of lines in the file"
                    },
                    "truncated": {
                        "type": "boolean",
                        "description": "Whether the output was truncated by the limit parameter"
                    }
                },
                "required": ["content", "total_lines", "truncated"]
            })),
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec!["file".to_string(), "read".to_string()],
            dangerous: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    async fn create_test_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let file_path = dir.path().join(name);
        let mut file = std::fs::File::create(&file_path).expect("Failed to create test file");
        file.write_all(content.as_bytes())
            .expect("Failed to write test file");
        file_path
    }

    #[tokio::test]
    async fn read_existing_file_returns_content_with_line_numbers() {
        // GIVEN: fichier "hello.txt" avec "line1\nline2\nline3" dans un sandbox temp
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        create_test_file(&temp_dir, "hello.txt", "line1\nline2\nline3").await;

        let file_read =
            FileRead::new(temp_dir.path().to_path_buf()).expect("Failed to create FileRead");

        // WHEN: file_read(path="hello.txt")
        let input = FileReadInput {
            path: "hello.txt".to_string(),
            offset: None,
            limit: None,
        };
        let result = file_read.run(input).await;

        // THEN: content contient "    1\tline1", total_lines=3
        assert!(result.is_ok());
        let output = result.expect("Read failed");
        assert_eq!(output.total_lines, 3);
        assert!(output.content.contains("    1\tline1"));
        assert!(output.content.contains("    2\tline2"));
        assert!(output.content.contains("    3\tline3"));
        assert!(!output.truncated);
    }

    #[tokio::test]
    async fn read_with_offset_and_limit() {
        // GIVEN: fichier de 100 lignes
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let mut content = String::new();
        for i in 1..=100 {
            content.push_str(&format!("line{}\n", i));
        }
        create_test_file(&temp_dir, "big.txt", &content).await;

        let file_read =
            FileRead::new(temp_dir.path().to_path_buf()).expect("Failed to create FileRead");

        // WHEN: file_read(path="big.txt", offset=10, limit=5)
        let input = FileReadInput {
            path: "big.txt".to_string(),
            offset: Some(10),
            limit: Some(5),
        };
        let result = file_read.run(input).await;

        // THEN: 5 lignes retournees a partir de la ligne 10, truncated=true
        assert!(result.is_ok());
        let output = result.expect("Read failed");
        assert_eq!(output.total_lines, 100);
        assert!(output.truncated);
        assert!(output.content.contains("   10\tline10"));
        assert!(output.content.contains("   14\tline14"));
        assert!(!output.content.contains("line15"));
    }

    #[tokio::test]
    async fn read_nonexistent_file_returns_not_found() {
        // GIVEN: sandbox vide
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_read =
            FileRead::new(temp_dir.path().to_path_buf()).expect("Failed to create FileRead");

        // WHEN: file_read(path="missing.txt")
        let input = FileReadInput {
            path: "missing.txt".to_string(),
            offset: None,
            limit: None,
        };
        let result = file_read.run(input).await;

        // THEN: Err(FileReadError::NotFound)
        assert!(result.is_err());
        match result.err() {
            Some(FileReadError::NotFound { path }) => {
                assert_eq!(path, "missing.txt");
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn read_binary_file_returns_binary_error() {
        // GIVEN: fichier avec des octets non UTF-8
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("binary.bin");
        let mut file = std::fs::File::create(&file_path).expect("Failed to create binary file");
        file.write_all(&[0xFF, 0xFE, 0xFD, 0xFC])
            .expect("Failed to write binary data");

        let file_read =
            FileRead::new(temp_dir.path().to_path_buf()).expect("Failed to create FileRead");

        // WHEN: file_read sur ce fichier
        let input = FileReadInput {
            path: "binary.bin".to_string(),
            offset: None,
            limit: None,
        };
        let result = file_read.run(input).await;

        // THEN: Err(FileReadError::BinaryFile)
        assert!(result.is_err());
        match result.err() {
            Some(FileReadError::BinaryFile { path }) => {
                assert_eq!(path, "binary.bin");
            }
            _ => panic!("Expected BinaryFile error"),
        }
    }

    #[tokio::test]
    async fn read_outside_sandbox_returns_violation() {
        // GIVEN: sandbox valide
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_read =
            FileRead::new(temp_dir.path().to_path_buf()).expect("Failed to create FileRead");

        // WHEN: file_read(path="../escape")
        let input = FileReadInput {
            path: "../escape".to_string(),
            offset: None,
            limit: None,
        };
        let result = file_read.run(input).await;

        // THEN: Err(FileReadError::SandboxViolation)
        assert!(result.is_err());
        match result.err() {
            Some(FileReadError::SandboxViolation { path }) => {
                assert_eq!(path, "../escape");
            }
            _ => panic!("Expected SandboxViolation error"),
        }
    }

    #[test]
    fn descriptor_is_valid() {
        // GIVEN: FileRead::descriptor()
        let descriptor = FileRead::descriptor();

        // WHEN: descriptor.validate()
        let result = descriptor.validate();

        // THEN: Ok(())
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn read_truncated_flag_is_correct() {
        // GIVEN: fichier de 50 lignes lu sans limit
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let mut content = String::new();
        for i in 1..=50 {
            content.push_str(&format!("line{}\n", i));
        }
        create_test_file(&temp_dir, "file.txt", &content).await;

        let file_read =
            FileRead::new(temp_dir.path().to_path_buf()).expect("Failed to create FileRead");

        // WHEN: file_read(path="file.txt")
        let input = FileReadInput {
            path: "file.txt".to_string(),
            offset: None,
            limit: None,
        };
        let result = file_read.run(input).await;

        // THEN: total_lines=50, truncated=false
        assert!(result.is_ok());
        let output = result.expect("Read failed");
        assert_eq!(output.total_lines, 50);
        assert!(!output.truncated);
    }
}
