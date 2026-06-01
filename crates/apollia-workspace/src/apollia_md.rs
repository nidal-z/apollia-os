//! Recursive lookup of the `APOLLIA.md` file starting from the current directory.
//!
//! Walks up the directory hierarchy (CWD then parents) up to
//! `apollia_md_search_depth` levels. The content is middle-trimmed so it never
//! exceeds `apollia_md_max_bytes` bytes in the prompt.

use std::path::{Path, PathBuf};

use apollia_core::truncate_middle;

/// Finds and reads the nearest `APOLLIA.md` file from the given directory.
///
/// Priority: CWD > immediate parent > ... > root (capped at `search_depth` levels).
pub struct ApolliamdFinder;

impl ApolliamdFinder {
    /// Searches for `APOLLIA.md` from `cwd` upward through parent directories.
    ///
    /// Returns `Some((path, truncated_content))` as soon as the file is found,
    /// `None` if no file exists within the depth limit.
    ///
    /// The content is middle-trimmed to `max_bytes` to protect the LLM context
    /// window.
    pub async fn find(
        cwd: &Path,
        max_bytes: usize,
        search_depth: usize,
    ) -> Option<(PathBuf, String)> {
        let mut current = cwd.to_owned();
        for _ in 0..=search_depth {
            let candidate = current.join("APOLLIA.md");
            if candidate.exists() {
                let content = tokio::fs::read_to_string(&candidate).await.ok()?;
                let (truncated, _) = truncate_middle(&content, max_bytes);
                return Some((candidate, truncated));
            }
            if !current.pop() {
                break;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_apollia_md_finder_in_cwd() {
        // GIVEN: APOLLIA.md in the temporary working directory
        let dir = tempfile::tempdir().expect("tempdir");
        tokio::fs::write(dir.path().join("APOLLIA.md"), "# Rules\nNo magic.")
            .await
            .expect("write");
        // WHEN
        let result = ApolliamdFinder::find(dir.path(), 8192, 5).await;
        // THEN
        assert!(result.is_some(), "should find APOLLIA.md in CWD");
        let (_, content) = result.unwrap();
        assert!(content.contains("Rules"), "content: {content}");
    }

    #[tokio::test]
    async fn test_apollia_md_finder_in_parent() {
        // GIVEN: APOLLIA.md in the parent, CWD in a subdirectory
        let dir = tempfile::tempdir().expect("tempdir");
        tokio::fs::write(dir.path().join("APOLLIA.md"), "parent rules")
            .await
            .expect("write");
        let sub = dir.path().join("src");
        tokio::fs::create_dir(&sub).await.expect("mkdir");
        // WHEN
        let result = ApolliamdFinder::find(&sub, 8192, 5).await;
        // THEN
        assert!(result.is_some(), "should find APOLLIA.md in parent");
        let (path, _) = result.unwrap();
        assert_eq!(path, dir.path().join("APOLLIA.md"));
    }

    #[tokio::test]
    async fn test_apollia_md_absent_returns_none() {
        // GIVEN: directory without APOLLIA.md (and none in its parents within the limit)
        let dir = tempfile::tempdir().expect("tempdir");
        // WHEN - depth 0: we do not walk up
        let result = ApolliamdFinder::find(dir.path(), 8192, 0).await;
        // THEN
        assert!(result.is_none(), "no APOLLIA.md at depth 0");
    }

    #[tokio::test]
    async fn test_apollia_md_content_truncated_to_max_bytes() {
        // GIVEN: 10 KB APOLLIA.md, max_bytes = 100
        let dir = tempfile::tempdir().expect("tempdir");
        let big_content = "x".repeat(10_000);
        tokio::fs::write(dir.path().join("APOLLIA.md"), &big_content)
            .await
            .expect("write");
        // WHEN
        let result = ApolliamdFinder::find(dir.path(), 100, 5).await;
        // THEN - middle-trim adds a marker, but the result is smaller than the original
        assert!(result.is_some());
        let (_, content) = result.unwrap();
        assert!(
            content.len() < big_content.len(),
            "content must be truncated: got {} bytes",
            content.len()
        );
    }
}
