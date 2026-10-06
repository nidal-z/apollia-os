use apollia_permissions::org_policy::{JournalExportPolicy, JournalPolicy};
use chrono::{DateTime, Utc};
use serde_json::json;

use super::*;
use crate::audit_journal::{JournalEntryDraft, JournalEntryKind, JournalNotice};
use crate::governance::{JOURNAL_EXPORT_RUN_ID, JOURNAL_STATE_RUN_ID};

const KEY: &[u8] = b"journal-policy-key-0015";

fn at(instant: &str) -> DateTime<Utc> {
    instant.parse().expect("instant")
}

/// A signed journal with one tool call in January and one in June.
async fn journal(dir: &tempfile::TempDir) -> AuditJournalHandle {
    let journal =
        AuditJournalHandle::open_with_key_bytes(&dir.path().join("audit_journal.db"), KEY.to_vec())
            .await
            .expect("open signed journal");
    for ts in ["2026-01-10T08:00:00Z", "2026-06-02T08:00:00Z"] {
        journal.append(JournalEntryDraft {
            run_id: "run-1".to_owned(),
            ts: ts.to_owned(),
            kind: JournalEntryKind::ToolCallCompleted,
            payload: json!({ "tool_name": "file_write", "agent_id": "quotes", "success": true }),
        });
    }
    journal
}

fn policy(
    retention_days: Option<u32>,
    daily_summary_dir: Option<&std::path::Path>,
) -> JournalPolicy {
    JournalPolicy {
        required: false,
        retention_days,
        export: JournalExportPolicy {
            detail: None,
            daily_summary_dir: daily_summary_dir.map(|dir| dir.display().to_string()),
        },
    }
}

async fn kinds_of(journal: &AuditJournalHandle, run: &str) -> Vec<JournalEntryKind> {
    journal
        .query_run(run)
        .await
        .into_iter()
        .map(|entry| entry.kind)
        .collect()
}

#[tokio::test]
async fn test_without_a_policy_nothing_is_removed_and_nothing_is_exported() {
    // GIVEN a journal whose oldest entry is years past any retention
    let dir = tempfile::tempdir().unwrap();
    let journal = journal(&dir).await;

    // WHEN a policy that says nothing of the journal is applied, ten years on
    apply_journal_policy(
        &journal,
        &JournalPolicy::default(),
        at("2036-01-01T00:00:00Z"),
    )
    .await;

    // THEN every entry is still there and the journal says nothing of itself
    assert_eq!(journal.query_run("run-1").await.len(), 2);
    assert!(journal.query_run(JOURNAL_STATE_RUN_ID).await.is_empty());
    assert!(journal.query_run(JOURNAL_EXPORT_RUN_ID).await.is_empty());
    assert!(journal.notices().is_empty());
    journal.shutdown().await;
}

#[tokio::test]
async fn test_a_day_reaches_the_folder_before_the_retention_removes_it() {
    // GIVEN a policy that keeps 90 days and exports to a folder
    let dir = tempfile::tempdir().unwrap();
    let journal = journal(&dir).await;
    let out = dir.path().join("exports");

    // WHEN it is applied on the last day of June
    apply_journal_policy(
        &journal,
        &policy(Some(90), Some(&out)),
        at("2026-06-30T00:00:00Z"),
    )
    .await;

    // THEN the entry of January was written to its daily file, then removed
    let january: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("journal-2026-01-10.json")).unwrap())
            .unwrap();
    assert_eq!(january["count"], 1);
    assert!(out.join("journal-2026-06-02.json").exists());
    let kept = journal.query_run("run-1").await;
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].ts, "2026-06-02T08:00:00Z");

    // AND the journal recorded both exports, by the policy, then the purge
    let exports = journal.query_run(JOURNAL_EXPORT_RUN_ID).await;
    assert_eq!(exports.len(), 2);
    assert_eq!(exports[0].payload["actor"], "policy");
    assert_eq!(exports[0].payload["detail"], false);
    assert_eq!(
        exports[0].payload["filters"]["since"],
        "2026-01-10T00:00:00Z"
    );
    assert_eq!(
        exports[0].payload["filters"]["until"],
        "2026-01-11T00:00:00Z"
    );
    assert_eq!(
        kinds_of(&journal, JOURNAL_STATE_RUN_ID).await,
        [JournalEntryKind::JournalPurged]
    );
    assert!(journal.notices().is_empty());
    assert!(journal.verify_journal().await.expect("verify").ok);
    journal.shutdown().await;
}

#[tokio::test]
async fn test_a_folder_that_cannot_be_used_is_said_and_recorded_and_stops_nothing() {
    // GIVEN a policy whose export folder is a file
    let dir = tempfile::tempdir().unwrap();
    let journal = journal(&dir).await;
    let blocked = dir.path().join("not-a-folder");
    std::fs::write(&blocked, b"a file").unwrap();
    let policy = policy(Some(90), Some(&blocked));

    // WHEN it is applied two days in a row
    apply_journal_policy(&journal, &policy, at("2026-06-29T00:01:00Z")).await;
    apply_journal_policy(&journal, &policy, at("2026-06-30T00:01:00Z")).await;

    // THEN the window is told which folder and why
    let notices = journal.notices();
    assert!(
        matches!(&notices[..], [JournalNotice::DailyExportFailed { directory, .. }]
            if directory == &blocked.display().to_string()),
        "{notices:?}"
    );

    // AND the journal recorded the failure once, not once per day
    let failures = journal.query_run(JOURNAL_EXPORT_RUN_ID).await;
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(failures[0].kind, JournalEntryKind::JournalExportFailed);
    assert_eq!(
        failures[0].payload["directory"],
        blocked.display().to_string()
    );

    // AND the retention was applied all the same
    assert_eq!(journal.query_run("run-1").await.len(), 1);

    // WHEN the folder is usable again
    std::fs::remove_file(&blocked).unwrap();
    apply_journal_policy(&journal, &policy, at("2026-07-01T00:01:00Z")).await;

    // THEN the notice is lifted
    assert!(journal.notices().is_empty());
    journal.shutdown().await;
}

#[tokio::test]
async fn test_a_journal_open_without_its_key_says_so_and_stays_usable() {
    // GIVEN a journal opened without a signing key, and one opened with it
    let dir = tempfile::tempdir().unwrap();
    let unsigned = AuditJournalHandle::open(&dir.path().join("unsigned.db"))
        .await
        .expect("open unsigned journal");
    let signed = journal(&dir).await;

    // WHEN each one records its state at startup
    record_journal_state_at_startup(&unsigned);
    record_journal_state_at_startup(&signed);

    // THEN the unsigned one says it, in the window and in its own chain
    assert_eq!(unsigned.notices(), [JournalNotice::Unsigned]);
    assert_eq!(
        kinds_of(&unsigned, JOURNAL_STATE_RUN_ID).await,
        [JournalEntryKind::JournalUnsigned]
    );

    // AND it keeps recording, its chain verified without a key
    unsigned.append(JournalEntryDraft {
        run_id: "run-1".to_owned(),
        ts: "2026-06-02T08:00:00Z".to_owned(),
        kind: JournalEntryKind::ToolCallStarted,
        payload: json!({ "tool_name": "file_write" }),
    });
    let report = unsigned.verify_journal().await.expect("verify");
    assert!(report.ok, "{report:?}");
    assert_eq!(report.entries_checked, 2);

    // AND the signed one has nothing to say
    assert!(signed.notices().is_empty());
    assert!(signed.query_run(JOURNAL_STATE_RUN_ID).await.is_empty());
    unsigned.shutdown().await;
    signed.shutdown().await;
}

#[test]
fn test_the_next_application_is_one_minute_past_midnight_utc() {
    // GIVEN an instant in the evening
    let now = at("2026-06-30T21:30:00Z");

    // WHEN the wait until the next application is computed
    let wait = until_next_day(now);

    // THEN it ends at 00:01 UTC the next day
    assert_eq!(wait, std::time::Duration::from_secs(2 * 3600 + 31 * 60));
}
