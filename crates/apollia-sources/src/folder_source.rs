//! The folder kind of the batch sources: a batch launched on a folder reads
//! one item per file, with its path and its text.
//!
//! A folder of quotes, of contracts, of applications is the most common thing
//! a user wants an agent to go through, file by file. Each item carries the
//! path of its file, relative to the folder, and the text a reader extracts
//! from it (PDF, Word, PowerPoint, workbooks, CSV, plain text), so the agent
//! works on the content without having to be granted the folder. A folder is
//! only read: a batch on a folder writes no file next to its items; its
//! results are exported.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use apollia_core::batch_source::{
    BatchSource, OriginRef, SourceError, SourceItem, SourceItems, SourceRegistry, SourceSpec,
};

use crate::document::{is_readable_document, read_document};

/// The kind of a folder source; its location is the path of the folder.
pub const FOLDER_KIND: &str = "folder";
/// The field of an item that holds the path of its file, relative to the folder.
pub const FILE_FIELD: &str = "File";
/// The field of an item that holds the text of its file.
pub const TEXT_FIELD: &str = "Text";

/// Option: `true` reads the sub-folders too.
pub const OPTION_RECURSIVE: &str = "recursive";

/// How deep the sub-folders are read when asked.
const MAX_DEPTH: usize = 4;
/// The most files one folder batch reads.
const MAX_FILES: usize = 5_000;
/// The longest text an item carries, in characters: past it, the text is cut
/// and the agent told so, rather than a single file filling its context.
const MAX_TEXT_CHARS: usize = 30_000;
/// Extensions read as plain text.
const TEXT_EXTENSIONS: [&str; 6] = ["txt", "md", "json", "xml", "html", "eml"];

/// Registers [`FOLDER_KIND`].
///
/// # Errors
///
/// [`SourceError::DuplicateKind`] when the kind is already registered.
pub fn register_folder_source(registry: &mut SourceRegistry) -> Result<(), SourceError> {
    registry.register(
        FOLDER_KIND,
        Box::new(|spec: &SourceSpec| {
            let root = PathBuf::from(&spec.location);
            if !root.is_dir() {
                return Err(SourceError::Io {
                    path: root,
                    source: std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "a folder source opens a folder of this machine",
                    ),
                });
            }
            let recursive = spec
                .options
                .get(OPTION_RECURSIVE)
                .is_some_and(|value| value == "true");
            Ok(Box::new(FolderSource::open(&root, recursive)) as Box<dyn BatchSource>)
        }),
    )
}

/// The files of one folder, read as batch items.
pub struct FolderSource {
    root: PathBuf,
    files: Vec<PathBuf>,
    fields: Vec<String>,
}

impl FolderSource {
    /// The files of `root`, its sub-folders too when `recursive`, sorted by
    /// path, hidden ones left out.
    #[must_use]
    pub fn open(root: &Path, recursive: bool) -> Self {
        let mut files = Vec::new();
        collect(root, 0, recursive, &mut files);
        files.sort();
        Self {
            root: root.to_path_buf(),
            files,
            fields: vec![FILE_FIELD.to_string(), TEXT_FIELD.to_string()],
        }
    }
}

impl BatchSource for FolderSource {
    fn fields(&self) -> &[String] {
        &self.fields
    }

    fn items(&self) -> Result<SourceItems<'_>, SourceError> {
        Ok(SourceItems {
            estimated_count: u64::try_from(self.files.len()).unwrap_or(u64::MAX),
            items: Box::new(self.files.iter().map(|path| Ok(self.item(path)))),
        })
    }
}

impl FolderSource {
    fn item(&self, path: &Path) -> SourceItem {
        let relative = path
            .strip_prefix(&self.root)
            .unwrap_or(path)
            .display()
            .to_string();
        let mut fields = BTreeMap::new();
        fields.insert(FILE_FIELD.to_string(), relative.clone());
        fields.insert(TEXT_FIELD.to_string(), text_of(path));
        SourceItem {
            origin: OriginRef::new(relative),
            fields,
        }
    }
}

fn collect(dir: &Path, depth: usize, recursive: bool, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if files.len() >= MAX_FILES {
            return;
        }
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if recursive && depth < MAX_DEPTH {
                collect(&entry.path(), depth + 1, recursive, files);
            }
        } else if kind.is_file() {
            files.push(entry.path());
        }
    }
}

/// The text of the file at `path`, cut at [`MAX_TEXT_CHARS`]; a sentence
/// saying why when it has none a reader extracts.
fn text_of(path: &Path) -> String {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let text = if is_readable_document(path) {
        match read_document(path) {
            Ok(document) => document.full_text(),
            Err(e) => return format!("(the text of this file could not be read: {e})"),
        }
    } else if TEXT_EXTENSIONS.contains(&extension.as_str()) {
        match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) => return format!("(this file could not be read: {e})"),
        }
    } else {
        return "(no text is read from a file of this kind)".to_string();
    };
    if text.chars().count() <= MAX_TEXT_CHARS {
        return text;
    }
    let cut: String = text.chars().take(MAX_TEXT_CHARS).collect();
    format!("{cut}\n(the text goes on past {MAX_TEXT_CHARS} characters and was cut here)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_folder_reads_one_item_per_file_with_its_text() {
        // GIVEN a folder with two text files, a hidden one and a sub-folder
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("b.txt"), "second").expect("write");
        std::fs::write(dir.path().join("a.txt"), "first").expect("write");
        std::fs::write(dir.path().join(".hidden"), "x").expect("write");
        std::fs::create_dir_all(dir.path().join("sub")).expect("mkdir");
        std::fs::write(dir.path().join("sub/c.txt"), "third").expect("write");

        // WHEN it is read without its sub-folders
        let source = FolderSource::open(dir.path(), false);
        let items: Vec<SourceItem> = source
            .items()
            .expect("items")
            .items
            .collect::<Result<_, _>>()
            .expect("read");

        // THEN each visible file is an item, in order, with its path and text
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].fields[FILE_FIELD], "a.txt");
        assert_eq!(items[0].fields[TEXT_FIELD], "first");
        assert_eq!(items[1].origin.as_str(), "b.txt");
    }

    #[test]
    fn test_sub_folders_are_read_when_asked() {
        // GIVEN a folder whose only file is in a sub-folder
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("Quotes")).expect("mkdir");
        std::fs::write(dir.path().join("Quotes/q.txt"), "quote").expect("write");

        // WHEN it is read with its sub-folders
        let source = FolderSource::open(dir.path(), true);

        // THEN the file is an item named by its path in the folder
        let items: Vec<SourceItem> = source
            .items()
            .expect("items")
            .items
            .collect::<Result<_, _>>()
            .expect("read");
        assert_eq!(items[0].fields[FILE_FIELD], "Quotes/q.txt");
    }
}
