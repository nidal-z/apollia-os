//! List files and directories inside the agent's sandbox.

use crate::descriptor::{ToolDescriptor, ToolKind};
use crate::sandbox_path::{SandboxPathError, SandboxRoot};
use apollia_core::SandboxProfile;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use thiserror::Error;

/// List files and directories inside a sandbox directory.
///
/// Returns entries with their type (`file` or `directory`) and size in bytes.
/// Supports optional recursive listing.
#[derive(Debug, Clone)]
pub struct FileList {
    sandbox: SandboxRoot,
}

/// Errors produced by [`FileList`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum FileListError {
    /// Path attempts to escape the sandbox root.
    #[error("sandbox violation: path '{path}' escapes the sandbox root")]
    SandboxViolation { path: String },

    /// Directory not found at the specified path.
    #[error("not found: '{path}'")]
    NotFound { path: String },

    /// Path exists but is not a directory.
    #[error("not a directory: '{path}'")]
    NotADirectory { path: String },

    /// I/O error during listing.
    #[error("I/O error on '{path}': {cause}")]
    IoError { path: String, cause: String },
}

impl From<SandboxPathError> for FileListError {
    fn from(e: SandboxPathError) -> Self {
        match e {
            SandboxPathError::SandboxViolation { path } => Self::SandboxViolation { path },
            SandboxPathError::InitFailed { path, cause } => Self::IoError { path, cause },
        }
    }
}

/// Input for a file list operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileListInput {
    /// Relative path to the directory inside the sandbox. Defaults to sandbox root (`.`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    /// Whether to list entries recursively. Defaults to `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recursive: Option<bool>,
}

/// A single entry in a directory listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileListEntry {
    /// Entry name relative to the listed directory.
    pub name: String,
    /// `"file"` or `"directory"`.
    pub entry_type: String,
    /// Size in bytes for files; `null` for directories.
    pub size_bytes: Option<u64>,
}

/// Output of a file list operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileListOutput {
    /// Entries sorted lexicographically by name.
    pub entries: Vec<FileListEntry>,
}

impl FileList {
    /// Create a new `FileList` tool instance.
    ///
    /// # Errors
    ///
    /// Returns `FileListError::IoError` if the sandbox root cannot be initialized.
    pub fn new(sandbox_root: PathBuf) -> Result<Self, FileListError> {
        let sandbox = SandboxRoot::new(sandbox_root)?;
        Ok(Self { sandbox })
    }

    /// Execute a file list operation.
    ///
    /// # Errors
    ///
    /// - `SandboxViolation` if `dir` escapes the sandbox.
    /// - `NotFound` if the directory does not exist.
    /// - `NotADirectory` if the path exists but is not a directory.
    /// - `IoError` on any I/O failure.
    pub async fn run(&self, input: FileListInput) -> Result<FileListOutput, FileListError> {
        let dir = input.dir.as_deref().unwrap_or(".");
        let recursive = input.recursive.unwrap_or(false);

        let resolved = self
            .sandbox
            .resolve(dir)
            .map_err(|_| FileListError::SandboxViolation {
                path: dir.to_string(),
            })?;

        let meta = tokio::fs::metadata(&resolved).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                FileListError::NotFound {
                    path: dir.to_string(),
                }
            } else {
                FileListError::IoError {
                    path: dir.to_string(),
                    cause: e.to_string(),
                }
            }
        })?;

        if !meta.is_dir() {
            return Err(FileListError::NotADirectory {
                path: dir.to_string(),
            });
        }

        let mut entries = Vec::new();
        collect_entries(&resolved, &resolved, recursive, &mut entries).await?;
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(FileListOutput { entries })
    }

    /// Return the tool descriptor for registration in the `ToolRegistry`.
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "file_list".to_string(),
            version: "1.0.0".to_string(),
            description: "List files and directories inside the agent's sandbox. Returns entries with type (file/directory) and size. Supports optional recursive listing.".to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "dir": {
                        "type": "string",
                        "description": "Relative path to the directory inside the sandbox. Defaults to sandbox root."
                    },
                    "recursive": {
                        "type": "boolean",
                        "description": "Whether to list entries recursively. Defaults to false."
                    }
                }
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "entries": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string" },
                                "entry_type": { "type": "string", "enum": ["file", "directory"] },
                                "size_bytes": { "type": ["integer", "null"] }
                            },
                            "required": ["name", "entry_type", "size_bytes"]
                        }
                    }
                },
                "required": ["entries"]
            })),
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec!["file".to_string(), "list".to_string()],
            dangerous: false,
            is_read_only: true,
            risk_score: 0,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        }
    }
}

/// Collect directory entries recursively into `out`.
///
/// Entry names are relative to `base_dir`. Entries within each directory are
/// collected before descending, so the sort applied by the caller covers all
/// levels uniformly.
fn collect_entries<'a>(
    base_dir: &'a std::path::Path,
    current_dir: &'a std::path::Path,
    recursive: bool,
    out: &'a mut Vec<FileListEntry>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), FileListError>> + Send + 'a>> {
    Box::pin(async move {
        let mut read_dir =
            tokio::fs::read_dir(current_dir)
                .await
                .map_err(|e| FileListError::IoError {
                    path: current_dir.display().to_string(),
                    cause: e.to_string(),
                })?;

        let mut local: Vec<(std::path::PathBuf, tokio::fs::DirEntry)> = Vec::new();

        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(|e| FileListError::IoError {
                path: current_dir.display().to_string(),
                cause: e.to_string(),
            })?
        {
            local.push((entry.path(), entry));
        }

        // Sort locally before pushing so the final sort at the top level is consistent.
        local.sort_by(|a, b| a.0.cmp(&b.0));

        for (abs_path, entry) in &local {
            let meta = entry.metadata().await.map_err(|e| FileListError::IoError {
                path: abs_path.display().to_string(),
                cause: e.to_string(),
            })?;

            let name = abs_path
                .strip_prefix(base_dir)
                .unwrap_or(abs_path.as_path())
                .display()
                .to_string();

            let (entry_type, size_bytes) = if meta.is_dir() {
                ("directory".to_string(), None)
            } else {
                ("file".to_string(), Some(meta.len()))
            };

            out.push(FileListEntry {
                name,
                entry_type,
                size_bytes,
            });

            if recursive && meta.is_dir() {
                collect_entries(base_dir, abs_path, recursive, out).await?;
            }
        }

        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_file(dir: &std::path::Path, rel: &str, content: &[u8]) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dirs");
        }
        let mut f = std::fs::File::create(path).expect("create file");
        f.write_all(content).expect("write content");
    }

    #[tokio::test]
    async fn list_returns_entries_with_type_and_size() {
        // GIVEN: sandbox avec "file.txt" (10 octets) et "subdir/"
        let tmp = TempDir::new().expect("temp dir");
        create_file(tmp.path(), "file.txt", b"0123456789");
        std::fs::create_dir(tmp.path().join("subdir")).expect("create subdir");

        let tool = FileList::new(tmp.path().to_path_buf()).expect("FileList::new");

        // WHEN: file_list(dir=".")
        let result = tool
            .run(FileListInput {
                dir: None,
                recursive: None,
            })
            .await
            .expect("run ok");

        // THEN: entries contiennent { name: "file.txt", entry_type: "file", size_bytes: 10 }
        //       et { name: "subdir", entry_type: "directory", size_bytes: null }
        let file_entry = result
            .entries
            .iter()
            .find(|e| e.name == "file.txt")
            .expect("file.txt");
        assert_eq!(file_entry.entry_type, "file");
        assert_eq!(file_entry.size_bytes, Some(10));

        let dir_entry = result
            .entries
            .iter()
            .find(|e| e.name == "subdir")
            .expect("subdir");
        assert_eq!(dir_entry.entry_type, "directory");
        assert!(dir_entry.size_bytes.is_none());
    }

    #[tokio::test]
    async fn list_recursive_returns_nested_entries() {
        // GIVEN: sandbox avec "a.txt" et "sub/b.txt"
        let tmp = TempDir::new().expect("temp dir");
        create_file(tmp.path(), "a.txt", b"a");
        create_file(tmp.path(), "sub/b.txt", b"b");

        let tool = FileList::new(tmp.path().to_path_buf()).expect("FileList::new");

        // WHEN: file_list(dir=".", recursive=true)
        let result = tool
            .run(FileListInput {
                dir: Some(".".to_string()),
                recursive: Some(true),
            })
            .await
            .expect("run ok");

        // THEN: entries contiennent "a.txt", "sub", et "sub/b.txt"
        let names: Vec<&str> = result.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"a.txt"), "missing a.txt in {names:?}");
        assert!(names.contains(&"sub"), "missing sub in {names:?}");
        assert!(
            names.contains(&"sub/b.txt"),
            "missing sub/b.txt in {names:?}"
        );
    }

    #[tokio::test]
    async fn list_nonexistent_dir_returns_not_found() {
        // GIVEN: sandbox sans "missing/"
        let tmp = TempDir::new().expect("temp dir");
        let tool = FileList::new(tmp.path().to_path_buf()).expect("FileList::new");

        // WHEN: file_list(dir="missing")
        let result = tool
            .run(FileListInput {
                dir: Some("missing".to_string()),
                recursive: None,
            })
            .await;

        // THEN: Err(NotFound)
        assert!(result.is_err());
        assert!(matches!(result.err(), Some(FileListError::NotFound { .. })));
    }

    #[tokio::test]
    async fn list_outside_sandbox_returns_violation() {
        // GIVEN: sandbox valide
        let tmp = TempDir::new().expect("temp dir");
        let tool = FileList::new(tmp.path().to_path_buf()).expect("FileList::new");

        // WHEN: file_list(dir="../")
        let result = tool
            .run(FileListInput {
                dir: Some("../".to_string()),
                recursive: None,
            })
            .await;

        // THEN: Err(SandboxViolation)
        assert!(result.is_err());
        assert!(matches!(
            result.err(),
            Some(FileListError::SandboxViolation { .. })
        ));
    }

    #[tokio::test]
    async fn list_results_are_sorted() {
        // GIVEN: sandbox avec "c.txt", "a.txt", "b.txt"
        let tmp = TempDir::new().expect("temp dir");
        create_file(tmp.path(), "c.txt", b"c");
        create_file(tmp.path(), "a.txt", b"a");
        create_file(tmp.path(), "b.txt", b"b");

        let tool = FileList::new(tmp.path().to_path_buf()).expect("FileList::new");

        // WHEN: file_list(dir=".")
        let result = tool
            .run(FileListInput {
                dir: None,
                recursive: None,
            })
            .await
            .expect("run ok");

        // THEN: entries triees ["a.txt", "b.txt", "c.txt"]
        let names: Vec<&str> = result.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["a.txt", "b.txt", "c.txt"]);
    }

    #[test]
    fn descriptor_is_valid() {
        // GIVEN the descriptor the file_list tool publishes
        let descriptor = FileList::descriptor();
        // WHEN it is validated
        // THEN it passes, so the registry will accept it
        assert!(descriptor.validate().is_ok());
    }
}
