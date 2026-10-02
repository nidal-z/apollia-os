//! `apollia-os batch` subcommands: run an agent on every item of a source,
//! follow a batch, list the batches.
//!
//! A batch runs in the runtime: `run` returns once its items are recorded,
//! and `status` reads its progress. Every item is a task of its own, listed by
//! `task list` like any other.

use std::path::PathBuf;

use clap::Subcommand;

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;
use crate::note;

/// Batch subcommands: `apollia-os batch <verb>`.
#[derive(Debug, Subcommand)]
pub enum BatchCommand {
    /// Run an agent on every item of a source, in the runtime.
    Run {
        /// The agent every item is run by: a stored agent without code.
        agent: String,
        /// The type of source, as a reader registers it.
        #[arg(long = "source-type")]
        source_type: String,
        /// What the source is opened on: a file, a folder, a connection.
        #[arg(long)]
        source: String,
        /// An option of the source type, as `key=value`; repeatable.
        #[arg(long = "option", value_name = "KEY=VALUE")]
        options: Vec<String>,
        /// An access granted for the duration of the batch; repeatable.
        #[arg(long = "grant")]
        grants: Vec<String>,
        /// How many items run at once, 1 to 16 (default 1).
        #[arg(long)]
        concurrency: Option<u32>,
        /// Failed items tolerated before the batch is interrupted (default 10).
        #[arg(long = "max-failures")]
        max_failures: Option<u32>,
    },
    /// Show a batch: its state, its counters and each of its items.
    Status {
        /// The batch id `batch run` printed.
        batch_id: String,
    },
    /// List the batches, newest first.
    List {
        /// At most this many batches.
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
}

/// Execute a `batch` subcommand. Returns the process exit code.
pub async fn run(cmd: &BatchCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let client = RuntimeClient::new(socket.unwrap_or_else(default_socket_path));
    match cmd {
        BatchCommand::Run { .. } => {
            let launch = match launch_body(cmd) {
                Ok(launch) => launch,
                Err(msg) => {
                    return crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &msg)
                }
            };
            run_launch(&client, &launch, json).await
        }
        BatchCommand::Status { batch_id } => run_status(&client, batch_id, json).await,
        BatchCommand::List { limit } => run_list(&client, *limit, json).await,
    }
}

/// The body of `POST /api/v1/batches` for a `run`, or the usage error that
/// prevents it.
fn launch_body(cmd: &BatchCommand) -> Result<serde_json::Value, String> {
    let BatchCommand::Run {
        agent,
        source_type,
        source,
        options,
        grants,
        concurrency,
        max_failures,
    } = cmd
    else {
        return Err("not a batch run".into());
    };
    let mut parsed = serde_json::Map::new();
    for option in options {
        let Some((key, value)) = option.split_once('=') else {
            return Err(format!("--option takes key=value, got '{option}'"));
        };
        if key.is_empty() {
            return Err(format!("--option takes key=value, got '{option}'"));
        }
        parsed.insert(
            key.to_string(),
            serde_json::Value::String(value.to_string()),
        );
    }
    let mut body = serde_json::json!({
        "agent": agent,
        "source": {"kind": source_type, "location": source, "options": parsed},
        "grants": grants,
    });
    if let Some(n) = *concurrency {
        body["concurrency"] = n.into();
    }
    if let Some(n) = *max_failures {
        body["maxFailures"] = n.into();
    }
    Ok(body)
}

fn print_json(value: &serde_json::Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_default()
    );
}

fn field<'a>(value: &'a serde_json::Value, key: &str) -> &'a str {
    value.get(key).and_then(|v| v.as_str()).unwrap_or("-")
}

fn number(value: &serde_json::Value, key: &str) -> u64 {
    value.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}

async fn run_launch(client: &RuntimeClient, launch: &serde_json::Value, json: bool) -> i32 {
    match client.launch_batch(launch).await {
        Ok(batch) => {
            if json {
                print_json(&batch);
            } else {
                println!("{}", field(&batch, "batchId"));
                note!(
                    "Batch of {} items on '{}', {} at a time. Follow it with: apollia-os batch status {}",
                    number(&batch, "total"),
                    field(&batch, "agent"),
                    number(&batch, "concurrency"),
                    field(&batch, "batchId")
                );
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

async fn run_status(client: &RuntimeClient, batch_id: &str, json: bool) -> i32 {
    match client.get_batch(batch_id).await {
        Ok(detail) => {
            if json {
                print_json(&detail);
                return exit_codes::SUCCESS;
            }
            let batch = &detail["batch"];
            println!("Batch      : {}", field(batch, "batchId"));
            println!("Agent      : {}", field(batch, "agent"));
            println!("State      : {}", field(batch, "state"));
            println!(
                "Items      : {} total, {} completed, {} failed, {} canceled",
                number(batch, "total"),
                number(batch, "completed"),
                number(batch, "failed"),
                number(batch, "canceled")
            );
            println!(
                "Threshold  : interrupted past {} failed items",
                number(batch, "maxFailures")
            );
            note!();
            note!("  {:<6} {:<20} {:<10} TASK", "RANK", "ORIGIN", "STATUS");
            for item in detail["items"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
                println!(
                    "  {:<6} {:<20} {:<10} {}",
                    number(item, "rank"),
                    field(item, "origin"),
                    field(item, "status"),
                    field(item, "taskId")
                );
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

async fn run_list(client: &RuntimeClient, limit: u32, json: bool) -> i32 {
    match client.list_batches(limit).await {
        Ok(list) => {
            if json {
                print_json(&list);
                return exit_codes::SUCCESS;
            }
            let batches = list["batches"].as_array().map(Vec::as_slice).unwrap_or(&[]);
            if batches.is_empty() {
                note!("No batch yet.");
                return exit_codes::SUCCESS;
            }
            note!(
                "  {:<36} {:<20} {:<12} {:>6} {:>6} {:>6}",
                "BATCH",
                "AGENT",
                "STATE",
                "TOTAL",
                "DONE",
                "FAILED"
            );
            for batch in batches {
                println!(
                    "  {:<36} {:<20} {:<12} {:>6} {:>6} {:>6}",
                    field(batch, "batchId"),
                    field(batch, "agent"),
                    field(batch, "state"),
                    number(batch, "total"),
                    number(batch, "completed"),
                    number(batch, "failed")
                );
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// An unreachable runtime, or one without a batch engine, is a runtime error
/// (2); a refused launch or an unknown batch is a general error (1).
fn handle_error(err: ClientError, json: bool) -> i32 {
    match err {
        ClientError::ServerError { status: 503, body } => {
            crate::output::emit_error(json, exit_codes::RUNTIME_ERROR, &body)
        }
        ClientError::ServerError { body, .. } => {
            crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &body)
        }
        other => crate::output::emit_client_error(json, &other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: BatchCommand,
    }

    #[test]
    fn test_batch_run_parses_its_source_and_limits() {
        // GIVEN a run with a source, an option, a grant, a concurrency and a threshold
        // WHEN parsed
        let cli = TestCli::parse_from([
            "test",
            "run",
            "request-clerk",
            "--source-type",
            "spreadsheet",
            "--source",
            "/data/requests.xlsx",
            "--option",
            "sheet=April",
            "--grant",
            "web_search",
            "--concurrency",
            "3",
            "--max-failures",
            "2",
        ]);
        // THEN every value lands in its field
        let BatchCommand::Run {
            agent,
            source_type,
            source,
            options,
            grants,
            concurrency,
            max_failures,
        } = cli.cmd
        else {
            panic!("expected Run");
        };
        assert_eq!(agent, "request-clerk");
        assert_eq!(source_type, "spreadsheet");
        assert_eq!(source, "/data/requests.xlsx");
        assert_eq!(options, vec!["sheet=April".to_string()]);
        assert_eq!(grants, vec!["web_search".to_string()]);
        assert_eq!((concurrency, max_failures), (Some(3), Some(2)));
    }

    #[test]
    fn test_batch_status_and_list_parse() {
        // GIVEN a status on an id and a list with no flag
        // WHEN parsed
        let status = TestCli::parse_from(["test", "status", "b-1"]);
        let list = TestCli::parse_from(["test", "list"]);
        // THEN the id is kept and the list limit defaults to 50
        assert!(matches!(status.cmd, BatchCommand::Status { ref batch_id } if batch_id == "b-1"));
        assert!(matches!(list.cmd, BatchCommand::List { limit: 50 }));
    }

    fn run_with(options: &[&str]) -> BatchCommand {
        BatchCommand::Run {
            agent: "clerk".into(),
            source_type: "spreadsheet".into(),
            source: "/data/a.xlsx".into(),
            options: options.iter().map(|o| (*o).to_string()).collect(),
            grants: vec![],
            concurrency: None,
            max_failures: None,
        }
    }

    #[test]
    fn test_launch_body_carries_only_the_limits_given() {
        // GIVEN a run with one option and no limit
        let run = run_with(&["sheet=April"]);
        // WHEN its body is built
        let body = launch_body(&run).unwrap();
        // THEN the source carries the option, and no limit is sent, so the
        //      runtime applies its defaults
        assert_eq!(body["source"]["options"]["sheet"], "April");
        assert_eq!(body["source"]["kind"], "spreadsheet");
        assert!(body.get("concurrency").is_none());
        assert!(body.get("maxFailures").is_none());
    }

    #[test]
    fn test_launch_body_refuses_an_option_without_equals() {
        // GIVEN an option without '='
        let run = run_with(&["sheet"]);
        // WHEN its body is built
        let err = launch_body(&run).unwrap_err();
        // THEN it is refused with the expected form
        assert!(err.contains("key=value"), "{err}");
    }

    #[tokio::test]
    async fn test_batch_list_without_runtime_is_a_runtime_error() {
        // GIVEN a client on a socket nothing listens on
        let client = RuntimeClient::new(PathBuf::from("/tmp/apollia-test-batch-absent.sock"));
        // WHEN the batches are listed
        let code = run_list(&client, 10, true).await;
        // THEN the exit code is the runtime error
        assert_eq!(code, exit_codes::RUNTIME_ERROR);
    }

    #[test]
    fn test_a_refused_launch_is_a_general_error_and_no_engine_a_runtime_error() {
        // GIVEN a 400 and a 503 from the runtime
        let refused = ClientError::ServerError {
            status: 400,
            body: "unknown source type 'spreadsheet'".into(),
        };
        let absent = ClientError::ServerError {
            status: 503,
            body: "the batch engine is not running in this runtime".into(),
        };
        // WHEN each is reported
        let (refused, absent) = (handle_error(refused, true), handle_error(absent, true));
        // THEN the refusal is 1 and the absent engine 2
        assert_eq!(refused, exit_codes::GENERAL_ERROR);
        assert_eq!(absent, exit_codes::RUNTIME_ERROR);
    }
}
