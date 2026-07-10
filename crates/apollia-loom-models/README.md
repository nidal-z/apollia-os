# apollia-loom-models

Abstract [Loom](https://github.com/tokio-rs/loom) models for the Apollia runtime
actor algorithms. Loom exhaustively permutes thread interleavings to prove a
concurrency algorithm race-free (or to expose a hazard).

## Why a standalone, workspace-excluded crate

`--cfg loom` is a global rustc flag. Tokio gates `tokio::net` behind
`cfg(not(loom))`, so building any Tokio-dependent crate under the flag fails to
compile. The models are abstract (they re-implement each actor's algorithm and
import no production code), so this crate keeps Tokio out of its tree and is
excluded from the workspace, mirroring how `fuzz/` is isolated.

The models therefore prove the **algorithm** is sound, not the exact Tokio code.
Each model cites the prod `file:line` it mirrors so drift stays auditable.

## Run

```sh
RUSTFLAGS="--cfg loom" LOOM_MAX_PREEMPTIONS=3 \
  cargo test --manifest-path crates/apollia-loom-models/Cargo.toml --release
```

Without `--cfg loom` the crate compiles to nothing and pulls no dependencies.

## Models

| Model | Mirrors | Proves |
|---|---|---|
| `registry_serial_consistency` | `registry.rs:148-179` | same-name register/unregister keep map + index consistent |
| `coordinator_semaphore` | `coordinator.rs:143-193` | permit count never exceeds capacity, no leak |
| `mailbox_lease_exclusivity` | `mailbox.rs:587-666` | fenced ack never deletes a re-leased message (finding F4) |
| `router_terminal_status_guard` | `router.rs:187-199` | a late completion never overwrites a terminal status |
| `shutdown_force_exit_flag` | `shutdown.rs:310-347` | the double-Ctrl-C latch is monotonic |
| `plan_gate_single_decision` | `plan_gate.rs:72` | concurrent decide delivers exactly once |
| `shutdown_drain_snapshot_gap` | `shutdown.rs:178-198` | subscribe-before-snapshot loses no completion (finding F3) |
