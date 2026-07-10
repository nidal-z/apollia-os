//! Loom concurrency models for the Apollia runtime actor algorithms.
//!
//! This crate carries no library code. Its purpose is the abstract Loom models
//! in `tests/models.rs`, which prove the runtime's core concurrency algorithms
//! are race-free (or expose a hazard) under exhaustive interleaving. See that
//! file's module docs and the crate `README.md` for the rationale and the run
//! command.
