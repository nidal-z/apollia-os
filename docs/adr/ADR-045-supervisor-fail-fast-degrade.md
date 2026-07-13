# ADR-045: Supervisor is fail-fast then degrade, no actor restart-on-crash

- Status: Proposed
- Date: 2026-07-13

## Context

The runtime `Supervisor` (`crates/apollia-runtime/src/supervisor.rs`) starts the
actor mesh in a strict order (`EventBus -> AgentRegistry -> ToolRegistry -> TaskRouter
-> APIServer`) and rolls back in reverse on a startup failure. That startup behavior
is real, tested, and matches principle #4 (fail fast: any startup-detectable error is
detected at startup).

Alongside it, the crate carried a second, aspirational supervision model that was
never wired: a `RestartPolicy` enum (`Always` / `OnFailure` / `Never`), a `ChildSpec`
type, a `RestartTracker` with a sliding-window `record_restart`, and a
`default_child_specs()` returning seven specs. The module docstring claimed the
Supervisor "monitors actor health via `watch()` and applies `RestartPolicy` on
failure".

None of that was true. `watch()` only returns on `ShutdownRequested` or `FatalError`
and ignored every other event ("in MVP"). It was never spawned in production
(`embedded.rs` calls `Supervisor::start()` and never `watch()`), took its
`Vec<RestartTracker>` as an unused `_trackers` argument, and `record_restart` /
`default_child_specs` had zero call sites outside tests. The types were referenced
only by their own unit tests plus a `pub use` re-export that nothing in the workspace
consumed. On a post-startup actor crash the runtime does nothing: the task ends and
the system keeps running in a degraded state until an explicit shutdown.

So the real behavior (fail-fast at startup, then degrade) and the advertised behavior
(supervised restart-on-crash) had diverged. `docs/agents/FORBIDDEN.md` prohibits
keeping half-finished machinery alive: "either ship the feature or remove the code."

Note the naming collision: a genuinely live `RunnerSupervisor`
(`crates/apollia-runtime/src/runner_supervisor/`) does restart the inference sidecar
*process* with health monitoring. That is a separate subsystem and is unaffected.

## Decision

We adopt fail-fast then degrade as the runtime's actor supervision model for
v0.1.0, and we delete the dead restart machinery instead of wiring it.

Removed from `supervisor.rs`: `RestartPolicy`, `ChildSpec`, `RestartTracker` (and its
`record_restart`), `default_child_specs`, the unused `_trackers` parameter of
`watch()`, the now-unreachable `SupervisorError::MaxRestartsExceeded` variant, and the
tests that only exercised those types. The `pub use` re-export in `lib.rs` drops the
three removed public names. The module docstring now states the real model.

The runtime therefore guarantees:

- **Startup**: ordered start with reverse-order rollback on any actor's failure to
  become ready within the timeout (unchanged, principle #4).
- **Runtime**: `watch()` listens for `ShutdownRequested` / `FatalError` and drives a
  coordinated shutdown. A post-startup actor crash degrades the runtime (the actor is
  gone, the rest continues) until an operator or a fatal event triggers shutdown.
- **Inference sidecar**: restart-on-crash remains handled by `RunnerSupervisor`,
  which is out of scope here.

This aligns the code with the root `AGENTS.md` principle #4 and the
`crates/apollia-runtime/AGENTS.md` section 4 shutdown description, both of which
already describe only startup detection and coordinated shutdown, never actor
restart-on-crash. No documentation there needed to change.

## Alternatives considered

**Wire real restart-on-crash.** Implement liveness detection, per-actor
`RestartPolicy` evaluation, re-spawn wiring, and restart-storm limits, then spawn
`watch()` in `embedded.rs`. Rejected for v0.1.0: this is a substantial new resilience
feature, not the closing of a guardrail hole. It needs its own design (which crashes
are recoverable, how to rebuild an actor's dependencies, how a restarted actor
re-announces readiness) and would carry real risk (restart loops masking a
deterministic crash). Keeping dead scaffolding as a placeholder for it is exactly what
`FORBIDDEN.md` forbids. When restart-on-crash is genuinely needed, it gets a fresh ADR
and a real implementation rather than reviving inert types.

## Consequences

- The advertised behavior now matches the code: no reader assumes a crashed actor is
  restarted.
- Public API surface shrinks by three re-exported names. These are internal crates
  with no external consumer, and 0.x semver permits the change; `cargo-semver-checks`
  flags it as an intended minor break.
- If a future need for restart-on-crash arises, it is a deliberate, ADR-tracked
  feature, not the reanimation of removed scaffolding.
