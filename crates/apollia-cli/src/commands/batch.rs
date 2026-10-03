//! `apollia-os batch` subcommands: run an agent on every item of a source,
//! follow a batch, list the batches, list the accesses a run may grant,
//! pause, resume, stop or retry a batch, and export its result.
//!
//! A batch runs in the runtime: `run` returns once its items are recorded,
//! and `status` reads its progress. Every item is a task of its own, listed by
//! `task list` like any other. An item whose agent reaches for a sensitive
//! access the run did not grant waits rather than failing; `retry` runs the
//! failed and waiting items again, with more grants if given. `export` writes
//! the result of a batch to a file, one row per item: Excel, CSV or JSON.

use std::io::Write;
use std::path::{Path, PathBuf};

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
        /// A sensitive access granted for the duration of the batch, as
        /// `batch accesses` lists it; repeatable.
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
    /// List the sensitive accesses of an agent, those `run --grant` may grant.
    Accesses {
        /// The agent a batch would run.
        agent: String,
    },
    /// Pause a running batch: no further item starts.
    Pause {
        /// The batch id.
        batch_id: String,
    },
    /// Resume a paused batch.
    Resume {
        /// The batch id.
        batch_id: String,
    },
    /// Stop a batch: running items are cancelled, pending ones never run.
    Stop {
        /// The batch id.
        batch_id: String,
    },
    /// Run the failed items of a batch, and those waiting for an access, again.
    Retry {
        /// The batch id.
        batch_id: String,
        /// A sensitive access granted for the rest of the batch; repeatable.
        #[arg(long = "grant")]
        grants: Vec<String>,
    },
    /// Export the result of a batch to a file: one row per item, its input
    /// columns, its output, its status and the sources it cites.
    Export {
        /// The batch id.
        batch_id: String,
        /// The file format.
        #[arg(long, value_parser = ["xlsx", "csv", "json"])]
        format: String,
        /// Where to write the file; `-` writes it to standard output.
        /// Default: `batch-<id>.<format>` in the current folder.
        #[arg(long, short = 'o', value_name = "PATH")]
        output: Option<PathBuf>,
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
        BatchCommand::Accesses { agent } => run_accesses(&client, agent, json).await,
        BatchCommand::Pause { batch_id } => {
            run_control(&client, batch_id, "pause", None, json).await
        }
        BatchCommand::Resume { batch_id } => {
            run_control(&client, batch_id, "resume", None, json).await
        }
        BatchCommand::Stop { batch_id } => run_control(&client, batch_id, "stop", None, json).await,
        BatchCommand::Retry { batch_id, grants } => {
            let body = serde_json::json!({ "grants": grants });
            run_control(&client, batch_id, "retry", Some(&body), json).await
        }
        BatchCommand::Export {
            batch_id,
            format,
            output,
        } => run_export(&client, batch_id, format, output.as_deref(), json).await,
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
                "Items      : {} total, {} completed, {} failed, {} canceled, {} waiting",
                number(batch, "total"),
                number(batch, "completed"),
                number(batch, "failed"),
                number(batch, "canceled"),
                number(batch, "awaiting")
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

async fn run_accesses(client: &RuntimeClient, agent: &str, json: bool) -> i32 {
    match client.batch_accesses(agent).await {
        Ok(body) => {
            if json {
                print_json(&body);
                return exit_codes::SUCCESS;
            }
            let accesses = body["accesses"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if accesses.is_empty() {
                note!("No sensitive access: every item of '{agent}' runs without asking.");
                return exit_codes::SUCCESS;
            }
            note!(
                "  {:<32} {:<16} {:<9} PERMITS",
                "ACCESS",
                "FAMILY",
                "GRANTABLE"
            );
            for access in accesses {
                println!(
                    "  {:<32} {:<16} {:<9} {}",
                    field(access, "accessId"),
                    field(access, "family"),
                    if access["grantable"].as_bool().unwrap_or(false) {
                        "yes"
                    } else {
                        "no"
                    },
                    field(access, "permits")
                );
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

async fn run_control(
    client: &RuntimeClient,
    batch_id: &str,
    verb: &str,
    body: Option<&serde_json::Value>,
    json: bool,
) -> i32 {
    match client.control_batch(batch_id, verb, body).await {
        Ok(batch) => {
            if json {
                print_json(&batch);
            } else {
                println!("{} {}", field(&batch, "batchId"), field(&batch, "state"));
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// Where `export` writes the file of `batch_id`: the path given, or
/// `batch-<id>.<format>` in the current folder. `None` is standard output.
fn export_target(batch_id: &str, format: &str, output: Option<&Path>) -> Option<PathBuf> {
    match output {
        Some(p) if p == Path::new("-") => None,
        Some(p) => Some(p.to_path_buf()),
        None => Some(PathBuf::from(format!("batch-{batch_id}.{format}"))),
    }
}

async fn run_export(
    client: &RuntimeClient,
    batch_id: &str,
    format: &str,
    output: Option<&Path>,
    json: bool,
) -> i32 {
    let bytes = match client.export_batch(batch_id, format).await {
        Ok(bytes) => bytes,
        Err(e) => return handle_error(e, json),
    };
    let Some(path) = export_target(batch_id, format, output) else {
        let mut stdout = std::io::stdout().lock();
        return match stdout.write_all(&bytes).and_then(|()| stdout.flush()) {
            Ok(()) => exit_codes::SUCCESS,
            Err(e) => crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!("could not write the export to standard output: {e}"),
            ),
        };
    };
    if let Err(e) = std::fs::write(&path, &bytes) {
        return crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("could not write {}: {e}", path.display()),
        );
    }
    if json {
        print_json(&serde_json::json!({
            "batchId": batch_id,
            "format": format,
            "path": path.display().to_string(),
            "bytes": bytes.len(),
        }));
    } else {
        println!("{}", path.display());
        note!(
            "Batch {batch_id} exported as {format}, {} bytes.",
            bytes.len()
        );
    }
    exit_codes::SUCCESS
}

/// An unreachable runtime, or one without a batch engine, is a runtime error
/// (2); a refused launch, an unknown batch or a batch in the wrong state is
/// a general error (1).
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
    fn test_batch_control_verbs_parse() {
        // GIVEN each control verb on a batch id, and a retry with a grant
        // WHEN parsed
        let pause = TestCli::parse_from(["test", "pause", "b-1"]);
        let resume = TestCli::parse_from(["test", "resume", "b-1"]);
        let stop = TestCli::parse_from(["test", "stop", "b-1"]);
        let retry = TestCli::parse_from(["test", "retry", "b-1", "--grant", "gmail_send"]);
        let accesses = TestCli::parse_from(["test", "accesses", "clerk"]);
        // THEN each lands on its verb with its id, and the grant is kept
        assert!(matches!(pause.cmd, BatchCommand::Pause { ref batch_id } if batch_id == "b-1"));
        assert!(matches!(resume.cmd, BatchCommand::Resume { ref batch_id } if batch_id == "b-1"));
        assert!(matches!(stop.cmd, BatchCommand::Stop { ref batch_id } if batch_id == "b-1"));
        assert!(matches!(
            retry.cmd,
            BatchCommand::Retry { ref batch_id, ref grants }
                if batch_id == "b-1" && grants == &vec!["gmail_send".to_string()]
        ));
        assert!(matches!(accesses.cmd, BatchCommand::Accesses { ref agent } if agent == "clerk"));
    }

    #[tokio::test]
    async fn test_batch_control_without_runtime_is_a_runtime_error() {
        // GIVEN a client on a socket nothing listens on
        let client = RuntimeClient::new(PathBuf::from("/tmp/apollia-test-batch-control.sock"));
        // WHEN a batch is stopped and an agent's accesses are listed
        let stop = run_control(&client, "b-1", "stop", None, true).await;
        let accesses = run_accesses(&client, "clerk", true).await;
        // THEN both are runtime errors
        assert_eq!(stop, exit_codes::RUNTIME_ERROR);
        assert_eq!(accesses, exit_codes::RUNTIME_ERROR);
    }

    #[test]
    fn test_batch_export_parses_its_format_and_output() {
        // GIVEN an export to a named file, one to standard output, and one in
        //       a format the runtime does not write
        // WHEN parsed
        let named =
            TestCli::parse_from(["test", "export", "b-1", "--format", "xlsx", "-o", "r.xlsx"]);
        let piped =
            TestCli::parse_from(["test", "export", "b-1", "--format", "csv", "--output", "-"]);
        let pdf = TestCli::try_parse_from(["test", "export", "b-1", "--format", "pdf"]);
        let bare = TestCli::try_parse_from(["test", "export", "b-1"]);
        // THEN the format and the path land in their fields, and an unknown or
        //      missing format is a usage error
        assert!(matches!(
            named.cmd,
            BatchCommand::Export { ref batch_id, ref format, ref output }
                if batch_id == "b-1" && format == "xlsx"
                    && output.as_deref() == Some(Path::new("r.xlsx"))
        ));
        let BatchCommand::Export { format, output, .. } = piped.cmd else {
            panic!("expected Export");
        };
        assert_eq!(format, "csv");
        assert_eq!(export_target("b-1", &format, output.as_deref()), None);
        assert!(pdf.is_err());
        assert!(bare.is_err());
    }

    #[test]
    fn test_an_export_without_a_path_is_named_after_its_batch() {
        // GIVEN an export without --output
        // WHEN its target is resolved
        let target = export_target("b-1", "json", None);
        // THEN it is batch-<id>.<format> in the current folder
        assert_eq!(target, Some(PathBuf::from("batch-b-1.json")));
    }

    #[tokio::test]
    async fn test_batch_export_without_runtime_is_a_runtime_error_and_writes_nothing() {
        // GIVEN a client on a socket nothing listens on, and a target path
        let client = RuntimeClient::new(PathBuf::from("/tmp/apollia-test-batch-export.sock"));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.csv");
        // WHEN a batch is exported
        let code = run_export(&client, "b-1", "csv", Some(&path), true).await;
        // THEN it is a runtime error and no file is written
        assert_eq!(code, exit_codes::RUNTIME_ERROR);
        assert!(!path.exists());
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
