//! Construction d'une arborescence textuelle du répertoire courant.
//!
//! Parcours BFS avec timeout d'une seconde pour ne jamais bloquer la collecte
//! sur des systèmes de fichiers lents ou très profonds.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Construit une arborescence textuelle d'un répertoire, limitée en lignes.
///
/// Ignore les répertoires habituellement volumineux et non pertinents pour
/// un agent IA : `.git`, `target`, `node_modules`, `__pycache__`, `dist`, etc.
pub struct DirectoryTreeBuilder;

impl DirectoryTreeBuilder {
    /// Construit l'arborescence de `cwd` en texte indenté, limitée à `max_lines`.
    ///
    /// Un timeout d'une seconde est appliqué à l'ensemble du parcours.
    /// Si le timeout est dépassé, retourne `"[arborescence : timeout]"`.
    pub async fn build(cwd: &Path, max_lines: usize) -> String {
        tokio::time::timeout(Duration::from_secs(1), Self::build_inner(cwd, max_lines))
            .await
            .unwrap_or_else(|_| "[arborescence : timeout]".to_owned())
    }

    /// Parcours BFS du répertoire sans timeout (enveloppé par [`build`](Self::build)).
    async fn build_inner(root: &Path, max_lines: usize) -> String {
        let mut lines: Vec<String> = Vec::new();
        // (chemin du répertoire, profondeur d'indentation)
        let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
        queue.push_back((root.to_owned(), 0));

        while let Some((dir, depth)) = queue.pop_front() {
            if lines.len() >= max_lines {
                break;
            }

            let mut rd = match tokio::fs::read_dir(&dir).await {
                Ok(rd) => rd,
                Err(_) => continue,
            };

            let mut children: Vec<tokio::fs::DirEntry> = Vec::new();
            while let Ok(Some(entry)) = rd.next_entry().await {
                let name = entry.file_name();
                if !Self::should_ignore(&name.to_string_lossy()) {
                    children.push(entry);
                }
            }

            // Tri déterministe : répertoires d'abord, puis fichiers, ordre alphabétique
            children.sort_by_key(|e| e.file_name());

            for entry in children {
                if lines.len() >= max_lines {
                    break;
                }

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
        }

        lines.join("\n")
    }

    /// Retourne `true` si l'entrée doit être ignorée dans l'arborescence.
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
        // GIVEN : 20 fichiers dans le répertoire
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
