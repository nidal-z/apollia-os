//! ctx.budget: read-only StepBudget view exposed to Python.
//!
//! Typed successor of the legacy [`crate::context::PyStepBudgetView`]. The
//! semantics are strictly identical (read-only snapshot at access time); only
//! the namespace name changes: an agent following the Ctx Protocol reads
//! `ctx.budget.steps_remaining` instead of `ctx.step_budget.steps_remaining`.
//!
//! Built by [`crate::context::RuntimeContext::new_with_llm`] on every agent
//! run by reading the live counters of the Rust `StepBudgetView`.
//!
//! ## Wall-clock
//!
//! `wall_clock_secs` is propagated from the manifest (field
//! `budget.wall_clock_secs`, alias `wall_clock_timeout_secs`) at
//! `bridge.call_run()` time. When present, `wall_clock_remaining` returns
//! `Some(max(0, wall_clock_secs - elapsed_seconds))`; otherwise `None`
//! (agent without a configured deadline, e.g. CLI dry-run mode).

use pyo3::prelude::*;

/// Snapshot view of the execution budget exposed to Python via `ctx.budget`.
///
/// The two counters are `i64` because they are widened from the `u32` the
/// runtime counts in; both are always `>= 0`. There is no negative sentinel:
/// the `StepBudgetView` they are read from clamps its two remaining counts at
/// zero, so an unbudgeted run surfaces the distance to `u32::MAX`, not `-1`.
/// `wall_clock_remaining` is `Option<f64>` because some profiles (CLI without
/// a deadline) impose no wall-clock limit, and that absence really is
/// signalled, as `None`.
#[pyclass(frozen, name = "BudgetView", module = "apollia._native")]
pub struct BudgetView {
    /// Steps left before `max_steps`; never negative.
    steps_remaining: i64,
    /// Tool calls left before `max_tool_calls`; never negative.
    tool_calls_remaining: i64,
    /// Seconds elapsed since the task started.
    elapsed_seconds: f64,
    /// Seconds left before the wall-clock deadline; `None` if no deadline.
    wall_clock_remaining: Option<f64>,
}

#[pymethods]
impl BudgetView {
    /// Steps left before reaching `max_steps` (ReAct).
    ///
    /// Always `>= 0`, clamped to 0 on exhaustion and on a transient overshoot
    /// during concurrent increment.
    #[getter]
    fn steps_remaining(&self) -> i64 {
        self.steps_remaining
    }

    /// Tool calls left before `max_tool_calls`.
    ///
    /// Always `>= 0`.
    #[getter]
    fn tool_calls_remaining(&self) -> i64 {
        self.tool_calls_remaining
    }

    /// Seconds elapsed since the task started.
    ///
    /// Always `>= 0`. Monotonic counter (based on `Instant::now`).
    #[getter]
    fn elapsed_seconds(&self) -> f64 {
        self.elapsed_seconds
    }

    /// Seconds left before the wall-clock deadline, or `None` if no deadline
    /// is imposed.
    #[getter]
    fn wall_clock_remaining(&self) -> Option<f64> {
        self.wall_clock_remaining
    }
}

impl BudgetView {
    /// Builds a view from the live Rust counters.
    ///
    /// Called by [`crate::context::RuntimeContext`] when Python requests
    /// `ctx.budget` (the PyObject is created on the fly on each access to stay
    /// a fresh snapshot).
    pub fn new(
        steps_remaining: i64,
        tool_calls_remaining: i64,
        elapsed_seconds: f64,
        wall_clock_remaining: Option<f64>,
    ) -> Self {
        Self {
            steps_remaining,
            tool_calls_remaining,
            elapsed_seconds,
            wall_clock_remaining,
        }
    }
}
