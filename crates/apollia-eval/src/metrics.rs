//! Metric records produced by running an evaluation suite.
//!
//! Three levels: [`RunMetrics`] is one task executed once (the JSONL unit),
//! [`TaskReport`] aggregates the runs of one task, and [`SuiteReport`] collects
//! the reports of a whole suite. Aggregation lives in `runner`.

/// Metrics for a single run of a single task.
///
/// One of these is written per run to the JSONL output.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RunMetrics {
    /// Identifier of the task this run belongs to.
    pub task_id: String,
    /// Zero-based index of this run within the task.
    pub run_index: u32,
    /// Whether every assertion held for this run.
    pub passed: bool,
    /// Why the run failed, when it did. `None` when it passed.
    pub failure_reason: Option<String>,
    /// Exit code reported by the runtime (`-1` when the run could not be driven).
    pub exit_code: i32,
    /// Number of execution steps the run took.
    pub steps: u32,
    /// Number of tool calls the run made.
    pub tool_calls: u32,
    /// Wall-clock duration of the run in milliseconds.
    pub wall_clock_ms: u64,
    /// Cost of the run in US dollars.
    pub cost_usd: f64,
}

/// Aggregated metrics for one task across all its runs.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskReport {
    /// Identifier of the task.
    pub task_id: String,
    /// Number of runs aggregated.
    pub runs: u32,
    /// Fraction of runs that passed, in `[0.0, 1.0]`.
    pub success_rate: f64,
    /// Median step count across runs.
    pub median_steps: u32,
    /// Median tool-call count across runs.
    pub median_tool_calls: u32,
    /// 50th-percentile wall-clock duration in milliseconds.
    pub p50_wall_clock_ms: u64,
    /// 95th-percentile wall-clock duration in milliseconds.
    pub p95_wall_clock_ms: u64,
    /// Sum of the per-run cost in US dollars.
    pub total_cost_usd: f64,
    /// Per-run records, retained in memory for the JSONL output. Not serialized
    /// into the aggregated report.
    #[serde(skip)]
    pub runs_detail: Vec<RunMetrics>,
}

/// Aggregated report for a whole suite.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SuiteReport {
    /// Name of the suite.
    pub suite: String,
    /// One report per task, in suite order.
    pub tasks: Vec<TaskReport>,
}
