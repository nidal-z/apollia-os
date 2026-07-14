# ADR-043 - Concurrency (Loom) and UB (Miri) verification tooling

- Status: Accepted
- Date: 2026-07-10

## Context

The core of the runtime is an actor model (bounded mpsc + cloneable handle, no
shared `Arc<Mutex>` between actors, principle #5), and the PyO3 FFI boundary
(`apollia-aip`) passes objects between Rust and Python. This workstream targets
two guarantees: prove that the concurrent algorithms are sound, and that the FFI
boundary does not introduce undefined behavior.

Two technical realities constrain the method:

1. **Loom cannot instrument Tokio.** The actors rely entirely on
   `tokio::sync` (`mpsc`, `broadcast`, `oneshot`, `Semaphore`) run on the
   Tokio scheduler, which Loom does not see. Worse, `--cfg loom` is a global
   rustc flag: Tokio conditions `tokio::net` behind `cfg(not(loom))`, so
   compiling a crate that depends on Tokio under this flag breaks the build
   (hyper-util / axum lose `tokio::net::UnixStream`).
2. **Miri cannot run the PyO3 boundary.** `apollia-aip` contains
   no hand-written `unsafe` block; all the `unsafe` comes from the pyo3 macros.
   Miri intercepts foreign function calls: `Python::with_gil` calls
   into libpython, unsupported. The only production `unsafe` elsewhere is
   `unsafe impl Send/Sync for LoadedWhisper` (whisper.cpp bindings), out of
   Miri's reach.

The brief assumed rewriting the actor synchronization primitives under
`cfg(loom)`. That is not feasible for Tokio actors. The real shape of the
tooling therefore had to be arbitrated.

## Decision

Adopt Loom and Miri as **dev-only** verification tooling, with no added runtime
dependency, with documented and honest coverage boundaries.

**Loom: abstract models in an excluded crate.** A standalone crate
`crates/apollia-loom-models`, excluded from the workspace (like `fuzz/`), with no Tokio
in its tree. Each model reimplements an actor's concurrent algorithm
with the Loom primitives and cites the production `file:line` it mirrors. Seven
models cover: registry eviction, coordinator semaphore, mailbox lease
exclusivity, router terminal-status guard, shutdown force-exit latch, plan-gate
single decision, drain snapshot/subscribe window.
`loom` enters only under `[target.'cfg(loom)'.dependencies]`.

**Miri: a suite of pure helpers, nightly job.** A `miri_pure` suite in
`apollia-aip`, as named unit tests for targeted filtering
(`cargo +nightly miri test -p apollia-aip --lib miri_pure`), touching only
interpreter-free helpers (date arithmetic, string parsing,
namespace composition). Miri is a rustup nightly component: no crate
added.

**Honesty boundary.** A Loom model proves the algorithm, not the exact Tokio
code. Two models (`mailbox_lease_exclusivity`, `shutdown_drain_snapshot_gap`)
model the recommended fix of a defect the production code does not yet
implement; the gap is a tracked finding (F3, F4), not a proof
of the current production. Miri covers the pure helpers; the PyO3 boundary and
the C bindings stay covered by integration tests and the SAFETY review.

**CI.** Two advisory jobs in `nightly.yml`: `loom` (on the pinned `1.95.0`,
`--cfg loom`) and `miri` (the repository's first nightly job, `nightly` + the
`miri` component). Neither blocks a PR.

## Alternatives considered

- **Rewrite the actors under `cfg(loom)` (the initial brief).** Rejected:
  impossible for Tokio actors, and `--cfg loom` breaks the compilation of the
  whole Tokio-dependent graph.
- **`loom` as a dev-dependency of `apollia-runtime`.** Rejected: verified
  empirically, `RUSTFLAGS="--cfg loom" cargo test -p apollia-runtime` fails to
  compile (loss of `tokio::net`). Hence the excluded crate.
- **ThreadSanitizer / `shuttle`.** Set aside for this workstream: TSan does not give
  Loom's exhaustiveness on small algorithms; `shuttle` covers async
  but adds a dependency and heavier instrumentation. Reconsider if
  a real async-interleaving need arises.
- **Extract the pure helpers from `apollia-aip` into a pyo3-free crate** for a
  broader Miri. Not retained now (out-of-scope refactor); noted as a
  fallback if one day Miri can no longer compile `apollia-aip`.

## Consequences

Positives:

- The critical concurrent invariants (terminal-status guard, semaphore,
  HITL gate single decision) are proven race-free on their algorithm.
- The Miri suite validates the absence of UB on the pure Rust code near the
  FFI.
- No runtime dependency; zero impact on the normal build (Loom crate excluded,
  Miri helpers = fast tests also run by `cargo test`).
- Two findings (F3 drain, F4 mailbox lease) are now backed by a model of the
  recommended fix.

Negatives / costs:

- The Loom models are abstract: they can diverge from the production code if
  an actor evolves without updating the model. Mitigation: each model cites
  the mirrored `file:line`.
- Miri requires a nightly toolchain (the repository's first recurring nightly
  usage outside fuzz).
- Loom demands a dedicated `RUSTFLAGS` invocation, outside the standard PR
  gates.
