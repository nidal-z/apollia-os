//! Search file contents for a regex pattern inside the agent's sandbox.

use std::path::{Path, PathBuf};

use apollia_core::SandboxProfile;
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use crate::descriptor::{ToolDescriptor, ToolKind};
use crate::sandbox_path::{SandboxPathError, SandboxRoot};

/// Search file contents for a regex pattern inside the sandbox.
///
/// Returns matching lines with file path, line number, and optional context.
/// Supports filtering files by glob pattern. Uses the `regex` crate (no
/// subprocess — pure Rust). Binary files are silently skipped.
#[derive(Debug, Clone)]
pub struct FileGrep {
    sandbox: SandboxRoot,
}

/// Errors produced by [`FileGrep`].
#[derive(Debug, Error)]
pub enum FileGrepError {
    /// Path attempts to escape the sandbox root.
    #[error("sandbox violation: path '{path}' escapes the sandbox root")]
    SandboxViolation { path: String },

    /// The provided regex pattern is syntactically invalid.
    #[error("invalid regex pattern: {0}")]
    InvalidRegex(String),

    /// I/O error during file traversal or reading.
    #[error("I/O error: {0}")]
    IoError(String),
}

impl From<SandboxPathError> for FileGrepError {
    fn from(e: SandboxPathError) -> Self {
        match e {
            SandboxPathError::SandboxViolation { path } => Self::SandboxViolation { path },
            SandboxPathError::InitFailed { path, cause } => {
                Self::IoError(format!("{path}: {cause}"))
            }
        }
    }
}

/// Input for a file grep operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileGrepInput {
    /// Regex pattern to search for (e.g. `fn main`, `TODO.*fixme`).
    pub pattern: String,
    /// Optional directory within the sandbox to search. Defaults to sandbox root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Optional glob pattern to filter which files are searched (e.g. `*.rs`, `**/*.toml`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glob: Option<String>,
    /// Number of context lines to include before and after each match. Capped at 10. Defaults to 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_lines: Option<u32>,
    /// Whether to perform a case-insensitive match. Defaults to false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub case_insensitive: Option<bool>,
    /// Maximum number of matches to return. Defaults to 100, capped at 500.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_results: Option<u32>,
}

/// A single match found during a grep operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepMatch {
    /// Path of the matched file, relative to the sandbox root.
    pub file: String,
    /// 1-based line number of the matching line.
    pub line_number: u32,
    /// Content of the matching line (without trailing newline).
    pub content: String,
    /// Lines immediately before the match (up to `context_lines` entries).
    pub context_before: Vec<String>,
    /// Lines immediately after the match (up to `context_lines` entries).
    pub context_after: Vec<String>,
}

/// Output of a file grep operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileGrepOutput {
    /// All matches found, up to `max_results`.
    pub matches: Vec<GrepMatch>,
    /// `true` if the result set was cut short because `max_results` was reached.
    pub truncated: bool,
    /// Number of text files actually searched (binary files excluded).
    pub files_searched: u32,
}

impl FileGrep {
    /// Create a new `FileGrep` tool instance rooted at `sandbox_root`.
    ///
    /// # Errors
    ///
    /// Returns `FileGrepError::IoError` if the sandbox directory cannot be created
    /// or canonicalized.
    pub fn new(sandbox_root: PathBuf) -> Result<Self, FileGrepError> {
        let sandbox = SandboxRoot::new(sandbox_root)?;
        Ok(Self { sandbox })
    }

    /// Search for `input.pattern` across files in the sandbox.
    ///
    /// The regex is compiled before the file walk so invalid patterns are
    /// detected eagerly (fail fast). Blocking I/O is dispatched to a
    /// dedicated thread via `spawn_blocking`.
    ///
    /// # Errors
    ///
    /// - `SandboxViolation` if `path` escapes the sandbox.
    /// - `InvalidRegex` if `pattern` is not a valid regex.
    /// - `IoError` on any I/O failure during traversal.
    pub async fn run(&self, input: FileGrepInput) -> Result<FileGrepOutput, FileGrepError> {
        let base = match &input.path {
            Some(p) => self
                .sandbox
                .resolve(p)
                .map_err(|_| FileGrepError::SandboxViolation { path: p.clone() })?,
            None => self.sandbox.path().to_path_buf(),
        };

        let max_results = input.max_results.unwrap_or(100).min(500) as usize;
        let context_lines = input.context_lines.unwrap_or(0).min(10) as usize;
        let case_insensitive = input.case_insensitive.unwrap_or(false);

        // Prepend (?i) for case-insensitive matching.
        let pattern_str = if case_insensitive {
            format!("(?i){}", input.pattern)
        } else {
            input.pattern.clone()
        };

        // Compile regex before any I/O — invalid patterns are detected here.
        let regex = regex::Regex::new(&pattern_str)
            .map_err(|e| FileGrepError::InvalidRegex(e.to_string()))?;

        let sandbox_root = self.sandbox.path().to_path_buf();
        let glob_filter = input.glob.clone();

        let output = tokio::task::spawn_blocking(move || {
            grep_files(
                &base,
                &sandbox_root,
                glob_filter.as_deref(),
                &regex,
                context_lines,
                max_results,
            )
        })
        .await
        .map_err(|e| FileGrepError::IoError(e.to_string()))??;

        Ok(output)
    }

    /// Return the tool descriptor for registration in the `ToolRegistry`.
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "file_grep".to_string(),
            version: "1.0.0".to_string(),
            description: "Search file contents for a regex pattern inside the agent's sandbox. \
                Returns matching lines with file path, line number, and optional context. \
                Supports filtering files by glob pattern. Binary files are silently skipped."
                .to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search for (e.g. 'fn main', 'TODO.*fixme')"
                    },
                    "path": {
                        "type": "string",
                        "description": "Optional directory within the sandbox to search. Defaults to sandbox root."
                    },
                    "glob": {
                        "type": "string",
                        "description": "Optional glob pattern to filter which files are searched (e.g. *.rs, **/*.toml)"
                    },
                    "context_lines": {
                        "type": "integer",
                        "minimum": 0,
                        "maximum": 10,
                        "description": "Number of lines to include before and after each match. Defaults to 0."
                    },
                    "case_insensitive": {
                        "type": "boolean",
                        "description": "Perform a case-insensitive match. Defaults to false."
                    },
                    "max_results": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 500,
                        "description": "Maximum number of matches to return. Defaults to 100."
                    }
                },
                "required": ["pattern"]
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "matches": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "file": { "type": "string" },
                                "line_number": { "type": "integer" },
                                "content": { "type": "string" },
                                "context_before": { "type": "array", "items": { "type": "string" } },
                                "context_after": { "type": "array", "items": { "type": "string" } }
                            },
                            "required": ["file", "line_number", "content", "context_before", "context_after"]
                        },
                        "description": "All matches found, up to max_results"
                    },
                    "truncated": {
                        "type": "boolean",
                        "description": "True if results were cut short by max_results"
                    },
                    "files_searched": {
                        "type": "integer",
                        "description": "Number of text files actually searched (binary files excluded)"
                    }
                },
                "required": ["matches", "truncated", "files_searched"]
            })),
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec![
                "file".to_string(),
                "search".to_string(),
                "grep".to_string(),
                "regex".to_string(),
            ],
            dangerous: false,
        }
    }
}

/// Collect all candidate files under `base`, optionally filtered by a glob pattern.
///
/// Files outside the sandbox root are silently excluded. Only regular files are returned.
fn collect_files(
    base: &Path,
    sandbox_root: &Path,
    glob_filter: Option<&str>,
) -> Result<Vec<PathBuf>, FileGrepError> {
    let pattern = glob_filter.unwrap_or("**/*");
    let full_pattern = base.join(pattern).to_string_lossy().into_owned();

    let entries = glob::glob(&full_pattern)
        .map_err(|e| FileGrepError::IoError(format!("glob pattern error: {e}")))?;

    let mut files = Vec::new();
    for entry in entries {
        let path = entry.map_err(|e| FileGrepError::IoError(e.to_string()))?;
        if path.is_file() && path.starts_with(sandbox_root) {
            files.push(path);
        }
    }

    Ok(files)
}

/// Search each candidate file for lines matching `regex`.
///
/// Binary files (non-UTF-8) are silently skipped and not included in `files_searched`.
/// Collection stops once `max_results` matches are gathered and `truncated` is set to `true`.
fn grep_files(
    base: &Path,
    sandbox_root: &Path,
    glob_filter: Option<&str>,
    regex: &regex::Regex,
    context_lines: usize,
    max_results: usize,
) -> Result<FileGrepOutput, FileGrepError> {
    let files = collect_files(base, sandbox_root, glob_filter)?;

    let mut matches: Vec<GrepMatch> = Vec::new();
    let mut files_searched: u32 = 0;
    let mut truncated = false;

    'file_loop: for file_path in &files {
        let bytes = std::fs::read(file_path).map_err(|e| FileGrepError::IoError(e.to_string()))?;

        // Silently skip binary (non-UTF-8) files.
        let content = match String::from_utf8(bytes) {
            Ok(s) => s,
            Err(_) => continue,
        };

        files_searched += 1;

        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            if !regex.is_match(line) {
                continue;
            }

            let context_before: Vec<String> = if context_lines > 0 {
                let start = i.saturating_sub(context_lines);
                lines[start..i].iter().map(|s| s.to_string()).collect()
            } else {
                Vec::new()
            };

            let context_after: Vec<String> = if context_lines > 0 {
                let end = (i + 1 + context_lines).min(lines.len());
                lines[(i + 1)..end].iter().map(|s| s.to_string()).collect()
            } else {
                Vec::new()
            };

            // strip_prefix is guaranteed to succeed: collect_files ensures all paths
            // start with sandbox_root.
            let relative_path = file_path
                .strip_prefix(sandbox_root)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| file_path.to_string_lossy().into_owned());

            matches.push(GrepMatch {
                file: relative_path,
                line_number: (i + 1) as u32,
                content: line.to_string(),
                context_before,
                context_after,
            });

            if matches.len() >= max_results {
                truncated = true;
                break 'file_loop;
            }
        }
    }

    Ok(FileGrepOutput {
        matches,
        truncated,
        files_searched,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_file(dir: &Path, rel: &str, content: &[u8]) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dirs");
        }
        let mut f = std::fs::File::create(&path).expect("create file");
        f.write_all(content).expect("write content");
    }

    #[tokio::test]
    async fn grep_returns_matching_lines_with_file_and_line_number() {
        // GIVEN: file "main.rs" containing "fn main() {}" on line 1
        let tmp = TempDir::new().expect("temp dir");
        create_file(tmp.path(), "main.rs", b"fn main() {}\n");

        let tool = FileGrep::new(tmp.path().to_path_buf()).expect("FileGrep::new");

        // WHEN: file_grep(pattern="fn main")
        let result = tool
            .run(FileGrepInput {
                pattern: "fn main".to_string(),
                path: None,
                glob: None,
                context_lines: None,
                case_insensitive: None,
                max_results: None,
            })
            .await
            .expect("run ok");

        // THEN: one match with file="main.rs", line_number=1
        assert_eq!(result.matches.len(), 1);
        let m = &result.matches[0];
        assert_eq!(m.file, "main.rs");
        assert_eq!(m.line_number, 1);
        assert_eq!(m.content, "fn main() {}");
    }

    #[tokio::test]
    async fn grep_context_lines_adds_surrounding_context() {
        // GIVEN: file with 7 lines, match on line 5
        let tmp = TempDir::new().expect("temp dir");
        let content = b"line1\nline2\nline3\nline4\nMATCH_HERE\nline6\nline7\n";
        create_file(tmp.path(), "ctx.txt", content);

        let tool = FileGrep::new(tmp.path().to_path_buf()).expect("FileGrep::new");

        // WHEN: file_grep(pattern="MATCH_HERE", context_lines=2)
        let result = tool
            .run(FileGrepInput {
                pattern: "MATCH_HERE".to_string(),
                path: None,
                glob: None,
                context_lines: Some(2),
                case_insensitive: None,
                max_results: None,
            })
            .await
            .expect("run ok");

        // THEN: context_before = ["line3", "line4"], context_after = ["line6", "line7"]
        assert_eq!(result.matches.len(), 1);
        let m = &result.matches[0];
        assert_eq!(m.line_number, 5);
        assert_eq!(m.context_before, vec!["line3", "line4"]);
        assert_eq!(m.context_after, vec!["line6", "line7"]);
    }

    #[tokio::test]
    async fn grep_glob_filter_limits_searched_files() {
        // GIVEN: "a.rs" and "b.py" both containing "TODO"
        let tmp = TempDir::new().expect("temp dir");
        create_file(tmp.path(), "a.rs", b"// TODO fix this\n");
        create_file(tmp.path(), "b.py", b"# TODO fix this\n");

        let tool = FileGrep::new(tmp.path().to_path_buf()).expect("FileGrep::new");

        // WHEN: file_grep(pattern="TODO", glob="*.rs")
        let result = tool
            .run(FileGrepInput {
                pattern: "TODO".to_string(),
                path: None,
                glob: Some("*.rs".to_string()),
                context_lines: None,
                case_insensitive: None,
                max_results: None,
            })
            .await
            .expect("run ok");

        // THEN: only "a.rs" appears in results, files_searched=1
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].file, "a.rs");
        assert_eq!(result.files_searched, 1);
    }

    #[tokio::test]
    async fn grep_case_insensitive_matches() {
        // GIVEN: file containing "Hello World"
        let tmp = TempDir::new().expect("temp dir");
        create_file(tmp.path(), "greet.txt", b"Hello World\n");

        let tool = FileGrep::new(tmp.path().to_path_buf()).expect("FileGrep::new");

        // WHEN: file_grep(pattern="hello", case_insensitive=true)
        let result = tool
            .run(FileGrepInput {
                pattern: "hello".to_string(),
                path: None,
                glob: None,
                context_lines: None,
                case_insensitive: Some(true),
                max_results: None,
            })
            .await
            .expect("run ok");

        // THEN: match is found
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].content, "Hello World");
    }

    #[tokio::test]
    async fn grep_max_results_truncates() {
        // GIVEN: file with 200 matching lines
        let tmp = TempDir::new().expect("temp dir");
        let content: String = (1..=200).map(|i| format!("match line {i}\n")).collect();
        create_file(tmp.path(), "large.txt", content.as_bytes());

        let tool = FileGrep::new(tmp.path().to_path_buf()).expect("FileGrep::new");

        // WHEN: file_grep(pattern="match", max_results=10)
        let result = tool
            .run(FileGrepInput {
                pattern: "match".to_string(),
                path: None,
                glob: None,
                context_lines: None,
                case_insensitive: None,
                max_results: Some(10),
            })
            .await
            .expect("run ok");

        // THEN: 10 matches returned and truncated=true
        assert_eq!(result.matches.len(), 10);
        assert!(result.truncated);
    }

    #[tokio::test]
    async fn grep_invalid_regex_returns_error() {
        // GIVEN: syntactically invalid pattern
        let tmp = TempDir::new().expect("temp dir");
        let tool = FileGrep::new(tmp.path().to_path_buf()).expect("FileGrep::new");

        // WHEN: file_grep(pattern="[invalid")
        let result = tool
            .run(FileGrepInput {
                pattern: "[invalid".to_string(),
                path: None,
                glob: None,
                context_lines: None,
                case_insensitive: None,
                max_results: None,
            })
            .await;

        // THEN: Err(InvalidRegex)
        assert!(result.is_err());
        assert!(
            matches!(result.err(), Some(FileGrepError::InvalidRegex(_))),
            "expected InvalidRegex"
        );
    }

    #[tokio::test]
    async fn grep_skips_binary_files() {
        // GIVEN: a binary file and a text file in the sandbox
        let tmp = TempDir::new().expect("temp dir");
        // Binary: contains bytes that are invalid UTF-8
        create_file(
            tmp.path(),
            "image.bin",
            &[0xFF, 0xFE, 0x00, 0x01, 0x80, 0x90],
        );
        create_file(tmp.path(), "source.txt", b"hello world\n");

        let tool = FileGrep::new(tmp.path().to_path_buf()).expect("FileGrep::new");

        // WHEN: file_grep(pattern=".*")
        let result = tool
            .run(FileGrepInput {
                pattern: ".*".to_string(),
                path: None,
                glob: None,
                context_lines: None,
                case_insensitive: None,
                max_results: None,
            })
            .await
            .expect("run ok");

        // THEN: only text file is searched, binary file is not counted
        assert_eq!(result.files_searched, 1);
        assert!(result.matches.iter().all(|m| m.file == "source.txt"));
    }

    #[test]
    fn descriptor_is_valid() {
        let descriptor = FileGrep::descriptor();
        assert!(descriptor.validate().is_ok());
    }
}
