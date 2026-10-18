//! The synthesis of a batch: once its items have run, the agent works once
//! on the table of their results and writes what the batch was for.
//!
//! A batch went through forty quotes and gave forty answers, one per row;
//! what a user wants from it is the ranking, the summary, the
//! recommendation, which no single item can write. When the launch asked for
//! one, the synthesis runs after the last item, is kept with the batch, and
//! is written to a file beside a source that is a file or a folder.

use std::path::{Path, PathBuf};

use apollia_tools::TaskRepository;
use tracing::Level;

use super::{BatchSynthesis, ItemRunner, SourceDescriptor, SYNTHESIS_KEY};

/// The longest table of results handed to the agent, in characters.
const MAX_RESULTS_CHARS: usize = 80_000;

/// Runs the synthesis `instruction` of `batch_id` on the results of its
/// items, and keeps it with the batch.
pub(super) async fn run_synthesis<R: ItemRunner>(
    repository: &TaskRepository,
    runner: &R,
    batch_id: &str,
    agent: &str,
    instruction: &str,
) {
    record(
        repository,
        batch_id,
        &BatchSynthesis {
            state: "running".into(),
            text: None,
            path: None,
        },
    )
    .await;
    let items = match repository.batch_items(batch_id).await {
        Ok(items) => items,
        Err(e) => {
            tracing::event!(Level::WARN, batch_id, error = %e, "batch.synthesis.read_failed");
            return;
        }
    };
    let mut table = String::new();
    for item in items.iter().filter(|item| item.status == "completed") {
        let line = format!(
            "- {}: {}\n",
            item.origin,
            item.output
                .as_deref()
                .unwrap_or_default()
                .replace('\n', " ")
        );
        if table.len() + line.len() > MAX_RESULTS_CHARS {
            table.push_str("- ... (the other results are left out: the table is too long)\n");
            break;
        }
        table.push_str(&line);
    }
    if table.is_empty() {
        record(
            repository,
            batch_id,
            &BatchSynthesis {
                state: "failed".into(),
                text: Some(
                    "no item of the batch completed, so there is nothing to synthesise".into(),
                ),
                path: None,
            },
        )
        .await;
        return;
    }
    let message = format!(
        "{}\n\nRESULTS OF THE BATCH, one line per item:\n{table}\n\
         Write the synthesis the instruction asks for, from these results, in the language of \
         the instruction.",
        instruction.trim()
    );
    let synthesis = match runner.synthesize(agent, message).await {
        Ok(text) => {
            let path = match repository.batch(batch_id).await {
                Ok(Some(row)) => write_beside_source(&row.source, &row.created_at, &text),
                _ => None,
            };
            tracing::event!(
                Level::INFO,
                batch_id,
                written = path.is_some(),
                "batch.synthesis.completed"
            );
            BatchSynthesis {
                state: "completed".into(),
                text: Some(text),
                path: path.map(|p| p.display().to_string()),
            }
        }
        Err(cause) => {
            tracing::event!(Level::WARN, batch_id, cause = %cause, "batch.synthesis.failed");
            BatchSynthesis {
                state: "failed".into(),
                text: Some(cause),
                path: None,
            }
        }
    };
    record(repository, batch_id, &synthesis).await;
}

/// Keeps `synthesis` in the stored source of `batch_id`.
async fn record(repository: &TaskRepository, batch_id: &str, synthesis: &BatchSynthesis) {
    let Ok(Some(row)) = repository.batch(batch_id).await else {
        return;
    };
    let mut source = row.source.clone();
    if let (Some(map), Ok(value)) = (source.as_object_mut(), serde_json::to_value(synthesis)) {
        map.insert(SYNTHESIS_KEY.to_string(), value);
    }
    if let Err(e) = repository.set_batch_source(batch_id, &source).await {
        tracing::event!(Level::WARN, batch_id, error = %e, "batch.synthesis.record_failed");
    }
}

/// Writes `text` beside a source that is a file or a folder, under a dated
/// name, and returns where; `None` for any other source or a write refused.
fn write_beside_source(
    source: &serde_json::Value,
    created_at: &str,
    text: &str,
) -> Option<PathBuf> {
    let descriptor: SourceDescriptor = serde_json::from_value(source.clone()).ok()?;
    let location = Path::new(&descriptor.location);
    let (dir, stem) = match descriptor.kind.as_str() {
        "folder" => (
            location.parent()?,
            location.file_name()?.to_string_lossy().into_owned(),
        ),
        "spreadsheet" | "csv" => (
            location.parent()?,
            location.file_stem()?.to_string_lossy().into_owned(),
        ),
        _ => return None,
    };
    let day: String = created_at.chars().take(10).collect();
    let path = dir.join(format!("{stem} (synthesis {day}).md"));
    match std::fs::write(&path, text) {
        Ok(()) => Some(path),
        Err(e) => {
            tracing::event!(Level::WARN, path = %path.display(), error = %e, "batch.synthesis.write_failed");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_synthesis_of_a_file_is_written_beside_it() {
        // GIVEN a batch read from a CSV file, and one read from a mailbox
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("quotes.csv");
        let csv = serde_json::json!({"kind": "csv", "location": file.display().to_string()});
        let mail = serde_json::json!({"kind": "mail", "location": "imap:ada@example.org"});

        // WHEN their synthesis is written
        let written = write_beside_source(&csv, "2026-10-18 09:00:00", "Ranking").expect("a file");

        // THEN the file sits beside the source under a dated name, and a
        // mailbox has none
        assert_eq!(written, dir.path().join("quotes (synthesis 2026-10-18).md"));
        assert_eq!(std::fs::read_to_string(&written).expect("read"), "Ranking");
        assert!(write_beside_source(&mail, "2026-10-18 09:00:00", "x").is_none());
    }
}
