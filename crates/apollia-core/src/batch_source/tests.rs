use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, Utc};

use crate::batch_source::{
    dated_backup_path, BatchSource, MemorySource, OriginRef, SourceError, SourceRegistry,
    SourceSpec, WriteGuard, WriteSession,
};

fn at_noon() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 1, 12, 0, 5).unwrap()
}

fn two_requests() -> MemorySource {
    MemorySource::new("/data/requests.xlsx", &["client", "request"])
        .with_item("Sheet1!2", &[("client", "Acme"), ("request", "refund")])
        .with_item("Sheet1!3", &[("client", "Globex"), ("request", "invoice")])
}

#[test]
fn memory_source_yields_fields_items_and_estimated_count() {
    // GIVEN a source of two requests
    let source = two_requests();

    // WHEN its fields and items are read
    let fields = source.fields().to_vec();
    let items = source.items().unwrap();
    let estimated = items.estimated_count;
    let read: Vec<_> = items.items.collect::<Result<_, _>>().unwrap();

    // THEN every item carries its origin and its fields, in source order
    assert_eq!(fields, vec!["client".to_owned(), "request".to_owned()]);
    assert_eq!(estimated, 2);
    assert_eq!(read.len(), 2);
    assert_eq!(read[0].origin, OriginRef::new("Sheet1!2"));
    assert_eq!(
        read[0].fields.get("client").map(String::as_str),
        Some("Acme")
    );
    assert_eq!(read[1].origin.as_str(), "Sheet1!3");
    assert_eq!(
        read[1].fields.get("request").map(String::as_str),
        Some("invoice")
    );
}

#[test]
fn write_session_takes_a_dated_backup_then_writes_the_column() {
    // GIVEN a writable source
    let mut source = two_requests();

    // WHEN a session is opened and a result is written into a new column
    let backup_path = {
        let mut session = WriteSession::open(&mut source, at_noon()).unwrap();
        session
            .write(&OriginRef::new("Sheet1!3"), "answer", "sent")
            .unwrap();
        assert_eq!(session.backup().taken_at(), at_noon());
        session.backup().path().to_path_buf()
    };

    // THEN the backup sits next to the original under a dated name, it was
    // taken before the write, and the value reads back
    assert_eq!(
        backup_path,
        PathBuf::from("/data/requests.backup-20261001T120005Z.xlsx")
    );
    assert_eq!(source.backups(), &[backup_path]);
    assert_eq!(source.value("Sheet1!3", "answer"), Some("sent"));
    assert_eq!(source.value("Sheet1!2", "answer"), None);
    assert!(source.fields().iter().any(|f| f == "answer"));
}

#[test]
fn write_refused_when_backup_fails() {
    // GIVEN a writable source whose backup cannot be taken
    let mut source = two_requests().with_failing_backup();

    // WHEN a write session is opened
    let opened = WriteSession::open(&mut source, at_noon());

    // THEN the session is refused, so nothing can be written
    assert!(matches!(opened, Err(SourceError::BackupFailed { .. })));
    assert!(source.backups().is_empty());
    assert_eq!(source.value("Sheet1!2", "answer"), None);
}

#[test]
fn write_refused_on_read_only_source() {
    // GIVEN a read-only source
    let mut source = two_requests().read_only();

    // WHEN a write session is opened
    let opened = WriteSession::open(&mut source, at_noon());

    // THEN it is refused and no backup is taken
    assert!(matches!(opened, Err(SourceError::NotWritable)));
    assert!(source.backups().is_empty());
}

#[test]
fn memory_source_refuses_a_guard_issued_for_another_source() {
    // GIVEN a backup taken on source A, and a source B never backed up. A
    // guard cannot be obtained outside a session; this module builds one
    // around A's backup to stand for a writer that forwards the guard it
    // was handed.
    let mut a = two_requests();
    let backup_of_a = WriteSession::open(&mut a, at_noon())
        .unwrap()
        .backup()
        .clone();
    let foreign = WriteGuard {
        backup: backup_of_a.clone(),
    };
    let mut b = MemorySource::new("/data/other.xlsx", &["client"])
        .with_item("Sheet1!2", &[("client", "Initech")]);

    // WHEN B is asked to write under A's guard
    let written = b.writer().unwrap().write_value(
        &foreign,
        &OriginRef::new("Sheet1!2"),
        "client",
        "overwritten",
    );

    // THEN B refuses, names the foreign backup, and keeps its value
    assert!(matches!(
        written,
        Err(SourceError::ForeignBackup { path }) if path == backup_of_a.path()
    ));
    assert!(b.backups().is_empty());
    assert_eq!(b.value("Sheet1!2", "client"), Some("Initech"));
}

#[test]
fn write_to_unknown_origin_is_an_error() {
    // GIVEN an open write session
    let mut source = two_requests();
    let mut session = WriteSession::open(&mut source, at_noon()).unwrap();

    // WHEN a value is written to an origin the source does not hold
    let written = session.write(&OriginRef::new("Sheet1!99"), "answer", "sent");

    // THEN the write is refused with the origin named
    assert!(matches!(
        written,
        Err(SourceError::UnknownOrigin { origin }) if origin.as_str() == "Sheet1!99"
    ));
}

#[test]
fn dated_backup_path_keeps_directory_and_extension() {
    // GIVEN files with and without an extension, and a path with no file name
    let with_ext = Path::new("/data/report.csv");
    let without_ext = Path::new("/data/notes");
    let root = Path::new("/");

    // WHEN their backup paths are computed
    let a = dated_backup_path(with_ext, at_noon()).unwrap();
    let b = dated_backup_path(without_ext, at_noon()).unwrap();
    let c = dated_backup_path(root, at_noon());

    // THEN the copy is a sibling carrying the timestamp, and a location with
    // no file name is refused
    assert_eq!(a, PathBuf::from("/data/report.backup-20261001T120005Z.csv"));
    assert_eq!(b, PathBuf::from("/data/notes.backup-20261001T120005Z"));
    assert!(matches!(c, Err(SourceError::InvalidLocation { .. })));
}

#[test]
fn new_registry_has_no_reader() {
    // GIVEN a fresh registry
    let registry = SourceRegistry::new();

    // WHEN its kinds are listed and an unknown kind is opened
    let kinds = registry.kinds();
    let opened = registry.open("spreadsheet", &SourceSpec::default());

    // THEN no kind is available
    assert!(kinds.is_empty());
    assert!(matches!(opened, Err(SourceError::UnknownKind { kind }) if kind == "spreadsheet"));
}

#[test]
fn registry_opens_a_registered_kind_and_refuses_a_duplicate() {
    // GIVEN a registry holding the in-memory kind
    let mut registry = SourceRegistry::new();
    registry
        .register(
            "memory",
            Box::new(|spec: &SourceSpec| {
                let source = MemorySource::new(spec.location.clone(), &["text"])
                    .with_item("1", &[("text", "hello")]);
                Ok(Box::new(source) as Box<dyn BatchSource>)
            }),
        )
        .unwrap();

    // WHEN the kind is opened, then registered a second time
    let spec = SourceSpec {
        location: "/data/inbox.txt".to_owned(),
        ..SourceSpec::default()
    };
    let mut opened = registry.open("memory", &spec).unwrap();
    let duplicate = registry.register(
        "memory",
        Box::new(|_: &SourceSpec| Err(SourceError::NotWritable)),
    );

    // THEN the opened source reads and writes through the contract, and the
    // duplicate is refused
    assert_eq!(registry.kinds(), vec!["memory"]);
    assert_eq!(opened.items().unwrap().estimated_count, 1);
    let mut session = WriteSession::open(opened.as_mut(), at_noon()).unwrap();
    session.write(&OriginRef::new("1"), "text", "bye").unwrap();
    assert_eq!(
        session.backup().path(),
        Path::new("/data/inbox.backup-20261001T120005Z.txt")
    );
    assert!(matches!(duplicate, Err(SourceError::DuplicateKind { kind }) if kind == "memory"));
}
