//! Automatic detection of code conventions via sampling plus a lightweight LLM.
//!
//! [`StyleDetector`] identifies the project's dominant file extension, samples
//! a configurable number of source files, then queries the default LLM backend
//! to extract the code conventions as bullet points.
//!
//! The LLM call is bounded by a configurable timeout (`style_detection_timeout_ms`).
//! Any error (LLM unavailable, timeout, empty directory) yields `None` without
//! propagating an error to the caller.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use apollia_llm::{ChatMessage, CompletionRequest, LlmRouter};

use crate::config::StyleProviderConfig;

/// Extensions excluded from dominant-language detection.
///
/// Only source code files are wanted, not configuration, documentation, or
/// build metadata.
const EXCLUDED_EXTENSIONS: &[&str] = &[
    "md", "toml", "json", "lock", "yaml", "yml", "txt", "log", "xml", "csv",
];

/// Directories skipped during the recursive walk.
const SKIPPED_DIRS: &[&str] = &[
    "target",
    "node_modules",
    ".git",
    "dist",
    "build",
    "__pycache__",
];

/// Automatically detects the project's code conventions via a lightweight LLM.
///
/// Non-blocking: the LLM call is bounded by `config.style_detection_timeout_ms`.
/// Returns `None` if the directory is empty, the LLM is unavailable, or the
/// timeout is exceeded.
pub struct StyleDetector;

impl StyleDetector {
    /// Detects the project's code conventions via sampling plus a lightweight LLM.
    ///
    /// Steps:
    /// 1. Identify the dominant extension across 2 directory levels.
    /// 2. Sample up to `config.style_sample_count` files of that extension.
    /// 3. Read `config.style_sample_lines_per_file` lines per file (head plus tail).
    /// 4. Call the default LLM backend with timeout `config.style_detection_timeout_ms`.
    ///
    /// Returns `None` if: the directory has no source files, the LLM times out, or the LLM is unavailable.
    pub async fn detect(
        cwd: &Path,
        llm_router: &LlmRouter,
        config: &StyleProviderConfig,
    ) -> Option<String> {
        let ext = Self::dominant_extension(cwd).await?;

        let files = Self::sample_files(cwd, &ext, config.sample_count).await;
        if files.is_empty() {
            return None;
        }

        let samples = Self::collect_samples(&files, config.sample_lines_per_file).await;

        let prompt = format!(
            "Extract the coding style conventions from these {} code samples in 5 bullet points max. \
             Focus on: naming conventions, error handling patterns, comment style, struct organization.\n\n{}",
            ext, samples
        );

        let backend = match llm_router.route_fast() {
            Ok(b) => b,
            Err(e) => {
                tracing::debug!(
                    error = %e,
                    reason = "no fast route is available",
                    "workspace.style.detection.skipped"
                );
                return None;
            }
        };
        let req = CompletionRequest {
            messages: vec![ChatMessage::user(prompt)],
            max_tokens: Some(512),
            ..Default::default()
        };

        let result = tokio::time::timeout(
            Duration::from_millis(config.timeout_ms),
            backend.complete(req),
        )
        .await;

        match result {
            Ok(Ok(resp)) => {
                let text = resp.content.trim().to_string();
                if text.is_empty() {
                    None
                } else {
                    Some(text)
                }
            }
            Ok(Err(e)) => {
                tracing::debug!(error = %e, "workspace.style.detection.failed");
                None
            }
            Err(_elapsed) => {
                tracing::debug!("workspace.style.detection.timeout");
                None
            }
        }
    }

    /// Returns the most frequent file extension in the directory (recursive, 2 levels).
    ///
    /// Excludes non-source extensions (`.md`, `.toml`, `.json`, `.lock`, `.yaml`, ...)
    /// and system directories (`target`, `node_modules`, ...).
    /// Returns `None` if no source file is found.
    pub async fn dominant_extension(cwd: &Path) -> Option<String> {
        let cwd = cwd.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let mut counts: HashMap<String, usize> = HashMap::new();
            collect_extensions_recursive(&cwd, 0, 2, &mut counts);
            counts
                .into_iter()
                .max_by_key(|(_, count)| *count)
                .map(|(ext, _)| ext)
        })
        .await
        .ok()
        .flatten()
    }

    /// Returns up to `max_count` files of the given extension with even distribution.
    ///
    /// If the number of files found is less than or equal to `max_count`, all
    /// files are returned. Otherwise, an evenly spaced selection ensures the
    /// whole project is represented.
    async fn sample_files(cwd: &Path, ext: &str, max_count: usize) -> Vec<PathBuf> {
        let cwd = cwd.to_path_buf();
        let ext = ext.to_owned();
        tokio::task::spawn_blocking(move || {
            let mut all: Vec<PathBuf> = Vec::new();
            collect_files_recursive(&cwd, &ext, 0, 2, &mut all);
            all.sort(); // deterministic order
            if all.len() <= max_count {
                return all;
            }
            let step = all.len() / max_count;
            (0..max_count).map(|i| all[i * step].clone()).collect()
        })
        .await
        .unwrap_or_default()
    }

    /// Reads up to `lines_per_file` lines per file (head plus tail, 50/50).
    ///
    /// Each file is prefixed with its name to give the excerpts context.
    /// If the file does not exceed `lines_per_file` lines, it is included whole.
    /// Unreadable files are silently skipped.
    async fn collect_samples(files: &[PathBuf], lines_per_file: usize) -> String {
        let half = (lines_per_file / 2).max(1);
        let mut parts = Vec::with_capacity(files.len());

        for path in files {
            if let Ok(content) = tokio::fs::read_to_string(path).await {
                let lines: Vec<&str> = content.lines().collect();
                let sample = if lines.len() <= lines_per_file {
                    content.as_str().to_owned()
                } else {
                    let head = lines[..half].join("\n");
                    let tail = lines[lines.len() - half..].join("\n");
                    format!(
                        "{}\n... [{} lines omitted] ...\n{}",
                        head,
                        lines.len() - lines_per_file,
                        tail
                    )
                };
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                parts.push(format!("=== {name} ===\n{sample}"));
            }
        }

        parts.join("\n\n")
    }
}

/// Walks `dir` recursively up to `max_depth` levels and counts source extensions.
fn collect_extensions_recursive(
    dir: &Path,
    depth: usize,
    max_depth: usize,
    counts: &mut HashMap<String, usize>,
) {
    if depth > max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !name.starts_with('.') && !SKIPPED_DIRS.contains(&name.as_str()) {
                collect_extensions_recursive(&path, depth + 1, max_depth, counts);
            }
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext_lower = ext.to_ascii_lowercase();
            if !EXCLUDED_EXTENSIONS.contains(&ext_lower.as_str()) {
                *counts.entry(ext_lower).or_insert(0) += 1;
            }
        }
    }
}

/// Walks `dir` recursively up to `max_depth` levels and collects files matching `ext`.
fn collect_files_recursive(
    dir: &Path,
    ext: &str,
    depth: usize,
    max_depth: usize,
    files: &mut Vec<PathBuf>,
) {
    if depth > max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !name.starts_with('.') && !SKIPPED_DIRS.contains(&name.as_str()) {
                collect_files_recursive(&path, ext, depth + 1, max_depth, files);
            }
        } else {
            let matches = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase() == ext)
                .unwrap_or(false);
            if matches {
                files.push(path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dominant_extension_rust_project() {
        // GIVEN: directory with *.rs (5 files) and 1 README.md
        let dir = tempfile::tempdir().unwrap();
        for i in 0..5 {
            tokio::fs::write(dir.path().join(format!("file{i}.rs")), "fn main() {}")
                .await
                .unwrap();
        }
        tokio::fs::write(dir.path().join("README.md"), "# readme")
            .await
            .unwrap();
        // WHEN
        let ext = StyleDetector::dominant_extension(dir.path()).await;
        // THEN
        assert_eq!(ext, Some("rs".to_string()));
    }

    #[tokio::test]
    async fn test_dominant_extension_empty_dir() {
        // GIVEN: empty directory
        let dir = tempfile::tempdir().unwrap();
        // WHEN
        let ext = StyleDetector::dominant_extension(dir.path()).await;
        // THEN
        assert!(ext.is_none());
    }

    #[tokio::test]
    async fn test_dominant_extension_excludes_non_source() {
        // GIVEN: directory containing only excluded files
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::write(dir.path().join("config.toml"), "[x]")
            .await
            .unwrap();
        tokio::fs::write(dir.path().join("data.json"), "{}")
            .await
            .unwrap();
        tokio::fs::write(dir.path().join("README.md"), "# readme")
            .await
            .unwrap();
        // WHEN
        let ext = StyleDetector::dominant_extension(dir.path()).await;
        // THEN
        assert!(ext.is_none());
    }

    #[tokio::test]
    async fn test_sample_files_respects_max_count() {
        // GIVEN: directory with 10 .rs files
        let dir = tempfile::tempdir().unwrap();
        for i in 0..10 {
            tokio::fs::write(dir.path().join(format!("f{i}.rs")), "fn f() {}")
                .await
                .unwrap();
        }
        // WHEN: max_count = 3
        let files = StyleDetector::sample_files(dir.path(), "rs", 3).await;
        // THEN
        assert!(
            files.len() <= 3,
            "expected at most 3 files, got {}",
            files.len()
        );
    }

    #[tokio::test]
    async fn test_sample_files_fewer_than_max() {
        // GIVEN: directory with 2 .rs files, max_count = 7
        let dir = tempfile::tempdir().unwrap();
        for i in 0..2 {
            tokio::fs::write(dir.path().join(format!("f{i}.rs")), "fn f() {}")
                .await
                .unwrap();
        }
        // WHEN
        let files = StyleDetector::sample_files(dir.path(), "rs", 7).await;
        // THEN: all files are returned
        assert_eq!(files.len(), 2);
    }

    #[tokio::test]
    async fn test_collect_samples_truncates_long_files() {
        // GIVEN: a 500-line file
        let dir = tempfile::tempdir().unwrap();
        let content: String = (0..500).map(|i| format!("line {i}\n")).collect();
        let path = dir.path().join("big.rs");
        tokio::fs::write(&path, &content).await.unwrap();
        // WHEN: lines_per_file = 20
        let result = StyleDetector::collect_samples(&[path], 20).await;
        // THEN: contains the truncation marker
        assert!(
            result.contains("lines omitted"),
            "expected truncation marker"
        );
    }

    #[tokio::test]
    async fn test_collect_samples_short_file_not_truncated() {
        // GIVEN: a 5-line file
        let dir = tempfile::tempdir().unwrap();
        let content = "fn a() {}\nfn b() {}\nfn c() {}\nfn d() {}\nfn e() {}";
        let path = dir.path().join("short.rs");
        tokio::fs::write(&path, content).await.unwrap();
        // WHEN
        let result = StyleDetector::collect_samples(&[path], 20).await;
        // THEN: no truncation marker
        assert!(!result.contains("lines omitted"));
        assert!(result.contains("fn a()"));
    }
}
