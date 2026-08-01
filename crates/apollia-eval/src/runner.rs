//! The evaluation runner: executes each task N times and aggregates metrics.
//!
//! The runner orchestrates; the [`RuntimeClient`] (injected) talks to the
//! runtime; [`crate::metrics`] holds the records. The client is a trait so the
//! runner is testable in pure Rust with a mock, without a live daemon. A run
//! whose client call errors, or whose assertion fails, counts as a failure
//! without stopping the aggregation.

use std::future::Future;
use std::path::Path;

use regex::Regex;

use crate::judge::JudgeVerdict;
use crate::metrics::{RunMetrics, SuiteReport, TaskReport};
use crate::suite::{Assertion, EvalSuite, EvalTask, OutputChannel};
use crate::EvalError;

/// Drives the live runtime for one agent task. Injected so the runner is testable.
pub trait RuntimeClient: Send + Sync {
    /// Runs one task once, returning the raw outcome or a typed error.
    ///
    /// # Errors
    ///
    /// Returns [`EvalError::Runtime`] when the task cannot be driven against the
    /// runtime (connection refused, timeout, protocol error).
    fn run_once(
        &self,
        task: &EvalTask,
    ) -> impl Future<Output = Result<RunOutcome, EvalError>> + Send;
}

/// The raw outcome of a single task run, before assertions are applied.
#[derive(Debug, Clone)]
pub struct RunOutcome {
    /// Streamed standard output of the run.
    pub stdout: String,
    /// Final result text of the run.
    pub result: String,
    /// Exit code reported by the runtime.
    pub exit_code: i32,
    /// Number of execution steps.
    pub steps: u32,
    /// Number of tool calls.
    pub tool_calls: u32,
    /// Wall-clock duration in milliseconds.
    pub wall_clock_ms: u64,
    /// Cost of the run in US dollars.
    pub cost_usd: f64,
}

/// Runs an [`EvalSuite`] against an injected [`RuntimeClient`].
///
/// An optional [`LlmRouter`](apollia_llm::LlmRouter) backs `llm_judge`
/// assertions. Without it they cannot be evaluated, and an assertion that
/// cannot be evaluated fails.
pub struct EvalRunner<C: RuntimeClient> {
    client: C,
    judge: Option<apollia_llm::LlmRouter>,
}

impl<C: RuntimeClient> EvalRunner<C> {
    /// Creates a runner over the given client.
    ///
    /// Without a judge router, an `llm_judge` assertion **fails** rather than
    /// being skipped. Use [`with_judge`](Self::with_judge) to evaluate them.
    pub fn new(client: C) -> Self {
        Self {
            client,
            judge: None,
        }
    }

    /// Creates a runner that evaluates `llm_judge` assertions with `router`.
    pub fn with_judge(client: C, router: apollia_llm::LlmRouter) -> Self {
        Self {
            client,
            judge: Some(router),
        }
    }

    /// Runs every task in the suite `runs` times and aggregates the metrics.
    ///
    /// # Errors
    ///
    /// Always returns `Ok`: a client error on one run is recorded as a failed
    /// run rather than aborting the suite. The `Result` is kept for forward
    /// compatibility with fail-fast modes.
    pub async fn run_suite(&self, suite: &EvalSuite) -> Result<SuiteReport, EvalError> {
        let mut tasks = Vec::with_capacity(suite.tasks.len());
        for task in &suite.tasks {
            tasks.push(self.run_task(task).await);
        }
        Ok(SuiteReport {
            suite: suite.name.clone(),
            tasks,
        })
    }

    /// Runs one task `task.runs` times and aggregates it into a [`TaskReport`].
    async fn run_task(&self, task: &EvalTask) -> TaskReport {
        let mut runs_detail = Vec::with_capacity(task.runs as usize);
        for run_index in 0..task.runs {
            runs_detail.push(self.run_one(task, run_index).await);
        }
        crate::metrics::aggregate_runs(task.id.clone(), runs_detail)
    }

    /// Runs one task once and turns the outcome into a [`RunMetrics`].
    async fn run_one(&self, task: &EvalTask, run_index: u32) -> RunMetrics {
        match self.client.run_once(task).await {
            Ok(outcome) => {
                let failure_reason = self.evaluate_assertions(&task.assertions, &outcome).await;
                RunMetrics {
                    task_id: task.id.clone(),
                    run_index,
                    passed: failure_reason.is_none(),
                    failure_reason,
                    exit_code: outcome.exit_code,
                    steps: outcome.steps,
                    tool_calls: outcome.tool_calls,
                    wall_clock_ms: outcome.wall_clock_ms,
                    cost_usd: outcome.cost_usd,
                }
            }
            Err(error) => RunMetrics {
                task_id: task.id.clone(),
                run_index,
                passed: false,
                failure_reason: Some(error.to_string()),
                exit_code: -1,
                steps: 0,
                tool_calls: 0,
                wall_clock_ms: 0,
                cost_usd: 0.0,
            },
        }
    }

    /// Evaluates the assertions against a run outcome.
    ///
    /// Returns the reason of the first failing assertion, or `None` when all
    /// hold. An `llm_judge` assertion is skipped (non-blocking) when no judge
    /// router is configured.
    async fn evaluate_assertions(
        &self,
        assertions: &[Assertion],
        outcome: &RunOutcome,
    ) -> Option<String> {
        for assertion in assertions {
            let failure = match assertion {
                Assertion::ExitCode { equals } => (outcome.exit_code != *equals).then(|| {
                    format!(
                        "exit code {} did not equal expected {}",
                        outcome.exit_code, equals
                    )
                }),
                Assertion::FileExists { path } => (!Path::new(path).exists())
                    .then(|| format!("expected file does not exist: {path}")),
                Assertion::Regex { on, pattern } => check_regex(on, pattern, outcome),
                Assertion::LlmJudge { rubric } => self.judge_assertion(rubric, outcome).await,
            };
            if failure.is_some() {
                return failure;
            }
        }
        None
    }

    /// Evaluates an `llm_judge` assertion against the run result.
    ///
    /// Returns a failure reason, or `None` when the judge passes.
    ///
    /// An assertion that could not be evaluated **fails**. It used to return
    /// `None`, which the caller reads as "no failure", so a suite whose only
    /// checks were `llm_judge` reported green without a single one having run.
    /// An operator validating an agent before production read a result that
    /// meant nothing. A harness that cannot run a check must say so, never
    /// count it as passed.
    ///
    /// The two ways it cannot run are kept distinct, because they are fixed
    /// differently: no router was configured at all, or the router had no
    /// backend available for this call.
    async fn judge_assertion(&self, rubric: &str, outcome: &RunOutcome) -> Option<String> {
        let Some(router) = self.judge.as_ref() else {
            return Some(
                "llm judge not evaluated: this runner has no judge router. \
                 The assertion was not checked, so the task cannot be reported \
                 as passing."
                    .to_string(),
            );
        };
        match crate::judge::judge(router, rubric, &outcome.result).await {
            JudgeVerdict::Pass => None,
            JudgeVerdict::Fail(reason) => Some(format!("llm judge failed: {reason}")),
            JudgeVerdict::Skipped => Some(
                "llm judge not evaluated: no backend was available. The \
                 assertion was not checked, so the task cannot be reported as \
                 passing."
                    .to_string(),
            ),
        }
    }
}

/// Selects the channel and checks the regex, returning a failure reason or `None`.
fn check_regex(on: &OutputChannel, pattern: &str, outcome: &RunOutcome) -> Option<String> {
    let haystack = match on {
        OutputChannel::Stdout => &outcome.stdout,
        OutputChannel::Result => &outcome.result,
    };
    match Regex::new(pattern) {
        Ok(re) if re.is_match(haystack) => None,
        Ok(_) => Some(format!("pattern /{pattern}/ did not match {on:?}")),
        Err(error) => Some(format!("invalid regex /{pattern}/: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    fn ok_outcome() -> RunOutcome {
        RunOutcome {
            stdout: "ok".to_string(),
            result: "done".to_string(),
            exit_code: 0,
            steps: 4,
            tool_calls: 2,
            wall_clock_ms: 100,
            cost_usd: 0.01,
        }
    }

    /// A client that always succeeds with a fixed outcome.
    struct AlwaysPassClient;

    impl RuntimeClient for AlwaysPassClient {
        async fn run_once(&self, _task: &EvalTask) -> Result<RunOutcome, EvalError> {
            Ok(ok_outcome())
        }
    }

    /// A client that fails on the `fail_on`-th call (1-based) and succeeds otherwise.
    struct FailOnNthClient {
        fail_on: u32,
        counter: AtomicU32,
    }

    impl RuntimeClient for FailOnNthClient {
        async fn run_once(&self, _task: &EvalTask) -> Result<RunOutcome, EvalError> {
            let nth = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
            if nth == self.fail_on {
                Err(EvalError::Runtime("simulated socket failure".to_string()))
            } else {
                Ok(ok_outcome())
            }
        }
    }

    fn task(id: &str, runs: u32, assertions: Vec<Assertion>) -> EvalTask {
        EvalTask {
            id: id.to_string(),
            prompt: "do a thing".to_string(),
            runs,
            agent: None,
            assertions,
        }
    }

    #[tokio::test]
    async fn test_run_suite_all_pass_reports_full_success() {
        // GIVEN a 1-task suite, runs=5, an always-pass client and a passing assertion
        let suite = EvalSuite {
            name: "all-pass".to_string(),
            tasks: vec![task("t1", 5, vec![Assertion::ExitCode { equals: 0 }])],
        };
        let runner = EvalRunner::new(AlwaysPassClient);

        // WHEN running the suite
        let report = runner
            .run_suite(&suite)
            .await
            .expect("run_suite is infallible");

        // THEN the single task reports a perfect success rate and the medians
        assert_eq!(report.tasks.len(), 1);
        let task_report = &report.tasks[0];
        assert_eq!(task_report.runs, 5);
        assert!((task_report.success_rate - 1.0).abs() < f64::EPSILON);
        assert_eq!(task_report.median_steps, 4);
        assert_eq!(task_report.median_tool_calls, 2);
        assert_eq!(task_report.p50_wall_clock_ms, 100);
        assert!((task_report.total_cost_usd - 0.05).abs() < 1e-9);
    }

    // (error case)
    #[tokio::test]
    async fn test_run_suite_one_client_error_is_counted_not_panicked() {
        // GIVEN a client that errors on run 2 of 4
        let suite = EvalSuite {
            name: "flaky".to_string(),
            tasks: vec![task("t1", 4, vec![])],
        };
        let runner = EvalRunner::new(FailOnNthClient {
            fail_on: 2,
            counter: AtomicU32::new(0),
        });

        // WHEN running the suite
        let report = runner
            .run_suite(&suite)
            .await
            .expect("run_suite is infallible");

        // THEN the failed run is counted, the rest aggregate, success_rate is 0.75
        let task_report = &report.tasks[0];
        assert_eq!(task_report.runs, 4);
        assert!((task_report.success_rate - 0.75).abs() < f64::EPSILON);
        let failed: Vec<&RunMetrics> = task_report
            .runs_detail
            .iter()
            .filter(|r| !r.passed)
            .collect();
        assert_eq!(failed.len(), 1);
        assert!(failed[0]
            .failure_reason
            .as_deref()
            .unwrap_or_default()
            .contains("simulated socket failure"));
    }

    // (assertion failure on a runtime-success run)
    #[tokio::test]
    async fn test_run_marked_failed_when_file_exists_assertion_fails() {
        // GIVEN a runtime-success run but a FileExists assertion on a missing path
        let suite = EvalSuite {
            name: "assert-fail".to_string(),
            tasks: vec![task(
                "t1",
                2,
                vec![Assertion::FileExists {
                    path: "/apollia/does/not/exist/42".to_string(),
                }],
            )],
        };
        let runner = EvalRunner::new(AlwaysPassClient);

        // WHEN running the suite
        let report = runner
            .run_suite(&suite)
            .await
            .expect("run_suite is infallible");

        // THEN every run is failed with the assertion reason
        let task_report = &report.tasks[0];
        assert!((task_report.success_rate - 0.0).abs() < f64::EPSILON);
        assert!(task_report.runs_detail.iter().all(|r| !r.passed
            && r.failure_reason
                .as_deref()
                .unwrap_or_default()
                .contains("does not exist")));
    }

    #[tokio::test]
    async fn test_empty_suite_returns_empty_report() {
        // GIVEN a suite with no tasks
        let suite = EvalSuite {
            name: "empty".to_string(),
            tasks: vec![],
        };
        let runner = EvalRunner::new(AlwaysPassClient);

        // WHEN running the suite
        let report = runner
            .run_suite(&suite)
            .await
            .expect("run_suite is infallible");

        // THEN the report is empty, no error
        assert_eq!(report.suite, "empty");
        assert!(report.tasks.is_empty());
    }

    // llm_judge wired end to end: a passing judge keeps the run passing
    #[tokio::test]
    async fn test_llm_judge_pass_keeps_run_passing() {
        // GIVEN a runner with a judge whose backend replies "PASS"
        let suite = EvalSuite {
            name: "judged".to_string(),
            tasks: vec![task(
                "t1",
                1,
                vec![Assertion::LlmJudge {
                    rubric: "must say done".to_string(),
                }],
            )],
        };
        let runner = EvalRunner::with_judge(
            AlwaysPassClient,
            crate::test_support::router_replying("PASS"),
        );

        // WHEN running the suite
        let report = runner
            .run_suite(&suite)
            .await
            .expect("run_suite is infallible");

        // THEN the run passes
        assert!((report.tasks[0].success_rate - 1.0).abs() < f64::EPSILON);
    }

    // llm_judge wired end to end: a failing judge fails the run with its reason
    #[tokio::test]
    async fn test_llm_judge_fail_fails_run_with_reason() {
        // GIVEN a runner with a judge whose backend replies "FAIL: <reason>"
        let suite = EvalSuite {
            name: "judged".to_string(),
            tasks: vec![task(
                "t1",
                1,
                vec![Assertion::LlmJudge {
                    rubric: "must cite a source".to_string(),
                }],
            )],
        };
        let runner = EvalRunner::with_judge(
            AlwaysPassClient,
            crate::test_support::router_replying("FAIL: no source was cited"),
        );

        // WHEN running the suite
        let report = runner
            .run_suite(&suite)
            .await
            .expect("run_suite is infallible");

        // THEN the run is failed and carries the judge reason
        let task_report = &report.tasks[0];
        assert!((task_report.success_rate - 0.0).abs() < f64::EPSILON);
        assert!(task_report.runs_detail[0]
            .failure_reason
            .as_deref()
            .unwrap_or_default()
            .contains("no source was cited"));
    }

    // llm_judge is non-blocking when no judge router is configured
    #[tokio::test]
    async fn test_llm_judge_without_router_fails_instead_of_passing() {
        // GIVEN a runner with no judge and an llm_judge assertion
        //
        // This test used to assert the opposite, that the run still passes, and
        // that is precisely why the defect survived: the harness reported green
        // on a check it had never run, and a test said that was correct.
        let suite = EvalSuite {
            name: "judged".to_string(),
            tasks: vec![task(
                "t1",
                1,
                vec![Assertion::LlmJudge {
                    rubric: "anything".to_string(),
                }],
            )],
        };
        let runner = EvalRunner::new(AlwaysPassClient);

        // WHEN running the suite
        let report = runner
            .run_suite(&suite)
            .await
            .expect("run_suite is infallible");

        // THEN the task fails, because an unevaluated assertion is not a passed
        // one, and the reason says which of the two causes applies
        assert!(
            report.tasks[0].success_rate.abs() < f64::EPSILON,
            "an assertion that was never evaluated must not count as passing"
        );
        let reasons: String = report.tasks[0]
            .runs_detail
            .iter()
            .filter_map(|r| r.failure_reason.as_deref())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            reasons.contains("no judge router"),
            "the failure must name the cause, got: {reasons}"
        );
    }
}
