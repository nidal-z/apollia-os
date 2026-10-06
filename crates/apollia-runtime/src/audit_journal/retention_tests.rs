//! Retention: what a purge removes, what its entry carries, and what
//! verification still catches afterwards.

use chrono::{DateTime, Utc};
use serde_json::json;

use crate::audit_journal::{
    AuditJournalHandle, JournalBreakReason, JournalEntryDraft, JournalEntryKind,
};
use crate::governance::{GovernanceEvent, GovernanceRecord, PurgeAnchor, JOURNAL_STATE_RUN_ID};

const KEY: &[u8] = b"retention-journal-key-0015";

fn at(instant: &str) -> DateTime<Utc> {
    instant.parse().expect("instant")
}

async fn open(dir: &tempfile::TempDir) -> AuditJournalHandle {
    AuditJournalHandle::open_with_key_bytes(&dir.path().join("audit_journal.db"), KEY.to_vec())
        .await
        .expect("open signed journal")
}

fn append(journal: &AuditJournalHandle, run_id: &str, ts: &str, i: u64) {
    journal.append(JournalEntryDraft {
        run_id: run_id.to_owned(),
        ts: ts.to_owned(),
        kind: JournalEntryKind::ToolCallStarted,
        payload: json!({ "i": i }),
    });
}

/// A journal with three entries of January (two of `old`, one of `long`) and
/// three of June (one of `long`, two of `recent`): global positions 0 to 5.
async fn two_seasons(dir: &tempfile::TempDir) -> AuditJournalHandle {
    let journal = open(dir).await;
    append(&journal, "old", "2026-01-10T08:00:00Z", 0);
    append(&journal, "old", "2026-01-10T08:00:01Z", 1);
    append(&journal, "long", "2026-01-11T08:00:00Z", 2);
    append(&journal, "long", "2026-06-01T08:00:00Z", 3);
    append(&journal, "recent", "2026-06-02T08:00:00Z", 4);
    append(&journal, "recent", "2026-06-02T08:00:01Z", 5);
    journal
}

/// Runs one statement on the file as someone with access to it would, the
/// append-only triggers dropped.
fn tamper(dir: &tempfile::TempDir, sql: &str) {
    let conn = rusqlite::Connection::open(dir.path().join("audit_journal.db")).expect("reopen raw");
    conn.execute_batch(&format!(
        "DROP TRIGGER IF EXISTS aje_no_update; DROP TRIGGER IF EXISTS aje_no_delete; {sql}"
    ))
    .expect("forced tamper");
}

/// The purge event the journal recorded last.
async fn last_purge(journal: &AuditJournalHandle) -> GovernanceEvent {
    let entry = journal
        .query_run(JOURNAL_STATE_RUN_ID)
        .await
        .into_iter()
        .rev()
        .find(|entry| entry.kind == JournalEntryKind::JournalPurged)
        .expect("a purge entry");
    assert!(entry.signature.is_some(), "the purge entry is signed");
    serde_json::from_value::<GovernanceRecord>(entry.payload)
        .expect("a governance record")
        .event
}

#[tokio::test]
async fn test_nothing_old_enough_removes_nothing_and_writes_nothing() {
    // GIVEN a journal whose oldest entry is 170 days old
    let dir = tempfile::tempdir().unwrap();
    let journal = two_seasons(&dir).await;

    // WHEN the retention is 365 days
    let outcome = journal
        .purge_by_retention(365, at("2026-06-30T00:00:00Z"))
        .await
        .expect("purge");

    // THEN nothing leaves, no purge entry is written, and the journal is
    // verified from its origin
    assert_eq!(outcome, None);
    assert!(journal.query_run(JOURNAL_STATE_RUN_ID).await.is_empty());
    let report = journal.verify_journal().await.expect("verify");
    assert!(report.ok, "{report:?}");
    assert_eq!(report.entries_checked, 6);
    assert_eq!(report.start, None);
    journal.shutdown().await;
}

#[tokio::test]
async fn test_purge_removes_the_old_entries_after_a_signed_entry_that_carries_their_anchor() {
    // GIVEN a journal with three entries of January and three of June
    let dir = tempfile::tempdir().unwrap();
    let journal = two_seasons(&dir).await;
    let january = journal.query_run("old").await;
    let long_before = journal.query_run("long").await;
    let removed_head = journal.query_page(10, 0).await;
    assert_eq!(removed_head.len(), 6);

    // WHEN the retention is 90 days on the last day of June
    let outcome = journal
        .purge_by_retention(90, at("2026-06-30T00:00:00Z"))
        .await
        .expect("purge")
        .expect("entries were old enough");

    // THEN the three entries of January are gone and the others are kept
    assert_eq!(outcome.removed, 3);
    assert_eq!(outcome.cutoff, "2026-04-01T00:00:00Z");
    assert!(journal.query_run("old").await.is_empty());
    assert_eq!(journal.query_run("long").await.len(), 1);
    assert_eq!(journal.query_run("recent").await.len(), 2);

    // AND the purge entry says what left: how many, the last global link
    // removed, and for the run that keeps entries the last one it lost
    let GovernanceEvent::JournalPurged {
        retention_days,
        cutoff,
        removed,
        anchor,
        runs,
    } = last_purge(&journal).await
    else {
        panic!("not a purge event");
    };
    assert_eq!((retention_days, removed), (90, 3));
    assert_eq!(cutoff, "2026-04-01T00:00:00Z");
    let anchor: PurgeAnchor = anchor.expect("an anchor");
    assert_eq!(anchor.global_seq, 2);
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs["long"].seq, 0);
    assert_eq!(runs["long"].hash, long_before[0].hash);
    assert_eq!(january.len(), 2);

    // AND verification starts from there and stays green, run by run too
    let report = journal.verify_journal().await.expect("verify");
    assert!(report.ok, "{report:?}");
    assert_eq!(report.entries_checked, 4);
    let start = report.start.expect("a start");
    assert_eq!(start.global_seq, 3);
    assert_eq!(start.ts, "2026-06-01T08:00:00Z");
    assert_eq!(start.purged_at, "2026-06-30T00:00:00Z");
    assert!(journal.verify_chain("long").await.expect("verify").ok);
    assert!(journal.verify_chain("recent").await.expect("verify").ok);
    journal.shutdown().await;

    // AND the journal still refuses a deletion that is not a purge
    let conn = rusqlite::Connection::open(dir.path().join("audit_journal.db")).expect("reopen raw");
    let refused = conn.execute("DELETE FROM audit_journal_entries", []);
    assert!(refused.is_err(), "the append-only trigger is back");
}

#[tokio::test]
async fn test_an_entry_altered_before_or_after_the_purge_entry_is_detected() {
    for (case, sql, global_seq) in [
        (
            "before the purge entry",
            "UPDATE audit_journal_entries SET payload = '{\"i\":666}' WHERE global_seq = 4;",
            4,
        ),
        (
            "after the purge entry",
            "UPDATE audit_journal_entries SET payload = '{\"i\":666}' WHERE global_seq = 7;",
            7,
        ),
    ] {
        // GIVEN a purged journal that received an entry after its purge
        let dir = tempfile::tempdir().unwrap();
        let journal = two_seasons(&dir).await;
        journal
            .purge_by_retention(90, at("2026-06-30T00:00:00Z"))
            .await
            .expect("purge");
        append(&journal, "recent", "2026-06-30T09:00:00Z", 7);
        assert!(journal.verify_journal().await.expect("verify").ok, "{case}");
        journal.shutdown().await;

        // WHEN one entry kept is rewritten on disk
        tamper(&dir, sql);

        // THEN verification names it
        let journal = open(&dir).await;
        let report = journal.verify_journal().await.expect("verify");
        assert!(!report.ok, "{case}: {report:?}");
        let broken = report.first_break.expect("a break");
        assert_eq!(broken.global_seq, global_seq, "{case}");
        assert_eq!(broken.reason, JournalBreakReason::PerRunBroken, "{case}");
        journal.shutdown().await;
    }
}

#[tokio::test]
async fn test_a_beginning_cut_further_than_the_purge_says_is_detected() {
    // GIVEN a purged journal
    let dir = tempfile::tempdir().unwrap();
    let journal = two_seasons(&dir).await;
    journal
        .purge_by_retention(90, at("2026-06-30T00:00:00Z"))
        .await
        .expect("purge");
    journal.shutdown().await;

    // WHEN the first entry kept is deleted on disk, as if the purge had gone
    // one entry further
    tamper(
        &dir,
        "DELETE FROM audit_journal_entries WHERE global_seq = 3;",
    );

    // THEN the journal no longer starts where its purge entry says
    let journal = open(&dir).await;
    let report = journal.verify_journal().await.expect("verify");
    assert!(!report.ok, "{report:?}");
    let broken = report.first_break.expect("a break");
    assert_eq!(broken.global_seq, 4);
    assert_eq!(broken.reason, JournalBreakReason::GlobalSeqGap);
    journal.shutdown().await;
}

#[tokio::test]
async fn test_a_purge_entry_rewritten_to_hide_a_deletion_is_detected() {
    // GIVEN a purged journal whose first entry kept is then deleted on disk
    let dir = tempfile::tempdir().unwrap();
    let journal = two_seasons(&dir).await;
    journal
        .purge_by_retention(90, at("2026-06-30T00:00:00Z"))
        .await
        .expect("purge");
    journal.shutdown().await;

    // WHEN the purge entry is rewritten to name the next link as its anchor
    tamper(
        &dir,
        "DELETE FROM audit_journal_entries WHERE global_seq = 3; \
         UPDATE audit_journal_entries \
            SET payload = json_remove(json_set(payload, '$.anchor.global_seq', 3, \
                '$.anchor.global_hash', \
                (SELECT global_prev_hash FROM audit_journal_entries WHERE global_seq = 4)), \
                '$.runs.long') \
          WHERE kind = 'journal_purged';",
    );

    // THEN the global chain reads as contiguous from the forged anchor, and
    // the purge entry no longer matches its own hash: verification says so
    let journal = open(&dir).await;
    let report = journal.verify_journal().await.expect("verify");
    assert!(!report.ok, "{report:?}");
    let broken = report.first_break.expect("a break");
    assert_eq!(broken.reason, JournalBreakReason::PerRunBroken);
    assert_eq!(broken.run_id, JOURNAL_STATE_RUN_ID);
    journal.shutdown().await;
}

#[tokio::test]
async fn test_a_second_purge_carries_the_start_on_and_a_run_emptied_starts_again() {
    // GIVEN a journal purged once, whose runs then go on
    let dir = tempfile::tempdir().unwrap();
    let journal = two_seasons(&dir).await;
    journal
        .purge_by_retention(90, at("2026-06-30T00:00:00Z"))
        .await
        .expect("purge");
    append(&journal, "old", "2026-09-01T08:00:00Z", 8);
    append(&journal, "long", "2026-09-01T08:00:01Z", 9);

    // WHEN a second purge removes June, the first purge entry included
    let outcome = journal
        .purge_by_retention(30, at("2026-09-15T00:00:00Z"))
        .await
        .expect("purge")
        .expect("entries were old enough");

    // THEN the four entries of June are gone, and the run emptied by the
    // first purge started again from its origin
    assert_eq!(outcome.removed, 4);
    let old = journal.query_run("old").await;
    assert_eq!(old.len(), 1);
    assert_eq!(old[0].seq, 0);

    // AND the second purge entry follows the first one in its run and names
    // it as the last entry that run lost
    let GovernanceEvent::JournalPurged { anchor, runs, .. } = last_purge(&journal).await else {
        panic!("not a purge event");
    };
    assert_eq!(anchor.expect("an anchor").global_seq, 6);
    assert_eq!(runs[JOURNAL_STATE_RUN_ID].seq, 0);
    assert_eq!(runs["long"].seq, 1);

    // AND the whole journal and each run still verify, across a restart
    journal.shutdown().await;
    let journal = open(&dir).await;
    let report = journal.verify_journal().await.expect("verify");
    assert!(report.ok, "{report:?}");
    assert_eq!(report.start.expect("a start").global_seq, 7);
    for run in ["old", "long", JOURNAL_STATE_RUN_ID] {
        assert!(journal.verify_chain(run).await.expect("verify").ok, "{run}");
    }
    journal.shutdown().await;
}
