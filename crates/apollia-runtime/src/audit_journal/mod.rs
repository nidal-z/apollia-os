//! Append-only, hash-chained audit journal scoped per run.
//!
//! Each entry is linked to the previous one in the same run by a SHA256
//! `prev_hash`, so deleting, reordering, or mutating an entry breaks the chain
//! and is detectable on recomputation. Append-only is also enforced at the
//! storage layer by SQLite triggers that abort any `UPDATE` or `DELETE`.
//!
//! The journal is distinct from the flat tool-invocation audit trail in
//! `apollia_tools::audit`: that one records tool calls without chaining; this
//! one provides tamper-evidence over the lifecycle of a run.

pub mod actor;
pub mod anchor;
pub mod entry;
pub mod error;
pub mod handle;
pub mod hash;
#[cfg(any(test, kani))]
mod proofs;
pub mod signer;
pub mod subscriber;
pub mod verify;

pub use entry::{JournalEntry, JournalEntryDraft, JournalEntryKind, PlanMutationSnapshot};
pub use error::AuditJournalError;
pub use handle::AuditJournalHandle;
pub use hash::{compute_entry_hash, compute_global_hash, SENTINEL_PREV_HASH};
pub use signer::{HmacSigner, JournalSigner, SignerError, SignerUnavailablePolicy};
pub use subscriber::AuditJournalSubscriber;
pub use verify::{
    BrokenLink, BrokenLinkReason, JournalAnchor, JournalBreak, JournalBreakReason,
    VerifyChainReport, VerifyJournalReport,
};

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_auth::{AuthError, SecretStore};
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    /// SecretStore double: serves a single optional key for any (service, user).
    struct MockStore {
        value: Option<String>,
    }

    impl SecretStore for MockStore {
        fn set(&self, _service: &str, _user: &str, _value: &str) -> Result<(), AuthError> {
            Ok(())
        }
        fn get(&self, _service: &str, _user: &str) -> Result<Option<String>, AuthError> {
            Ok(self.value.clone())
        }
        fn delete(&self, _service: &str, _user: &str) -> Result<(), AuthError> {
            Ok(())
        }
        fn backend_id(&self) -> &'static str {
            "mock"
        }
    }

    fn temp_db() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("apollia_journal_sign_{}.db", uuid::Uuid::new_v4()))
    }

    // An entry is signed and the signature persists
    #[tokio::test]
    async fn test_ac1_entry_signed_and_persisted() {
        // GIVEN a journal opened with a signer whose key is present
        let store = MockStore {
            value: Some(STANDARD.encode(b"audit-key-material-0001")),
        };
        let path = temp_db();
        let handle = AuditJournalHandle::open_with_signer(
            &path,
            &store,
            "journal-hmac-key",
            SignerUnavailablePolicy::FailHard,
        )
        .await
        .expect("open with signer");

        // WHEN an entry is appended
        handle.append(JournalEntryDraft {
            run_id: "run-1".to_string(),
            ts: "2026-01-01T00:00:00Z".to_string(),
            kind: JournalEntryKind::ToolCallStarted,
            payload: serde_json::json!({"tool": "bash"}),
        });

        // THEN the persisted entry carries a signature and a key id
        let entries = handle.query_run("run-1").await;
        assert_eq!(entries.len(), 1);
        assert!(entries[0].signature.is_some());
        assert!(entries[0].signing_key_id.is_some());
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // shutdown() drains the queue before returning (deterministic, no sleep).
    #[tokio::test]
    async fn test_shutdown_drains_before_returning() {
        // GIVEN a journal with many appends enqueued fire-and-forget
        let path = temp_db();
        let handle = AuditJournalHandle::open(&path).await.expect("open");
        const N: usize = 64;
        for i in 0..N {
            handle.append(JournalEntryDraft {
                run_id: "run-drain".to_string(),
                ts: "2026-01-01T00:00:00Z".to_string(),
                kind: JournalEntryKind::ToolCallStarted,
                payload: serde_json::json!({ "i": i }),
            });
        }

        // WHEN shutdown is awaited: it must block until the actor has drained the
        // whole queue (the Shutdown message is processed FIFO, after every append)
        handle.shutdown().await;

        // THEN reopening the persisted journal shows every appended entry, proving
        // the queue was fully drained before shutdown returned (no lost tail).
        let reopened = AuditJournalHandle::open(&path).await.expect("reopen");
        let entries = reopened.query_run("run-drain").await;
        assert_eq!(
            entries.len(),
            N,
            "all appends must be persisted before shutdown returns"
        );
        reopened.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // Warn-and-continue: a missing key opens an unsigned journal
    #[tokio::test]
    async fn test_ac2_warn_and_continue_on_missing_key() {
        // GIVEN a store with no key and the warn-and-continue policy
        let store = MockStore { value: None };
        let path = temp_db();
        let handle = AuditJournalHandle::open_with_signer(
            &path,
            &store,
            "journal-hmac-key",
            SignerUnavailablePolicy::WarnAndContinue,
        )
        .await
        .expect("open degraded");

        // WHEN an entry is appended
        handle.append(JournalEntryDraft {
            run_id: "run-1".to_string(),
            ts: "2026-01-01T00:00:00Z".to_string(),
            kind: JournalEntryKind::AgentStarted,
            payload: serde_json::json!({}),
        });

        // THEN the journal works and entries are unsigned
        let entries = handle.query_run("run-1").await;
        assert_eq!(entries.len(), 1);
        assert!(entries[0].signature.is_none());
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // Fail-hard: a missing key refuses to open (error case)
    #[tokio::test]
    async fn test_ac2_fail_hard_on_missing_key() {
        // GIVEN a store with no key and the fail-hard policy
        let store = MockStore { value: None };
        let path = temp_db();
        // WHEN opening with a signer
        let result = AuditJournalHandle::open_with_signer(
            &path,
            &store,
            "journal-hmac-key",
            SignerUnavailablePolicy::FailHard,
        )
        .await;
        // THEN opening fails with SignerUnavailable
        assert!(matches!(
            result,
            Err(AuditJournalError::SignerUnavailable(_))
        ));
        tokio::fs::remove_file(&path).await.ok();
    }

    // Central tamper test: a signed journal whose stored payload is altered on
    // disk (after disabling the append-only triggers, as an attacker with file
    // access would) MUST fail verification.
    #[tokio::test]
    async fn test_tampering_fails_verification() {
        // GIVEN a signed journal with three entries, then closed
        let key = STANDARD.encode(b"tamper-test-audit-key");
        let path = temp_db();
        {
            let store = MockStore {
                value: Some(key.clone()),
            };
            let handle = AuditJournalHandle::open_with_signer(
                &path,
                &store,
                "journal-hmac-key",
                SignerUnavailablePolicy::FailHard,
            )
            .await
            .expect("open with signer");
            for i in 0..3 {
                handle.append(JournalEntryDraft {
                    run_id: "run-1".to_string(),
                    ts: "2026-01-01T00:00:00Z".to_string(),
                    kind: JournalEntryKind::ToolCallStarted,
                    payload: serde_json::json!({ "i": i }),
                });
            }

            // Sanity: an intact journal verifies before tampering
            let report = handle.verify_chain("run-1").await.expect("verify");
            assert!(report.ok, "intact chain should verify, got {report:?}");
            assert_eq!(report.entries_checked, 3);
            handle.shutdown().await;
        }

        // WHEN an attacker disables the triggers and rewrites a stored payload
        {
            let conn = rusqlite::Connection::open(&path).expect("reopen raw");
            conn.execute_batch(
                "DROP TRIGGER IF EXISTS aje_no_update; \
                 UPDATE audit_journal_entries SET payload = '{\"i\":666}' WHERE seq = 1;",
            )
            .expect("forced tamper");
        }

        // THEN reopening and verifying reports a broken link at the tampered seq
        let store = MockStore { value: Some(key) };
        let handle = AuditJournalHandle::open_with_signer(
            &path,
            &store,
            "journal-hmac-key",
            SignerUnavailablePolicy::FailHard,
        )
        .await
        .expect("reopen with signer");
        let report = handle.verify_chain("run-1").await.expect("verify");
        assert!(!report.ok, "tampered chain must fail verification");
        let link = report.first_broken_link.expect("broken link");
        assert_eq!(link.seq, 1);
        assert_eq!(link.reason, verify::BrokenLinkReason::HashMismatch);
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // An unknown run yields an empty report (mapped to not-found by callers)
    #[tokio::test]
    async fn test_verify_unknown_run_is_empty() {
        // GIVEN an empty journal
        let (handle, path) = open_temp().await;
        // WHEN verifying a run that has no entries
        let report = handle.verify_chain("nope").await.expect("verify");
        // THEN the report is ok with zero entries checked
        assert!(report.ok);
        assert_eq!(report.entries_checked, 0);
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    async fn open_temp() -> (AuditJournalHandle, std::path::PathBuf) {
        let db_path =
            std::env::temp_dir().join(format!("apollia_journal_test_{}.db", uuid::Uuid::new_v4()));
        let handle = AuditJournalHandle::open(&db_path)
            .await
            .expect("open journal");
        (handle, db_path)
    }

    fn draft(
        run_id: &str,
        kind: JournalEntryKind,
        payload: serde_json::Value,
    ) -> JournalEntryDraft {
        JournalEntryDraft {
            run_id: run_id.to_string(),
            ts: "2026-01-01T00:00:00Z".to_string(),
            kind,
            payload,
        }
    }

    // First entry points at the sentinel, the next links to the previous hash
    #[tokio::test]
    async fn test_chain_links_consecutive_entries() {
        // GIVEN a journal with two appended entries in one run
        let (handle, path) = open_temp().await;
        handle.append(draft(
            "run-1",
            JournalEntryKind::AgentStarted,
            serde_json::json!({"n": 1}),
        ));
        handle.append(draft(
            "run-1",
            JournalEntryKind::AgentStopped,
            serde_json::json!({"n": 2}),
        ));

        // WHEN reading the run back
        let entries = handle.query_run("run-1").await;

        // THEN the first prev_hash is the sentinel and the second links to the first
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].seq, 0);
        assert_eq!(entries[0].prev_hash, SENTINEL_PREV_HASH);
        assert_eq!(entries[1].seq, 1);
        assert_eq!(entries[1].prev_hash, entries[0].hash);
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // Interleaved runs keep independent, continuous chains
    #[tokio::test]
    async fn test_runs_are_isolated() {
        // GIVEN appends interleaved across two runs
        let (handle, path) = open_temp().await;
        handle.append(draft(
            "run-a",
            JournalEntryKind::ToolCallStarted,
            serde_json::json!({}),
        ));
        handle.append(draft(
            "run-b",
            JournalEntryKind::ToolCallStarted,
            serde_json::json!({}),
        ));
        handle.append(draft(
            "run-a",
            JournalEntryKind::ToolCallCompleted,
            serde_json::json!({}),
        ));

        // WHEN each run is read
        let a = handle.query_run("run-a").await;
        let b = handle.query_run("run-b").await;

        // THEN each run has its own continuous chain
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].seq, 0);
        assert_eq!(a[1].seq, 1);
        assert_eq!(a[1].prev_hash, a[0].hash);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].seq, 0);
        assert_eq!(b[0].prev_hash, SENTINEL_PREV_HASH);
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // A page spans every run and comes back newest global position first
    #[tokio::test]
    async fn test_query_page_spans_runs_newest_first() {
        // GIVEN four entries appended across two interleaved runs
        let (handle, path) = open_temp().await;
        for (run, kind) in [
            ("run-a", JournalEntryKind::ToolCallStarted),
            ("run-b", JournalEntryKind::ToolCallStarted),
            ("run-a", JournalEntryKind::ToolCallCompleted),
            ("run-b", JournalEntryKind::ToolCallCompleted),
        ] {
            handle.append(draft(run, kind, serde_json::json!({})));
        }

        // WHEN a page is read without naming any run
        let page = handle.query_page(10, 0).await;

        // THEN it holds every entry, newest append first, both runs present
        assert_eq!(page.len(), 4);
        assert_eq!(page[0].run_id, "run-b");
        assert_eq!(page[0].seq, 1);
        assert_eq!(page[3].run_id, "run-a");
        assert_eq!(page[3].seq, 0);
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // limit bounds one page, offset walks past it without overlap or gap
    #[tokio::test]
    async fn test_query_page_limit_and_offset_page_through() {
        // GIVEN five entries in one run
        let (handle, path) = open_temp().await;
        for _ in 0..5 {
            handle.append(draft(
                "run-p",
                JournalEntryKind::ToolCallStarted,
                serde_json::json!({}),
            ));
        }

        // WHEN the journal is read in two pages of three
        let first = handle.query_page(3, 0).await;
        let second = handle.query_page(3, 3).await;

        // THEN the pages are contiguous and cover the whole journal once
        assert_eq!(first.len(), 3);
        assert_eq!(second.len(), 2);
        let seqs: Vec<u64> = first.iter().chain(second.iter()).map(|e| e.seq).collect();
        assert_eq!(seqs, vec![4, 3, 2, 1, 0]);
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // An empty journal answers a page rather than failing
    #[tokio::test]
    async fn test_query_page_on_empty_journal_is_empty() {
        // GIVEN a journal that has never been appended to
        let (handle, path) = open_temp().await;

        // WHEN a page is read
        let page = handle.query_page(20, 0).await;

        // THEN it is empty, and no error surfaced
        assert!(page.is_empty());
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // Append-only: a raw UPDATE and DELETE are refused by the triggers
    #[tokio::test]
    async fn test_journal_is_append_only() {
        // GIVEN a journal with one entry, then closed
        let (handle, path) = open_temp().await;
        handle.append(draft(
            "run-1",
            JournalEntryKind::AgentStarted,
            serde_json::json!({}),
        ));
        handle.shutdown().await;

        // WHEN attempting a direct UPDATE then DELETE
        let conn = rusqlite::Connection::open(&path).expect("reopen");
        let update_err = conn
            .execute("UPDATE audit_journal_entries SET payload = '{}'", [])
            .expect_err("update must abort")
            .to_string();
        let delete_err = conn
            .execute("DELETE FROM audit_journal_entries", [])
            .expect_err("delete must abort")
            .to_string();

        // THEN both are refused with the append-only message
        assert!(
            update_err.contains("audit journal is append-only"),
            "got: {update_err}"
        );
        assert!(
            delete_err.contains("audit journal is append-only"),
            "got: {delete_err}"
        );
        drop(conn);
        tokio::fs::remove_file(&path).await.ok();
    }

    // Reopening an existing store is idempotent and keeps the chain going
    #[tokio::test]
    async fn test_idempotent_reopen_continues_chain() {
        // GIVEN a store with one entry, closed
        let (handle, path) = open_temp().await;
        handle.append(draft(
            "run-1",
            JournalEntryKind::AgentStarted,
            serde_json::json!({"i": 0}),
        ));
        handle.shutdown().await;

        // WHEN reopening and appending again
        let handle2 = AuditJournalHandle::open(&path).await.expect("reopen");
        handle2.append(draft(
            "run-1",
            JournalEntryKind::AgentStopped,
            serde_json::json!({"i": 1}),
        ));

        // THEN the new entry continues the chain from the persisted head
        let entries = handle2.query_run("run-1").await;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].seq, 1);
        assert_eq!(entries[1].prev_hash, entries[0].hash);
        handle2.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // A mutated payload no longer matches its recomputed hash
    #[tokio::test]
    async fn test_recompute_detects_mutation() {
        // GIVEN a stored entry
        let (handle, path) = open_temp().await;
        handle.append(draft(
            "run-1",
            JournalEntryKind::ToolCallStarted,
            serde_json::json!({"v": 1}),
        ));
        let entries = handle.query_run("run-1").await;
        let mut tampered = entries[0].clone();

        // WHEN the payload is altered
        tampered.payload = serde_json::json!({"v": 999});

        // THEN the recomputed hash diverges from the stored one
        assert_ne!(compute_entry_hash(&tampered), entries[0].hash);
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // ---------------------------------------------------------------------
    // Global chain: tamper-evidence against truncation and whole-run deletion
    // ---------------------------------------------------------------------

    /// Open a signed journal on a fresh temp file with a fixed key.
    async fn open_signed_temp() -> (AuditJournalHandle, std::path::PathBuf, String) {
        let key = STANDARD.encode(b"global-chain-audit-key");
        let path = temp_db();
        let store = MockStore {
            value: Some(key.clone()),
        };
        let handle = AuditJournalHandle::open_with_signer(
            &path,
            &store,
            "journal-hmac-key",
            SignerUnavailablePolicy::FailHard,
        )
        .await
        .expect("open signed");
        (handle, path, key)
    }

    /// Reopen a raw connection with the append-only triggers dropped, as an
    /// attacker with file access would, and run one statement.
    fn raw_tamper(path: &std::path::Path, sql: &str) {
        let conn = rusqlite::Connection::open(path).expect("reopen raw");
        conn.execute_batch(&format!(
            "DROP TRIGGER IF EXISTS aje_no_update; DROP TRIGGER IF EXISTS aje_no_delete; {sql}"
        ))
        .expect("forced tamper");
    }

    // An intact global chain across two interleaved runs verifies
    #[tokio::test]
    async fn test_global_chain_intact_verifies() {
        // GIVEN interleaved appends across two runs
        let (handle, path, _key) = open_signed_temp().await;
        handle.append(draft(
            "run-a",
            JournalEntryKind::ToolCallStarted,
            serde_json::json!({}),
        ));
        handle.append(draft(
            "run-b",
            JournalEntryKind::ToolCallStarted,
            serde_json::json!({}),
        ));
        handle.append(draft(
            "run-a",
            JournalEntryKind::ToolCallCompleted,
            serde_json::json!({}),
        ));

        // WHEN verifying the whole journal
        let report = handle.verify_journal().await.expect("verify journal");

        // THEN it is intact, covers two runs, and matches the head anchor
        assert!(report.ok, "intact journal should verify, got {report:?}");
        assert_eq!(report.entries_checked, 3);
        assert_eq!(report.runs_checked, 2);
        assert!(report.head_matches_state);
        assert!(report.first_break.is_none());
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // Deleting an interior global entry breaks the global chain
    #[tokio::test]
    async fn test_interior_run_deletion_breaks_global_chain() {
        // GIVEN a three-entry journal, closed
        let (handle, path, key) = open_signed_temp().await;
        for i in 0..3 {
            handle.append(draft(
                "run-1",
                JournalEntryKind::ToolCallStarted,
                serde_json::json!({ "i": i }),
            ));
        }
        handle.shutdown().await;

        // WHEN an attacker deletes the middle global entry
        raw_tamper(
            &path,
            "DELETE FROM audit_journal_entries WHERE global_seq = 1;",
        );

        // THEN reopening and verifying reports a global break naming the position
        let store = MockStore { value: Some(key) };
        let handle = AuditJournalHandle::open_with_signer(
            &path,
            &store,
            "journal-hmac-key",
            SignerUnavailablePolicy::FailHard,
        )
        .await
        .expect("reopen");
        let report = handle.verify_journal().await.expect("verify");
        assert!(!report.ok, "interior deletion must fail, got {report:?}");
        let brk = report.first_break.expect("break");
        assert_eq!(brk.run_id, "run-1");
        assert!(matches!(
            brk.reason,
            verify::JournalBreakReason::GlobalSeqGap
                | verify::JournalBreakReason::GlobalPrevHashMismatch
        ));
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // Deleting a whole non-tail run leaves a gap in the global chain
    #[tokio::test]
    async fn test_whole_run_deletion_detected() {
        // GIVEN two runs, run-b sitting in the global interior
        let (handle, path, key) = open_signed_temp().await;
        handle.append(draft(
            "run-a",
            JournalEntryKind::ToolCallStarted,
            serde_json::json!({}),
        ));
        handle.append(draft(
            "run-b",
            JournalEntryKind::ToolCallStarted,
            serde_json::json!({}),
        ));
        handle.append(draft(
            "run-a",
            JournalEntryKind::ToolCallCompleted,
            serde_json::json!({}),
        ));
        handle.shutdown().await;

        // WHEN an attacker deletes every entry of run-b
        raw_tamper(
            &path,
            "DELETE FROM audit_journal_entries WHERE run_id = 'run-b';",
        );

        // THEN the whole-journal verification fails
        let store = MockStore { value: Some(key) };
        let handle = AuditJournalHandle::open_with_signer(
            &path,
            &store,
            "journal-hmac-key",
            SignerUnavailablePolicy::FailHard,
        )
        .await
        .expect("reopen");
        let report = handle.verify_journal().await.expect("verify");
        assert!(!report.ok, "whole-run deletion must fail, got {report:?}");
        assert!(report.first_break.is_some());
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // Truncating the global tail is caught by the head anchor
    #[tokio::test]
    async fn test_tail_truncation_detected_vs_exported_anchor() {
        // GIVEN a three-entry journal, its head anchor exported, then closed
        let (handle, path, key) = open_signed_temp().await;
        for i in 0..3 {
            handle.append(draft(
                "run-1",
                JournalEntryKind::ToolCallStarted,
                serde_json::json!({ "i": i }),
            ));
        }
        let exported = handle
            .journal_anchor()
            .await
            .expect("anchor")
            .expect("some");
        assert_eq!(exported.global_seq, 2);
        handle.shutdown().await;

        // WHEN an attacker truncates the global tail (leaves a valid short chain)
        raw_tamper(
            &path,
            "DELETE FROM audit_journal_entries WHERE global_seq = 2;",
        );

        // THEN the chain itself is intact but the head no longer matches the
        // persisted anchor, and the exported anchor is ahead of the tail
        let store = MockStore { value: Some(key) };
        let handle = AuditJournalHandle::open_with_signer(
            &path,
            &store,
            "journal-hmac-key",
            SignerUnavailablePolicy::FailHard,
        )
        .await
        .expect("reopen");
        let report = handle.verify_journal().await.expect("verify");
        assert!(!report.ok, "tail truncation must fail, got {report:?}");
        assert!(
            report.first_break.is_none(),
            "the short chain is internally valid"
        );
        assert!(!report.head_matches_state);
        assert!(exported.global_seq > report.entries_checked - 1);
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // Rolling back the mutable state row is detected against the entries head
    #[tokio::test]
    async fn test_state_row_rollback_detected() {
        // GIVEN a three-entry journal, closed
        let (handle, path, key) = open_signed_temp().await;
        for i in 0..3 {
            handle.append(draft(
                "run-1",
                JournalEntryKind::ToolCallStarted,
                serde_json::json!({ "i": i }),
            ));
        }
        handle.shutdown().await;

        // WHEN an attacker rolls the state row back (no trigger guards it)
        let conn = rusqlite::Connection::open(&path).expect("reopen raw");
        conn.execute(
            "UPDATE audit_journal_state SET global_seq = 0, global_hash = 'deadbeef' WHERE id = 0",
            [],
        )
        .expect("rollback state");
        drop(conn);

        // THEN the terminal entries head no longer matches the state anchor
        let store = MockStore { value: Some(key) };
        let handle = AuditJournalHandle::open_with_signer(
            &path,
            &store,
            "journal-hmac-key",
            SignerUnavailablePolicy::FailHard,
        )
        .await
        .expect("reopen");
        let report = handle.verify_journal().await.expect("verify");
        assert!(!report.ok, "state rollback must fail, got {report:?}");
        assert!(!report.head_matches_state);
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // A payload mutation is caught by the per-run chain inside whole-journal verify
    #[tokio::test]
    async fn test_entry_mutation_detected_by_journal_verify() {
        // GIVEN a two-entry journal, closed
        let (handle, path, key) = open_signed_temp().await;
        for i in 0..2 {
            handle.append(draft(
                "run-1",
                JournalEntryKind::ToolCallStarted,
                serde_json::json!({ "i": i }),
            ));
        }
        handle.shutdown().await;

        // WHEN an attacker mutates a stored payload (leaving the hash stale)
        raw_tamper(
            &path,
            "UPDATE audit_journal_entries SET payload = '{\"i\":666}' WHERE seq = 0;",
        );

        // THEN whole-journal verify fails via the per-run chain
        let store = MockStore { value: Some(key) };
        let handle = AuditJournalHandle::open_with_signer(
            &path,
            &store,
            "journal-hmac-key",
            SignerUnavailablePolicy::FailHard,
        )
        .await
        .expect("reopen");
        let report = handle.verify_journal().await.expect("verify");
        assert!(!report.ok, "mutation must fail, got {report:?}");
        let brk = report.first_break.expect("break");
        assert_eq!(brk.reason, verify::JournalBreakReason::PerRunBroken);
        assert_eq!(brk.run_id, "run-1");
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // S1 exploit: an attacker with file access drops the HMAC layer by setting
    // every signature to NULL. The keyless hash chain and the mutable anchor
    // stay internally consistent, so before the signatures-required fix this
    // verified ok. It must now fail: a signed journal requires signatures.
    #[tokio::test]
    async fn test_signature_strip_fails_verification() {
        // GIVEN a signed three-entry journal, closed
        let (handle, path, key) = open_signed_temp().await;
        for i in 0..3 {
            handle.append(draft(
                "run-1",
                JournalEntryKind::ToolCallStarted,
                serde_json::json!({ "i": i }),
            ));
        }
        handle.shutdown().await;

        // WHEN the attacker strips both the per-run and the global signatures,
        // leaving the hashes, the chain linkage, and the head anchor untouched
        raw_tamper(
            &path,
            "UPDATE audit_journal_entries SET \
                 signature = NULL, signing_key_id = NULL, \
                 global_signature = NULL, global_signing_key_id = NULL;",
        );

        // THEN reopening with the signer and verifying reports a signature break,
        // even though nothing but the signatures changed
        let store = MockStore {
            value: Some(key.clone()),
        };
        let handle = AuditJournalHandle::open_with_signer(
            &path,
            &store,
            "journal-hmac-key",
            SignerUnavailablePolicy::FailHard,
        )
        .await
        .expect("reopen");
        let report = handle.verify_journal().await.expect("verify");
        assert!(!report.ok, "stripped signatures must fail, got {report:?}");
        let brk = report.first_break.expect("break");
        assert_eq!(
            brk.reason,
            verify::JournalBreakReason::GlobalSignatureInvalid
        );

        // AND a per-run verify of the same run also fails on the missing signature
        let chain = handle.verify_chain("run-1").await.expect("verify chain");
        assert!(!chain.ok, "stripped run signatures must fail");
        assert_eq!(
            chain.first_broken_link.expect("link").reason,
            verify::BrokenLinkReason::SignatureInvalid
        );
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // Companion to the strip test: the same signed journal, untampered, verifies
    // ok, so the required-signatures mode does not reject legitimate entries.
    #[tokio::test]
    async fn test_signed_journal_still_verifies_under_required_mode() {
        // GIVEN a signed three-entry journal
        let (handle, path, _key) = open_signed_temp().await;
        for i in 0..3 {
            handle.append(draft(
                "run-1",
                JournalEntryKind::ToolCallStarted,
                serde_json::json!({ "i": i }),
            ));
        }

        // WHEN verifying the whole journal and the run
        let journal = handle.verify_journal().await.expect("verify");
        let chain = handle.verify_chain("run-1").await.expect("verify chain");

        // THEN both pass: required signatures are present and valid
        assert!(
            journal.ok,
            "intact signed journal must verify, got {journal:?}"
        );
        assert_eq!(journal.entries_checked, 3);
        assert!(chain.ok, "intact signed run must verify, got {chain:?}");
        assert_eq!(chain.entries_checked, 3);
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // Reopening continues the global chain monotonically without a gap
    #[tokio::test]
    async fn test_idempotent_reopen_continues_global_chain() {
        // GIVEN a signed journal with one entry, closed
        let (handle, path, key) = open_signed_temp().await;
        handle.append(draft(
            "run-1",
            JournalEntryKind::AgentStarted,
            serde_json::json!({"i": 0}),
        ));
        handle.shutdown().await;

        // WHEN reopening and appending in a second run
        let store = MockStore { value: Some(key) };
        let handle = AuditJournalHandle::open_with_signer(
            &path,
            &store,
            "journal-hmac-key",
            SignerUnavailablePolicy::FailHard,
        )
        .await
        .expect("reopen");
        handle.append(draft(
            "run-2",
            JournalEntryKind::AgentStarted,
            serde_json::json!({"i": 1}),
        ));

        // THEN the global chain spans both runs with no gap
        let report = handle.verify_journal().await.expect("verify");
        assert!(report.ok, "reopened journal should verify, got {report:?}");
        assert_eq!(report.entries_checked, 2);
        assert_eq!(report.runs_checked, 2);
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // Without a signer, the global chain still links by hash and verifies
    #[tokio::test]
    async fn test_unsigned_global_chain_links() {
        // GIVEN an unsigned journal with two entries
        let (handle, path) = open_temp().await;
        handle.append(draft(
            "run-1",
            JournalEntryKind::ToolCallStarted,
            serde_json::json!({}),
        ));
        handle.append(draft(
            "run-1",
            JournalEntryKind::ToolCallCompleted,
            serde_json::json!({}),
        ));

        // WHEN verifying the whole journal
        let report = handle.verify_journal().await.expect("verify");

        // THEN the hash-only global chain verifies
        assert!(report.ok, "unsigned journal should verify, got {report:?}");
        assert_eq!(report.entries_checked, 2);
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // An empty journal verifies with nothing checked
    #[tokio::test]
    async fn test_verify_empty_journal_ok() {
        // GIVEN an empty journal
        let (handle, path) = open_temp().await;
        // WHEN verifying the whole journal
        let report = handle.verify_journal().await.expect("verify");
        // THEN it is ok with zero entries and zero runs
        assert!(report.ok);
        assert_eq!(report.entries_checked, 0);
        assert_eq!(report.runs_checked, 0);
        assert!(report.head_matches_state);
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }
}
