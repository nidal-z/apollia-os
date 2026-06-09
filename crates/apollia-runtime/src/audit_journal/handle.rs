//! Clonable async handle to the journal actor.

use std::path::Path;

use crate::audit_journal::actor::{JournalActor, JournalMessage, SCHEMA};
use crate::audit_journal::entry::{JournalEntry, JournalEntryDraft};
use crate::audit_journal::error::AuditJournalError;

/// Capacity of the channel between the handle and the actor.
const CHANNEL_CAPACITY: usize = 1024;

/// Clonable handle to the append-only hash-chained audit journal.
///
/// Created via [`AuditJournalHandle::open`]. Several handles may coexist and
/// append to the same actor; the actor serializes writes and owns the chain.
#[derive(Clone)]
pub struct AuditJournalHandle {
    sender: tokio::sync::mpsc::Sender<JournalMessage>,
}

impl AuditJournalHandle {
    /// Open the journal database and start the actor.
    ///
    /// Creates the file if absent, enables WAL mode, and applies the schema
    /// idempotently (`CREATE ... IF NOT EXISTS`), so reopening an existing store
    /// is a no-op on the schema.
    pub async fn open(db_path: &Path) -> Result<Self, AuditJournalError> {
        let db_path = db_path.to_path_buf();
        let (sender, receiver) = tokio::sync::mpsc::channel::<JournalMessage>(CHANNEL_CAPACITY);
        let (init_tx, init_rx) = tokio::sync::oneshot::channel::<Result<(), AuditJournalError>>();

        tokio::task::spawn_blocking(move || {
            let conn = match rusqlite::Connection::open(&db_path) {
                Ok(c) => c,
                Err(e) => {
                    let _ = init_tx.send(Err(AuditJournalError::Open(e.to_string())));
                    return;
                }
            };
            if let Err(e) = conn.execute_batch("PRAGMA journal_mode=WAL;") {
                let _ = init_tx.send(Err(AuditJournalError::SchemaInit(e.to_string())));
                return;
            }
            if let Err(e) = conn.execute_batch(SCHEMA) {
                let _ = init_tx.send(Err(AuditJournalError::SchemaInit(e.to_string())));
                return;
            }
            let _ = init_tx.send(Ok(()));
            JournalActor::new(conn, receiver).run();
        });

        init_rx
            .await
            .map_err(|_| AuditJournalError::Open("init channel disconnected".to_string()))??;
        Ok(Self { sender })
    }

    /// Append a drafted entry (fire-and-forget).
    ///
    /// Returns immediately. The actor assigns `seq`, links `prev_hash`, and
    /// computes `hash`. On a saturated channel the draft is dropped with a warn.
    pub fn append(&self, draft: JournalEntryDraft) {
        match self
            .sender
            .try_send(JournalMessage::Append(Box::new(draft)))
        {
            Ok(()) => {}
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!("audit.journal.channel_full");
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                tracing::warn!("audit.journal.actor_disconnected");
            }
        }
    }

    /// Return all entries of a run, ordered by ascending `seq`.
    pub async fn query_run(&self, run_id: &str) -> Vec<JournalEntry> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        if self
            .sender
            .send(JournalMessage::QueryRun {
                run_id: run_id.to_string(),
                reply: reply_tx,
            })
            .await
            .is_err()
        {
            return Vec::new();
        }
        reply_rx.await.unwrap_or_default()
    }

    /// Return the last hash of a run, if the run has any entry.
    pub async fn last_hash(&self, run_id: &str) -> Option<String> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        if self
            .sender
            .send(JournalMessage::LastHash {
                run_id: run_id.to_string(),
                reply: reply_tx,
            })
            .await
            .is_err()
        {
            return None;
        }
        reply_rx.await.unwrap_or(None)
    }

    /// Send the shutdown signal and let the actor drain its queue.
    pub async fn shutdown(self) {
        let _ = self.sender.send(JournalMessage::Shutdown).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }
}
