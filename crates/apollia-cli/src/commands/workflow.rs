//! `apollia-os workflow` subcommands: run a workflow of an installed package,
//! follow a run, read the history of a workflow, stop a run.
//!
//! A workflow is named `<package>/<workflow>` and runs in the runtime: `run`
//! returns once the run is recorded, unless `--wait` follows it to its end,
//! and `status` reads each of its steps.

use std::path::PathBuf;
use std::time::Duration;

use clap::Subcommand;

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;
use crate::note;

/// How often `run --wait` reads the run again.
const WAIT_POLL: Duration = Duration::from_millis(500);

/// Workflow subcommands: `apollia-os workflow <verb>`.
#[derive(Debug, Subcommand)]
pub enum WorkflowCommand {
    /// Run a workflow, in the runtime.
    Run {
        /// The workflow, `<package>/<workflow>`.
        workflow: String,
        /// An input, as `key=value`; repeatable. A value is read as the type
        /// the workflow declares for it.
        #[arg(long = "input", value_name = "KEY=VALUE")]
        inputs: Vec<String>,
        /// Follow the run to its end; exit 3 when it fails or is stopped.
        #[arg(long)]
        wait: bool,
    },
    /// Show a run: its status, its result and each of its steps.
    Status {
        /// The run id `workflow run` printed.
        run_id: String,
    },
    /// The runs of a workflow, newest first.
    Logs {
        /// The workflow, `<package>/<workflow>`.
        workflow: String,
        /// At most this many runs.
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
    /// Stop a run: its step in progress is interrupted, the next ones never start.
    Cancel {
        /// The run id.
        run_id: String,
        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        confirm: bool,
    },
}

/// Execute a `workflow` subcommand. Returns the process exit code.
pub async fn run(cmd: &WorkflowCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let client = RuntimeClient::new(socket.unwrap_or_else(default_socket_path));
    match cmd {
        WorkflowCommand::Run {
            workflow,
            inputs,
            wait,
        } => {
            let (ids, inputs) = match (split_id(workflow), parse_inputs(inputs)) {
                (Ok(ids), Ok(inputs)) => (ids, inputs),
                (Err(msg), _) | (_, Err(msg)) => {
                    return crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &msg)
                }
            };
            run_launch(&client, ids, &inputs, *wait, json).await
        }
        WorkflowCommand::Status { run_id } => run_status(&client, run_id, json).await,
        WorkflowCommand::Logs { workflow, limit } => match split_id(workflow) {
            Ok(ids) => run_logs(&client, ids, *limit, json).await,
            Err(msg) => crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &msg),
        },
        WorkflowCommand::Cancel { run_id, confirm } => {
            if let Some(code) = crate::output::require_confirmation(
                *confirm,
                json,
                &format!("stop workflow run '{run_id}'"),
            ) {
                return code;
            }
            run_cancel(&client, run_id, json).await
        }
    }
}

/// `<package>/<workflow>` split in two, or the usage error.
fn split_id(workflow: &str) -> Result<(&str, &str), String> {
    let valid = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    };
    match workflow.split_once('/') {
        Some((package, name)) if valid(package) && valid(name) => Ok((package, name)),
        _ => Err(format!(
            "'{workflow}' is not a workflow: expected <package>/<workflow>, each [a-z0-9_-]"
        )),
    }
}

/// The `--input key=value` pairs as an object of text values; the runtime
/// reads each as the type the workflow declares.
fn parse_inputs(inputs: &[String]) -> Result<serde_json::Value, String> {
    let mut parsed = serde_json::Map::new();
    for input in inputs {
        match input.split_once('=') {
            Some((key, value)) if !key.is_empty() => {
                parsed.insert(
                    key.to_string(),
                    serde_json::Value::String(value.to_string()),
                );
            }
            _ => return Err(format!("--input takes key=value, got '{input}'")),
        }
    }
    Ok(serde_json::Value::Object(parsed))
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

async fn run_launch(
    client: &RuntimeClient,
    (package, workflow): (&str, &str),
    inputs: &serde_json::Value,
    wait: bool,
    json: bool,
) -> i32 {
    let run = match client.launch_workflow(package, workflow, inputs).await {
        Ok(run) => run,
        Err(e) => return handle_error(e, json),
    };
    let run_id = field(&run, "runId").to_string();
    if !wait {
        if json {
            print_json(&run);
        } else {
            println!("{run_id}");
            note!("Follow it with: apollia-os workflow status {run_id}");
        }
        return exit_codes::SUCCESS;
    }
    loop {
        let detail = match client.get_workflow_run(&run_id).await {
            Ok(detail) => detail,
            Err(e) => return handle_error(e, json),
        };
        let status = field(&detail["run"], "status");
        if status != "running" {
            let code = if status == "completed" {
                exit_codes::SUCCESS
            } else {
                exit_codes::TASK_FAILED
            };
            print_detail(&detail, json);
            return code;
        }
        tokio::time::sleep(WAIT_POLL).await;
    }
}

fn print_detail(detail: &serde_json::Value, json: bool) {
    if json {
        print_json(detail);
        return;
    }
    let run = &detail["run"];
    println!("Run        : {}", field(run, "runId"));
    println!("Workflow   : {}", field(run, "workflowId"));
    println!("Status     : {}", field(run, "status"));
    println!("Started    : {}", field(run, "startedAt"));
    println!("Ended      : {}", field(run, "endedAt"));
    if let Some(error) = run.get("error").and_then(|e| e.as_str()) {
        println!("Error      : {error}");
    }
    if let Some(output) = run.get("output").filter(|o| !o.is_null()) {
        println!("Output     : {output}");
    }
    note!();
    note!(
        "  {:<4} {:<24} {:<12} {:<26} ENDED",
        "#",
        "STEP",
        "STATUS",
        "STARTED"
    );
    for step in detail["steps"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
        println!(
            "  {:<4} {:<24} {:<12} {:<26} {}",
            step.get("rank").and_then(|r| r.as_u64()).unwrap_or(0),
            field(step, "name"),
            field(step, "status"),
            field(step, "startedAt"),
            field(step, "endedAt")
        );
    }
}

async fn run_status(client: &RuntimeClient, run_id: &str, json: bool) -> i32 {
    match client.get_workflow_run(run_id).await {
        Ok(detail) => {
            print_detail(&detail, json);
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

async fn run_logs(
    client: &RuntimeClient,
    (package, workflow): (&str, &str),
    limit: u32,
    json: bool,
) -> i32 {
    match client.list_workflow_runs(package, workflow, limit).await {
        Ok(list) => {
            if json {
                print_json(&list);
                return exit_codes::SUCCESS;
            }
            let runs = list["runs"].as_array().map(Vec::as_slice).unwrap_or(&[]);
            if runs.is_empty() {
                note!("No run of {package}/{workflow} yet.");
                return exit_codes::SUCCESS;
            }
            note!("  {:<36} {:<10} {:<26} ENDED", "RUN", "STATUS", "STARTED");
            for run in runs {
                println!(
                    "  {:<36} {:<10} {:<26} {}",
                    field(run, "runId"),
                    field(run, "status"),
                    field(run, "startedAt"),
                    field(run, "endedAt")
                );
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

async fn run_cancel(client: &RuntimeClient, run_id: &str, json: bool) -> i32 {
    match client.cancel_workflow_run(run_id).await {
        Ok(run) => {
            if json {
                print_json(&run);
            } else {
                println!("Workflow run {run_id} {}", field(&run, "status"));
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// An unreachable runtime, or one without a workflow engine, is a runtime
/// error (2); a refused input, an unknown workflow or run, or a run already
/// ended is a general error (1).
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
        cmd: WorkflowCommand,
    }

    #[test]
    fn test_workflow_run_parses_its_inputs_and_wait() {
        // GIVEN a run with two inputs and --wait
        // WHEN parsed
        let cli = TestCli::parse_from([
            "test",
            "run",
            "invoices/close-month",
            "--input",
            "folder=/data/march",
            "--input",
            "limit=3",
            "--wait",
        ]);
        // THEN the workflow, the inputs and the flag land in their fields
        let WorkflowCommand::Run {
            workflow,
            inputs,
            wait,
        } = cli.cmd
        else {
            panic!("expected Run");
        };
        assert_eq!(workflow, "invoices/close-month");
        assert_eq!(inputs, vec!["folder=/data/march", "limit=3"]);
        assert!(wait);
    }

    #[test]
    fn test_workflow_status_logs_and_cancel_parse() {
        // GIVEN a status, a logs without limit, and a cancel with --confirm
        // WHEN parsed
        let status = TestCli::parse_from(["test", "status", "r-1"]);
        let logs = TestCli::parse_from(["test", "logs", "invoices/close-month"]);
        let cancel = TestCli::parse_from(["test", "cancel", "r-1", "--confirm"]);
        // THEN each keeps its argument and the logs limit defaults to 50
        assert!(matches!(status.cmd, WorkflowCommand::Status { ref run_id } if run_id == "r-1"));
        assert!(matches!(logs.cmd, WorkflowCommand::Logs { limit: 50, .. }));
        assert!(matches!(
            cancel.cmd,
            WorkflowCommand::Cancel { ref run_id, confirm: true } if run_id == "r-1"
        ));
    }

    #[test]
    fn test_inputs_are_sent_as_text_and_a_malformed_one_is_refused() {
        // GIVEN inputs with a number, a path holding '=', and one without '='
        let good = vec!["limit=3".to_string(), "query=a=b".to_string()];
        let bad = vec!["folder".to_string()];
        // WHEN they are parsed
        let parsed = parse_inputs(&good).unwrap();
        let err = parse_inputs(&bad).unwrap_err();
        // THEN values stay text, split on the first '=', and the malformed one is refused
        assert_eq!(parsed, serde_json::json!({"limit": "3", "query": "a=b"}));
        assert!(err.contains("key=value"), "{err}");
    }

    #[test]
    fn test_a_workflow_id_needs_a_package_and_a_name() {
        // GIVEN identifiers with and without a package
        // WHEN split
        // THEN only `<package>/<workflow>` in the allowed alphabet passes
        assert_eq!(
            split_id("invoices/close-month").unwrap(),
            ("invoices", "close-month")
        );
        assert!(split_id("close-month").is_err());
        assert!(split_id("a/b/c").is_err());
        assert!(split_id("Invoices/x").is_err());
    }

    #[tokio::test]
    async fn test_workflow_logs_without_runtime_is_a_runtime_error() {
        // GIVEN a client on a socket nothing listens on
        let client = RuntimeClient::new(PathBuf::from("/tmp/apollia-test-workflow-absent.sock"));
        // WHEN the history of a workflow is read
        let code = run_logs(&client, ("invoices", "close-month"), 10, true).await;
        // THEN the exit code is the runtime error
        assert_eq!(code, exit_codes::RUNTIME_ERROR);
    }

    #[test]
    fn test_a_refused_input_is_a_general_error_and_no_engine_a_runtime_error() {
        // GIVEN a 400 and a 503 from the runtime
        let refused = ClientError::ServerError {
            status: 400,
            body: "input 'folder': is required".into(),
        };
        let absent = ClientError::ServerError {
            status: 503,
            body: "the workflow engine is not running in this runtime".into(),
        };
        // WHEN each is reported
        let (refused, absent) = (handle_error(refused, true), handle_error(absent, true));
        // THEN the refusal is 1 and the absent engine 2
        assert_eq!(refused, exit_codes::GENERAL_ERROR);
        assert_eq!(absent, exit_codes::RUNTIME_ERROR);
    }
}
