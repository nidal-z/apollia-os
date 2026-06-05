//! Metric records and aggregation produced by running an evaluation suite.
//!
//! Three levels: [`RunMetrics`] is one task executed once (the JSONL unit),
//! [`TaskReport`] aggregates the runs of one task, and [`SuiteReport`] collects
//! the reports of a whole suite. [`aggregate_runs`] turns per-run records into a
//! [`TaskReport`]; it is reused both by the runner and by the CLI `report`
//! command rebuilding a report from a JSONL file.

/// Metrics for a single run of a single task.
///
/// One of these is written per run to the JSONL output, and read back by the
/// `report` command.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

/// Aggregates the per-run records of one task into a [`TaskReport`].
///
/// `success_rate` is `passed / runs`; medians and percentiles are computed over
/// the runs supplied. An empty `runs_detail` yields a zeroed report, never a
/// panic.
pub fn aggregate_runs(task_id: String, runs_detail: Vec<RunMetrics>) -> TaskReport {
    let runs = runs_detail.len() as u32;
    let passed = runs_detail.iter().filter(|r| r.passed).count() as u32;
    let success_rate = if runs == 0 {
        0.0
    } else {
        f64::from(passed) / f64::from(runs)
    };

    let steps: Vec<u32> = runs_detail.iter().map(|r| r.steps).collect();
    let tool_calls: Vec<u32> = runs_detail.iter().map(|r| r.tool_calls).collect();
    let wall: Vec<u64> = runs_detail.iter().map(|r| r.wall_clock_ms).collect();
    let total_cost_usd = runs_detail.iter().map(|r| r.cost_usd).sum();

    TaskReport {
        task_id,
        runs,
        success_rate,
        median_steps: median(&steps),
        median_tool_calls: median(&tool_calls),
        p50_wall_clock_ms: percentile(&wall, 50.0),
        p95_wall_clock_ms: percentile(&wall, 95.0),
        total_cost_usd,
        runs_detail,
    }
}

/// Returns the median of `values`, or `0` when empty.
///
/// For an even count, the two middle values are averaged (integer division).
fn median(values: &[u32]) -> u32 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) / 2
    }
}

/// Returns the `p`-th percentile of `values` by the nearest-rank method, or `0`
/// when empty. `p` is a percentage in `[0.0, 100.0]`.
fn percentile(values: &[u64], p: f64) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = (p / 100.0 * sorted.len() as f64).ceil() as usize;
    let index = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percentile_on_known_series() {
        // GIVEN a known ten-value series
        let series: Vec<u64> = (1..=10).map(|n| n * 10).collect();

        // WHEN computing p50 and p95 by nearest rank
        // THEN p50 is the 5th value and p95 is the 10th
        assert_eq!(percentile(&series, 50.0), 50);
        assert_eq!(percentile(&series, 95.0), 100);
        // AND an empty series yields zero, not a panic
        assert_eq!(percentile(&[], 50.0), 0);
    }

    #[test]
    fn test_median_even_and_empty() {
        // GIVEN an even-length series and an empty one
        // WHEN computing the median
        // THEN the even case averages the two middle values, the empty case is zero
        assert_eq!(median(&[10, 20, 30, 40]), 25);
        assert_eq!(median(&[7]), 7);
        assert_eq!(median(&[]), 0);
    }

    #[test]
    fn test_aggregate_runs_empty_is_zeroed() {
        // GIVEN no runs
        // WHEN aggregating
        let report = aggregate_runs("t".to_string(), vec![]);

        // THEN the report is zeroed, not a panic
        assert_eq!(report.runs, 0);
        assert!((report.success_rate - 0.0).abs() < f64::EPSILON);
        assert_eq!(report.p95_wall_clock_ms, 0);
    }
}
