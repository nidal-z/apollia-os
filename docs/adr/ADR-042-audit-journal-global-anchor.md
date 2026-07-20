# ADR-042: Global anchor chain for audit-journal truncation evidence

- Status: Accepted
- Date: 2026-07-10

## Context

The audit journal (`crates/apollia-runtime/src/audit_journal/`) is an append-only,
per-run log, hash-chained with SHA256 (ADR-033). Each entry links to the previous
entry of the same run by `prev_hash`, so mutating, reordering, or deleting an entry
inside a run breaks that run's chain and is detectable on recomputation.

The per-run design leaves two holes that undermine the core product promise ("you can
prove what your agents did", EU AI Act record-keeping):

- Truncating the tail of a run leaves a shorter, still-valid chain, so
  `verify_entries` returns `ok: true`. No anchor records that the run reached a given
  `seq` and head hash.
- Deleting every entry of a run leaves nothing to verify: `entries_checked == 0`
  reports `ok: true`. Nothing records that the run ever existed.

Because the chains are per-run and independent, nothing links runs together, so a
whole run can vanish without any surviving structure noticing. Adding a cross-run
integrity structure is a security-boundary change (ARCHITECTURE Section E), so it is
decided here.

## Decision

We adopt a global anchor chain: a second hash chain that weaves every entry across all
runs into one monotone sequence, layered on top of the existing per-run chains without
changing them.

- We add a global link per entry, stored in additive columns on
  `audit_journal_entries` (`global_seq`, `global_prev_hash`, `global_hash`,
  `global_signature`, `global_signing_key_id`). The global hash is computed over the
  entry's own per-run `hash`, its `global_seq`, and the previous `global_prev_hash`
  (`compute_global_hash`), so the existing `compute_entry_hash` and all its vectors
  stay byte-identical. The global link is signed by reusing the existing `HmacSigner`.
- We persist the terminal `(global_seq, global_hash)` in a single mutable
  `audit_journal_state` row, upserted in the same SQLite transaction as each entry
  insert, so a crash can never leave the anchor ahead of or behind the entries table.
  The row is deliberately not protected by the append-only triggers: it is a head
  pointer, and its trust comes from the chain plus off-machine export, not from
  immutability.
- We add a whole-journal verification (`verify_journal`) that checks `global_seq`
  contiguity from zero (a gap reveals interior deletion or whole-run deletion), the
  global linkage and recomputed global hash, the global signature, every run's per-run
  chain, and the terminal head against the persisted anchor. It is exposed as
  `GET /api/v1/audit/verify` and `apollia audit verify` (no argument).
- We expose the head anchor for off-machine export via `GET /api/v1/audit/anchor` and
  `apollia audit anchor`. Exporting and storing it externally is the only defense
  against truncation of the global tail once the signing key can be compromised.

Pre-migration rows keep NULL global columns and fall outside the global guarantee;
their per-run chain is still verified independently. The per-run `verify_chain(run_id)`
surface and `VerifyChainReport` are unchanged: whole-run deletion is a global invariant,
surfaced only by whole-journal verification, to avoid a run registry that would itself
be deletable.

## Alternatives considered

### A per-run anchor only (rejected)
**For:** smallest change, one row per run recording its max `seq` and head hash.
**Against:** does not detect deletion of a whole run together with its anchor row, and
adds no cross-run structure.

### A periodic global checkpoint (rejected)
**For:** cheaper than one link per entry.
**Against:** truncation and deletion between two checkpoints stay invisible; the
granularity of the guarantee becomes the checkpoint interval.

### Fold `global_seq` into the existing entry hash (rejected)
**For:** one hash instead of a layered one; a key-less attacker cannot strip the global
layer.
**Against:** rewrites `compute_entry_hash`, invalidating every existing test vector and
making pre-migration rows unrecomputable, so per-run verification breaks for old data.
Its only edge is illusory: whole-run deletion is re-forgeable only by a key holder, who
can re-sign either design, so under key compromise both fail and without the key both
detect.

### Chosen: a global link layered over the entry hash, plus a mutable head anchor
**For:** detects interior deletion and whole-run deletion cryptographically; keeps the
per-run hash and all its vectors unchanged; one INSERT and one UPSERT per append, off
the runtime path; reuses the existing HMAC signer.
**Trade-offs:** truncation of the global tail is only detectable against an exported
anchor once the key is compromised; the state row is rollback-able; a schema migration
to version.

## Consequences

**Positives:**
- Interior deletion and whole-run deletion become detectable, closing the two holes in
  the "provable" promise.
- The head anchor is a single exportable value, the seed for future external anchoring
  (a transparency-log-style timestamp) without a redesign.

**Negatives / Trade-offs:**
- A new journal schema (five columns, one table, one index) to version for forward
  compatibility.
- Whole-journal verification reads the entire journal, so its cost grows with journal
  size.

**Watch:**
- The honesty of the guarantee: tamper-evidence, not tamper-proof. A holder of the
  signing key can recompute and re-sign a shorter consistent chain; only an externally
  exported anchor defeats this.
- Serialization stability of `compute_global_hash` across versions.

## Architectural principles

- Audit lineage: append-only, hash-chained, no deletes, extended across runs, consistent
  with the permissions audit spirit (ADR-015) and observability (ADR-012).
- Principle #5 (one actor, one responsibility): the global head stays owned by the
  single-writer journal actor; the anchor is advanced inside the append transaction, no
  shared lock.
- Principle #7 (non-negotiable safeguards): truncation and deletion evidence reinforces
  runtime authority and traceability.
- Principle #1 (local-first): the journal and its anchor stay on the machine; export is
  an explicit operator action.

## Related

- [ADR-033](ADR-033-plan-construction-audit-replay.md) established the signed,
  hash-chained per-run journal that this extends.
- [ADR-015](ADR-015-permission-tool-governance.md) the append-only audit spirit and
  SQLite triggers.
- [ADR-016](ADR-016-secrets-keyring-api-auth.md) the secret storage backing the signing key.

## Addendum: signatures-required verification (2026-07-20)

- Status: Accepted
- Date: 2026-07-20

A pre-launch review found that verification only checked a signature when the entry
*carried* one. Because the hash chains are keyless by design, an attacker with file
access could rewrite entries, recompute both the per-run and global hash chains,
set `signature` and `global_signature` to NULL, and update the mutable head anchor to
match. Verification then returned `ok: true`: the HMAC layer was bypassed by removing
it, not by defeating it. No verification mode required signatures to be present.

### Decision

Verification now enforces a signatures-required mode. When a signer is configured
(so every entry is expected to be signed), `verify_entries` and `verify_journal`
require every verified per-run entry and global link to carry a valid signature by
the active key. A missing signature fails with `BrokenLinkReason::SignatureInvalid`
(per-run) or `JournalBreakReason::GlobalSignatureInvalid` (global), reusing the
existing reasons so no report schema, client, or reference-doc changes. The mode is
driven by signer presence (`signer.is_some()`): an unsigned journal keeps the
keyless hash-only walk unchanged. Existing failure reasons keep their meaning; they
now also cover an absent-but-required signature.

The signing key file (`<data_dir>/journal-hmac-key`) is created owner-only (`0600`)
before any byte is written, closing the earlier write-then-chmod window. A missing
or unreadable key at startup still degrades to an unsigned journal, but the fallback
is now surfaced as an explicit `audit.journal.unsigned_fallback` warning.

### Scope reaffirmed

This does not change the honesty boundary above: the guarantee stays tamper-evidence,
not tamper-proof. The signer is a symmetric HMAC, so a party with file access can read
the key and re-sign a consistent shorter chain; the signatures-required mode closes the
strip-signature bypass for any verifier that holds the key (the runtime itself and
`apollia audit verify`), and the exported anchor remains the durable defense against a
key holder. Eliminating that residual (an asymmetric signature with the private key
kept off-machine, so a keyless third party can verify and a file-access attacker cannot
re-sign) would reopen the HMAC choice recorded here and is deliberately left to a future
epic, out of the frozen-quality release.

Known edge: a journal created unsigned and later given a key would flag its pre-key
entries as `SignatureInvalid` under the required mode. This does not arise for a journal
signed from first boot; lenient verification of a mixed-era journal would need an
explicit opt-out.
