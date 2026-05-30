//! `apollia workspace`: inspect and initialise the current workspace.
//!
//! Two subcommands:
//! - `status`: shows the git branch, modified files, presence of APOLLIA.md, and a file count.
//! - `init`:   creates APOLLIA.md from the standard template (fails if the file exists, unless `--force`).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use serde::Serialize;
use thiserror::Error;

use apollia_workspace::git::GitContextCollector;

use crate::exit_codes;

/// Template written by `apollia workspace init`.
const APOLLIA_MD_TEMPLATE: &str = "# APOLLIA.md — Instructions for AI agents\n\
\n\
## Project context\n\
<!-- Describe your project here: goal, tech stack, constraints. -->\n\
\n\
## Rules for the agents\n\
<!-- Coding conventions, style, commit rules, etc. -->\n\
\n\
## Important files\n\
<!-- List the key files/directories the agent should know about. -->\n\
\n\
## Useful commands\n\
<!-- Examples: `cargo test`, `make build`, `npm run dev`. -->\n";

/// `apollia workspace` subcommand with two actions.
#[derive(Debug, clap::Subcommand)]
pub enum WorkspaceCommand {
    /// Show the status of the current workspace.
    Status,
    /// Initialise APOLLIA.md in the current directory.
    Init {
        /// Overwrite APOLLIA.md if it already exists.
        #[arg(long, default_value_t = false)]
        force: bool,
    },
}

/// Errors emitted by the `workspace` subcommand.
#[derive(Debug, Error)]
pub enum WorkspaceCliError {
    /// APOLLIA.md already exists and `--force` was not supplied.
    #[error("APOLLIA.md already exists. Use --force to overwrite.")]
    FileExists,
    /// Filesystem read/write error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Workspace status data, serialisable to JSON.
#[derive(Debug, Serialize)]
struct WorkspaceStatusOutput {
    workspace_root: String,
    git_branch: Option<String>,
    modified_files: Vec<String>,
    apollia_md_found: bool,
    file_count: usize,
}

/// Entry point of the `apollia workspace` subcommand.
pub async fn run(cmd: &WorkspaceCommand, json: bool) -> i32 {
    match cmd {
        WorkspaceCommand::Status => run_workspace_status(json).await,
        WorkspaceCommand::Init { force } => run_workspace_init(*force, json).await,
    }
}

/// Displays the status of the current workspace (git branch, modified files, APOLLIA.md, count).
async fn run_workspace_status(json: bool) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: unable to read current directory: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    let status = collect_workspace_status(&cwd).await;

    if json {
        match serde_json::to_string_pretty(&status) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("Error: JSON serialisation: {e}");
                return exit_codes::GENERAL_ERROR;
            }
        }
    } else {
        print_workspace_status(&status);
    }

    exit_codes::SUCCESS
}

/// Initialises APOLLIA.md in the current directory.
async fn run_workspace_init(force: bool, json: bool) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: unable to read current directory: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    match init_apollia_md(&cwd, force).await {
        Ok(path) => {
            if json {
                let output = serde_json::json!({
                    "path": path.display().to_string(),
                    "created": true,
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                );
            } else {
                println!("APOLLIA.md created: {}", path.display());
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: {e}");
            exit_codes::GENERAL_ERROR
        }
    }
}

/// Collects the workspace status data for `cwd`.
///
/// Orchestrates the git collection, file count, and APOLLIA.md detection.
async fn collect_workspace_status(cwd: &Path) -> WorkspaceStatusOutput {
    let git_result = GitContextCollector::collect(cwd, 200).await;
    let modified_files = parse_modified_files(git_result.status.as_deref());
    let apollia_md_found = cwd.join("APOLLIA.md").exists();
    let file_count = count_files(cwd).await;

    WorkspaceStatusOutput {
        workspace_root: cwd.display().to_string(),
        git_branch: git_result.branch,
        modified_files,
        apollia_md_found,
        file_count,
    }
}

/// Writes the APOLLIA.md template into `cwd`.
///
/// Returns `Err(WorkspaceCliError::FileExists)` if the file already exists and `force` is `false`.
async fn init_apollia_md(cwd: &Path, force: bool) -> Result<PathBuf, WorkspaceCliError> {
    let target = cwd.join("APOLLIA.md");
    if target.exists() && !force {
        return Err(WorkspaceCliError::FileExists);
    }
    tokio::fs::write(&target, APOLLIA_MD_TEMPLATE).await?;
    Ok(target)
}

/// Displays the workspace status in human-readable form.
fn print_workspace_status(s: &WorkspaceStatusOutput) {
    println!("Workspace : {}", s.workspace_root);
    match &s.git_branch {
        Some(branch) => println!("Branch git: {branch}"),
        None => println!("Branch git: (not a git repository)"),
    }
    if s.modified_files.is_empty() {
        println!("Modified files: (none)");
    } else {
        println!("Modified files:");
        for f in &s.modified_files {
            println!("  {f}");
        }
    }
    println!(
        "APOLLIA.md: {}",
        if s.apollia_md_found {
            "found"
        } else {
            "missing"
        }
    );
    println!("Files: {}", s.file_count);
}

/// Extracts the list of modified paths from `git status --short` output.
///
/// Expected per-line format: `XY PATH` (X = index status, Y = working tree status, PATH = path).
fn parse_modified_files(status_output: Option<&str>) -> Vec<String> {
    let Some(output) = status_output else {
        return Vec::new();
    };
    output
        .lines()
        .filter_map(|line| {
            if line.len() < 3 {
                return None;
            }
            let path = line[3..].trim();
            if path.is_empty() {
                None
            } else {
                Some(path.to_owned())
            }
        })
        .collect()
}

/// Counts files recursively under `root`, ignoring irrelevant directories.
///
/// Ignores: `.git`, `target`, `node_modules`, `__pycache__`, `dist`, `.next`, `.DS_Store`.
async fn count_files(root: &Path) -> usize {
    let mut count: usize = 0;
    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    queue.push_back(root.to_owned());

    while let Some(dir) = queue.pop_front() {
        let mut rd = match tokio::fs::read_dir(&dir).await {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = rd.next_entry().await {
            let name = entry.file_name();
            if should_ignore_for_count(&name.to_string_lossy()) {
                continue;
            }
            let is_dir = entry
                .file_type()
                .await
                .map(|ft| ft.is_dir())
                .unwrap_or(false);
            if is_dir {
                queue.push_back(entry.path());
            } else {
                count += 1;
            }
        }
    }
    count
}

/// Returns `true` if the entry name should be ignored in the file count.
fn should_ignore_for_count(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | "node_modules" | "__pycache__" | ".DS_Store" | ".next" | "dist"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn workspace_status_shows_git_branch() {
        // GIVEN the current apollia repository (a valid git repo)
        let cwd = std::env::current_dir().expect("current_dir");
        // WHEN we collect the status
        let status = collect_workspace_status(&cwd).await;
        // THEN the git branch is detected
        assert!(
            status.git_branch.is_some(),
            "doit détecter la branche dans un dépôt git"
        );
    }

    #[tokio::test]
    async fn workspace_init_creates_apollia_md() {
        // GIVEN a temporary directory without APOLLIA.md
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(!dir.path().join("APOLLIA.md").exists());
        // WHEN init without --force in an empty directory
        let result = init_apollia_md(dir.path(), false).await;
        // THEN the file is created successfully
        assert!(
            result.is_ok(),
            "init doit réussir si le fichier n'existe pas"
        );
        assert!(
            dir.path().join("APOLLIA.md").exists(),
            "APOLLIA.md doit être créé"
        );
    }

    #[tokio::test]
    async fn workspace_init_fails_if_exists_without_force() {
        // GIVEN an existing APOLLIA.md
        let dir = tempfile::tempdir().expect("tempdir");
        tokio::fs::write(dir.path().join("APOLLIA.md"), "existing")
            .await
            .expect("write");
        // WHEN init without --force
        let result = init_apollia_md(dir.path(), false).await;
        // THEN Err(FileExists)
        assert!(
            matches!(result, Err(WorkspaceCliError::FileExists)),
            "doit retourner FileExists sans --force"
        );
    }

    #[tokio::test]
    async fn workspace_init_force_overwrites() {
        // GIVEN an existing APOLLIA.md with arbitrary content
        let dir = tempfile::tempdir().expect("tempdir");
        tokio::fs::write(dir.path().join("APOLLIA.md"), "ancien contenu")
            .await
            .expect("write");
        // WHEN init --force
        let result = init_apollia_md(dir.path(), true).await;
        // THEN success, with the standard template content
        assert!(result.is_ok(), "init --force doit réussir");
        let content = tokio::fs::read_to_string(dir.path().join("APOLLIA.md"))
            .await
            .expect("read");
        assert!(
            content.contains("APOLLIA.md"),
            "le template doit être présent après --force : {content}"
        );
        assert!(
            !content.contains("ancien contenu"),
            "l'ancien contenu doit avoir été effacé"
        );
    }

    #[tokio::test]
    async fn workspace_status_json_output() {
        // GIVEN the current apollia repository
        let cwd = std::env::current_dir().expect("current_dir");
        // WHEN we serialise the status to JSON
        let status = collect_workspace_status(&cwd).await;
        let json_str = serde_json::to_string_pretty(&status).expect("serialize");
        // THEN the JSON is valid and contains all the required fields
        let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("parse");
        assert!(
            parsed["workspace_root"].is_string(),
            "workspace_root manquant"
        );
        assert!(
            parsed["git_branch"].is_string() || parsed["git_branch"].is_null(),
            "git_branch doit être string ou null"
        );
        assert!(
            parsed["modified_files"].is_array(),
            "modified_files doit être un tableau"
        );
        assert!(
            parsed["apollia_md_found"].is_boolean(),
            "apollia_md_found doit être boolean"
        );
        assert!(
            parsed["file_count"].is_number(),
            "file_count doit être un nombre"
        );
    }

    #[test]
    fn parse_modified_files_extracts_paths() {
        // GIVEN git status --short output with three entries
        let status = " M src/main.rs\n?? Cargo.toml\n M crates/foo/bar.rs";
        // WHEN
        let files = parse_modified_files(Some(status));
        // THEN three paths are extracted
        assert_eq!(files.len(), 3, "doit extraire 3 fichiers");
        assert!(files.contains(&"src/main.rs".to_owned()));
        assert!(files.contains(&"Cargo.toml".to_owned()));
        assert!(files.contains(&"crates/foo/bar.rs".to_owned()));
    }

    #[test]
    fn parse_modified_files_returns_empty_for_none() {
        // GIVEN no git status (outside a repository)
        // WHEN / THEN
        assert!(
            parse_modified_files(None).is_empty(),
            "None doit retourner une liste vide"
        );
    }

    #[tokio::test]
    async fn count_files_ignores_git_and_target() {
        // GIVEN a directory with .git/, target/, and src/main.rs
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
        tokio::fs::write(dir.path().join("src").join("main.rs"), "fn main(){}")
            .await
            .expect("write main.rs");
        tokio::fs::write(dir.path().join(".git").join("config"), "git config")
            .await
            .expect("write git config");
        tokio::fs::write(dir.path().join("target").join("out"), "binary")
            .await
            .expect("write target out");
        // WHEN
        let count = count_files(dir.path()).await;
        // THEN only src/main.rs is counted
        assert_eq!(
            count, 1,
            ".git et target doivent être ignorés : got {count}"
        );
    }
}
