//! Builds a textual tree of the current directory.
//!
//! Uses BFS traversal with a one-second timeout so collection never blocks on
//! slow or very deep filesystems.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Builds a textual tree of a directory, capped by line count.
///
/// Skips directories that are typically large and irrelevant to an AI agent:
/// `.git`, `target`, `node_modules`, `__pycache__`, `dist`, and similar.
pub struct DirectoryTreeBuilder;

impl DirectoryTreeBuilder {
    /// Builds the tree of `cwd` as indented text, capped at `max_lines`.
    ///
    /// A one-second timeout covers the whole traversal. On timeout, returns
    /// `"[arborescence : timeout]"`.
    pub async fn build(cwd: &Path, max_lines: usize) -> String {
        tokio::time::timeout(Duration::from_secs(1), Self::build_inner(cwd, max_lines))
            .await
            .unwrap_or_else(|_| "[arborescence : timeout]".to_owned())
    }

    /// BFS traversal of the directory without timeout (wrapped by [`build`](Self::build)).
    async fn build_inner(root: &Path, max_lines: usize) -> String {
        let mut lines: Vec<String> = Vec::new();
        // (directory path, indentation depth)
        let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
        queue.push_back((root.to_owned(), 0));

        while let Some((dir, depth)) = queue.pop_front() {
            if lines.len() >= max_lines {
                break;
            }

            let children = Self::collect_children(&dir).await;

            for entry in children {
                if lines.len() >= max_lines {
                    break;
                }
                Self::emit_entry(entry, depth, &mut lines, &mut queue).await;
            }
        }

        lines.join("\n")
    }

    /// Reads and filters the direct children of `dir`, sorted deterministically.
    ///
    /// Returns an empty vector if the directory is not readable.
    async fn collect_children(dir: &Path) -> Vec<tokio::fs::DirEntry> {
        let mut rd = match tokio::fs::read_dir(dir).await {
            Ok(rd) => rd,
            Err(_) => return Vec::new(),
        };

        let mut children: Vec<tokio::fs::DirEntry> = Vec::new();
        while let Ok(Some(entry)) = rd.next_entry().await {
            let name = entry.file_name();
            if !Self::should_ignore(&name.to_string_lossy()) {
                children.push(entry);
            }
        }

        // Deterministic sort by file name (alphabetical).
        children.sort_by_key(|e| e.file_name());
        children
    }

    /// Emits the line for an entry and, if it is a directory, enqueues it for BFS.
    async fn emit_entry(
        entry: tokio::fs::DirEntry,
        depth: usize,
        lines: &mut Vec<String>,
        queue: &mut VecDeque<(PathBuf, usize)>,
    ) {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let indent = "  ".repeat(depth);

        let is_dir = entry
            .file_type()
            .await
            .map(|ft| ft.is_dir())
            .unwrap_or(false);

        if is_dir {
            lines.push(format!("{}{}/", indent, name_str));
            queue.push_back((entry.path(), depth + 1));
        } else {
            lines.push(format!("{}{}", indent, name_str));
        }
    }

    /// Returns `true` if the entry should be excluded from the tree.
    fn should_ignore(name: &str) -> bool {
        matches!(
            name,
            ".git" | "target" | "node_modules" | "__pycache__" | ".DS_Store" | ".next" | "dist"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tree_builder_ignores_git_and_target() {
        // GIVEN
        let dir = tempfile::tempdir().expect("tempdir");
        tokio::fs::create_dir(dir.path().join(".git"))
            .await
            .expect("mkdir .git");
        tokio::fs::create_dir(dir.path().join("target"))
            .await
            .expect("mkdir target");
        tokio::fs::create_dir(dir.path().join("src"))
            .await
            .expect("mkdir src");
        // WHEN
        let tree = DirectoryTreeBuilder::build(dir.path(), 50).await;
        // THEN
        assert!(!tree.contains(".git"), ".git must be ignored: {tree}");
        assert!(!tree.contains("target"), "target must be ignored: {tree}");
        assert!(tree.contains("src"), "src must appear: {tree}");
    }

    #[tokio::test]
    async fn test_tree_builder_respects_max_lines() {
        // GIVEN: 20 files in the directory
        let dir = tempfile::tempdir().expect("tempdir");
        for i in 0..20 {
            tokio::fs::write(dir.path().join(format!("file{:02}.txt", i)), "x")
                .await
                .expect("write");
        }
        // WHEN
        let tree = DirectoryTreeBuilder::build(dir.path(), 5).await;
        // THEN
        assert!(
            tree.lines().count() <= 5,
            "must be limited to 5 lines: got {}",
            tree.lines().count()
        );
    }

    #[tokio::test]
    async fn test_tree_builder_ignores_node_modules() {
        // GIVEN
        let dir = tempfile::tempdir().expect("tempdir");
        tokio::fs::create_dir(dir.path().join("node_modules"))
            .await
            .expect("mkdir node_modules");
        tokio::fs::write(dir.path().join("index.js"), "")
            .await
            .expect("write");
        // WHEN
        let tree = DirectoryTreeBuilder::build(dir.path(), 50).await;
        // THEN
        assert!(
            !tree.contains("node_modules"),
            "node_modules must be ignored: {tree}"
        );
        assert!(tree.contains("index.js"), "index.js must appear: {tree}");
    }
}
