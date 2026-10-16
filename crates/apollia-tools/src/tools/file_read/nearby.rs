//! The files of the same name in the folders granted to the agent, said with
//! a path it was not found at.
//!
//! An agent that guesses where a file is (`Devis/conditions.pdf` for a file at
//! the root of the folder) gets "not found" and guesses again. Naming where a
//! file of that name is lets the next attempt, or a new plan, take the right
//! path at once. The search is bounded in depth and in entries visited, skips
//! hidden folders, and never leaves the granted folders.

use std::path::{Path, PathBuf};

/// How deep under a granted folder the search goes.
const MAX_DEPTH: usize = 5;
/// How many entries the search reads before it stops, over every folder.
const MAX_VISITED: usize = 5_000;
/// How many places are named.
const MAX_FOUND: usize = 3;

/// The sentence appended to "not found", empty when no file of that name is
/// in the granted folders. Paths under `anchor` are given relative to it, the
/// way the agent writes them.
pub(super) fn suggestion(anchor: &Path, roots: &[PathBuf], missing: &str) -> String {
    let Some(name) = Path::new(missing).file_name().and_then(|n| n.to_str()) else {
        return String::new();
    };
    let found = find(roots, name);
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

/// The files named `name`, compared without case, under `roots`, breadth
/// first so the shallowest come first.
fn find(roots: &[PathBuf], name: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut visited = 0usize;
    let mut queue: std::collections::VecDeque<(PathBuf, usize)> =
        roots.iter().map(|root| (root.clone(), 0)).collect();
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
                if depth < MAX_DEPTH && !entry_name.starts_with('.') {
                    queue.push_back((entry.path(), depth + 1));
                }
            } else if entry_name.eq_ignore_ascii_case(name) {
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
        let said = suggestion(&root, std::slice::from_ref(&root), "Quotes/conditions.pdf");

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
        let said = suggestion(&root, std::slice::from_ref(&root), "missing.pdf");

        // THEN nothing is appended
        assert!(said.is_empty());
    }
}
