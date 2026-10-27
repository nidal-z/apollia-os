//! The files of the folders granted to the agent, listed for the planner.
//!
//! A planner that does not see the folder guesses its paths: a quote named
//! without its sub-folder, the purchase conditions put in the sub-folder of
//! the quotes. Each guess failed a step, and each failed step cost a new plan,
//! about a minute of a batch item. The planner is now given the files it can
//! reach, with the path to write for each, bounded so a large folder does not
//! fill its prompt. A folder in pause nested in a granted one is not entered:
//! the planner is not told the name of a file the tools would refuse.

use std::path::{Path, PathBuf};

use apollia_tools::sandbox_path::real_path;

/// How deep under a granted folder the listing goes.
const MAX_DEPTH: usize = 4;
/// How many files are named, over every folder.
const MAX_FILES: usize = 200;

/// The note given to the planner, `None` when no folder is granted or none
/// holds a file. Paths under the first folder are given relative to it, as
/// the file tools resolve them; paths under another folder are absolute.
pub(super) fn planning_note(folders: &apollia_tools::FolderRoots) -> Option<String> {
    let anchor = folders.anchor()?;
    let paused: Vec<PathBuf> = folders
        .paused
        .iter()
        .filter_map(|folder| real_path(folder))
        .collect();
    let mut walk = Walk {
        paused: &paused,
        files: Vec::new(),
        more: 0,
    };
    for folder in &folders.read {
        walk.dir(folder, 0);
    }
    let Walk { files, more, .. } = walk;
    if files.is_empty() {
        return None;
    }
    let lines: Vec<String> = files
        .iter()
        .map(|file| match file.strip_prefix(anchor) {
            Ok(relative) => format!("- {}", relative.display()),
            Err(_) => format!("- {}", file.display()),
        })
        .collect();
    let mut note = format!(
        "FILES THE AGENT CAN REACH (write these paths exactly; do not guess another):\n{}",
        lines.join("\n")
    );
    if more > 0 {
        note.push_str(&format!(
            "\n- ... and {more} more files: list or search a folder to find one not named here"
        ));
    }
    Some(note)
}

/// A listing in progress: the folders in pause it does not enter, where they
/// really are, and what it found.
struct Walk<'a> {
    paused: &'a [PathBuf],
    files: Vec<PathBuf>,
    more: usize,
}

impl Walk<'_> {
    fn dir(&mut self, dir: &Path, depth: usize) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if depth < MAX_DEPTH && !self.in_pause(&entry.path()) {
                    self.dir(&entry.path(), depth + 1);
                }
            } else if self.files.len() < MAX_FILES {
                self.files.push(entry.path());
            } else {
                self.more += 1;
            }
        }
    }

    fn in_pause(&self, dir: &Path) -> bool {
        !self.paused.is_empty()
            && real_path(dir).is_some_and(|real| self.paused.iter().any(|p| real.starts_with(p)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_planner_is_told_the_exact_paths_of_the_files() {
        // GIVEN a granted folder with a file at its root and one in a sub-folder
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join("Quotes")).expect("mkdir");
        std::fs::write(root.join("conditions.pdf"), b"%PDF").expect("write");
        std::fs::write(root.join("Quotes/quote-01.pdf"), b"%PDF").expect("write");
        std::fs::write(root.join(".hidden"), b"x").expect("write");

        // WHEN the note is written
        let note = planning_note(&apollia_tools::FolderRoots {
            read: vec![root],
            ..Default::default()
        })
        .expect("a note");

        // THEN each file is named by the path the tools resolve, hidden ones left out
        assert!(note.contains("- conditions.pdf"), "{note}");
        assert!(note.contains("- Quotes/quote-01.pdf"), "{note}");
        assert!(!note.contains(".hidden"), "{note}");
    }

    #[test]
    fn test_the_planner_is_not_told_the_files_of_a_nested_folder_in_pause() {
        // GIVEN a granted folder holding a folder in pause
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join("inner")).expect("mkdir");
        std::fs::write(root.join("o.md"), b"x").expect("write");
        std::fs::write(root.join("inner/secret.md"), b"x").expect("write");

        // WHEN the note is written
        let note = planning_note(&apollia_tools::FolderRoots {
            read: vec![root.clone()],
            paused: vec![root.join("inner")],
            ..Default::default()
        })
        .expect("a note");

        // THEN the file of the sound folder is named, none of the folder in pause
        assert!(note.contains("- o.md"), "{note}");
        assert!(!note.contains("secret.md"), "{note}");
    }

    #[test]
    fn test_no_folder_gives_no_note() {
        // GIVEN no granted folder
        // WHEN the note is asked for
        // THEN there is none
        assert!(planning_note(&apollia_tools::FolderRoots::default()).is_none());
    }
}
