//! Hash-chain and signature verification of a run's journal.
//!
//! [`verify_entries`] is the pure verification core: it recomputes each entry
//! hash, checks the `prev_hash` linkage, and (when a signer is provided)
//! verifies the signature. It is reused by the actor (with its own signer) and
//! by tests or offline tools that supply an arbitrary signer.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::audit_journal::anchor::{AnchorRow, GlobalRow};
use crate::audit_journal::entry::JournalEntry;
use crate::audit_journal::hash::{compute_entry_hash, compute_global_hash, SENTINEL_PREV_HASH};
use crate::audit_journal::signer::JournalSigner;

/// Reason a chain link failed verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrokenLinkReason {
    /// The recomputed hash differs from the stored one (content was mutated).
    HashMismatch,
    /// The `prev_hash` does not match the previous entry (insert, delete, or
    /// reorder).
    PrevHashMismatch,
    /// The signature did not verify under the active key.
    SignatureInvalid,
    /// The entry was signed by a key the verifier does not hold.
    UnknownSigningKey,
}

/// The first broken link found while walking a chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BrokenLink {
    /// Sequence number of the offending entry.
    pub seq: u64,
    /// Why the link is considered broken.
    pub reason: BrokenLinkReason,
}

/// Outcome of verifying a run's chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct VerifyChainReport {
    /// `true` when the whole chain verified.
    pub ok: bool,
    /// Run that was verified.
    pub run_id: String,
    /// Number of entries inspected (all of them when `ok`).
    pub entries_checked: u64,
    /// The first broken link, or `None` when `ok`.
    pub first_broken_link: Option<BrokenLink>,
}

/// Verifies a run's entries (assumed ordered by ascending `seq`).
///
/// Walks the chain from the sentinel, recomputing each hash and checking the
/// `prev_hash` linkage. When `signer` is provided and an entry carries a
/// signature, the signature is verified; an entry signed by a different key id
/// yields [`BrokenLinkReason::UnknownSigningKey`]. Stops at and reports the
/// first broken link.
pub fn verify_entries(
    run_id: &str,
    entries: &[JournalEntry],
    signer: Option<&dyn JournalSigner>,
) -> VerifyChainReport {
    let mut expected_prev = SENTINEL_PREV_HASH.to_string();
    let mut checked = 0u64;

    for entry in entries {
        checked += 1;

        if compute_entry_hash(entry) != entry.hash {
            return broken(run_id, checked, entry.seq, BrokenLinkReason::HashMismatch);
        }
        if entry.prev_hash != expected_prev {
            return broken(
                run_id,
                checked,
                entry.seq,
                BrokenLinkReason::PrevHashMismatch,
            );
        }
        if let (Some(signer), Some(sig)) = (signer, entry.signature.as_deref()) {
            match entry.signing_key_id.as_deref() {
                Some(kid) if kid == signer.key_id() => {
                    if !signature_matches(signer, entry.hash.as_bytes(), sig) {
                        return broken(
                            run_id,
                            checked,
                            entry.seq,
                            BrokenLinkReason::SignatureInvalid,
                        );
                    }
                }
                _ => {
                    return broken(
                        run_id,
                        checked,
                        entry.seq,
                        BrokenLinkReason::UnknownSigningKey,
                    );
                }
            }
        }

        expected_prev = entry.hash.clone();
    }

    VerifyChainReport {
        ok: true,
        run_id: run_id.to_string(),
        entries_checked: checked,
        first_broken_link: None,
    }
}

/// Re-signs `payload` with `signer` and compares to `expected` in constant
/// time. Works for any [`JournalSigner`] since signing is deterministic.
fn signature_matches(signer: &dyn JournalSigner, payload: &[u8], expected: &str) -> bool {
    match signer.sign(payload) {
        Ok(actual) => constant_time_eq::constant_time_eq(actual.as_bytes(), expected.as_bytes()),
        Err(_) => false,
    }
}

/// Builds a failing report for a broken link.
fn broken(run_id: &str, checked: u64, seq: u64, reason: BrokenLinkReason) -> VerifyChainReport {
    VerifyChainReport {
        ok: false,
        run_id: run_id.to_string(),
        entries_checked: checked,
        first_broken_link: Some(BrokenLink { seq, reason }),
    }
}

/// Reason a global-chain link failed whole-journal verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum JournalBreakReason {
    /// The `global_seq` sequence is not contiguous from zero: an entry was
    /// deleted or a whole run was removed from the interior of the chain.
    GlobalSeqGap,
    /// The `global_prev_hash` does not link to the previous global link.
    GlobalPrevHashMismatch,
    /// The recomputed global hash differs from the stored one.
    GlobalHashMismatch,
    /// The global signature did not verify under the active key.
    GlobalSignatureInvalid,
    /// The global link was signed by a key the verifier does not hold.
    UnknownSigningKey,
    /// The entry's own per-run chain failed verification (see the per-run
    /// report for the exact reason).
    PerRunBroken,
}

/// The first broken link found while walking the global chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct JournalBreak {
    /// Global sequence number of the offending entry.
    pub global_seq: u64,
    /// Run the offending entry belongs to.
    pub run_id: String,
    /// Why the link is considered broken.
    pub reason: JournalBreakReason,
}

/// Outcome of verifying the whole journal across all runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct VerifyJournalReport {
    /// `true` when the global chain, every per-run chain, and the head anchor
    /// all verified.
    pub ok: bool,
    /// Number of globally-chained entries inspected.
    pub entries_checked: u64,
    /// Number of distinct runs covered by the global chain.
    pub runs_checked: u64,
    /// The first broken link, or `None` when the chain is intact.
    pub first_break: Option<JournalBreak>,
    /// Whether the terminal global head matches the persisted state anchor. A
    /// `false` here with no `first_break` signals global-tail truncation or a
    /// rolled-back state row (detectable in-database only until the key is
    /// compromised; export the anchor off-machine for a durable guarantee).
    pub head_matches_state: bool,
}

/// The exportable head anchor of the global chain.
///
/// Printing and storing this off-machine is the only defense against truncation
/// of the global tail once the signing key can be compromised.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct JournalAnchor {
    /// Highest committed global sequence number.
    pub global_seq: u64,
    /// Global hash at that position: the head of the chain.
    pub global_hash: String,
    /// Signing key id in force, when the journal is signed.
    pub key_id: Option<String>,
    /// When the anchor was last advanced (RFC3339).
    pub updated_ts: String,
}

/// Verifies the whole journal: the global chain, every per-run chain, and the
/// head anchor.
///
/// `rows` are the globally-chained entries ordered by ascending `global_seq`
/// (rows predating the global-chain migration are excluded upstream). Walks the
/// global chain checking `global_seq` contiguity from zero, `global_prev_hash`
/// linkage, the recomputed global hash, and the global signature; verifies each
/// run's per-run chain via [`verify_entries`]; and compares the terminal global
/// head to `anchor`. Stops at and reports the first global break.
pub(crate) fn verify_journal(
    rows: &[GlobalRow],
    anchor: Option<&AnchorRow>,
    signer: Option<&dyn JournalSigner>,
) -> VerifyJournalReport {
    let mut expected_prev = SENTINEL_PREV_HASH.to_string();
    let mut checked = 0u64;
    let mut run_ids: BTreeSet<&str> = BTreeSet::new();
    let mut per_run: BTreeMap<&str, Vec<&JournalEntry>> = BTreeMap::new();

    for (index, row) in rows.iter().enumerate() {
        checked += 1;
        run_ids.insert(row.entry.run_id.as_str());

        if row.global_seq != index as u64 {
            return journal_broken(row, checked, JournalBreakReason::GlobalSeqGap);
        }
        let recomputed =
            compute_global_hash(&row.entry.hash, &row.global_prev_hash, row.global_seq);
        if recomputed != row.global_hash {
            return journal_broken(row, checked, JournalBreakReason::GlobalHashMismatch);
        }
        if row.global_prev_hash != expected_prev {
            return journal_broken(row, checked, JournalBreakReason::GlobalPrevHashMismatch);
        }
        if let (Some(signer), Some(sig)) = (signer, row.global_signature.as_deref()) {
            match row.global_signing_key_id.as_deref() {
                Some(kid) if kid == signer.key_id() => {
                    if !signature_matches(signer, row.global_hash.as_bytes(), sig) {
                        return journal_broken(
                            row,
                            checked,
                            JournalBreakReason::GlobalSignatureInvalid,
                        );
                    }
                }
                _ => {
                    return journal_broken(row, checked, JournalBreakReason::UnknownSigningKey);
                }
            }
        }

        expected_prev = row.global_hash.clone();
        per_run
            .entry(row.entry.run_id.as_str())
            .or_default()
            .push(&row.entry);
    }

    for (run_id, entries) in &per_run {
        let mut owned: Vec<JournalEntry> = entries.iter().map(|e| (*e).clone()).collect();
        owned.sort_by_key(|e| e.seq);
        let report = verify_entries(run_id, &owned, signer);
        if !report.ok {
            let seq = report.first_broken_link.as_ref().map_or(0, |b| b.seq);
            let global_seq = rows
                .iter()
                .find(|r| r.entry.run_id == **run_id && r.entry.seq == seq)
                .map_or(0, |r| r.global_seq);
            return VerifyJournalReport {
                ok: false,
                entries_checked: checked,
                runs_checked: run_ids.len() as u64,
                first_break: Some(JournalBreak {
                    global_seq,
                    run_id: (*run_id).to_string(),
                    reason: JournalBreakReason::PerRunBroken,
                }),
                head_matches_state: false,
            };
        }
    }

    let terminal = rows.last().map(|r| (r.global_seq, r.global_hash.as_str()));
    let head_matches_state = match (terminal, anchor) {
        (None, None) => true,
        (Some((gs, gh)), Some(a)) => gs == a.global_seq && gh == a.global_hash,
        _ => false,
    };

    VerifyJournalReport {
        ok: head_matches_state,
        entries_checked: checked,
        runs_checked: run_ids.len() as u64,
        first_break: None,
        head_matches_state,
    }
}

/// Builds a failing whole-journal report for a broken global link.
fn journal_broken(
    row: &GlobalRow,
    checked: u64,
    reason: JournalBreakReason,
) -> VerifyJournalReport {
    VerifyJournalReport {
        ok: false,
        entries_checked: checked,
        runs_checked: 0,
        first_break: Some(JournalBreak {
            global_seq: row.global_seq,
            run_id: row.entry.run_id.clone(),
            reason,
        }),
        head_matches_state: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit_journal::entry::JournalEntryKind;
    use crate::audit_journal::signer::HmacSigner;

    fn signed_chain(signer: &HmacSigner, n: u64) -> Vec<JournalEntry> {
        let mut entries = Vec::new();
        let mut prev = SENTINEL_PREV_HASH.to_string();
        for seq in 0..n {
            let mut e = JournalEntry {
                seq,
                run_id: "run-1".to_string(),
                ts: "2026-01-01T00:00:00Z".to_string(),
                kind: JournalEntryKind::ToolCallStarted,
                payload: serde_json::json!({ "i": seq }),
                prev_hash: prev.clone(),
                hash: String::new(),
                signature: None,
                signing_key_id: None,
            };
            e.hash = compute_entry_hash(&e);
            e.signature = Some(signer.sign(e.hash.as_bytes()).expect("sign"));
            e.signing_key_id = Some(signer.key_id().to_string());
            prev = e.hash.clone();
            entries.push(e);
        }
        entries
    }

    // A pristine signed chain verifies
    #[test]
    fn test_intact_chain_ok() {
        // GIVEN a 3-entry signed chain
        let s = HmacSigner::from_key_bytes(b"k1".to_vec()).unwrap();
        let entries = signed_chain(&s, 3);
        // WHEN verified with the same signer
        let report = verify_entries("run-1", &entries, Some(&s));
        // THEN it is ok and all entries were checked
        assert!(report.ok);
        assert_eq!(report.entries_checked, 3);
        assert!(report.first_broken_link.is_none());
    }

    // A mutated payload without rehash breaks on hash
    #[test]
    fn test_payload_mutation_breaks_hash() {
        // GIVEN a chain whose middle entry payload is altered
        let s = HmacSigner::from_key_bytes(b"k1".to_vec()).unwrap();
        let mut entries = signed_chain(&s, 3);
        entries[1].payload = serde_json::json!({ "i": 999 });
        // WHEN verified
        let report = verify_entries("run-1", &entries, Some(&s));
        // THEN the broken link is the hash mismatch at seq 1
        assert!(!report.ok);
        let link = report.first_broken_link.expect("link");
        assert_eq!(link.seq, 1);
        assert_eq!(link.reason, BrokenLinkReason::HashMismatch);
    }

    // Verifying with the wrong key fails on signature
    #[test]
    fn test_wrong_key_breaks_signature() {
        // GIVEN a chain signed with K1
        let k1 = HmacSigner::from_key_bytes(b"k1".to_vec()).unwrap();
        let k2 = HmacSigner::from_key_bytes(b"k2".to_vec()).unwrap();
        let entries = signed_chain(&k1, 2);
        // WHEN verified with K2
        let report = verify_entries("run-1", &entries, Some(&k2));
        // THEN the first link is an unknown signing key (K2 != K1 id)
        assert!(!report.ok);
        let link = report.first_broken_link.expect("link");
        assert_eq!(link.seq, 0);
        assert_eq!(link.reason, BrokenLinkReason::UnknownSigningKey);
    }

    // A chain mixing an LlmCompletion and PlanMutation entries still verifies;
    // verify treats the PlanMutation payload opaquely (no kind-specific branch).
    #[test]
    fn test_chain_verifies_with_plan_entries() {
        // GIVEN a chain of an Unknown entry then two PlanMutation entries
        let s = HmacSigner::from_key_bytes(b"k1".to_vec()).unwrap();
        let mut entries = Vec::new();
        let mut prev = SENTINEL_PREV_HASH.to_string();
        let kinds = [
            JournalEntryKind::LlmCompletion,
            JournalEntryKind::PlanMutation,
            JournalEntryKind::PlanMutation,
        ];
        for (seq, kind) in kinds.into_iter().enumerate() {
            let mut e = JournalEntry {
                seq: seq as u64,
                run_id: "run-1".to_string(),
                ts: "2026-01-01T00:00:00Z".to_string(),
                kind,
                payload: serde_json::json!({ "ordinal": seq }),
                prev_hash: prev.clone(),
                hash: String::new(),
                signature: None,
                signing_key_id: None,
            };
            e.hash = compute_entry_hash(&e);
            e.signature = Some(s.sign(e.hash.as_bytes()).expect("sign"));
            e.signing_key_id = Some(s.key_id().to_string());
            prev = e.hash.clone();
            entries.push(e);
        }

        // WHEN verified
        let report = verify_entries("run-1", &entries, Some(&s));

        // THEN the whole mixed chain is intact
        assert!(report.ok);
        assert_eq!(report.entries_checked, 3);
        assert!(report.first_broken_link.is_none());
    }

    // Deleting a middle entry breaks the prev_hash linkage
    #[test]
    fn test_deletion_breaks_prev_hash() {
        // GIVEN a chain with its middle entry removed
        let s = HmacSigner::from_key_bytes(b"k1".to_vec()).unwrap();
        let mut entries = signed_chain(&s, 3);
        entries.remove(1);
        // WHEN verified
        let report = verify_entries("run-1", &entries, Some(&s));
        // THEN the link after the gap fails on prev_hash
        assert!(!report.ok);
        let link = report.first_broken_link.expect("link");
        assert_eq!(link.reason, BrokenLinkReason::PrevHashMismatch);
    }
}
