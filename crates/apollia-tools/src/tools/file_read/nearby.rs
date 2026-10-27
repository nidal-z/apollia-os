//! The files of the same name in the folders granted to the agent, said with
//! a path it was not found at.
//!
//! An agent that guesses where a file is (`Devis/conditions.pdf` for a file at
//! the root of the folder) gets "not found" and guesses again. Naming where a
//! file of that name is lets the next attempt, or a new plan, take the right
//! path at once. The search is bounded in depth and in entries visited, skips
//! hidden folders, never leaves the granted folders, and neither enters nor
//! names what the sandbox does not serve: a folder in pause stays unsaid.

use std::path::{Path, PathBuf};

use crate::sandbox_path::SandboxRoot;

/// How deep under a granted folder the search goes.
const MAX_DEPTH: usize = 5;
/// How many entries the search reads before it stops, over every folder.
const MAX_VISITED: usize = 5_000;
/// How many places are named.
const MAX_FOUND: usize = 3;

/// The sentence appended to "not found", empty when no file of that name is
/// in the folders `sandbox` serves. Paths under its anchor are given relative
/// to it, the way the agent writes them.
pub(super) fn suggestion(sandbox: &SandboxRoot, missing: &str) -> String {
    let Some(name) = Path::new(missing).file_name().and_then(|n| n.to_str()) else {
        return String::new();
    };
    let anchor = sandbox.path();
    let found = find(sandbox, name);
    if found.is_empty() {
        return String::new();
    }
    let shown: Vec<String> = found
        .iter()
        .map(|path| match path.strip_prefix(anchor) {
            Ok(relative) => relative.display().to_string(),
            Err(_) => path.display().to_string(),
        })
        .collect();
    format!(
        ". A file of that name is in the folders granted to this agent: {}",
        shown.join(", ")
    )
}

/// The files named `name`, compared without case, under the roots of
/// `sandbox` and served by it, breadth first so the shallowest come first. A
/// link is never followed into a folder; a directory it does not serve is not
/// entered.
fn find(sandbox: &SandboxRoot, name: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut visited = 0usize;
    let mut queue: std::collections::VecDeque<(PathBuf, usize)> = sandbox
        .roots()
        .iter()
        .filter(|root| sandbox.serves(root))
        .map(|root| (root.clone(), 0))
        .collect();
    while let Some((dir, depth)) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited > MAX_VISITED || found.len() >= MAX_FOUND {
                return found;
            }
            let file_name = entry.file_name();
            let Some(entry_name) = file_name.to_str() else {
                continue;
            };
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                let path = entry.path();
                if depth < MAX_DEPTH && !entry_name.starts_with('.') && sandbox.serves(&path) {
                    queue.push_back((path, depth + 1));
                }
            } else if entry_name.eq_ignore_ascii_case(name) && sandbox.serves(&entry.path()) {
                found.push(entry.path());
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_file_of_the_same_name_elsewhere_in_the_folder_is_named() {
        // GIVEN a granted folder holding the file at its root, not in the
        // sub-folder the agent guessed
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().canonicalize().expect("canonical");
        std::fs::create_dir_all(root.join("Quotes")).expect("mkdir");
        std::fs::write(root.join("conditions.pdf"), b"%PDF").expect("write");

        // WHEN the guessed path is not found
        let sandbox = SandboxRoot::new(root.clone()).expect("sandbox");
        let said = suggestion(&sandbox, "Quotes/conditions.pdf");

        // THEN the sentence names where the file is, as the agent writes paths
        assert_eq!(
            said,
            ". A file of that name is in the folders granted to this agent: conditions.pdf"
        );
    }

    #[test]
    fn test_nothing_is_said_when_no_file_has_that_name() {
        // GIVEN a granted folder without the file
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().canonicalize().expect("canonical");
        std::fs::write(root.join("other.pdf"), b"%PDF").expect("write");

        // WHEN a path to an absent file is not found
        let sandbox = SandboxRoot::new(root.clone()).expect("sandbox");
        let said = suggestion(&sandbox, "missing.pdf");

        // THEN nothing is appended
        assert!(said.is_empty());
    }

    #[test]
    fn test_a_file_in_a_folder_in_pause_is_not_named() {
        // GIVEN a granted folder holding a folder in pause, the only one with
        // a file of the name asked for, and a link to it
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().canonicalize().expect("canonical");
        std::fs::create_dir_all(root.join("inner")).expect("mkdir");
        std::fs::write(root.join("inner/n.md"), b"secret").expect("write");
        std::fs::create_dir_all(root.join("other")).expect("mkdir");
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("inner/n.md"), root.join("other/n.md"))
            .expect("symlink");
        let sandbox = SandboxRoot::new(
            crate::SandboxSpec::from(root.clone()).with_paused(vec![root.join("inner")]),
        )
        .expect("sandbox");

        // WHEN a path to that name is not found
        let said = suggestion(&sandbox, "n.md");

        // THEN nothing is named
        assert!(said.is_empty(), "{said}");
    }
}
