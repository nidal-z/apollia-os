//! Search for files matching a glob pattern inside the agent's sandbox.

use std::path::PathBuf;
use std::time::SystemTime;

/// Maximum number of file results returned by a single [`FileGlob`] call.
///
/// Prevents unbounded memory usage when a glob pattern matches a large directory tree.
/// When exceeded, [`FileGlobError::GlobLimitExceeded`] is returned and no partial
/// results are provided.
const MAX_GLOB_RESULTS: usize = 10_000;

use apollia_core::SandboxProfile;
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use crate::descriptor::{ToolDescriptor, ToolKind};
use crate::sandbox_path::{SandboxPathError, SandboxRoot};

/// Search for files matching a glob pattern inside the agent's sandbox.
///
/// Supports `**` for recursive matching (e.g. `**/*.rs`).
/// Results are sorted by modification time (most recent first).
#[derive(Debug, Clone)]
pub struct FileGlob {
    sandbox: SandboxRoot,
}

/// Errors produced by [`FileGlob`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum FileGlobError {
    /// Path attempts to escape the sandbox root.
    #[error("sandbox violation: path '{path}' escapes the sandbox root")]
    SandboxViolation { path: String },

    /// Glob pattern is syntactically invalid.
    #[error("invalid glob pattern: {0}")]
    InvalidPattern(String),

    /// I/O error during glob traversal.
    #[error("I/O error: {0}")]
    IoError(String),

    /// The glob matched more than [`MAX_GLOB_RESULTS`] files.
    ///
    /// The caller must narrow the pattern or base directory to reduce the result set.
    /// No partial results are returned.
    #[error("glob result limit exceeded: found more than {limit} matches")]
    GlobLimitExceeded {
        /// The hard limit that was exceeded.
        limit: usize,
    },
}

impl From<SandboxPathError> for FileGlobError {
    fn from(e: SandboxPathError) -> Self {
        match e {
            SandboxPathError::SandboxViolation { path } => Self::SandboxViolation { path },
            SandboxPathError::InitFailed { path, cause } => {
                Self::IoError(format!("{path}: {cause}"))
            }
        }
    }
}

/// Input for a file glob operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileGlobInput {
    /// Glob pattern to match against files in the sandbox (e.g. `**/*.rs`, `*.toml`).
    pub pattern: String,
    /// Optional base directory within the sandbox. Defaults to sandbox root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Output of a file glob operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileGlobOutput {
    /// Matched file paths, relative to the sandbox root, sorted by modification time (most recent first).
    pub matches: Vec<String>,
}

impl FileGlob {
    /// Create a new `FileGlob` tool instance.
    ///
    /// # Errors
    ///
    /// Returns `FileGlobError::IoError` if the sandbox root cannot be initialized.
    pub fn new(sandbox_root: PathBuf) -> Result<Self, FileGlobError> {
        let sandbox = SandboxRoot::new(sandbox_root)?;
        Ok(Self { sandbox })
    }

    /// Execute a glob search within the sandbox.
    ///
    /// # Errors
    ///
    /// - `SandboxViolation` if `path` escapes the sandbox.
    /// - `InvalidPattern` if `pattern` is syntactically invalid.
    /// - `IoError` on any I/O failure during traversal.
    pub async fn run(&self, input: FileGlobInput) -> Result<FileGlobOutput, FileGlobError> {
        let base = match &input.path {
            Some(p) => self
                .sandbox
                .resolve(p)
                .map_err(|_| FileGlobError::SandboxViolation { path: p.clone() })?,
            None => self.sandbox.path().to_path_buf(),
        };

        let sandbox_root = self.sandbox.path().to_path_buf();
        let pattern = input.pattern.clone();

        // Glob traversal is blocking filesystem I/O: run off the async executor.
        let matches =
            tokio::task::spawn_blocking(move || glob_files(&base, &sandbox_root, &pattern))
                .await
                .map_err(|e| FileGlobError::IoError(e.to_string()))??;

        Ok(FileGlobOutput { matches })
    }

    /// Return the tool descriptor for registration in the `ToolRegistry`.
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "file_glob".to_string(),
            version: "1.0.0".to_string(),
            description: "Search for files matching a glob pattern inside the agent's sandbox. \
                Supports ** for recursive matching. \
                Results are sorted by modification time (most recent first)."
                .to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern to match (e.g. **/*.rs, *.toml)"
                    },
                    "path": {
                        "type": "string",
                        "description": "Optional base directory within the sandbox. Defaults to sandbox root."
                    }
                },
                "required": ["pattern"]
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "matches": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "File paths relative to the sandbox root, sorted by modification time (most recent first)"
                    }
                },
                "required": ["matches"]
            })),
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec!["file".to_string(), "glob".to_string(), "search".to_string()],
            dangerous: false,
            is_read_only: true,
            risk_score: 0,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        }
    }
}

/// Perform the blocking glob search and return file paths sorted by mtime descending.
///
/// Directories are excluded from results. Entries outside the sandbox root are silently
/// skipped (defensive: the sandbox check at `run` already guards the base path).
fn glob_files(
    base: &std::path::Path,
    sandbox_root: &std::path::Path,
    pattern: &str,
) -> Result<Vec<String>, FileGlobError> {
    // Build the absolute pattern string: {base}/{pattern}
    let full_pattern = base.join(pattern).to_string_lossy().into_owned();

    let paths = glob::glob(&full_pattern)
        .map_err(|e| FileGlobError::InvalidPattern(format!("{pattern}: {e}")))?;

    let mut entries: Vec<(PathBuf, SystemTime)> = Vec::new();

    for path_result in paths {
        let path = path_result.map_err(|e| FileGlobError::IoError(e.to_string()))?;

        // Exclude directories and any path that escaped the sandbox.
        if path.is_dir() || !path.starts_with(sandbox_root) {
            continue;
        }

        if entries.len() >= MAX_GLOB_RESULTS {
            return Err(FileGlobError::GlobLimitExceeded {
                limit: MAX_GLOB_RESULTS,
            });
        }

        let mtime = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);

        entries.push((path, mtime));
    }

    // Sort by mtime descending (most recent first).
    entries.sort_by_key(|b| std::cmp::Reverse(b.1));

    let matches = entries
        .into_iter()
        .filter_map(|(path, _)| {
            path.strip_prefix(sandbox_root)
                .ok()
                .map(|rel| rel.to_string_lossy().into_owned())
        })
        .collect();

    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{Duration, SystemTime};
    use tempfile::TempDir;

    fn create_file(dir: &std::path::Path, rel: &str, content: &[u8]) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dirs");
        }
        let mut f = std::fs::File::create(&path).expect("create file");
        f.write_all(content).expect("write content");
    }

    #[tokio::test]
    async fn glob_recursive_pattern_finds_nested_files() {
        // GIVEN: sandbox with "src/main.rs", "src/lib.rs", "tests/test.rs", "README.md"
        let tmp = TempDir::new().expect("temp dir");
        create_file(tmp.path(), "src/main.rs", b"fn main() {}");
        create_file(tmp.path(), "src/lib.rs", b"// lib");
        create_file(tmp.path(), "tests/test.rs", b"// test");
        create_file(tmp.path(), "README.md", b"# readme");

        let tool = FileGlob::new(tmp.path().to_path_buf()).expect("FileGlob::new");

        // WHEN: file_glob(pattern="**/*.rs")
        let result = tool
            .run(FileGlobInput {
                pattern: "**/*.rs".to_string(),
                path: None,
            })
            .await
            .expect("run ok");

        // THEN: matches contains the 3 .rs files, not README.md
        assert_eq!(
            result.matches.len(),
            3,
            "expected 3 .rs files: {:?}",
            result.matches
        );
        assert!(result.matches.iter().any(|m| m.ends_with("src/main.rs")));
        assert!(result.matches.iter().any(|m| m.ends_with("src/lib.rs")));
        assert!(result.matches.iter().any(|m| m.ends_with("tests/test.rs")));
        assert!(!result.matches.iter().any(|m| m.contains("README")));
    }

    #[tokio::test]
    async fn glob_non_recursive_pattern_matches_current_dir_only() {
        // GIVEN: sandbox with "Cargo.toml" and "sub/config.toml"
        let tmp = TempDir::new().expect("temp dir");
        create_file(tmp.path(), "Cargo.toml", b"[package]");
        create_file(tmp.path(), "sub/config.toml", b"[config]");

        let tool = FileGlob::new(tmp.path().to_path_buf()).expect("FileGlob::new");

        // WHEN: file_glob(pattern="*.toml")
        let result = tool
            .run(FileGlobInput {
                pattern: "*.toml".to_string(),
                path: None,
            })
            .await
            .expect("run ok");

        // THEN: matches contains only "Cargo.toml"
        assert_eq!(
            result.matches.len(),
            1,
            "expected 1 match: {:?}",
            result.matches
        );
        assert!(result.matches[0].ends_with("Cargo.toml"));
    }

    #[tokio::test]
    async fn glob_results_sorted_by_mtime_desc() {
        // GIVEN: "old.txt" with mtime in the past, "new.txt" with mtime in the future
        let tmp = TempDir::new().expect("temp dir");
        let old_path = tmp.path().join("old.txt");
        let new_path = tmp.path().join("new.txt");

        create_file(tmp.path(), "old.txt", b"old");
        create_file(tmp.path(), "new.txt", b"new");

        // Set old.txt mtime to a clearly older timestamp
        {
            let f = std::fs::OpenOptions::new()
                .write(true)
                .open(&old_path)
                .expect("open old.txt");
            f.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000))
                .expect("set mtime on old.txt");
        }
        // Set new.txt mtime to a clearly newer timestamp
        {
            let f = std::fs::OpenOptions::new()
                .write(true)
                .open(&new_path)
                .expect("open new.txt");
            f.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000_000))
                .expect("set mtime on new.txt");
        }

        let tool = FileGlob::new(tmp.path().to_path_buf()).expect("FileGlob::new");

        // WHEN: file_glob(pattern="*.txt")
        let result = tool
            .run(FileGlobInput {
                pattern: "*.txt".to_string(),
                path: None,
            })
            .await
            .expect("run ok");

        // THEN: "new.txt" appears before "old.txt"
        assert_eq!(
            result.matches.len(),
            2,
            "expected 2 matches: {:?}",
            result.matches
        );
        assert!(
            result.matches[0].ends_with("new.txt"),
            "expected new.txt first, got: {:?}",
            result.matches
        );
        assert!(result.matches[1].ends_with("old.txt"));
    }

    #[tokio::test]
    async fn glob_invalid_pattern_returns_error() {
        // GIVEN: a syntactically invalid pattern
        let tmp = TempDir::new().expect("temp dir");
        let tool = FileGlob::new(tmp.path().to_path_buf()).expect("FileGlob::new");

        // WHEN: file_glob(pattern="[invalid")
        let result = tool
            .run(FileGlobInput {
                pattern: "[invalid".to_string(),
                path: None,
            })
            .await;

        // THEN: Err(InvalidPattern)
        assert!(result.is_err());
        assert!(
            matches!(result.err(), Some(FileGlobError::InvalidPattern(_))),
            "expected InvalidPattern"
        );
    }

    #[tokio::test]
    async fn glob_results_are_relative_to_sandbox() {
        // GIVEN: sandbox with "src/main.rs"
        let tmp = TempDir::new().expect("temp dir");
        create_file(tmp.path(), "src/main.rs", b"fn main() {}");

        let tool = FileGlob::new(tmp.path().to_path_buf()).expect("FileGlob::new");

        // WHEN: file_glob(pattern="**/*.rs")
        let result = tool
            .run(FileGlobInput {
                pattern: "**/*.rs".to_string(),
                path: None,
            })
            .await
            .expect("run ok");

        // THEN: matches contains "src/main.rs" (not the absolute path)
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0], "src/main.rs");
        assert!(!result.matches[0].starts_with('/'));
    }

    #[tokio::test]
    async fn test_file_glob_within_limit_returns_results() {
        // GIVEN a sandbox with 5 files, well below MAX_GLOB_RESULTS
        let tmp = TempDir::new().expect("temp dir");
        for i in 0..5 {
            create_file(tmp.path(), &format!("file{i}.txt"), b"content");
        }
        let tool = FileGlob::new(tmp.path().to_path_buf()).expect("FileGlob::new");

        // WHEN matching all .txt files
        let result = tool
            .run(FileGlobInput {
                pattern: "*.txt".to_string(),
                path: None,
            })
            .await
            .expect("should succeed within limit");

        // THEN all 5 files are returned
        assert_eq!(result.matches.len(), 5);
    }

    #[tokio::test]
    async fn test_file_glob_exceeds_limit_returns_error() {
        // GIVEN a sandbox with MAX_GLOB_RESULTS + 1 files
        let tmp = TempDir::new().expect("temp dir");
        let over_limit = MAX_GLOB_RESULTS + 1;
        for i in 0..over_limit {
            create_file(tmp.path(), &format!("f{i}.txt"), b"x");
        }
        let tool = FileGlob::new(tmp.path().to_path_buf()).expect("FileGlob::new");

        // WHEN matching all .txt files
        let result = tool
            .run(FileGlobInput {
                pattern: "*.txt".to_string(),
                path: None,
            })
            .await;

        // THEN GlobLimitExceeded is returned with the correct limit
        assert!(
            matches!(result, Err(FileGlobError::GlobLimitExceeded { limit }) if limit == MAX_GLOB_RESULTS),
            "expected GlobLimitExceeded, got: {result:?}"
        );
    }

    #[test]
    fn descriptor_is_valid() {
        // GIVEN the descriptor the file_glob tool publishes
        let descriptor = FileGlob::descriptor();
        // WHEN it is validated
        // THEN it passes, so the registry will accept it
        assert!(descriptor.validate().is_ok());
    }
}
