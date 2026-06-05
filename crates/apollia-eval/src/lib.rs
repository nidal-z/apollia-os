//! Sovereign agent evaluation harness for Apollia (ADR-032).
//!
//! `apollia-eval` measures agent performance locally and reproducibly. A suite
//! is a declarative TOML document describing tasks, the number of runs per task,
//! and typed pass/fail assertions. This crate parses the suite model here, and
//! (in sibling modules) drives a live runtime to aggregate success rate, task
//! length, wall-clock distribution, and cost. Nothing leaves the machine.
//!
//! This module exposes the suite model and its typed assertions. Execution and
//! metric aggregation live in `runner` and `metrics`.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod suite;

pub use suite::{Assertion, EvalError, EvalSuite, EvalTask, OutputChannel};
