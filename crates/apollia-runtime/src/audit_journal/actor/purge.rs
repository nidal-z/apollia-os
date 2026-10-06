//! Retention: removing the entries older than the organisation policy keeps.
//!
//! The journal has no erase function. The one removal it knows is this one: at
//! the request of the policy, the entries dated before a cutoff leave, and the
//! journal says so in an entry of its own, chained and signed like any other.
//! That entry carries the anchor of what was removed (the last global link,
//! and per run that keeps entries the last entry it lost), so verification
//! starts from it and anything altered before or after it still shows.
//!
//! What leaves is a beginning, never a middle: the global chain loses its
//! links up to the first entry that is not old enough, so the links kept stay
//! contiguous. The entry and the removal are one transaction.

use std::collections::BTreeMap;

use rusqlite::OptionalExtension as _;

use super::{row_to_entry, JournalActor};
use crate::audit_journal::error::AuditJournalError;
use crate::audit_journal::hash::SENTINEL_PREV_HASH;
use crate::audit_journal::verify::PurgeFacts;
use crate::governance::{
    GovernanceEvent, GovernanceRecord, PurgeAnchor, RunCut, JOURNAL_STATE_RUN_ID,
};

/// The rows a purge removes: the global links up to `?1`, and the rows that
/// predate the global chain when they are older than the cutoff `?2`.
///
/// Dates are compared on their first nineteen characters
/// (`YYYY-MM-DDTHH:MM:SS`): every entry is stamped in UTC, with or without a
/// fraction of a second, and a retention counts in days.
const PURGED: &str = "((global_seq IS NOT NULL AND global_seq <= ?1) \
     OR (global_seq IS NULL AND substr(ts, 1, 19) < ?2))";

/// The trigger that refuses a `DELETE`, as the schema creates it. A purge
/// lifts it for its own removal and puts it back before it commits.
const NO_DELETE_TRIGGER: &str = "CREATE TRIGGER IF NOT EXISTS aje_no_delete \
     BEFORE DELETE ON audit_journal_entries \
     BEGIN SELECT RAISE(ABORT, 'audit journal is append-only'); END;";

/// What a purge is asked with.
pub(crate) struct PurgeRequest {
    /// Days an entry is kept, as the policy sets them.
    pub(crate) retention_days: u32,
    /// Entries dated before this instant leave (RFC 3339 UTC).
    pub(crate) cutoff: String,
    /// Date of the purge entry (RFC 3339 UTC).
    pub(crate) ts: String,
}

/// What a purge removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurgeOutcome {
    /// How many entries were removed.
    pub removed: u64,
    /// Entries dated before this instant were removed (RFC 3339 UTC).
    pub cutoff: String,
}

fn sqlite(e: rusqlite::Error) -> AuditJournalError {
    AuditJournalError::Sqlite(e.to_string())
}

impl JournalActor {
    /// Removes the entries older than the cutoff of `request`, after the
    /// entry that says so. `None` when no entry is old enough: nothing is
    /// written.
    pub(super) fn purge(
        &mut self,
        request: &PurgeRequest,
    ) -> Result<Option<PurgeOutcome>, AuditJournalError> {
        let cutoff: String = request.cutoff.chars().take(19).collect();
        let last_removed = self.last_global_seq_older_than(&cutoff)?;
        let bounds = rusqlite::params![last_removed, cutoff];

        let removed: i64 = self
            .conn
            .query_row(
                &format!("SELECT COUNT(*) FROM audit_journal_entries WHERE {PURGED}"),
                bounds,
                |row| row.get(0),
            )
            .map_err(sqlite)?;
        if removed == 0 {
            return Ok(None);
        }

        let mut runs = self.cut_runs(last_removed, &cutoff)?;
        // The run of the purge entry itself: when all it held leaves, the
        // entry written now follows the last one removed.
        let state_run_kept: i64 = self
            .conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM audit_journal_entries \
                     WHERE run_id = ?3 AND NOT {PURGED}"
                ),
                rusqlite::params![last_removed, cutoff, JOURNAL_STATE_RUN_ID],
                |row| row.get(0),
            )
            .map_err(sqlite)?;
        if state_run_kept == 0 {
            if let Some(head) = self.head_for(JOURNAL_STATE_RUN_ID)? {
                runs.insert(
                    JOURNAL_STATE_RUN_ID.to_owned(),
                    RunCut {
                        seq: head.seq,
                        hash: head.hash.clone(),
                    },
                );
            }
        }

        let event = GovernanceEvent::JournalPurged {
            retention_days: request.retention_days,
            cutoff: request.cutoff.clone(),
            removed: removed as u64,
            anchor: self.removed_anchor(last_removed)?,
            runs,
        };
        let draft = GovernanceRecord::draft(&event, request.ts.clone())
            .map_err(|e| AuditJournalError::Serialize(e.to_string()))?;
        let (entry, link) = self.chain(draft).ok_or_else(|| {
            AuditJournalError::Sqlite("the head of the journal cannot be read".to_owned())
        })?;

        let tx = self.conn.transaction().map_err(sqlite)?;
        Self::insert_in(&tx, &entry, &link)?;
        tx.execute_batch("DROP TRIGGER IF EXISTS aje_no_delete;")
            .map_err(sqlite)?;
        tx.execute(
            &format!("DELETE FROM audit_journal_entries WHERE {PURGED}"),
            bounds,
        )
        .map_err(sqlite)?;
        tx.execute_batch(NO_DELETE_TRIGGER).map_err(sqlite)?;
        tx.commit().map_err(sqlite)?;

        // A run that lost everything starts again from the sentinel: the
        // heads kept in memory would chain it to an entry that is gone.
        self.heads.clear();
        self.advance(entry, link);
        tracing::event!(
            tracing::Level::INFO,
            removed,
            cutoff = %request.cutoff,
            retention_days = request.retention_days,
            "audit.journal.purged"
        );
        Ok(Some(PurgeOutcome {
            removed: removed as u64,
            cutoff: request.cutoff.clone(),
        }))
    }

    /// The last global link older than `cutoff` that no younger link
    /// precedes: every link up to it leaves. `-1` when the first link is not
    /// old enough, or the global chain is empty.
    fn last_global_seq_older_than(&self, cutoff: &str) -> Result<i64, AuditJournalError> {
        let first_kept: Option<i64> = self
            .conn
            .query_row(
                "SELECT MIN(global_seq) FROM audit_journal_entries \
                 WHERE global_seq IS NOT NULL AND substr(ts, 1, 19) >= ?1",
                rusqlite::params![cutoff],
                |row| row.get(0),
            )
            .map_err(sqlite)?;
        match first_kept {
            Some(first_kept) => Ok(first_kept - 1),
            None => {
                let head: Option<i64> = self
                    .conn
                    .query_row(
                        "SELECT MAX(global_seq) FROM audit_journal_entries \
                         WHERE global_seq IS NOT NULL",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(sqlite)?;
                Ok(head.unwrap_or(-1))
            }
        }
    }

    /// The anchor the purge entry carries: the last global link removed now,
    /// or, when this purge removes no global link, the one an earlier purge
    /// removed. `None` while the global chain starts at its origin.
    fn removed_anchor(&self, last_removed: i64) -> Result<Option<PurgeAnchor>, AuditJournalError> {
        let removed_now: Option<String> = self
            .conn
            .query_row(
                "SELECT global_hash FROM audit_journal_entries WHERE global_seq = ?1",
                rusqlite::params![last_removed],
                |row| row.get(0),
            )
            .optional()
            .map_err(sqlite)?;
        if let Some(global_hash) = removed_now {
            return Ok(Some(PurgeAnchor {
                global_seq: last_removed as u64,
                global_hash,
            }));
        }
        let first: Option<(i64, String)> = self
            .conn
            .query_row(
                "SELECT global_seq, global_prev_hash FROM audit_journal_entries \
                 WHERE global_seq IS NOT NULL ORDER BY global_seq ASC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sqlite)?;
        Ok(first
            .filter(|(global_seq, _)| *global_seq > 0)
            .map(|(global_seq, global_prev_hash)| PurgeAnchor {
                global_seq: (global_seq - 1) as u64,
                global_hash: global_prev_hash,
            }))
    }

    /// Per run that keeps entries and does not start at its origin once the
    /// purge is made: the last entry it lost, now or in an earlier purge.
    fn cut_runs(
        &self,
        last_removed: i64,
        cutoff: &str,
    ) -> Result<BTreeMap<String, RunCut>, AuditJournalError> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT e.run_id, e.seq, e.prev_hash FROM audit_journal_entries e \
                 JOIN (SELECT run_id, MIN(seq) AS first_seq FROM audit_journal_entries \
                       WHERE NOT {PURGED} GROUP BY run_id) kept \
                   ON kept.run_id = e.run_id AND kept.first_seq = e.seq \
                 WHERE e.prev_hash != ?3"
            ))
            .map_err(sqlite)?;
        let rows = stmt
            .query_map(
                rusqlite::params![last_removed, cutoff, SENTINEL_PREV_HASH],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .map_err(sqlite)?;
        let mut runs = BTreeMap::new();
        for row in rows {
            let (run_id, first_seq, prev_hash) = row.map_err(sqlite)?;
            runs.insert(
                run_id,
                RunCut {
                    seq: (first_seq - 1).max(0) as u64,
                    hash: prev_hash,
                },
            );
        }
        Ok(runs)
    }

    /// The last entry `run_id` lost to a purge, as the last purge entry of
    /// the journal says it. `None` for a run that starts at its origin.
    pub(super) fn run_cut(&self, run_id: &str) -> Result<Option<RunCut>, AuditJournalError> {
        let last_purge = self
            .conn
            .query_row(
                "SELECT seq, run_id, ts, kind, payload, prev_hash, hash, signature, signing_key_id \
                 FROM audit_journal_entries WHERE kind = 'journal_purged' \
                 ORDER BY global_seq DESC LIMIT 1",
                [],
                row_to_entry,
            )
            .optional()
            .map_err(sqlite)?;
        Ok(last_purge
            .as_ref()
            .and_then(PurgeFacts::of)
            .and_then(|facts| facts.runs.get(run_id).cloned()))
    }
}
