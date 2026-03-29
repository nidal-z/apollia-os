//! Native file I/O tool with agent sandbox isolation.
//!
//! Exposes `read`, `write`, and `list` operations restricted to the agent's
//! dedicated sandbox directory. Path traversal attempts are detected and rejected
//! before any I/O is performed (Principe #4 — Fail fast).

use std::path::{Component, Path, PathBuf};

use serde_json::json;
use thiserror::Error;

use crate::descriptor::{ToolDescriptor, ToolKind};

/// Native file I/O tool with agent sandbox isolation.
///
/// All paths are validated against `sandbox_root` using component-level
/// normalization. No filesystem access occurs before path validation succeeds.
///
/// Each agent has a dedicated sandbox under `~/.apollia/sandboxes/<agent_id>/workspace/`.
pub struct FileIo {
    /// Canonical root directory of the agent's sandbox.
    ///
    /// Canonicalized at construction time so prefix checks are symlink-stable.
    sandbox_root: PathBuf,
}

/// Errors produced by [`FileIo`] operations.
#[derive(Debug, Error)]
pub enum FileIoError {
    /// Attempted access to a path outside the agent's sandbox.
    #[error("sandbox violation: path '{path}' escapes the sandbox root")]
    SandboxViolation {
        /// The offending path as provided by the caller.
        path: String,
    },
    /// File or directory does not exist.
    #[error("not found: '{path}'")]
    NotFound {
        /// The path that was not found.
        path: String,
    },
    /// Generic I/O error (permission denied, disk full, etc.).
    #[error("I/O error on '{path}': {cause}")]
    IoError {
        /// The path involved in the failed operation.
        path: String,
        /// Human-readable cause from the underlying OS error.
        cause: String,
    },
}

impl FileIo {
    /// Creates a `FileIo` instance rooted at `sandbox_root`.
    ///
    /// Creates `sandbox_root` and all intermediate directories if they do not exist.
    /// The path is then canonicalized so that subsequent prefix checks are
    /// resilient to symlinks and relative components.
    ///
    /// # Errors
    ///
    /// Returns `FileIoError::IoError` if the directory cannot be created or canonicalized.
    pub fn new(sandbox_root: PathBuf) -> Result<Self, FileIoError> {
        std::fs::create_dir_all(&sandbox_root).map_err(|e| FileIoError::IoError {
            path: sandbox_root.display().to_string(),
            cause: e.to_string(),
        })?;

        let canonical = sandbox_root
            .canonicalize()
            .map_err(|e| FileIoError::IoError {
                path: sandbox_root.display().to_string(),
                cause: e.to_string(),
            })?;

        Ok(Self {
            sandbox_root: canonical,
        })
    }

    /// Reads the content of a file inside the sandbox.
    ///
    /// # Errors
    ///
    /// - [`FileIoError::SandboxViolation`] if `path` escapes the sandbox root.
    /// - [`FileIoError::NotFound`] if the file does not exist.
    /// - [`FileIoError::IoError`] for other I/O failures.
    pub async fn read(&self, path: &str) -> Result<Vec<u8>, FileIoError> {
        let resolved = self.resolve_path(path)?;
        tokio::fs::read(&resolved).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                FileIoError::NotFound {
                    path: path.to_string(),
                }
            } else {
                FileIoError::IoError {
                    path: path.to_string(),
                    cause: e.to_string(),
                }
            }
        })
    }

    /// Writes `content` to a file inside the sandbox.
    ///
    /// Creates intermediate directories as needed. Overwrites the file if it exists.
    ///
    /// # Errors
    ///
    /// - [`FileIoError::SandboxViolation`] if `path` escapes the sandbox root.
    /// - [`FileIoError::IoError`] for I/O failures.
    pub async fn write(&self, path: &str, content: &[u8]) -> Result<(), FileIoError> {
        let resolved = self.resolve_path(path)?;

        if let Some(parent) = resolved.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| FileIoError::IoError {
                    path: path.to_string(),
                    cause: e.to_string(),
                })?;
        }

        tokio::fs::write(&resolved, content)
            .await
            .map_err(|e| FileIoError::IoError {
                path: path.to_string(),
                cause: e.to_string(),
            })
    }

    /// Lists files in `dir` whose names match the glob `pattern`.
    ///
    /// Only files directly inside `dir` are listed (non-recursive).
    /// Returned names are relative to `dir`, sorted lexicographically.
    ///
    /// Glob syntax: `*` matches any sequence of characters; `?` matches any single character.
    ///
    /// # Errors
    ///
    /// - [`FileIoError::SandboxViolation`] if `dir` escapes the sandbox root.
    /// - [`FileIoError::NotFound`] if `dir` does not exist.
    /// - [`FileIoError::IoError`] for other I/O failures.
    pub async fn list(&self, dir: &str, pattern: &str) -> Result<Vec<String>, FileIoError> {
        let resolved_dir = self.resolve_path(dir)?;

        let mut read_dir = tokio::fs::read_dir(&resolved_dir).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                FileIoError::NotFound {
                    path: dir.to_string(),
                }
            } else {
                FileIoError::IoError {
                    path: dir.to_string(),
                    cause: e.to_string(),
                }
            }
        })?;

        let mut names = Vec::new();
        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(|e| FileIoError::IoError {
                path: dir.to_string(),
                cause: e.to_string(),
            })?
        {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy().into_owned();
            if glob_match(pattern, &name) {
                names.push(name);
            }
        }

        names.sort();
        Ok(names)
    }

    /// Returns the [`ToolDescriptor`] for the `file_io` tool.
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "file_io".to_string(),
            version: "1.0.0".to_string(),
            description: "Read, write, or list files in the sandbox. Use 'list' to explore \
                          before 'read' when unsure of contents. Paths are sandbox-restricted."
                .to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["read", "write", "list"]
                    },
                    "path": { "type": "string" },
                    "content": { "type": "string" },
                    "dir": { "type": "string" },
                    "pattern": { "type": "string" }
                },
                "required": ["operation"]
            }),
            output_schema: None,
            sandbox_profile: apollia_core::SandboxProfile::FileSystem,
            tags: vec!["io".to_string(), "filesystem".to_string()],
            dangerous: false,
        }
    }

    /// Validates and resolves `path` relative to `sandbox_root`.
    ///
    /// Returns the absolute normalized path if it lies within the sandbox.
    /// No I/O is performed — traversal is detected via component normalization.
    ///
    /// # Errors
    ///
    /// Returns [`FileIoError::SandboxViolation`] if:
    /// - `path` is absolute, or
    /// - the normalized path escapes `sandbox_root`.
    fn resolve_path(&self, path: &str) -> Result<PathBuf, FileIoError> {
        // Reject absolute paths immediately (Principe #4 — Fail fast, before any join).
        if Path::new(path).is_absolute() {
            return Err(FileIoError::SandboxViolation {
                path: path.to_string(),
            });
        }

        // Build target and normalize component-by-component to resolve "..".
        let target = self.sandbox_root.join(path);
        let normalized = normalize_path(&target);

        // Verify the resolved path is still within the sandbox.
        if !normalized.starts_with(&self.sandbox_root) {
            return Err(FileIoError::SandboxViolation {
                path: path.to_string(),
            });
        }

        Ok(normalized)
    }
}

/// Normalizes `path` by resolving `.` and `..` components without any I/O.
///
/// Used to detect path traversal before touching the filesystem.
fn normalize_path(path: &Path) -> PathBuf {
    let mut components: Vec<Component> = Vec::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                // Pop the last normal component; root and prefix are immovable.
                if matches!(components.last(), Some(Component::Normal(_))) {
                    components.pop();
                }
            }
            Component::CurDir => {} // Skip "." — no-op.
            other => components.push(other),
        }
    }
    components.iter().collect()
}

/// Returns `true` if `text` matches the glob `pattern`.
///
/// Supported wildcards: `*` (any sequence of characters), `?` (any single character).
/// Case-sensitive. Does not treat `/` specially — used for file name matching only.
fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    glob_match_inner(&p, &t)
}

fn glob_match_inner(pattern: &[char], text: &[char]) -> bool {
    match (pattern.first(), text.first()) {
        (None, None) => true,
        (None, Some(_)) => false,
        (Some('*'), _) => {
            // '*' matches zero characters, or one character plus the rest.
            glob_match_inner(&pattern[1..], text)
                || (!text.is_empty() && glob_match_inner(pattern, &text[1..]))
        }
        (Some('?'), Some(_)) => glob_match_inner(&pattern[1..], &text[1..]),
        (Some(p), Some(t)) if p == t => glob_match_inner(&pattern[1..], &text[1..]),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tokio::fs;

    fn test_sandbox() -> PathBuf {
        std::env::temp_dir().join(format!("apollia_file_io_test_{}", uuid::Uuid::new_v4()))
    }

    // --- Read existing file ---

    #[tokio::test]
    async fn test_ac1_read_existing_file() {
        // GIVEN
        let sandbox = test_sandbox();
        let file_io = FileIo::new(sandbox.clone()).expect("FileIo creation failed");
        fs::write(sandbox.join("config.json"), b"{}").await.unwrap();

        // WHEN
        let content = file_io.read("config.json").await.expect("read failed");

        // THEN
        assert_eq!(content, b"{}");
        fs::remove_dir_all(&sandbox).await.ok();
    }

    // --- Path traversal rejected ---

    #[tokio::test]
    async fn test_ac2_path_traversal_rejected() {
        // GIVEN
        let sandbox = test_sandbox();
        let file_io = FileIo::new(sandbox.clone()).expect("FileIo creation failed");

        // WHEN
        let result = file_io.read("../../etc/passwd").await;

        // THEN
        assert!(matches!(result, Err(FileIoError::SandboxViolation { .. })));
        fs::remove_dir_all(&sandbox).await.ok();
    }

    // --- Write creates file and intermediate directories ---

    #[tokio::test]
    async fn test_ac3_write_creates_file_and_dirs() {
        // GIVEN
        let sandbox = test_sandbox();
        let file_io = FileIo::new(sandbox.clone()).expect("FileIo creation failed");

        // WHEN
        file_io
            .write("output/result.json", b"{}")
            .await
            .expect("write failed");

        // THEN
        let content = file_io
            .read("output/result.json")
            .await
            .expect("read after write failed");
        assert_eq!(content, b"{}");
        fs::remove_dir_all(&sandbox).await.ok();
    }

    // --- Nonexistent file returns NotFound ---

    #[tokio::test]
    async fn test_ac4_read_nonexistent_file_returns_not_found() {
        // GIVEN
        let sandbox = test_sandbox();
        let file_io = FileIo::new(sandbox.clone()).expect("FileIo creation failed");

        // WHEN
        let result = file_io.read("nonexistent.txt").await;

        // THEN
        assert!(matches!(result, Err(FileIoError::NotFound { .. })));
        fs::remove_dir_all(&sandbox).await.ok();
    }

    // --- List files matching glob pattern ---

    #[tokio::test]
    async fn test_ac5_list_json_files() {
        // GIVEN
        let sandbox = test_sandbox();
        let file_io = FileIo::new(sandbox.clone()).expect("FileIo creation failed");
        fs::write(sandbox.join("a.json"), b"{}").await.unwrap();
        fs::write(sandbox.join("b.json"), b"{}").await.unwrap();
        fs::write(sandbox.join("c.json"), b"{}").await.unwrap();
        fs::write(sandbox.join("notes.txt"), b"text").await.unwrap();

        // WHEN
        let mut files = file_io.list(".", "*.json").await.expect("list failed");
        files.sort();

        // THEN
        assert_eq!(files, vec!["a.json", "b.json", "c.json"]);
        fs::remove_dir_all(&sandbox).await.ok();
    }

    // --- Absolute path rejected ---

    #[tokio::test]
    async fn test_ac6_absolute_path_rejected() {
        // GIVEN
        let sandbox = test_sandbox();
        let file_io = FileIo::new(sandbox.clone()).expect("FileIo creation failed");

        // WHEN
        let result = file_io.read("/etc/passwd").await;

        // THEN
        assert!(matches!(result, Err(FileIoError::SandboxViolation { .. })));
        fs::remove_dir_all(&sandbox).await.ok();
    }

    // --- Descriptor validity ---

    #[test]
    fn test_descriptor_is_valid() {
        let descriptor = FileIo::descriptor();
        assert_eq!(descriptor.name, "file_io");
        assert!(descriptor.validate().is_ok());
    }

    // --- normalize_path unit tests ---

    #[test]
    fn test_normalize_path_resolves_parent_dir() {
        // GIVEN
        let path = PathBuf::from("/tmp/sandbox/foo/../bar");

        // WHEN
        let result = normalize_path(&path);

        // THEN
        assert_eq!(result, PathBuf::from("/tmp/sandbox/bar"));
    }

    #[test]
    fn test_normalize_path_resolves_current_dir() {
        // GIVEN
        let path = PathBuf::from("/tmp/sandbox/./foo");

        // WHEN
        let result = normalize_path(&path);

        // THEN
        assert_eq!(result, PathBuf::from("/tmp/sandbox/foo"));
    }

    // --- glob_match unit tests ---

    #[test]
    fn test_glob_match_star_extension() {
        assert!(glob_match("*.json", "config.json"));
        assert!(glob_match("*.json", ".json"));
        assert!(!glob_match("*.json", "config.toml"));
    }

    #[test]
    fn test_glob_match_question_mark() {
        assert!(glob_match("file?.txt", "file1.txt"));
        assert!(!glob_match("file?.txt", "file10.txt"));
    }

    #[test]
    fn test_glob_match_exact() {
        assert!(glob_match("exact.txt", "exact.txt"));
        assert!(!glob_match("exact.txt", "other.txt"));
    }
}
