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
pub mod entry;
pub mod error;
pub mod handle;
pub mod hash;
pub mod signer;
pub mod subscriber;
pub mod verify;

pub use entry::{JournalEntry, JournalEntryDraft, JournalEntryKind};
pub use error::AuditJournalError;
pub use handle::AuditJournalHandle;
pub use hash::{compute_entry_hash, SENTINEL_PREV_HASH};
pub use signer::{HmacSigner, JournalSigner, SignerError, SignerUnavailablePolicy};
pub use subscriber::AuditJournalSubscriber;
pub use verify::{BrokenLink, BrokenLinkReason, VerifyChainReport};

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

    // AC-1: an entry is signed and the signature persists
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
        tokio::time::sleep(tokio::time::Duration::from_millis(80)).await;

        // THEN the persisted entry carries a signature and a key id
        let entries = handle.query_run("run-1").await;
        assert_eq!(entries.len(), 1);
        assert!(entries[0].signature.is_some());
        assert!(entries[0].signing_key_id.is_some());
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // AC-2 warn-and-continue: a missing key opens an unsigned journal
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
        tokio::time::sleep(tokio::time::Duration::from_millis(80)).await;

        // THEN the journal works and entries are unsigned
        let entries = handle.query_run("run-1").await;
        assert_eq!(entries.len(), 1);
        assert!(entries[0].signature.is_none());
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // AC-2 fail-hard: a missing key refuses to open (error case)
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
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

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
        tokio::time::sleep(tokio::time::Duration::from_millis(80)).await;

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
        tokio::time::sleep(tokio::time::Duration::from_millis(80)).await;

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
        tokio::time::sleep(tokio::time::Duration::from_millis(80)).await;
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
        tokio::time::sleep(tokio::time::Duration::from_millis(80)).await;
        handle.shutdown().await;

        // WHEN reopening and appending again
        let handle2 = AuditJournalHandle::open(&path).await.expect("reopen");
        handle2.append(draft(
            "run-1",
            JournalEntryKind::AgentStopped,
            serde_json::json!({"i": 1}),
        ));
        tokio::time::sleep(tokio::time::Duration::from_millis(80)).await;

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
        tokio::time::sleep(tokio::time::Duration::from_millis(80)).await;
        let entries = handle.query_run("run-1").await;
        let mut tampered = entries[0].clone();

        // WHEN the payload is altered
        tampered.payload = serde_json::json!({"v": 999});

        // THEN the recomputed hash diverges from the stored one
        assert_ne!(compute_entry_hash(&tampered), entries[0].hash);
        handle.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }
}
