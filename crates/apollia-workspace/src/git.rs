//! Git context collector via subprocesses.
//!
//! All git commands run through [`tokio::process::Command`]. No `git2`
//! dependency, which keeps the build free of extra C dependencies.
//! Any error (git missing, directory outside a repo) is swallowed silently.

use std::path::Path;

/// Raw result of git collection for a given directory.
///
/// Every field is optional or has an inert default, so it never panics when
/// git is missing or the directory is not a repository.
#[derive(Debug, Default)]
pub struct GitResult {
    /// Current branch (`git branch --show-current`). `None` outside a repo.
    pub branch: Option<String>,
    /// Truncated `git status --short` output, or `None` if the working tree is clean or outside a repo.
    pub status: Option<String>,
    /// Last 5 commits (`git log --oneline -n5`).
    pub recent_commits: Vec<String>,
    /// `true` when `git status --short` produces empty output.
    pub is_clean: bool,
}

/// Collects a directory's git context via subprocesses.
///
/// Every method is fail-silent: a git error (missing command, directory
/// outside a repo, permissions) yields `None` or empty values without ever
/// emitting an `ERROR` log.
pub struct GitContextCollector;

impl GitContextCollector {
    /// Collects the branch, status, and recent commits of the git repo at `cwd`.
    ///
    /// Runs three subprocesses in sequence. Each is independent: a failure in
    /// one does not prevent the others.
    pub async fn collect(cwd: &Path, git_status_max_lines: usize) -> GitResult {
        let branch = Self::run_git(cwd, &["branch", "--show-current"])
            .await
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());

        let status_raw = Self::run_git(cwd, &["status", "--short"]).await;
        let is_clean = status_raw
            .as_deref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(false);
        let status = status_raw
            .map(|s| {
                s.lines()
                    .take(git_status_max_lines)
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .filter(|s| !s.is_empty());

        let commits_raw = Self::run_git(cwd, &["log", "--oneline", "-n5"]).await;
        let recent_commits = commits_raw
            .map(|s| s.lines().map(|l| l.to_owned()).collect())
            .unwrap_or_default();

        GitResult {
            branch,
            status,
            recent_commits,
            is_clean,
        }
    }

    /// Runs a git command with the given arguments in `cwd`.
    ///
    /// Returns `Some(stdout)` when the command succeeds (exit code 0 and valid
    /// UTF-8), `None` in every other case.
    async fn run_git(cwd: &Path, args: &[&str]) -> Option<String> {
        let output = tokio::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .await
            .ok()?;

        if output.status.success() {
            String::from_utf8(output.stdout).ok()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_git_collector_in_repo() {
        // GIVEN: CWD is the Apollia repo directory itself
        let cwd = std::env::current_dir().expect("current_dir");
        // WHEN
        let result = GitContextCollector::collect(&cwd, 200).await;
        // THEN
        assert!(
            result.branch.is_some(),
            "should detect branch in a git repo"
        );
    }

    #[tokio::test]
    async fn test_git_collector_outside_repo() {
        // GIVEN: /tmp is not a git repository
        let cwd = std::path::Path::new("/tmp");
        // WHEN
        let result = GitContextCollector::collect(cwd, 200).await;
        // THEN
        assert!(result.branch.is_none(), "no branch outside a git repo");
        assert!(result.status.is_none(), "no status outside a git repo");
    }

    #[tokio::test]
    async fn test_git_status_max_lines_respected() {
        // GIVEN: a repo with at least one modified file (or not, we are testing the limit)
        let cwd = std::env::current_dir().expect("current_dir");
        let max_lines = 2;
        // WHEN
        let result = GitContextCollector::collect(&cwd, max_lines).await;
        // THEN: if any status exists, it does not exceed max_lines lines
        if let Some(status) = &result.status {
            assert!(
                status.lines().count() <= max_lines,
                "status truncated to {max_lines}: got {}",
                status.lines().count()
            );
        }
    }
}
