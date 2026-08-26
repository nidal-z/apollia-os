//! [`GitProvider`] exposes the git state of the current directory.
//!
//! Collects the current branch, the status of modified files, and recent
//! commits. Inactive outside a git repository (`is_applicable` returns `false`).

use std::path::Path;

use apollia_core::workspace::{WorkspaceProvider, WorkspaceSection, WorkspaceSlice};

use crate::config::GitProviderConfig;
use crate::git::GitContextCollector;

/// Exposes the git state: current branch, modified files, recent commits.
///
/// Inactive outside a git repository: [`is_applicable`](GitProvider::is_applicable)
/// walks up the parent directories looking for a `.git`.
pub struct GitProvider {
    config: GitProviderConfig,
}

impl GitProvider {
    /// Builds a git provider with the given configuration.
    pub fn new(config: GitProviderConfig) -> Self {
        Self { config }
    }
}

impl Default for GitProvider {
    fn default() -> Self {
        Self::new(GitProviderConfig::default())
    }
}

#[async_trait::async_trait]
impl WorkspaceProvider for GitProvider {
    fn name(&self) -> &str {
        "git"
    }

    fn description(&self) -> &str {
        "Git branch, file status and recent commits"
    }

    fn priority(&self) -> u8 {
        10
    }

    /// Returns `false` if `cwd` is not inside a git repository.
    fn is_applicable(&self, cwd: &Path) -> bool {
        std::iter::successors(Some(cwd), |p| p.parent()).any(|p| p.join(".git").exists())
    }

    async fn collect(&self, cwd: &Path) -> WorkspaceSlice {
        let result = GitContextCollector::collect(cwd, self.config.git_status_max_lines).await;

        let Some(branch) = result.branch else {
            return WorkspaceSlice::empty("git");
        };

        let clean_marker = if result.is_clean { " (clean)" } else { "" };
        let mut content = format!("Branche : {}{}", branch, clean_marker);

        if let Some(status) = &result.status {
            content.push_str(&format!("\n\nStatus :\n{}", status));
        }
        if !result.recent_commits.is_empty() {
            content.push_str(&format!(
                "\n\nRecent commits:\n{}",
                result.recent_commits.join("\n")
            ));
        }

        WorkspaceSlice {
            source: "git".to_owned(),
            sections: vec![WorkspaceSection {
                title: "Git".to_owned(),
                content,
                source: "git".to_owned(),
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
    async fn git_provider_inactive_outside_repo() {
        // GIVEN a directory without .git
        let tmp = tempfile::tempdir().expect("tempdir");
        let provider = GitProvider::default();
        // WHEN / THEN
        assert!(!provider.is_applicable(tmp.path()));
    }

    #[tokio::test]
    async fn git_provider_active_in_repo() {
        // GIVEN a directory with .git
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(tmp.path().join(".git")).expect("create .git");
        let provider = GitProvider::default();
        // WHEN / THEN
        assert!(provider.is_applicable(tmp.path()));
    }

    #[tokio::test]
    async fn git_provider_collects_in_real_repo() {
        // GIVEN the current repo
        let cwd = std::env::current_dir().expect("current_dir");
        let provider = GitProvider::default();
        // WHEN
        let slice = provider.collect(&cwd).await;
        // THEN at least one git section
        assert!(!slice.sections.is_empty(), "expected git section in repo");
        assert!(
            slice.sections[0].content.contains("Branche"),
            "content: {}",
            slice.sections[0].content
        );
    }
}
