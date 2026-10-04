//! Writing the answers of a batch back into its source: into a copy by
//! default, into the original on request after its backup and its grant,
//! and never into a source that cannot write.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use apollia_core::batch_source::{
    BatchSource, MemorySource, OriginRef, SourceError, SourceItems, SourceWriter, WriteGuard,
};
use apollia_core::RuntimeEvent;
use chrono::{DateTime, Utc};

use super::runner::{decide_original_write, journal_original_write};
use super::tests::{repository, spawn, wait_state, Scripted, WAIT};
use super::*;
use crate::governance::{
    governance_run_id, AccessCall, AccessDecision, DeclaredAccess, EffectiveAccess,
    GovernanceContract, GovernanceError, GovernanceEvent, InstallSubject, NoPolicyGovernance,
};
use crate::test_support::collect_events_until;

const LOCATION: &str = "/data/requests.xlsx";
const ANSWER: &str = r#"{"category": "billing", "urgency": "high"}"#;

/// What the source in memory saw: the destination of each opening, its
/// backups, and its writes as `(origin, column, value)`.
#[derive(Default)]
struct Seen {
    destinations: Vec<Option<String>>,
    backups: Vec<PathBuf>,
    writes: Vec<(String, String, String)>,
}

/// A source in memory that reports what is done to it.
struct Recorded {
    inner: MemorySource,
    seen: Arc<Mutex<Seen>>,
}

impl BatchSource for Recorded {
    fn fields(&self) -> &[String] {
        self.inner.fields()
    }

    fn items(&self) -> Result<SourceItems<'_>, SourceError> {
        self.inner.items()
    }

    fn writer(&mut self) -> Option<&mut dyn SourceWriter> {
        if self.inner.writer().is_some() {
            Some(self)
        } else {
            None
        }
    }
}

impl SourceWriter for Recorded {
    fn back_up(&mut self, taken_at: DateTime<Utc>) -> Result<PathBuf, SourceError> {
        let path = SourceWriter::back_up(&mut self.inner, taken_at)?;
        self.seen.lock().unwrap().backups.push(path.clone());
        Ok(path)
    }

    fn write_value(
        &mut self,
        guard: &WriteGuard,
        origin: &OriginRef,
        column: &str,
        value: &str,
    ) -> Result<(), SourceError> {
        self.inner.write_value(guard, origin, column, value)?;
        self.seen.lock().unwrap().writes.push((
            origin.to_string(),
            column.to_owned(),
            value.to_owned(),
        ));
        Ok(())
    }
}

/// How the source in memory behaves.
#[derive(Clone, Copy, Default)]
struct Behaviour {
    read_only: bool,
    failing_backup: bool,
}

/// A registry with one `memory` kind of two items that succeed with
/// [`ANSWER`], and one that fails.
fn recorded_registry(behaviour: Behaviour, seen: Arc<Mutex<Seen>>) -> Arc<SourceRegistry> {
    let mut registry = SourceRegistry::new();
    registry
        .register(
            "memory",
            Box::new(move |spec| {
                seen.lock()
                    .unwrap()
                    .destinations
                    .push(spec.options.get(write_back::OPTION_DESTINATION).cloned());
                let mut inner = MemorySource::new(&spec.location, &["outcome", "answer"])
                    .with_item("row-0", &[("outcome", "ok"), ("answer", ANSWER)])
                    .with_item("row-1", &[("outcome", "fail"), ("answer", ANSWER)])
                    .with_item("row-2", &[("outcome", "ok"), ("answer", "plain words")]);
                if behaviour.read_only {
                    inner = inner.read_only();
                }
                if behaviour.failing_backup {
                    inner = inner.with_failing_backup();
                }
                Ok(Box::new(Recorded {
                    inner,
                    seen: seen.clone(),
                }) as Box<dyn BatchSource>)
            }),
        )
        .unwrap();
    Arc::new(registry)
}

fn writing_launch(destination: Option<WriteDestination>, grants: &[&str]) -> BatchLaunch {
    BatchLaunch {
        agent: "clerk".into(),
        source: SourceDescriptor {
            kind: "memory".into(),
            location: LOCATION.into(),
            options: BTreeMap::new(),
            write: Some(SourceWrite {
                destination: destination.unwrap_or_default(),
                columns: vec!["category".into(), "urgency".into()],
            }),
        },
        grants: grants.iter().map(|g| (*g).to_owned()).collect(),
        concurrency: None,
        max_failures: None,
    }
}

/// Waits for the end of `batch_id` to be announced, which comes after its
/// answers are written.
async fn wait_end(bus: &mut tokio::sync::broadcast::Receiver<RuntimeEvent>, batch_id: &str) {
    let (seen, ended) = collect_events_until(
        WAIT,
        bus,
        |e| matches!(e, RuntimeEvent::BatchEnded { batch_id: id, .. } if id == batch_id),
    )
    .await;
    assert!(ended, "the end of {batch_id} was never announced: {seen:?}");
}

fn expected_writes() -> Vec<(String, String, String)> {
    vec![
        ("row-0".into(), "category".into(), "billing".into()),
        ("row-0".into(), "urgency".into(), "high".into()),
        ("row-2".into(), "category".into(), "plain words".into()),
    ]
}

// GIVEN a launch that asks for the answers in two columns, naming no destination
// WHEN the batch runs to its end
// THEN the source is opened on a copy, one backup step is taken, and only the
//      completed items fill their columns; nothing is granted or journaled
#[tokio::test]
async fn test_the_answers_go_into_a_copy_by_default() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let seen = Arc::new(Mutex::new(Seen::default()));
    let runner = Arc::new(Scripted::default());
    let (handle, bus) = spawn(
        repo.clone(),
        recorded_registry(Behaviour::default(), seen.clone()),
        runner.clone(),
    );
    let mut events = bus.subscribe();

    // WHEN
    let batch = handle.launch(writing_launch(None, &[])).await.unwrap();
    wait_end(&mut events, &batch.batch_id).await;

    // THEN
    let seen = seen.lock().unwrap();
    assert_eq!(
        seen.destinations,
        vec![Some("copy".to_string()), Some("copy".to_string())]
    );
    assert_eq!(seen.backups.len(), 1);
    assert_eq!(seen.writes, expected_writes());
    assert!(runner.original_writes.lock().unwrap().is_empty());
    assert!(batch.grants.is_empty());
}

// GIVEN a launch that asks for the original and grants writing into it
// WHEN the batch runs to its end
// THEN governance journals the grant before anything runs, the source is
//      opened on the original, its dated backup is taken, then the answers
//      are written
#[tokio::test]
async fn test_on_request_the_original_is_written_after_its_backup() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let seen = Arc::new(Mutex::new(Seen::default()));
    let runner = Arc::new(Scripted::default());
    let (handle, bus) = spawn(
        repo.clone(),
        recorded_registry(Behaviour::default(), seen.clone()),
        runner.clone(),
    );
    let mut events = bus.subscribe();

    // WHEN
    let batch = handle
        .launch(writing_launch(
            Some(WriteDestination::Original),
            &[SOURCE_WRITE_ACCESS],
        ))
        .await
        .unwrap();
    wait_end(&mut events, &batch.batch_id).await;

    // THEN
    assert_eq!(batch.grants, vec![SOURCE_WRITE_ACCESS.to_string()]);
    assert_eq!(
        *runner.original_writes.lock().unwrap(),
        vec![(batch.batch_id.clone(), LOCATION.to_string())]
    );
    let seen = seen.lock().unwrap();
    assert_eq!(
        seen.destinations,
        vec![Some("original".to_string()), Some("original".to_string())]
    );
    assert_eq!(seen.backups.len(), 1);
    let backup = seen.backups[0].file_name().unwrap().to_string_lossy();
    assert!(backup.starts_with("requests.backup-"), "{backup}");
    assert_eq!(seen.writes, expected_writes());
}

// GIVEN a source whose backup fails, asked for the original with its grant
// WHEN the batch runs to its end
// THEN its end is announced and not a single answer is written
#[tokio::test]
async fn test_without_a_successful_backup_nothing_is_written() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let seen = Arc::new(Mutex::new(Seen::default()));
    let behaviour = Behaviour {
        failing_backup: true,
        ..Behaviour::default()
    };
    let (handle, bus) = spawn(
        repo.clone(),
        recorded_registry(behaviour, seen.clone()),
        Arc::new(Scripted::default()),
    );
    let mut events = bus.subscribe();

    // WHEN
    let batch = handle
        .launch(writing_launch(
            Some(WriteDestination::Original),
            &[SOURCE_WRITE_ACCESS],
        ))
        .await
        .unwrap();
    wait_end(&mut events, &batch.batch_id).await;

    // THEN
    let done = wait_state(&repo, &batch.batch_id, "completed").await;
    assert_eq!((done.completed, done.failed), (2, 1));
    let seen = seen.lock().unwrap();
    assert!(seen.backups.is_empty());
    assert!(seen.writes.is_empty());
}

// GIVEN a launch on the original without the grant, and one granting it for a copy
// WHEN they are launched
// THEN both are refused before anything is recorded or journaled
#[tokio::test]
async fn test_the_original_without_its_grant_is_refused_at_launch() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let seen = Arc::new(Mutex::new(Seen::default()));
    let runner = Arc::new(Scripted::default());
    let (handle, _bus) = spawn(
        repo.clone(),
        recorded_registry(Behaviour::default(), seen.clone()),
        runner.clone(),
    );

    // WHEN
    let ungranted = handle
        .launch(writing_launch(Some(WriteDestination::Original), &[]))
        .await;
    let granted_for_a_copy = handle
        .launch(writing_launch(None, &[SOURCE_WRITE_ACCESS]))
        .await;

    // THEN
    assert!(
        matches!(&ungranted, Err(BatchError::Invalid(m)) if m.contains("access to grant")),
        "{ungranted:?}"
    );
    assert!(
        matches!(&granted_for_a_copy, Err(BatchError::Invalid(m)) if m.contains("only when")),
        "{granted_for_a_copy:?}"
    );
    assert!(handle.list(10).await.unwrap().is_empty());
    assert!(runner.original_writes.lock().unwrap().is_empty());
    assert!(seen.lock().unwrap().destinations.is_empty());
}

// GIVEN governance that refuses writing into the original
// WHEN a launch asks for the original with its grant
// THEN the launch is refused with the reason, and nothing is recorded
#[tokio::test]
async fn test_a_refusal_of_governance_refuses_the_original_at_launch() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let seen = Arc::new(Mutex::new(Seen::default()));
    let runner = Arc::new(Scripted {
        deny_original_write: true,
        ..Scripted::default()
    });
    let (handle, _bus) = spawn(
        repo.clone(),
        recorded_registry(Behaviour::default(), seen.clone()),
        runner.clone(),
    );

    // WHEN
    let refused = handle
        .launch(writing_launch(
            Some(WriteDestination::Original),
            &[SOURCE_WRITE_ACCESS],
        ))
        .await;

    // THEN
    assert!(
        matches!(&refused, Err(BatchError::Invalid(m)) if m.contains("refused by the policy")),
        "{refused:?}"
    );
    assert!(handle.list(10).await.unwrap().is_empty());
    assert!(runner.original_writes.lock().unwrap().is_empty());
}

// GIVEN a source that cannot write
// WHEN a batch is launched on it with and without the write option
// THEN the option is refused as unavailable, and the batch without it runs
#[tokio::test]
async fn test_a_source_that_cannot_write_makes_the_option_unavailable() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let seen = Arc::new(Mutex::new(Seen::default()));
    let behaviour = Behaviour {
        read_only: true,
        ..Behaviour::default()
    };
    let (handle, _bus) = spawn(
        repo.clone(),
        recorded_registry(behaviour, seen.clone()),
        Arc::new(Scripted::default()),
    );

    // WHEN
    let with_write = handle.launch(writing_launch(None, &[])).await;
    let mut plain = writing_launch(None, &[]);
    plain.source.write = None;
    let without_write = handle.launch(plain).await;

    // THEN
    assert!(
        matches!(&with_write, Err(BatchError::Invalid(m)) if m.contains("cannot be written")),
        "{with_write:?}"
    );
    let batch = without_write.unwrap();
    assert_eq!(handle.list(10).await.unwrap().len(), 1);
    wait_state(&repo, &batch.batch_id, "completed").await;
    assert!(seen.lock().unwrap().writes.is_empty());
}

// GIVEN columns that are missing, blank, or named twice
// WHEN a batch is launched with them
// THEN each launch is refused
#[tokio::test]
async fn test_the_columns_of_results_are_checked_at_launch() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let seen = Arc::new(Mutex::new(Seen::default()));
    let (handle, _bus) = spawn(
        repo.clone(),
        recorded_registry(Behaviour::default(), seen),
        Arc::new(Scripted::default()),
    );
    let with_columns = |columns: &[&str]| {
        let mut launch = writing_launch(None, &[]);
        if let Some(write) = launch.source.write.as_mut() {
            write.columns = columns.iter().map(|c| (*c).to_owned()).collect();
        }
        launch
    };

    // WHEN
    let none = handle.launch(with_columns(&[])).await;
    let blank = handle.launch(with_columns(&["category", " "])).await;
    let twice = handle.launch(with_columns(&["Category", "category"])).await;

    // THEN
    for refused in [none, blank, twice] {
        assert!(
            matches!(refused, Err(BatchError::Invalid(_))),
            "{refused:?}"
        );
    }
    assert!(handle.list(10).await.unwrap().is_empty());
}

// GIVEN answers as a JSON object, a fenced object, plain text, and an object
//       naming none of the columns
// WHEN their cells are computed for two columns
// THEN an object fills the columns it names, anything else goes whole in the first
#[test]
fn test_an_answer_fills_one_column_per_field_it_names() {
    // GIVEN
    let columns = vec!["category".to_string(), "urgency".to_string()];
    let fenced = "```json\n{\"Category\": \"billing\", \"urgency\": 3}\n```";

    // WHEN
    let object = answer_cells(&columns, ANSWER);
    let in_fence = answer_cells(&columns, fenced);
    let plain = answer_cells(&columns, "  plain words \n");
    let unrelated = answer_cells(&columns, r#"{"other": "x"}"#);

    // THEN
    let cells = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(c, v)| ((*c).to_owned(), (*v).to_owned()))
            .collect()
    };
    assert_eq!(
        object,
        cells(&[("category", "billing"), ("urgency", "high")])
    );
    assert_eq!(
        in_fence,
        cells(&[("category", "billing"), ("urgency", "3")])
    );
    assert_eq!(plain, cells(&[("category", "plain words")]));
    assert_eq!(unrelated, cells(&[("category", r#"{"other": "x"}"#)]));
}

/// Governance that refuses every use of an access.
struct Refusing;

impl GovernanceContract for Refusing {
    fn effective_accesses(&self, _: &str, _: &[DeclaredAccess]) -> Vec<EffectiveAccess> {
        Vec::new()
    }

    fn decide(&self, call: &AccessCall<'_>) -> AccessDecision {
        AccessDecision::DeniedByPolicy {
            reason: format!("{} is not allowed here", call.access_id),
        }
    }

    fn signature_required(&self, _: &InstallSubject<'_>) -> bool {
        false
    }

    fn record(&self, _: &GovernanceEvent) -> Result<(), GovernanceError> {
        Ok(())
    }
}

// GIVEN the default governance over a signed journal, and a refusing policy
// WHEN writing into the original is decided and its grant journaled
// THEN the default allows it and the journal of the agent holds the grant,
//      scoped to the batch; the policy refuses with its reason; no
//      governance at all refuses
#[tokio::test]
async fn test_writing_into_the_original_is_decided_by_governance_and_journaled() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.db");
    let journal = crate::audit_journal::AuditJournalHandle::open_with_key_bytes(
        &path,
        b"batch-write-journal-key-01".to_vec(),
    )
    .await
    .unwrap();
    let default = NoPolicyGovernance::new(journal.clone());

    // WHEN
    let allowed = decide_original_write(Some(&default), "clerk", LOCATION);
    let journaled = journal_original_write(Some(&default), "b-1", "clerk", LOCATION);
    let refused = decide_original_write(Some(&Refusing), "clerk", LOCATION);
    let without = decide_original_write(None, "clerk", LOCATION);

    // THEN
    assert_eq!(allowed, Ok(()));
    assert_eq!(journaled, Ok(()));
    assert!(
        matches!(&refused, Err(m) if m.contains("source:write is not allowed here")),
        "{refused:?}"
    );
    assert!(without.is_err());
    let entries = journal.query_run(&governance_run_id("clerk")).await;
    assert_eq!(entries.len(), 1);
    let payload = &entries[0].payload;
    assert_eq!(payload["access_id"], SOURCE_WRITE_ACCESS);
    assert_eq!(payload["change"], "granted");
    let reason = payload["reason"].as_str().unwrap();
    assert!(
        reason.contains("lot:b-1") && reason.contains(LOCATION),
        "{reason}"
    );
    journal.shutdown().await;
}

fn write_csv(path: &Path) {
    std::fs::write(path, "text,answer\nhello,plain one\nworld,plain two\n").unwrap();
}

fn write_xlsx(path: &Path) {
    let mut workbook = rust_xlsxwriter::Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.write_string(0, 0, "text").unwrap();
    sheet.write_string(0, 1, "answer").unwrap();
    sheet.write_string(1, 0, "hello").unwrap();
    sheet.write_string(1, 1, ANSWER).unwrap();
    sheet.write_string(2, 0, "world").unwrap();
    sheet
        .write_string(2, 1, r#"{"category": "support", "urgency": "low"}"#)
        .unwrap();
    workbook.save(path).unwrap();
}

fn table_registry() -> Arc<SourceRegistry> {
    let mut registry = SourceRegistry::new();
    apollia_sources::register_table_sources(&mut registry).unwrap();
    Arc::new(registry)
}

fn table_launch(kind: &str, file: &Path, write: SourceWrite, grants: &[&str]) -> BatchLaunch {
    BatchLaunch {
        agent: "clerk".into(),
        source: SourceDescriptor {
            kind: kind.into(),
            location: file.display().to_string(),
            options: BTreeMap::new(),
            write: Some(write),
        },
        grants: grants.iter().map(|g| (*g).to_owned()).collect(),
        concurrency: None,
        max_failures: None,
    }
}

/// The files of `dir` other than `except`, by name.
fn others(dir: &Path, except: &[&Path]) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file() && !except.contains(&p.as_path()))
        .collect();
    found.sort();
    found
}

// GIVEN a CSV file read by the table source, with plain-text answers
// WHEN a batch writes its answers with no destination named
// THEN a copy holds the added column, and the original keeps its bytes
#[tokio::test]
async fn test_on_a_csv_file_the_answers_go_into_a_copy() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let files = tempfile::tempdir().unwrap();
    let original = files.path().join("requests.csv");
    write_csv(&original);
    let before = std::fs::read(&original).unwrap();
    let (handle, bus) = spawn(
        repo.clone(),
        table_registry(),
        Arc::new(Scripted::default()),
    );
    let mut events = bus.subscribe();
    let write = SourceWrite {
        destination: WriteDestination::Copy,
        columns: vec!["summary".into()],
    };

    // WHEN
    let batch = handle
        .launch(table_launch(
            apollia_sources::CSV_KIND,
            &original,
            write,
            &[],
        ))
        .await
        .unwrap();
    wait_end(&mut events, &batch.batch_id).await;

    // THEN
    assert_eq!(std::fs::read(&original).unwrap(), before);
    let copies = others(files.path(), &[&original]);
    assert_eq!(copies.len(), 1, "{copies:?}");
    let copy = std::fs::read_to_string(&copies[0]).unwrap();
    assert!(copy.starts_with("text,answer,summary"), "{copy}");
    assert!(
        copy.contains("plain one") && copy.contains("plain two"),
        "{copy}"
    );
    assert_eq!(copy.lines().count(), 3, "{copy}");
}

// GIVEN an .xlsx workbook read by the table source, with answers of two fields
// WHEN a batch writes them into the original, with its grant
// THEN a dated backup holds the original bytes, and the original holds one
//      column per field
#[tokio::test]
async fn test_on_a_workbook_the_original_is_written_after_a_dated_backup() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let files = tempfile::tempdir().unwrap();
    let original = files.path().join("requests.xlsx");
    write_xlsx(&original);
    let before = std::fs::read(&original).unwrap();
    let (handle, bus) = spawn(
        repo.clone(),
        table_registry(),
        Arc::new(Scripted::default()),
    );
    let mut events = bus.subscribe();
    let write = SourceWrite {
        destination: WriteDestination::Original,
        columns: vec!["category".into(), "urgency".into()],
    };

    // WHEN
    let batch = handle
        .launch(table_launch(
            apollia_sources::SPREADSHEET_KIND,
            &original,
            write,
            &[SOURCE_WRITE_ACCESS],
        ))
        .await
        .unwrap();
    wait_end(&mut events, &batch.batch_id).await;

    // THEN
    let backups = others(files.path(), &[&original]);
    assert_eq!(backups.len(), 1, "{backups:?}");
    let name = backups[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert!(name.starts_with("requests.backup-"), "{name}");
    assert_eq!(std::fs::read(&backups[0]).unwrap(), before);
    let reread = table_registry()
        .open(
            apollia_sources::SPREADSHEET_KIND,
            &apollia_core::batch_source::SourceSpec {
                location: original.display().to_string(),
                options: BTreeMap::new(),
            },
        )
        .unwrap();
    let rows: Vec<(String, String)> = reread
        .items()
        .unwrap()
        .items
        .map(|item| {
            let item = item.unwrap();
            (
                item.fields.get("category").cloned().unwrap_or_default(),
                item.fields.get("urgency").cloned().unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            ("billing".to_string(), "high".to_string()),
            ("support".to_string(), "low".to_string()),
        ]
    );
}
