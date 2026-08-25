//! `apollia-os eval run/report`: drive an evaluation suite over the local socket.
//!
//! `run` parses a TOML suite, drives each task against the running runtime via
//! the existing socket client, aggregates the metrics, prints a table (or JSON),
//! and writes a per-run JSONL. `report` re-prints a previously written JSONL as a
//! summary table. The harness fails fast and clearly when the daemon is down; it
//! never hangs.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::Subcommand;

use apollia_eval::{
    aggregate_runs, EvalError, EvalRunner, EvalSuite, EvalTask, RunMetrics, RunOutcome,
    RuntimeClient as EvalRuntimeClient, SuiteReport,
};

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;

/// Bound on the daemon health probe, so a missing daemon fails fast.
const HEALTH_TIMEOUT: Duration = Duration::from_secs(5);

/// Bound on a single task run, so a stuck task cannot hang the harness.
const RUN_TIMEOUT: Duration = Duration::from_secs(300);

/// `apollia-os eval` sub-commands.
#[derive(Debug, Subcommand)]
pub enum EvalCommand {
    /// Run an eval suite against the running runtime.
    Run {
        /// Path to the TOML suite.
        suite: PathBuf,
        /// Write the per-run JSONL here (default: `<suite>.results.jsonl`).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Default agent for tasks that do not name one.
        #[arg(long)]
        agent: Option<String>,
    },
    /// Re-print a previously written JSONL as a summary table.
    Report {
        /// Path to the JSONL produced by `eval run`.
        jsonl: PathBuf,
    },
}

/// Drives the runtime for one task over the socket, by submit + poll.
///
/// Step and tool-call counts come from the task response when the runtime
/// exposes them; otherwise they are `0` (the run path does not yet surface
/// them, tracked separately). Cost comes from the session budget, wall-clock is
/// measured client-side.
struct SocketRuntimeClient {
    client: RuntimeClient,
    default_agent: Option<String>,
}

impl SocketRuntimeClient {
    /// Polls the task until it reaches a terminal state, returning its JSON.
    async fn poll_until_terminal(&self, task_id: &str) -> Result<serde_json::Value, EvalError> {
        loop {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let task_json = self
                .client
                .get_task(task_id)
                .await
                .map_err(map_client_error)?;
            let status = task_json
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            if matches!(status, "completed" | "failed" | "canceled") {
                return Ok(task_json);
            }
        }
    }
}

impl EvalRuntimeClient for SocketRuntimeClient {
    async fn run_once(&self, task: &EvalTask) -> Result<RunOutcome, EvalError> {
        let agent = task
            .agent
            .as_deref()
            .or(self.default_agent.as_deref())
            .ok_or_else(|| {
                EvalError::Runtime(format!(
                    "task '{}' has no agent; set its `agent` field or pass --agent",
                    task.id
                ))
            })?;
        let start = Instant::now();
        let input = serde_json::json!({
            "parts": [{ "type": "text", "text": task.prompt }]
        });

        // A refused connection fails fast and clearly here, never hangs.
        let submitted = self
            .client
            .submit_task(agent, input)
            .await
            .map_err(map_client_error)?;
        let task_id = submitted
            .get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| EvalError::Runtime("runtime response missing task_id".to_string()))?
            .to_string();

        let task_json = tokio::time::timeout(RUN_TIMEOUT, self.poll_until_terminal(&task_id))
            .await
            .map_err(|_| {
                EvalError::Runtime("run timed out waiting for the runtime".to_string())
            })??;

        Ok(build_outcome(&task_json, start))
    }
}

/// Builds a [`RunOutcome`] from a terminal task response.
fn build_outcome(task_json: &serde_json::Value, start: Instant) -> RunOutcome {
    let success = task_json.get("status").and_then(|v| v.as_str()) == Some("completed");
    let result = task_json
        .get("result")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let cost_usd = task_json
        .pointer("/token_budget/session_cost_usd")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
    let steps = task_json
        .get("steps")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as u32;
    let tool_calls = task_json
        .get("tool_calls")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as u32;

    RunOutcome {
        stdout: result.clone(),
        result,
        exit_code: i32::from(!success),
        steps,
        tool_calls,
        wall_clock_ms: start.elapsed().as_millis() as u64,
        cost_usd,
    }
}

/// Maps a socket client error to a typed eval error with a clear message.
fn map_client_error(err: ClientError) -> EvalError {
    match err {
        ClientError::ConnectionRefused => EvalError::Runtime(
            "runtime not reachable (connection refused); start the daemon with `apollia-os start`"
                .to_string(),
        ),
        other => EvalError::Runtime(other.to_string()),
    }
}

/// Executes the `eval` sub-command. Returns the process exit code.
pub async fn run(cmd: &EvalCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    match cmd {
        EvalCommand::Run { suite, out, agent } => {
            run_suite(suite, out.as_deref(), agent.as_deref(), socket, json).await
        }
        EvalCommand::Report { jsonl } => report(jsonl, json),
    }
}

/// Runs a suite end to end: parse, probe the daemon, execute, render, persist.
async fn run_suite(
    suite_path: &Path,
    out: Option<&Path>,
    default_agent: Option<&str>,
    socket: Option<PathBuf>,
    json: bool,
) -> i32 {
    let suite = match EvalSuite::from_path(suite_path) {
        Ok(suite) => suite,
        Err(err) => return output_error(&err.to_string(), json, exit_codes::GENERAL_ERROR),
    };

    let socket_path = socket.unwrap_or_else(default_socket_path);
    let client = RuntimeClient::new(socket_path);

    // Fail fast (and clearly) when the daemon is not reachable.
    match tokio::time::timeout(HEALTH_TIMEOUT, client.health()).await {
        Ok(Ok(_)) => {}
        Ok(Err(_)) | Err(_) => {
            return output_error(
                "runtime not reachable; start the daemon with `apollia-os start`",
                json,
                exit_codes::RUNTIME_ERROR,
            );
        }
    }

    let runner = EvalRunner::new(SocketRuntimeClient {
        client,
        default_agent: default_agent.map(str::to_string),
    });
    let report = match runner.run_suite(&suite).await {
        Ok(report) => report,
        Err(err) => return output_error(&err.to_string(), json, exit_codes::RUNTIME_ERROR),
    };

    let out_path = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| suite_path.with_extension("results.jsonl"));
    if let Err(err) = write_jsonl(&out_path, &report) {
        return output_error(
            &format!("failed to write JSONL to {}: {err}", out_path.display()),
            json,
            exit_codes::GENERAL_ERROR,
        );
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
    } else {
        print_table(&report);
        println!("  per-run JSONL written to {}", out_path.display());
    }
    exit_codes::SUCCESS
}

/// Re-prints a JSONL of per-run records as an aggregated summary table.
fn report(jsonl_path: &Path, json: bool) -> i32 {
    let content = match std::fs::read_to_string(jsonl_path) {
        Ok(content) => content,
        Err(err) => {
            return output_error(
                &format!("failed to read {}: {err}", jsonl_path.display()),
                json,
                exit_codes::GENERAL_ERROR,
            );
        }
    };

    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, Vec<RunMetrics>> =
        std::collections::HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let metric: RunMetrics = match serde_json::from_str(line) {
            Ok(metric) => metric,
            Err(err) => {
                return output_error(
                    &format!("invalid JSONL line: {err}"),
                    json,
                    exit_codes::GENERAL_ERROR,
                );
            }
        };
        if !groups.contains_key(&metric.task_id) {
            order.push(metric.task_id.clone());
        }
        groups
            .entry(metric.task_id.clone())
            .or_default()
            .push(metric);
    }

    let tasks = order
        .into_iter()
        .map(|id| {
            let runs = groups.remove(&id).unwrap_or_default();
            aggregate_runs(id, runs)
        })
        .collect();
    let suite_name = jsonl_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("report")
        .to_string();
    let report = SuiteReport {
        suite: suite_name,
        tasks,
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
    } else {
        print_table(&report);
    }
    exit_codes::SUCCESS
}

/// Writes one JSON line per run to `path`, flattening every task's runs.
fn write_jsonl(path: &Path, report: &SuiteReport) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    for task in &report.tasks {
        for run in &task.runs_detail {
            let line = serde_json::to_string(run).unwrap_or_default();
            writeln!(file, "{line}")?;
        }
    }
    Ok(())
}

/// Prints the aggregated report as an aligned human-readable table.
fn print_table(report: &SuiteReport) {
    println!("Suite: {}", report.suite);
    println!(
        "  {:<24} {:>5} {:>8} {:>6} {:>6} {:>9} {:>9} {:>10}",
        "TASK", "RUNS", "SUCCESS", "STEPS", "TOOLS", "P50_MS", "P95_MS", "COST_USD"
    );
    for task in &report.tasks {
        println!(
            "  {:<24} {:>5} {:>7.0}% {:>6} {:>6} {:>9} {:>9} {:>10.4}",
            truncate(&task.task_id, 24),
            task.runs,
            task.success_rate * 100.0,
            task.median_steps,
            task.median_tool_calls,
            task.p50_wall_clock_ms,
            task.p95_wall_clock_ms,
            task.total_cost_usd,
        );
    }
}

/// Truncates `text` to `max` characters, appending an ellipsis when shortened.
fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{head}…")
}

/// Outputs an error and returns the given exit code.
fn output_error(msg: &str, json: bool, code: i32) -> i32 {
    crate::output::emit_error(json, code, msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_suite(content: &str) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().expect("create temp suite");
        file.write_all(content.as_bytes())
            .expect("write temp suite");
        file
    }

    // AC: invalid suite -> non-zero exit, no daemon needed
    #[tokio::test]
    async fn test_run_missing_suite_file_is_general_error() {
        // GIVEN a path to a suite file that does not exist
        let cmd = EvalCommand::Run {
            suite: PathBuf::from("/apollia/does/not/exist/suite.toml"),
            out: None,
            agent: None,
        };

        // WHEN running it
        let code = run(&cmd, None, true).await;

        // THEN it is a general error, not success
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    // AC: invalid suite content -> non-zero exit
    #[tokio::test]
    async fn test_run_invalid_toml_is_general_error() {
        // GIVEN a suite file with malformed TOML
        let suite = write_suite("this is = = not valid toml");
        let cmd = EvalCommand::Run {
            suite: suite.path().to_path_buf(),
            out: None,
            agent: None,
        };

        // WHEN running it
        let code = run(&cmd, None, true).await;

        // THEN it is a general error
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    // AC: daemon absent -> fast clear error, exit non-zero, no hang
    #[tokio::test]
    async fn test_run_fails_fast_when_daemon_absent() {
        // GIVEN a valid suite and a socket path pointing at no daemon
        let suite = write_suite(
            r#"
            name = "smoke"
            [[tasks]]
            id = "t1"
            prompt = "do a thing"
            agent = "demo"
            "#,
        );
        let cmd = EvalCommand::Run {
            suite: suite.path().to_path_buf(),
            out: None,
            agent: None,
        };
        let dead_socket = PathBuf::from("/tmp/apollia-eval-nonexistent-test.sock");

        // WHEN running against the unreachable socket, bounded so a hang would fail
        let code =
            tokio::time::timeout(Duration::from_secs(10), run(&cmd, Some(dead_socket), true))
                .await
                .expect("must not hang when the daemon is absent");

        // THEN it is a runtime error
        assert_eq!(code, exit_codes::RUNTIME_ERROR);
    }

    #[test]
    fn test_truncate_shortens_long_ids() {
        // GIVEN a string longer than the max
        // WHEN truncating
        // THEN it is shortened with an ellipsis and fits the budget
        let out = truncate("a-very-long-task-identifier-indeed", 10);
        assert_eq!(out.chars().count(), 10);
        assert!(out.ends_with('…'));
    }
}
