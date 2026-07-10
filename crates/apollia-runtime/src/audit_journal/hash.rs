//! Deterministic canonical serialization and SHA256 entry hashing.
//!
//! The hash input is a canonical JSON encoding of the entry content fields with
//! object keys sorted lexicographically at every depth. The encoder is written
//! explicitly rather than delegating to `serde_json` so the result never
//! depends on a transitive feature (such as `preserve_order`): tamper-evidence
//! must not hinge on a build-time flag.

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::audit_journal::entry::JournalEntry;

/// `prev_hash` of the first entry of any run: 64 ASCII zeros.
pub const SENTINEL_PREV_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// Computes the canonical SHA256 hash of an entry.
///
/// The canonical form covers `kind`, `payload`, `prev_hash`, `run_id`, `seq`,
/// and `ts` (keys sorted), then the `prev_hash` bytes are appended again before
/// hashing, per the journal specification. The `hash` field itself is excluded,
/// so the same function recomputes the hash during verification from a stored
/// entry. Returns the lowercase hex digest.
pub fn compute_entry_hash(entry: &JournalEntry) -> String {
    let mut canonical = String::new();
    canonical.push('{');
    push_str_field(&mut canonical, "kind", entry.kind.tag());
    canonical.push(',');
    push_key(&mut canonical, "payload");
    write_canonical_value(&entry.payload, &mut canonical);
    canonical.push(',');
    push_str_field(&mut canonical, "prev_hash", &entry.prev_hash);
    canonical.push(',');
    push_str_field(&mut canonical, "run_id", &entry.run_id);
    canonical.push(',');
    push_key(&mut canonical, "seq");
    canonical.push_str(&entry.seq.to_string());
    canonical.push(',');
    push_str_field(&mut canonical, "ts", &entry.ts);
    canonical.push('}');

    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    hasher.update(entry.prev_hash.as_bytes());
    hex::encode(hasher.finalize())
}

/// Computes the SHA256 hash of a global-chain link.
///
/// The global chain weaves every entry across all runs into one monotone
/// sequence, on top of the per-run chains. A link commits the entry's own
/// `entry_hash`, its position `global_seq`, and the `global_prev_hash` of the
/// preceding link (the sentinel for the genesis link at `global_seq == 0`). The
/// canonical form is `{ "entry_hash":..., "global_prev_hash":..., "global_seq":n }`
/// (keys sorted), then the `global_prev_hash` bytes are appended again before
/// hashing, mirroring [`compute_entry_hash`]. Returns the lowercase hex digest.
pub fn compute_global_hash(entry_hash: &str, global_prev_hash: &str, global_seq: u64) -> String {
    let mut canonical = String::new();
    canonical.push('{');
    push_str_field(&mut canonical, "entry_hash", entry_hash);
    canonical.push(',');
    push_str_field(&mut canonical, "global_prev_hash", global_prev_hash);
    canonical.push(',');
    push_key(&mut canonical, "global_seq");
    canonical.push_str(&global_seq.to_string());
    canonical.push('}');

    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    hasher.update(global_prev_hash.as_bytes());
    hex::encode(hasher.finalize())
}

/// Writes a JSON object key followed by a colon.
fn push_key(out: &mut String, key: &str) {
    push_json_string(out, key);
    out.push(':');
}

/// Writes a `"key":"value"` pair (value JSON-escaped).
fn push_str_field(out: &mut String, key: &str, value: &str) {
    push_key(out, key);
    push_json_string(out, value);
}

/// Recursively writes a JSON value in canonical form (object keys sorted).
fn write_canonical_value(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => push_json_string(out, s),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical_value(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_key(out, key);
                if let Some(v) = map.get(*key) {
                    write_canonical_value(v, out);
                } else {
                    out.push_str("null");
                }
            }
            out.push('}');
        }
    }
}

/// Writes a JSON string literal with standard escaping. Deterministic and
/// fallible-free (no `serde_json` round trip), so it never silently corrupts a
/// hash on an encoding error.
fn push_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit_journal::entry::JournalEntryKind;

    fn entry(seq: u64, run_id: &str, payload: serde_json::Value, prev: &str) -> JournalEntry {
        JournalEntry {
            seq,
            run_id: run_id.to_string(),
            ts: "2026-01-01T00:00:00Z".to_string(),
            kind: JournalEntryKind::ToolCallStarted,
            payload,
            prev_hash: prev.to_string(),
            hash: String::new(),
            signature: None,
            signing_key_id: None,
        }
    }

    // Hash is a 64-char lowercase hex string
    #[test]
    fn test_hash_is_lowercase_hex_64() {
        // GIVEN an entry
        let e = entry(0, "run-1", serde_json::json!({"a": 1}), SENTINEL_PREV_HASH);
        // WHEN hashing
        let h = compute_entry_hash(&e);
        // THEN it is 64 lowercase hex chars
        assert_eq!(h.len(), 64);
        assert!(h
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    // Same content yields the same hash, independent of object key insertion order
    #[test]
    fn test_canonical_key_order_independent() {
        // GIVEN two payloads with identical content but different key order
        let a = entry(
            1,
            "run-1",
            serde_json::json!({"x": 1, "y": 2}),
            SENTINEL_PREV_HASH,
        );
        let b = entry(
            1,
            "run-1",
            serde_json::json!({"y": 2, "x": 1}),
            SENTINEL_PREV_HASH,
        );
        // WHEN hashing
        // THEN both hashes match
        assert_eq!(compute_entry_hash(&a), compute_entry_hash(&b));
    }

    // Mutating any content field changes the hash
    #[test]
    fn test_mutation_changes_hash() {
        // GIVEN a baseline entry
        let base = entry(1, "run-1", serde_json::json!({"a": 1}), SENTINEL_PREV_HASH);
        let baseline = compute_entry_hash(&base);
        // WHEN the payload changes
        let mutated = entry(1, "run-1", serde_json::json!({"a": 2}), SENTINEL_PREV_HASH);
        // THEN the hash differs
        assert_ne!(baseline, compute_entry_hash(&mutated));
    }

    // String escaping does not break determinism
    #[test]
    fn test_escaping_is_stable() {
        // GIVEN a payload with control and quote characters
        let e = entry(
            0,
            "run-1",
            serde_json::json!({"k": "line\nwith \"quotes\" and \\slash"}),
            SENTINEL_PREV_HASH,
        );
        // WHEN hashing twice
        // THEN both runs agree
        assert_eq!(compute_entry_hash(&e), compute_entry_hash(&e));
    }

    // The global hash is a deterministic 64-char lowercase hex string
    #[test]
    fn test_global_hash_deterministic() {
        // GIVEN a fixed entry hash, prev, and global seq
        let h = "a".repeat(64);
        // WHEN computing the global hash twice
        let g1 = compute_global_hash(&h, SENTINEL_PREV_HASH, 0);
        let g2 = compute_global_hash(&h, SENTINEL_PREV_HASH, 0);
        // THEN both agree and it is 64 lowercase hex chars
        assert_eq!(g1, g2);
        assert_eq!(g1.len(), 64);
        assert!(g1
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    // Changing the entry hash, the global seq, or the prev link changes the hash
    #[test]
    fn test_global_hash_sensitive_to_each_input() {
        // GIVEN a baseline global link
        let h = "a".repeat(64);
        let prev = "b".repeat(64);
        let base = compute_global_hash(&h, &prev, 5);
        // WHEN each input changes independently
        let other_entry = compute_global_hash(&"c".repeat(64), &prev, 5);
        let other_seq = compute_global_hash(&h, &prev, 6);
        let other_prev = compute_global_hash(&h, &"d".repeat(64), 5);
        // THEN the hash differs in every case
        assert_ne!(base, other_entry);
        assert_ne!(base, other_seq);
        assert_ne!(base, other_prev);
    }
}
