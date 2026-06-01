//! [`TreeProvider`], exposes the directory tree of the current working directory.
//!
//! BFS traversal with a one-second timeout; system directories are skipped
//! (`.git`, `target`, `node_modules`, `__pycache__`, `dist`, etc.).

use std::path::Path;

use apollia_core::workspace::{WorkspaceProvider, WorkspaceSection, WorkspaceSlice};

use crate::tree::DirectoryTreeBuilder;

/// Builds a textual directory tree of the current working directory.
///
/// Uses [`DirectoryTreeBuilder`] with a default cap of 100 lines.
pub struct TreeProvider {
    max_lines: usize,
}

impl TreeProvider {
    /// Creates a tree provider with the given maximum number of lines.
    pub fn new(max_lines: usize) -> Self {
        Self { max_lines }
    }
}

impl Default for TreeProvider {
    fn default() -> Self {
        Self::new(100)
    }
}

#[async_trait::async_trait]
impl WorkspaceProvider for TreeProvider {
    fn name(&self) -> &str {
        "tree"
    }

    fn description(&self) -> &str {
        "Arborescence du répertoire courant"
    }

    fn priority(&self) -> u8 {
        30
    }

    async fn collect(&self, cwd: &Path) -> WorkspaceSlice {
        let tree = DirectoryTreeBuilder::build(cwd, self.max_lines).await;

        if tree.is_empty() || tree == "[arborescence : timeout]" {
            return WorkspaceSlice::empty("tree");
        }

        WorkspaceSlice {
            source: "tree".to_owned(),
            sections: vec![WorkspaceSection {
                title: "Arborescence".to_owned(),
                content: tree,
                source: "tree".to_owned(),
            }],
            errors: vec![],
            collected_at: std::time::Instant::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn tree_provider_returns_section() {
        // GIVEN a directory with a few files
        let dir = tempfile::tempdir().expect("tempdir");
        tokio::fs::write(dir.path().join("main.rs"), "fn main() {}")
            .await
            .expect("write");
        let provider = TreeProvider::default();
        // WHEN
        let slice = provider.collect(dir.path()).await;
        // THEN
        assert!(!slice.is_empty(), "tree should have content");
        assert!(slice.sections[0].content.contains("main.rs"));
    }

    #[tokio::test]
    async fn tree_provider_respects_max_lines() {
        // GIVEN a directory with 20 files and max_lines = 5
        let dir = tempfile::tempdir().expect("tempdir");
        for i in 0..20 {
            tokio::fs::write(dir.path().join(format!("f{i}.rs")), "")
                .await
                .expect("write");
        }
        let provider = TreeProvider::new(5);
        // WHEN
        let slice = provider.collect(dir.path()).await;
        // THEN
        if !slice.is_empty() {
            let lines = slice.sections[0].content.lines().count();
            assert!(lines <= 5, "max 5 lines, got {lines}");
        }
    }
}
