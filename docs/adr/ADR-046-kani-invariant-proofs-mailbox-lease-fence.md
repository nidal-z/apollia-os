# ADR-046 - Kani proofs of the cardinal invariants and the mailbox lease fence

- Status: Accepted
- Date: 2026-07-13

## Context

Two invariants condition the runtime's credibility and must hold under
any ordering of operations, not just on a sample:

1. **Non-bypassable budget** (principle #7). `StepBudget`
   (`crates/apollia-oria/src/budget.rs`) must never let `used > cap` on
   a dimension, whatever the sequence of increments.
2. **Mailbox lease exclusivity.** An acknowledged message is deleted, an expired
   lease becomes deliverable again, and a stale `ack` must never delete a
   message re-leased to another consumer.

Tests sample; a model-checker (Kani, bit-precise, AWS) proves over
a whole bounded space. Two realities constrained the method:

1. **Kani does not run on the dev machines.** Kani links its own toolchain
   via rustup, absent from the repository's Homebrew machines. A local proof is
   therefore not possible; a locally executable fallback is needed.
2. **Kani models neither `Instant` nor SQLite nor Tokio.** The budget's
   wall-clock dimension rests on `Instant::now`, and the mailbox state lives in SQLite. The
   model-checker can only prove pure logic.

Exploration further confirmed two real defects:

- **C9-F4**: `handle_ack` deleted by `(message_id, to_agent)` without any
  lease-owner fence. A stale consumer, whose lease had expired then been
  re-leased, could delete the message a second consumer was
  processing.
- **Budget increment overflow**: `prev + 1` in `increment_*` is a checked `+`
  that panics in debug at `u32::MAX`.

## Decision

Adopt Kani as **dev-only** proof tooling (no runtime dependency),
reserved for the cardinal invariants, with a locally executable proptest
mirror, and fix the two defects the proofs expose.

**Extraction into provable pure logic.** The decision embedded in the methods
is extracted into pure helpers cited line by line: on the budget side `effective_cap`,
`dimension_exhausted`, `remaining`; on the mailbox side `is_deliverable`,
`owner_matches` and the lease transitions. Each helper re-encodes exactly one
production predicate, and the production methods call it (no
divergent model). The `#[cfg(kani)]` harnesses prove these helpers; the real
atomic `StepBudget` and the real SQLite store are backed to the model by the
proptest tests and the end-to-end regression test
(`test_ack_fenced_to_lease_owner`).

**Mailbox lease fence (fix for C9-F4).** Addition of a
`lease_owner` column, set to the `run_id` that leases on `receive`, and a
null-safe fence `lease_owner IS ?` on `ack` and `nack`. An `ack`/`nack` whose
`run_id` differs from the current owner acts on zero rows. The `run_id` already
flows to `receive` and `ack`; `nack` now receives it through the same
internal channel, without a change to the Python SDK contract (`nack(message_id)`
unchanged, the `run_id` is injected on the Rust side). An idempotent
`ALTER TABLE ... ADD COLUMN` migration covers existing stores.

**Budget overflow fix.** `prev + 1` becomes `prev.saturating_add(1)`,
identical for any reachable state, without a panic at `u32::MAX`, and provable without
`assume`.

**CI.** An advisory `kani` job in `nightly.yml` installs `kani-verifier`
(`cargo install --locked kani-verifier && cargo kani setup`) and runs
`cargo kani -p apollia-oria` and `cargo kani -p apollia-runtime`. It never
blocks a PR.

## Alternatives considered

- **Explicit lease token (returned by `receive`, required at `ack`).** Rejected
  for this workstream: a stronger guarantee but cross-cutting churn (signatures
  `receive`/`ack`, event `AgentMessageDelivered`, axum route, Python SDK).
  Kept as a possible evolution if the `None`/`None` residue must be
  closed.
- **Fence on lease validity only (`lease_until > now` at ack).** Rejected: does not
  close the race. After a re-lease, the lease is valid again, so a stale
  ack would still pass.
- **Prove the full atomic `StepBudget` under Kani.** Rejected: `Instant::now`,
  `tokio::sync::watch` and time are not modelable. Kani proves the pure
  helpers; the rest is backed by proptest.
- **Stick to the Loom models (the concurrency-tooling workstream).** Insufficient alone: Loom proves an
  interleaving of an abstract algorithm; Kani adds the bit-precise proof of the
  exact fence predicate, and the budget had no Loom model.

## Consequences

Positives:

- The non-bypassable cap, exhaustion stability and the absence of
  increment overflow are proven over the whole `u32` domain.
- Lease exclusivity (C9-F4) is fixed in production and proven: a
  stale consumer can no longer delete a re-leased message.
- The proptest mirror is green under `cargo test`; the regression test fails against the
  non-fenced code and passes once fenced.
- No runtime dependency; the Kani job is isolated and advisory.

Negatives / costs:

- The proofs hold only over their **bounded** space (all `u32` for the budget;
  identities and Unix times bounded by `kani::assume` for the mailbox). Do not
  oversell "proven correct" beyond that.
- Two points stay out of proof: the budget's wall-clock dimension (`Instant`)
  and the owner `None`/`None` collision (two leases without `run_id` are not
  mutually fenced). Documented as such.
- Kani requires rustup: proof in CI only, never local on this repository.
