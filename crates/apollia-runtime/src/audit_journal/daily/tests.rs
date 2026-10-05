use chrono::NaiveDate;
use serde_json::{json, Value};

use super::*;
use crate::audit_journal::{JournalEntryDraft, JournalEntryKind};

fn day(text: &str) -> NaiveDate {
    text.parse().expect("day")
}

/// A journal with one tool call on the 1st, two on the 2nd and one on the 4th.
async fn journal(dir: &tempfile::TempDir) -> AuditJournalHandle {
    let journal = AuditJournalHandle::open(&dir.path().join("audit_journal.db"))
        .await
        .expect("open journal");
    for (i, ts) in [
        "2026-10-01T08:00:00Z",
        "2026-10-02T08:00:00Z",
        "2026-10-02T23:59:59Z",
        "2026-10-04T10:00:00Z",
    ]
    .into_iter()
    .enumerate()
    {
        journal.append(JournalEntryDraft {
            run_id: "run-1".to_owned(),
            ts: ts.to_owned(),
            kind: JournalEntryKind::ToolCallCompleted,
            payload: json!({
                "tool_name": "file_write", "agent_id": "quotes", "family": "write_or_delete",
                "target": "/home/u/a.md", "success": true, "content": format!("SECRET-{i}"),
            }),
        });
    }
    journal
}

fn read(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).expect("read file")).expect("json")
}

#[tokio::test]
async fn test_each_day_passed_gets_its_file_with_the_summary_and_the_anchor() {
    // GIVEN a journal with entries on the 1st, the 2nd and the 4th
    let dir = tempfile::tempdir().unwrap();
    let journal = journal(&dir).await;
    let out = dir.path().join("exports");

    // WHEN the export runs on the 4th
    let written = export_missing_days(&journal, &out, day("2026-10-04"))
        .await
        .expect("export");

    // THEN the 1st and the 2nd have their file, the 3rd has none for lack of
    // entries, and the 4th waits for its day to end
    let days: Vec<(String, u64)> = written
        .iter()
        .map(|file| (file.day.to_string(), file.entries))
        .collect();
    assert_eq!(
        days,
        [("2026-10-01".to_owned(), 1), ("2026-10-02".to_owned(), 2)]
    );
    let mut names: Vec<String> = std::fs::read_dir(&out)
        .unwrap()
        .map(|file| file.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["journal-2026-10-01.json", "journal-2026-10-02.json"]
    );

    // AND a file is a summary that carries the head anchor of the chain,
    // never the content of an entry
    let anchor = journal.journal_anchor().await.unwrap().expect("anchor");
    let second = read(&out.join("journal-2026-10-02.json"));
    assert_eq!(second["day"], "2026-10-02");
    assert_eq!(second["detail"], false);
    assert_eq!(second["count"], 2);
    assert_eq!(second["anchor"]["global_seq"], anchor.global_seq);
    assert_eq!(second["anchor"]["global_hash"], anchor.global_hash);
    assert_eq!(second["entries"][0]["ts"], "2026-10-02T23:59:59Z");
    assert_eq!(second["entries"][0]["agent"], "quotes");
    assert_eq!(second["entries"][0]["target"], "/home/u/a.md");
    assert!(second["entries"][0]["hash"].is_string());
    assert!(!second.to_string().contains("SECRET"), "{second}");
    journal.shutdown().await;
}

#[tokio::test]
async fn test_a_later_run_catches_up_from_the_last_file_and_rewrites_none() {
    // GIVEN the files of the 1st and the 2nd, the first one marked by hand
    let dir = tempfile::tempdir().unwrap();
    let journal = journal(&dir).await;
    let out = dir.path().join("exports");
    export_missing_days(&journal, &out, day("2026-10-04"))
        .await
        .expect("export");
    std::fs::write(out.join("journal-2026-10-01.json"), b"kept").unwrap();

    // WHEN the runtime starts again three days later
    let written = export_missing_days(&journal, &out, day("2026-10-07"))
        .await
        .expect("export");

    // THEN only the 4th is written, and the file of the 1st is untouched
    assert_eq!(written.len(), 1);
    assert_eq!(written[0].day, day("2026-10-04"));
    assert_eq!(read(&written[0].path)["count"], 1);
    assert_eq!(
        std::fs::read(out.join("journal-2026-10-01.json")).unwrap(),
        b"kept"
    );

    // AND a run the same day has nothing left to write
    let again = export_missing_days(&journal, &out, day("2026-10-07"))
        .await
        .expect("export");
    assert!(again.is_empty());
    journal.shutdown().await;
}

#[tokio::test]
async fn test_a_folder_that_cannot_be_used_is_an_error_that_names_it() {
    // GIVEN a journal, and a file where the folder should be
    let dir = tempfile::tempdir().unwrap();
    let journal = journal(&dir).await;
    let blocked = dir.path().join("not-a-folder");
    std::fs::write(&blocked, b"a file").unwrap();

    // WHEN the export is asked there, then in a relative folder
    let unusable = export_missing_days(&journal, &blocked, day("2026-10-04")).await;
    let relative = export_missing_days(&journal, Path::new("exports"), day("2026-10-04")).await;

    // THEN each one is refused with its reason
    assert!(
        matches!(&unusable, Err(DailyExportError::Directory { directory, .. })
            if directory == &blocked.display().to_string()),
        "{unusable:?}"
    );
    assert!(
        matches!(relative, Err(DailyExportError::NotAbsolute(_))),
        "{relative:?}"
    );
    journal.shutdown().await;
}
