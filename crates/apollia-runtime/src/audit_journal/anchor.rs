//! Global chain: cross-run tamper-evidence and the exportable head anchor.
//!
//! The per-run chains ([`crate::audit_journal::hash::compute_entry_hash`]) make
//! each run internally tamper-evident but leave two holes: truncating the tail
//! of a run, and deleting a whole run, both leave a shorter valid chain. The
//! global chain closes them by weaving every entry across all runs into one
//! monotone sequence (`global_seq`) linked by `global_prev_hash`, on top of the
//! existing per-run `hash`. A gap in `global_seq` or a broken global link then
//! reveals any interior deletion or whole-run deletion.
//!
//! The terminal `(global_seq, global_hash)` is the head anchor. It is cached in
//! the mutable `audit_journal_state` row (a rollback-able in-database signal)
//! and can be exported off-machine, which is the only defense against
//! truncation of the global tail once the signing key is compromised.

use rusqlite::{Connection, Transaction};

use crate::audit_journal::entry::{JournalEntry, JournalEntryKind};
use crate::audit_journal::error::AuditJournalError;

/// In-memory head of the global chain: last used `global_seq` and its hash.
pub(crate) struct GlobalHead {
    pub(crate) global_seq: u64,
    pub(crate) global_hash: String,
}

/// The global-chain link computed for a fresh entry, written alongside it.
pub(crate) struct GlobalLink {
    pub(crate) global_seq: u64,
    pub(crate) global_prev_hash: String,
    pub(crate) global_hash: String,
    pub(crate) global_signature: Option<String>,
    pub(crate) global_signing_key_id: Option<String>,
}

/// A persisted entry together with its global-chain link, read back for
/// whole-journal verification.
pub(crate) struct GlobalRow {
    pub(crate) entry: JournalEntry,
    pub(crate) global_seq: u64,
    pub(crate) global_prev_hash: String,
    pub(crate) global_hash: String,
    pub(crate) global_signature: Option<String>,
    pub(crate) global_signing_key_id: Option<String>,
}

/// The persisted head anchor, as stored in `audit_journal_state`.
pub(crate) struct AnchorRow {
    pub(crate) global_seq: u64,
    pub(crate) global_hash: String,
    pub(crate) updated_ts: String,
}

/// Load the global-chain head from the entries table (the source of truth),
/// never from the mutable state row.
pub(crate) fn load_global_head(conn: &Connection) -> Result<Option<GlobalHead>, AuditJournalError> {
    let mut stmt = conn
        .prepare(
            "SELECT global_seq, global_hash FROM audit_journal_entries \
             WHERE global_seq IS NOT NULL ORDER BY global_seq DESC LIMIT 1",
        )
        .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?;
    let mut rows = stmt
        .query([])
        .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?;
    match rows
        .next()
        .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?
    {
        Some(row) => {
            let global_seq: i64 = row
                .get(0)
                .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?;
            let global_hash: String = row
                .get(1)
                .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?;
            Ok(Some(GlobalHead {
                global_seq: global_seq as u64,
                global_hash,
            }))
        }
        None => Ok(None),
    }
}

/// Read every globally-chained entry, ordered by ascending `global_seq`.
///
/// Rows predating the global-chain migration have a `NULL` `global_seq` and are
/// excluded: they fall outside the global guarantee and their per-run chain is
/// verified independently.
pub(crate) fn query_global_chain(conn: &Connection) -> Result<Vec<GlobalRow>, AuditJournalError> {
    let mut stmt = conn
        .prepare(
            "SELECT seq, run_id, ts, kind, payload, prev_hash, hash, signature, signing_key_id, \
             global_seq, global_prev_hash, global_hash, global_signature, global_signing_key_id \
             FROM audit_journal_entries \
             WHERE global_seq IS NOT NULL ORDER BY global_seq ASC",
        )
        .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?;
    let rows = stmt
        .query_map([], |row| {
            let payload_str: String = row.get(4)?;
            let payload = serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);
            let kind_tag: String = row.get(3)?;
            let entry = JournalEntry {
                seq: row.get::<_, i64>(0)? as u64,
                run_id: row.get(1)?,
                ts: row.get(2)?,
                kind: JournalEntryKind::from_tag(&kind_tag),
                payload,
                prev_hash: row.get(5)?,
                hash: row.get(6)?,
                signature: row.get(7)?,
                signing_key_id: row.get(8)?,
            };
            Ok(GlobalRow {
                entry,
                global_seq: row.get::<_, i64>(9)? as u64,
                global_prev_hash: row.get(10)?,
                global_hash: row.get(11)?,
                global_signature: row.get(12)?,
                global_signing_key_id: row.get(13)?,
            })
        })
        .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| AuditJournalError::Sqlite(e.to_string()))
}

/// Advance the persisted head anchor inside the append transaction.
///
/// The state row is a single mutable row (`id = 0`); it is deliberately not
/// protected by the append-only triggers because it must be updated in place.
/// Its trust comes from the global chain plus off-machine export, not from
/// immutability.
pub(crate) fn upsert_state(
    tx: &Transaction<'_>,
    global_seq: u64,
    global_hash: &str,
    updated_ts: &str,
) -> Result<(), AuditJournalError> {
    tx.execute(
        "INSERT INTO audit_journal_state (id, global_seq, global_hash, updated_ts) \
         VALUES (0, ?1, ?2, ?3) \
         ON CONFLICT(id) DO UPDATE SET \
             global_seq = excluded.global_seq, \
             global_hash = excluded.global_hash, \
             updated_ts = excluded.updated_ts",
        rusqlite::params![global_seq as i64, global_hash, updated_ts],
    )
    .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?;
    Ok(())
}

/// Read the persisted head anchor, if the state row exists.
pub(crate) fn load_anchor(conn: &Connection) -> Result<Option<AnchorRow>, AuditJournalError> {
    let mut stmt = conn
        .prepare("SELECT global_seq, global_hash, updated_ts FROM audit_journal_state WHERE id = 0")
        .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?;
    let mut rows = stmt
        .query([])
        .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?;
    match rows
        .next()
        .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?
    {
        Some(row) => {
            let global_seq: i64 = row
                .get(0)
                .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?;
            Ok(Some(AnchorRow {
                global_seq: global_seq as u64,
                global_hash: row
                    .get(1)
                    .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?,
                updated_ts: row
                    .get(2)
                    .map_err(|e| AuditJournalError::Sqlite(e.to_string()))?,
            }))
        }
        None => Ok(None),
    }
}
