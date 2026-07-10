//! Property and formal harnesses for the global-chain tamper-evidence invariant.
//!
//! Central invariant: with the signing key uncompromised, removing or mutating
//! any single entry makes [`verify_journal`] fail, provided the caller supplies
//! the full head anchor (an off-machine export). The property test exercises it
//! over random chains; the `#[cfg(kani)]` harness proves it symbolically over a
//! bounded tamper index and is the seed for the exhaustive proof tracked in the
//! dedicated formal-verification effort.

use crate::audit_journal::anchor::{AnchorRow, GlobalRow};
use crate::audit_journal::entry::{JournalEntry, JournalEntryKind};
use crate::audit_journal::hash::{compute_entry_hash, compute_global_hash, SENTINEL_PREV_HASH};
use crate::audit_journal::signer::{HmacSigner, JournalSigner};
use crate::audit_journal::verify::verify_journal;

/// Build a fully-valid signed global chain of `n` entries in one run, plus the
/// head anchor at its terminal position.
fn build_chain(signer: &HmacSigner, n: usize) -> (Vec<GlobalRow>, Option<AnchorRow>) {
    let mut rows: Vec<GlobalRow> = Vec::new();
    let mut prev = SENTINEL_PREV_HASH.to_string();
    let mut global_prev = SENTINEL_PREV_HASH.to_string();
    for seq in 0..n {
        let mut entry = JournalEntry {
            seq: seq as u64,
            run_id: "run-1".to_string(),
            ts: "2026-01-01T00:00:00Z".to_string(),
            kind: JournalEntryKind::ToolCallStarted,
            payload: serde_json::json!({ "i": seq }),
            prev_hash: prev.clone(),
            hash: String::new(),
            signature: None,
            signing_key_id: None,
        };
        entry.hash = compute_entry_hash(&entry);
        entry.signature = signer.sign(entry.hash.as_bytes()).ok();
        entry.signing_key_id = Some(signer.key_id().to_string());

        let global_seq = seq as u64;
        let global_hash = compute_global_hash(&entry.hash, &global_prev, global_seq);
        let global_signature = signer.sign(global_hash.as_bytes()).ok();

        prev = entry.hash.clone();
        rows.push(GlobalRow {
            entry,
            global_seq,
            global_prev_hash: global_prev.clone(),
            global_hash: global_hash.clone(),
            global_signature,
            global_signing_key_id: Some(signer.key_id().to_string()),
        });
        global_prev = global_hash;
    }

    let anchor = rows.last().map(|r| AnchorRow {
        global_seq: r.global_seq,
        global_hash: r.global_hash.clone(),
        updated_ts: "2026-01-01T00:00:00Z".to_string(),
    });
    (rows, anchor)
}

#[cfg(test)]
mod property {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Against the full exported anchor, any single removal or payload
        /// mutation flips the whole-journal verdict to failing.
        #[test]
        fn prop_any_single_removal_or_mutation_breaks_global_verify(
            n in 1usize..=8,
            raw_idx in 0usize..8,
            mutate in any::<bool>(),
        ) {
            // GIVEN a valid signed chain and its exported head anchor
            let idx = raw_idx % n;
            let signer = HmacSigner::from_key_bytes(b"prop-key".to_vec()).unwrap();
            let (mut rows, anchor) = build_chain(&signer, n);
            prop_assert!(
                verify_journal(&rows, anchor.as_ref(), Some(&signer)).ok,
                "the pristine chain must verify"
            );

            // WHEN one entry is mutated or removed
            if mutate {
                rows[idx].entry.payload = serde_json::json!({ "tampered": idx });
            } else {
                rows.remove(idx);
            }

            // THEN whole-journal verification fails
            let report = verify_journal(&rows, anchor.as_ref(), Some(&signer));
            prop_assert!(!report.ok, "tampered chain must fail: {report:?}");
        }
    }
}

// SEED harness for `cargo kani`. Kani is not yet wired into the toolchain; this
// bounded proof (concrete hashes, symbolic tamper index) compiles and runs under
// `cargo kani` today and is the seed for the exhaustive symbolic proof tracked
// in the dedicated formal-verification effort.
#[cfg(kani)]
#[kani::proof]
fn kani_global_chain_tamper_evident() {
    const N: usize = 3;
    let signer = HmacSigner::from_key_bytes(b"kani-key".to_vec()).unwrap();
    let (mut rows, anchor) = build_chain(&signer, N);
    assert!(verify_journal(&rows, anchor.as_ref(), Some(&signer)).ok);

    let idx: usize = kani::any();
    kani::assume(idx < N);
    rows.remove(idx);
    assert!(!verify_journal(&rows, anchor.as_ref(), Some(&signer)).ok);
}
