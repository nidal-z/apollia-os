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

pub use entry::{JournalEntry, JournalEntryDraft, JournalEntryKind};
pub use error::AuditJournalError;
pub use handle::AuditJournalHandle;
pub use hash::{compute_entry_hash, SENTINEL_PREV_HASH};

#[cfg(test)]
mod tests {
    use super::*;

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
