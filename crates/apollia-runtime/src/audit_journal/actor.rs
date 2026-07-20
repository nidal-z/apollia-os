//! Background actor that exclusively owns the journal SQLite connection.
//!
//! Mirrors the existing audit-trail actor (`apollia_tools::audit`): a
//! `spawn_blocking` task driven by a bounded `mpsc` channel. The actor is the
//! single writer, so it can hold the per-run `(seq, last_hash)` state in memory
//! and guarantee a continuous chain without cross-task locking.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::audit_journal::anchor::{self, GlobalHead, GlobalLink};
use crate::audit_journal::entry::{JournalEntry, JournalEntryDraft, JournalEntryKind};
use crate::audit_journal::error::AuditJournalError;
use crate::audit_journal::hash::{compute_entry_hash, compute_global_hash, SENTINEL_PREV_HASH};
use crate::audit_journal::signer::JournalSigner;
use crate::audit_journal::verify::{
    verify_entries, verify_journal, JournalAnchor, VerifyChainReport, VerifyJournalReport,
};

/// SQL schema: the chained table, its index, the append-only triggers, and the
/// mutable head-anchor row.
///
/// `audit_journal_state` holds the terminal global head. It is deliberately not
/// protected by the append-only triggers: it is a single row updated in place.
/// Its trust comes from the global chain plus off-machine export, not from
/// immutability.
pub(crate) const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS audit_journal_entries (
        seq         INTEGER NOT NULL,
        run_id      TEXT    NOT NULL,
        ts          TEXT    NOT NULL,
        kind        TEXT    NOT NULL,
        payload     TEXT    NOT NULL,
        prev_hash   TEXT    NOT NULL,
        hash        TEXT    NOT NULL,
        PRIMARY KEY (run_id, seq)
    );
    CREATE INDEX IF NOT EXISTS idx_aje_run_id_seq
        ON audit_journal_entries(run_id, seq ASC);
    CREATE TRIGGER IF NOT EXISTS aje_no_update
        BEFORE UPDATE ON audit_journal_entries
        BEGIN SELECT RAISE(ABORT, 'audit journal is append-only'); END;
    CREATE TRIGGER IF NOT EXISTS aje_no_delete
        BEFORE DELETE ON audit_journal_entries
        BEGIN SELECT RAISE(ABORT, 'audit journal is append-only'); END;
    CREATE TABLE IF NOT EXISTS audit_journal_state (
        id          INTEGER PRIMARY KEY CHECK (id = 0),
        global_seq  INTEGER NOT NULL,
        global_hash TEXT    NOT NULL,
        updated_ts  TEXT    NOT NULL
    );
";

/// Signature columns added by a later migration. Each statement runs on its
/// own; the "duplicate column name" error is ignored to stay idempotent.
pub(crate) const MIGRATION_SIGNATURE_COLUMNS: &[&str] = &[
    "ALTER TABLE audit_journal_entries ADD COLUMN signature      TEXT",
    "ALTER TABLE audit_journal_entries ADD COLUMN signing_key_id TEXT",
];

/// Global-chain columns added by a later migration. Same idempotent idiom as
/// the signature columns. Rows predating this migration keep NULL global
/// columns and fall outside the global guarantee.
pub(crate) const MIGRATION_GLOBAL_COLUMNS: &[&str] = &[
    "ALTER TABLE audit_journal_entries ADD COLUMN global_seq            INTEGER",
    "ALTER TABLE audit_journal_entries ADD COLUMN global_prev_hash      TEXT",
    "ALTER TABLE audit_journal_entries ADD COLUMN global_hash           TEXT",
    "ALTER TABLE audit_journal_entries ADD COLUMN global_signature      TEXT",
    "ALTER TABLE audit_journal_entries ADD COLUMN global_signing_key_id TEXT",
];

/// The unique index on `global_seq`, applied after the columns exist (it
/// references a column added by [`MIGRATION_GLOBAL_COLUMNS`], so it cannot live
/// in [`SCHEMA`], which runs first).
pub(crate) const MIGRATION_GLOBAL_INDEX: &[&str] =
    &["CREATE UNIQUE INDEX IF NOT EXISTS idx_aje_global_seq \
     ON audit_journal_entries(global_seq) WHERE global_seq IS NOT NULL"];

/// Messages processed by the [`JournalActor`].
pub(crate) enum JournalMessage {
    /// Append a drafted entry (fire-and-forget).
    Append(Box<JournalEntryDraft>),
    /// Return all entries of a run ordered by ascending `seq`.
    QueryRun {
        run_id: String,
        reply: tokio::sync::oneshot::Sender<Vec<JournalEntry>>,
    },
    /// Return the last hash of a run, if any.
    LastHash {
        run_id: String,
        reply: tokio::sync::oneshot::Sender<Option<String>>,
    },
    /// Recompute and verify the chain and signatures of a run.
    VerifyChain {
        run_id: String,
        reply: tokio::sync::oneshot::Sender<VerifyChainReport>,
    },
    /// Verify the whole journal: the global chain, every per-run chain, and the
    /// head anchor.
    VerifyJournal {
        reply: tokio::sync::oneshot::Sender<VerifyJournalReport>,
    },
    /// Return the exportable head anchor of the global chain, if any.
    Anchor {
        reply: tokio::sync::oneshot::Sender<Option<JournalAnchor>>,
    },
    /// Return the distinct run ids present in the journal.
    ListRunIds {
        reply: tokio::sync::oneshot::Sender<Vec<String>>,
    },
    /// Stop the actor after draining the queue. The actor acks once every
    /// message enqueued before this one has been processed (FIFO), letting the
    /// caller await a deterministic drain instead of sleeping.
    Shutdown {
        ack: tokio::sync::oneshot::Sender<()>,
    },
}

/// In-memory chain head for a run: last used `seq` and last `hash`.
struct ChainHead {
    seq: u64,
    hash: String,
}

/// The journal actor. Never exposed directly; reached through the handle.
pub(crate) struct JournalActor {
    conn: rusqlite::Connection,
    receiver: tokio::sync::mpsc::Receiver<JournalMessage>,
    heads: HashMap<String, ChainHead>,
    global_head: Option<GlobalHead>,
    global_head_loaded: bool,
    signer: Option<Arc<dyn JournalSigner>>,
    warn_unsigned: bool,
}

impl JournalActor {
    /// Build the actor over an open connection.
    ///
    /// `signer` is `None` when no signing is requested or under the degraded
    /// warn-and-continue mode. `warn_unsigned` requests a per-append warning
    /// when an entry is persisted without a signature because of degradation
    /// (as opposed to signing simply being disabled).
    pub(crate) fn new(
        conn: rusqlite::Connection,
        receiver: tokio::sync::mpsc::Receiver<JournalMessage>,
        signer: Option<Arc<dyn JournalSigner>>,
        warn_unsigned: bool,
    ) -> Self {
        Self {
            conn,
            receiver,
            heads: HashMap::new(),
            global_head: None,
            global_head_loaded: false,
            signer,
            warn_unsigned,
        }
    }

    /// Main blocking loop, runs until [`JournalMessage::Shutdown`].
    pub(crate) fn run(mut self) {
        while let Some(msg) = self.receiver.blocking_recv() {
            match msg {
                JournalMessage::Append(draft) => self.handle_append(*draft),
                JournalMessage::QueryRun { run_id, reply } => {
                    let rows = self.query_run(&run_id).unwrap_or_default();
                    let _ = reply.send(rows);
                }
                JournalMessage::LastHash { run_id, reply } => {
                    let _ = reply.send(self.last_hash(&run_id));
                }
                JournalMessage::VerifyChain { run_id, reply } => {
                    let entries = self.query_run(&run_id).unwrap_or_default();
                    let report = verify_entries(
                        &run_id,
                        &entries,
                        self.signer.as_deref(),
                        self.signer.is_some(),
                    );
                    let _ = reply.send(report);
                }
                JournalMessage::VerifyJournal { reply } => {
                    let _ = reply.send(self.verify_journal_now());
                }
                JournalMessage::Anchor { reply } => {
                    let _ = reply.send(self.anchor_now());
                }
                JournalMessage::ListRunIds { reply } => {
                    let _ = reply.send(self.list_run_ids().unwrap_or_default());
                }
                JournalMessage::Shutdown { ack } => {
                    let _ = ack.send(());
                    break;
                }
            }
        }
    }

    /// Chain, hash, sign, and insert a drafted entry, advancing both the
    /// per-run chain and the global chain in one transaction.
    fn handle_append(&mut self, draft: JournalEntryDraft) {
        let head = match self.head_for(&draft.run_id) {
            Ok(head) => head,
            Err(e) => {
                tracing::error!(error = %e, run_id = %draft.run_id, "audit.journal.head_lookup_failed");
                return;
            }
        };
        let (seq, prev_hash) = match head {
            Some(h) => (h.seq + 1, h.hash.clone()),
            None => (0, SENTINEL_PREV_HASH.to_string()),
        };

        let mut entry = JournalEntry {
            seq,
            run_id: draft.run_id,
            ts: draft.ts,
            kind: draft.kind,
            payload: draft.payload,
            prev_hash,
            hash: String::new(),
            signature: None,
            signing_key_id: None,
        };
        entry.hash = compute_entry_hash(&entry);
        self.sign_entry(&mut entry);

        if let Err(e) = self.ensure_global_head() {
            tracing::error!(error = %e, run_id = %entry.run_id, "audit.journal.global_head_lookup_failed");
            return;
        }
        let (global_seq, global_prev_hash) = match &self.global_head {
            Some(h) => (h.global_seq + 1, h.global_hash.clone()),
            None => (0, SENTINEL_PREV_HASH.to_string()),
        };
        let global_hash = compute_global_hash(&entry.hash, &global_prev_hash, global_seq);
        let (global_signature, global_signing_key_id) = self.sign_global(&global_hash, &entry);
        let link = GlobalLink {
            global_seq,
            global_prev_hash,
            global_hash: global_hash.clone(),
            global_signature,
            global_signing_key_id,
        };

        if let Err(e) = self.insert(&entry, &link) {
            tracing::error!(
                error = %e,
                run_id = %entry.run_id,
                seq = entry.seq,
                "audit.journal.insert_failed"
            );
            return;
        }

        self.heads.insert(
            entry.run_id.clone(),
            ChainHead {
                seq: entry.seq,
                hash: entry.hash,
            },
        );
        self.global_head = Some(GlobalHead {
            global_seq,
            global_hash,
        });
    }

    /// Sign an entry over its `hash` if a signer is configured.
    ///
    /// On a signing failure or absent signer the entry stays unsigned; a
    /// per-append warning is emitted only when degradation was requested.
    fn sign_entry(&self, entry: &mut JournalEntry) {
        match &self.signer {
            Some(signer) => match signer.sign(entry.hash.as_bytes()) {
                Ok(sig) => {
                    entry.signature = Some(sig);
                    entry.signing_key_id = Some(signer.key_id().to_string());
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        run_id = %entry.run_id,
                        seq = entry.seq,
                        "audit.journal.sign_failed"
                    );
                }
            },
            None if self.warn_unsigned => {
                tracing::warn!(
                    run_id = %entry.run_id,
                    seq = entry.seq,
                    "audit.journal.unsigned_append"
                );
            }
            None => {}
        }
    }

    /// Sign a global link over its `global_hash` if a signer is configured.
    ///
    /// Returns the signature and key id, or `(None, None)` when unsigned. The
    /// degraded-mode warning is emitted once per append by [`Self::sign_entry`],
    /// so this does not warn again.
    fn sign_global(
        &self,
        global_hash: &str,
        entry: &JournalEntry,
    ) -> (Option<String>, Option<String>) {
        match &self.signer {
            Some(signer) => match signer.sign(global_hash.as_bytes()) {
                Ok(sig) => (Some(sig), Some(signer.key_id().to_string())),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        run_id = %entry.run_id,
                        seq = entry.seq,
                        "audit.journal.global_sign_failed"
                    );
                    (None, None)
                }
            },
            None => (None, None),
        }
    }

    /// Load the global-chain head once from the entries table (source of
    /// truth), never from the mutable state row.
    fn ensure_global_head(&mut self) -> Result<(), AuditJournalError> {
        if !self.global_head_loaded {
            self.global_head = anchor::load_global_head(&self.conn)?;
            self.global_head_loaded = true;
        }
        Ok(())
    }

    /// Verify the whole journal from a consistent snapshot: the single-writer
    /// loop guarantees the previous append has committed before this runs.
    ///
    /// A configured signer means every entry is expected to be signed, so
    /// verification requires signatures: stripping them off a recomputed chain
    /// fails instead of silently passing.
    fn verify_journal_now(&self) -> VerifyJournalReport {
        let rows = anchor::query_global_chain(&self.conn).unwrap_or_default();
        let anchor_row = anchor::load_anchor(&self.conn).ok().flatten();
        verify_journal(
            &rows,
            anchor_row.as_ref(),
            self.signer.as_deref(),
            self.signer.is_some(),
        )
    }

    /// Build the exportable head anchor from the persisted state row plus the
    /// signer's key id.
    fn anchor_now(&self) -> Option<JournalAnchor> {
        match anchor::load_anchor(&self.conn) {
            Ok(Some(a)) => Some(JournalAnchor {
                global_seq: a.global_seq,
                global_hash: a.global_hash,
                key_id: self.signer.as_ref().map(|s| s.key_id().to_string()),
                updated_ts: a.updated_ts,
            }),
            _ => None,
        }
    }

    /// Return the cached head for a run, loading it from SQLite on first use.
    fn head_for(&mut self, run_id: &str) -> Result<Option<&ChainHead>, AuditJournalError> {
        if !self.heads.contains_key(run_id) {
            if let Some(head) = self.load_head(run_id)? {
                self.heads.insert(run_id.to_string(), head);
            } else {
                return Ok(None);
            }
        }
        Ok(self.heads.get(run_id))
    }

    /// Query the persisted chain head for a run.
    fn load_head(&self, run_id: &str) -> Result<Option<ChainHead>, AuditJournalError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT seq, hash FROM audit_journal_entries \
                 WHERE run_id = ?1 ORDER BY seq DESC LIMIT 1",
            )
            .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?;
        let mut rows = stmt
            .query(rusqlite::params![run_id])
            .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?;
        match rows
            .next()
            .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?
        {
            Some(row) => {
                let seq: i64 = row
                    .get(0)
                    .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?;
                let hash: String = row
                    .get(1)
                    .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?;
                Ok(Some(ChainHead {
                    seq: seq as u64,
                    hash,
                }))
            }
            None => Ok(None),
        }
    }

    /// Insert one fully-chained entry and advance the head anchor atomically.
    ///
    /// The entry insert and the state upsert run in one transaction (RAII
    /// rollback on any early return), so a crash can never leave the anchor
    /// ahead of or behind the entries table.
    fn insert(&mut self, e: &JournalEntry, link: &GlobalLink) -> Result<(), AuditJournalError> {
        let payload = serde_json::to_string(&e.payload)
            .map_err(|err| AuditJournalError::Serialize(err.to_string()))?;
        let tx = self
            .conn
            .transaction()
            .map_err(|err| AuditJournalError::Sqlite(err.to_string()))?;
        tx.execute(
            "INSERT INTO audit_journal_entries \
             (seq, run_id, ts, kind, payload, prev_hash, hash, signature, signing_key_id, \
              global_seq, global_prev_hash, global_hash, global_signature, global_signing_key_id) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            rusqlite::params![
                e.seq as i64,
                e.run_id,
                e.ts,
                e.kind.tag(),
                payload,
                e.prev_hash,
                e.hash,
                e.signature,
                e.signing_key_id,
                link.global_seq as i64,
                link.global_prev_hash,
                link.global_hash,
                link.global_signature,
                link.global_signing_key_id,
            ],
        )
        .map_err(|err| AuditJournalError::Sqlite(err.to_string()))?;
        anchor::upsert_state(&tx, link.global_seq, &link.global_hash, &e.ts)?;
        tx.commit()
            .map_err(|err| AuditJournalError::Sqlite(err.to_string()))?;
        Ok(())
    }

    /// Return the distinct run ids present in the journal, ordered.
    fn list_run_ids(&self) -> Result<Vec<String>, AuditJournalError> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT run_id FROM audit_journal_entries ORDER BY run_id ASC")
            .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?;
        let mut ids = Vec::new();
        for row in rows {
            ids.push(row.map_err(|e| AuditJournalError::Sqlite(e.to_string()))?);
        }
        Ok(ids)
    }

    /// Read all entries of a run, ordered by ascending `seq`.
    fn query_run(&self, run_id: &str) -> Result<Vec<JournalEntry>, AuditJournalError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT seq, run_id, ts, kind, payload, prev_hash, hash, signature, signing_key_id \
                 FROM audit_journal_entries WHERE run_id = ?1 ORDER BY seq ASC",
            )
            .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?;
        let rows = stmt
            .query_map(rusqlite::params![run_id], |row| {
                let payload_str: String = row.get(4)?;
                let payload: Value = serde_json::from_str(&payload_str).unwrap_or(Value::Null);
                let kind_tag: String = row.get(3)?;
                Ok(JournalEntry {
                    seq: row.get::<_, i64>(0)? as u64,
                    run_id: row.get(1)?,
                    ts: row.get(2)?,
                    kind: JournalEntryKind::from_tag(&kind_tag),
                    payload,
                    prev_hash: row.get(5)?,
                    hash: row.get(6)?,
                    signature: row.get(7)?,
                    signing_key_id: row.get(8)?,
                })
            })
            .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| AuditJournalError::Sqlite(e.to_string()))
    }

    /// Last hash of a run, from the cache or SQLite.
    fn last_hash(&mut self, run_id: &str) -> Option<String> {
        match self.head_for(run_id) {
            Ok(head) => head.map(|h| h.hash.clone()),
            Err(e) => {
                tracing::error!(error = %e, run_id = %run_id, "audit.journal.last_hash_failed");
                None
            }
        }
    }
}
