//! Tauri IPC commands behind the demonstration dataset of an example.
//!
//! An example the application proposes may come with a dataset to try it on:
//! a workbook to run a batch on, and a small folder of reference documents.
//! The dataset is not part of the package. This binary carries it, and copies
//! it into a folder the user chooses, on request and never before.
//!
//! `copy_example_dataset` writes the dataset into the chosen folder. A file
//! already there is left as it is.
//!
//! The reference documents are embedded as they are. The workbook is built at
//! copy time from an embedded table of tab-separated text, so what it holds
//! can be read in the sources.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// The dataset of one example, as this binary carries it.
struct Dataset {
    /// Package name of the example it comes with.
    example: &'static str,
    /// File name of the workbook, at the root of the copy.
    workbook: &'static str,
    /// Name of the one sheet of the workbook.
    sheet: &'static str,
    /// The table of the workbook: one line per row, cells separated by tabs,
    /// the header row first.
    table: &'static str,
    /// Name of the folder of reference documents, at the root of the copy.
    folder: &'static str,
    /// The files copied as they are: path inside the copy with `/`
    /// separators, and content.
    files: &'static [(&'static str, &'static str)],
}

/// The datasets this binary carries.
const DATASETS: &[Dataset] = &[Dataset {
    example: "qualify-requests",
    workbook: "demandes.xlsx",
    sheet: "Demandes",
    table: include_str!("../../example-datasets/qualify-requests/demandes.tsv"),
    folder: "referentiel",
    files: &[
        (
            "LISEZ-MOI.md",
            include_str!("../../example-datasets/qualify-requests/LISEZ-MOI.md"),
        ),
        (
            "referentiel/01-reglement-du-fonds.md",
            include_str!(
                "../../example-datasets/qualify-requests/referentiel/01-reglement-du-fonds.md"
            ),
        ),
        (
            "referentiel/02-pieces-justificatives.md",
            include_str!(
                "../../example-datasets/qualify-requests/referentiel/02-pieces-justificatives.md"
            ),
        ),
        (
            "referentiel/03-grille-de-qualification.md",
            include_str!(
                "../../example-datasets/qualify-requests/referentiel/03-grille-de-qualification.md"
            ),
        ),
    ],
}];

/// Why a dataset could not be copied.
#[derive(Debug, thiserror::Error)]
enum DatasetError {
    /// No dataset comes with that example.
    #[error("the example `{example}` has no demonstration dataset")]
    UnknownExample { example: String },
    /// The destination is not an absolute path.
    #[error("the destination `{path}` is not an absolute path")]
    DestinationNotAbsolute { path: PathBuf },
    /// A folder or a file could not be written.
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The workbook could not be built.
    #[error("the workbook could not be built: {0}")]
    Workbook(#[from] rust_xlsxwriter::XlsxError),
}

/// What a copy wrote, and where.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExampleDatasetCopy {
    /// The folder the dataset was copied into.
    destination: String,
    /// Absolute path of the workbook.
    workbook: String,
    /// Absolute path of the folder of reference documents.
    folder: String,
    /// How many rows the workbook holds under its header row.
    rows: usize,
    /// The files that were already there and were left as they are, by their
    /// path inside the copy.
    kept: Vec<String>,
}

fn find(example: &str) -> Result<&'static Dataset, DatasetError> {
    DATASETS
        .iter()
        .find(|dataset| dataset.example == example)
        .ok_or_else(|| DatasetError::UnknownExample {
            example: example.to_owned(),
        })
}

impl Dataset {
    /// The rows of the table, the header row first. A blank line is no row.
    fn table_rows(&self) -> Vec<Vec<&'static str>> {
        self.table
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.split('\t').collect())
            .collect()
    }

    /// How many rows the workbook holds under its header row.
    fn row_count(&self) -> usize {
        self.table_rows().len().saturating_sub(1)
    }

    /// The table as an Excel workbook of one sheet. A cell that reads as a
    /// whole number is written as a number, every other cell as text.
    fn workbook_bytes(&self) -> Result<Vec<u8>, DatasetError> {
        let mut workbook = rust_xlsxwriter::Workbook::new();
        let sheet = workbook.add_worksheet();
        sheet.set_name(self.sheet)?;
        let bold = rust_xlsxwriter::Format::new().set_bold();
        for (r, row) in self.table_rows().into_iter().enumerate() {
            let r =
                u32::try_from(r).map_err(|_| rust_xlsxwriter::XlsxError::RowColumnLimitError)?;
            for (col, cell) in row.into_iter().enumerate() {
                let col = u16::try_from(col)
                    .map_err(|_| rust_xlsxwriter::XlsxError::RowColumnLimitError)?;
                if r == 0 {
                    sheet.write_string_with_format(r, col, cell, &bold)?;
                } else if let Ok(number) = cell.parse::<u32>() {
                    sheet.write_number(r, col, f64::from(number))?;
                } else if !cell.is_empty() {
                    sheet.write_string(r, col, cell)?;
                }
            }
        }
        Ok(workbook.save_to_buffer()?)
    }

    /// Writes the dataset into `destination`, created when it does not exist.
    /// A file already there is never replaced: it is left as it is and named
    /// in the answer.
    fn copy_into(&self, destination: &Path) -> Result<ExampleDatasetCopy, DatasetError> {
        if !destination.is_absolute() {
            return Err(DatasetError::DestinationNotAbsolute {
                path: destination.to_path_buf(),
            });
        }
        let folder = destination.join(self.folder);
        create_dir(&folder)?;
        let mut kept = Vec::new();
        for (relative, content) in self.files {
            let path = relative
                .split('/')
                .fold(destination.to_path_buf(), |path, part| path.join(part));
            if let Some(parent) = path.parent() {
                create_dir(parent)?;
            }
            if !write_new(&path, content.as_bytes())? {
                kept.push((*relative).to_owned());
            }
        }
        let workbook = destination.join(self.workbook);
        if workbook.exists() || !write_new(&workbook, &self.workbook_bytes()?)? {
            kept.push(self.workbook.to_owned());
        }
        Ok(ExampleDatasetCopy {
            destination: destination.display().to_string(),
            workbook: workbook.display().to_string(),
            folder: folder.display().to_string(),
            rows: self.row_count(),
            kept,
        })
    }
}

fn create_dir(path: &Path) -> Result<(), DatasetError> {
    std::fs::create_dir_all(path).map_err(|source| DatasetError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Writes `content` at `path` when nothing is there. Answers whether the file
/// was written: `false` means a file was already there and was not touched.
fn write_new(path: &Path, content: &[u8]) -> Result<bool, DatasetError> {
    let io = |source| DatasetError::Io {
        path: path.to_path_buf(),
        source,
    };
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => file.write_all(content).map(|()| true).map_err(io),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(io(error)),
    }
}

/// Copies the dataset of the example `example` into `destination`, a folder
/// the user chose. A file already there is left as it is.
#[tauri::command]
pub async fn copy_example_dataset(
    example: String,
    destination: String,
) -> Result<ExampleDatasetCopy, String> {
    let dataset = find(&example).map_err(|error| error.to_string())?;
    let copy = tokio::task::spawn_blocking(move || dataset.copy_into(Path::new(&destination)))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
    tracing::event!(
        tracing::Level::INFO,
        example = %example,
        kept = copy.kept.len(),
        "example.dataset.copied"
    );
    Ok(copy)
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUALIFY: &str = "qualify-requests";

    fn files_under(root: &Path) -> Vec<String> {
        fn walk(dir: &Path, root: &Path, found: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, root, found);
                } else {
                    let relative = path.strip_prefix(root).unwrap();
                    let parts: Vec<_> = relative
                        .components()
                        .map(|part| part.as_os_str().to_string_lossy().into_owned())
                        .collect();
                    found.push(parts.join("/"));
                }
            }
        }
        let mut found = Vec::new();
        walk(root, root, &mut found);
        found.sort();
        found
    }

    #[test]
    fn test_a_copy_writes_the_workbook_and_the_reference_folder_where_chosen() {
        // GIVEN a folder the user chose, which does not exist yet
        let home = tempfile::tempdir().unwrap();
        let destination = home.path().join("chosen").join("example");

        // WHEN the dataset is copied into it
        let copy = find(QUALIFY).unwrap().copy_into(&destination).unwrap();

        // THEN the folder holds the workbook, the note and the three reference
        // documents, nothing was kept, and nothing landed outside it
        assert_eq!(
            files_under(home.path()),
            vec![
                "chosen/example/LISEZ-MOI.md",
                "chosen/example/demandes.xlsx",
                "chosen/example/referentiel/01-reglement-du-fonds.md",
                "chosen/example/referentiel/02-pieces-justificatives.md",
                "chosen/example/referentiel/03-grille-de-qualification.md",
            ]
        );
        assert!(copy.kept.is_empty());
        assert_eq!(copy.rows, 12);
        assert_eq!(Path::new(&copy.workbook), destination.join("demandes.xlsx"));
        assert_eq!(Path::new(&copy.folder), destination.join("referentiel"));
    }

    #[test]
    fn test_the_copied_workbook_reads_as_one_batch_item_per_request() {
        // GIVEN the dataset copied into a folder
        let home = tempfile::tempdir().unwrap();
        let copy = find(QUALIFY).unwrap().copy_into(home.path()).unwrap();

        // WHEN the workbook is previewed the way a batch launch previews it
        let preview =
            apollia_sources::preview_table(Path::new(&copy.workbook), None, None).unwrap();

        // THEN its first row is proposed as the header, with the eight
        // columns of a request under their names, and a batch would read
        // twelve items
        let header = find(QUALIFY).unwrap().table_rows().remove(0);
        let names: Vec<_> = preview.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(preview.sheet.as_deref(), Some("Demandes"));
        assert_eq!(preview.header_row, Some(1));
        assert_eq!(names.len(), 8);
        assert_eq!(names, header);
        assert_eq!(preview.estimated_items, 12);
    }

    #[test]
    fn test_every_row_of_the_table_has_the_cells_of_the_header() {
        // GIVEN the table the workbook is built from
        let rows = find(QUALIFY).unwrap().table_rows();

        // WHEN its rows are measured against the header row
        let width = rows[0].len();

        // THEN each has that many cells, none of them blank, and each request
        // carries its own reference
        assert!(rows
            .iter()
            .all(|row| row.len() == width && row.iter().all(|cell| !cell.trim().is_empty())));
        let mut references: Vec<_> = rows[1..].iter().map(|row| row[0]).collect();
        references.sort_unstable();
        references.dedup();
        assert_eq!(references.len(), rows.len() - 1);
    }

    #[test]
    fn test_a_second_copy_replaces_no_file_already_there() {
        // GIVEN a copy the user then edited, one reference document removed
        let home = tempfile::tempdir().unwrap();
        let dataset = find(QUALIFY).unwrap();
        dataset.copy_into(home.path()).unwrap();
        let note = home.path().join("LISEZ-MOI.md");
        std::fs::write(&note, "mine").unwrap();
        let workbook = home.path().join("demandes.xlsx");
        std::fs::write(&workbook, "edited").unwrap();
        let removed = home
            .path()
            .join("referentiel")
            .join("02-pieces-justificatives.md");
        std::fs::remove_file(&removed).unwrap();

        // WHEN the dataset is copied there again
        let copy = dataset.copy_into(home.path()).unwrap();

        // THEN the edited files are untouched and named as kept, and only the
        // missing document is written back
        assert_eq!(std::fs::read_to_string(&note).unwrap(), "mine");
        assert_eq!(std::fs::read_to_string(&workbook).unwrap(), "edited");
        assert!(removed.is_file());
        assert_eq!(
            copy.kept,
            vec![
                "LISEZ-MOI.md",
                "referentiel/01-reglement-du-fonds.md",
                "referentiel/03-grille-de-qualification.md",
                "demandes.xlsx",
            ]
        );
    }

    #[test]
    fn test_a_relative_destination_is_refused_and_writes_nothing() {
        // GIVEN a destination that is not an absolute path
        let destination = Path::new("somewhere/relative");

        // WHEN the dataset is copied into it
        let refused = find(QUALIFY).unwrap().copy_into(destination);

        // THEN the copy is refused before any write
        assert!(matches!(
            refused,
            Err(DatasetError::DestinationNotAbsolute { .. })
        ));
        assert!(!destination.exists());
    }

    #[test]
    fn test_an_example_without_a_dataset_is_refused() {
        // GIVEN an example that comes with no dataset
        // WHEN its dataset is looked up
        let refused = find("ghost");

        // THEN it is refused by name
        assert_eq!(
            refused.err().map(|error| error.to_string()).as_deref(),
            Some("the example `ghost` has no demonstration dataset")
        );
    }
}
