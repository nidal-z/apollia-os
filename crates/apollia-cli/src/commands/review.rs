//! `apollia-os review`: submit a code review request to the `apollia-review` agent.
//!
//! Three input modes:
//!   `--task <id>`  Review the plan of an existing Apollia task.
//!   `--pr <n>`     Fetch `gh pr diff <n>` and review the diff.
//!   `--diff <f>`   Read a local `.diff` / `.patch` file and review its contents.
//!
//! When none of these flags are given, the agent falls back to `git diff HEAD~1`.
//!
//! The command blocks until the review agent completes and then prints either a
//! human-readable table (`--json` omitted) or the raw `ReviewReport` JSON.

use std::path::PathBuf;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

/// Arguments for `apollia-os review`.
#[derive(Debug, clap::Args)]
pub struct ReviewArgs {
    /// Apollia task ID whose execution plan should be reviewed.
    #[arg(long, value_name = "ID")]
    pub task: Option<String>,

    /// GitHub pull-request number to fetch via `gh pr diff`.
    #[arg(long, value_name = "N")]
    pub pr: Option<u64>,

    /// Local diff / patch file to analyse.
    #[arg(long, value_name = "FILE")]
    pub diff: Option<PathBuf>,
}

/// Execute `apollia-os review [--task ID] [--pr N] [--diff FILE]`.
///
/// Returns a POSIX exit code.
pub async fn run(args: &ReviewArgs, socket: Option<PathBuf>, json_output: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let client = RuntimeClient::new(socket_path);

    match dispatch(&client, args).await {
        Ok(report) => {
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).unwrap_or_default()
                );
            } else {
                print_human(&report);
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ConnectionRefused) => crate::output::emit_error(
            json_output,
            exit_codes::RUNTIME_ERROR,
            "runtime not started. Run `apollia-os start` first",
        ),
        Err(e) => crate::output::emit_error(json_output, exit_codes::GENERAL_ERROR, &e.to_string()),
    }
}

/// Determine the review strategy and call the appropriate runtime endpoint.
async fn dispatch(
    client: &RuntimeClient,
    args: &ReviewArgs,
) -> Result<serde_json::Value, ClientError> {
    if let Some(task_id) = &args.task {
        // Task-based review: POST /api/v1/tasks/{id}/review
        client.post_task_review(task_id).await
    } else {
        // Direct review: submit to apollia-review with the given inputs.
        let mut data = serde_json::Map::new();
        if let Some(pr) = args.pr {
            data.insert("pr_number".into(), serde_json::json!(pr));
        }
        if let Some(diff_path) = &args.diff {
            data.insert(
                "diff_file".into(),
                serde_json::json!(diff_path.to_string_lossy()),
            );
        }
        client.submit_review(serde_json::Value::Object(data)).await
    }
}

/// Render the `ReviewReport` as a human-readable summary.
fn print_human(report: &serde_json::Value) {
    let score = report.get("score").and_then(|v| v.as_u64()).unwrap_or(0);
    let summary = report.get("summary").and_then(|v| v.as_str()).unwrap_or("");
    let issues = report
        .get("issues")
        .and_then(|v| v.as_array())
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    let bar_len = (score / 10) as usize;
    let bar = "█".repeat(bar_len) + &"░".repeat(10 - bar_len);
    println!("Score  {score:3}/100  [{bar}]");
    println!("Summary: {summary}");

    if issues.is_empty() {
        println!("No issues.");
        return;
    }

    println!("\nIssues ({}):", issues.len());
    for issue in issues {
        let severity = issue
            .get("severity")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let category = issue
            .get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let message = issue.get("message").and_then(|v| v.as_str()).unwrap_or("");
        let location = issue.get("location").and_then(|v| v.as_str()).unwrap_or("");
        let loc_suffix = if location.is_empty() {
            String::new()
        } else {
            format!("  @ {location}")
        };
        println!("  [{severity}] {category}: {message}{loc_suffix}");
    }
}
