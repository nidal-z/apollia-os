//! `apollia-os audit` subcommands: query audit trail via the runtime API.
//!
//! Provides `list`, `stats`, `export`, and `verify` operations on the audit
//! trail and the hash-chained audit journal.

use std::path::PathBuf;

use clap::Subcommand;

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;

mod journal;
mod replay;
mod report;
mod verify;

use journal::{run_journal, run_show};
use replay::{run_anchor, run_replay};
use report::{run_export, run_list, run_stats};
use verify::{run_verify, run_verify_journal};

/// Audit subcommands: `apollia-os audit <verb>`.
#[derive(Debug, Subcommand)]
pub enum AuditCommand {
    /// List recent audit events (default).
    #[command(name = "list")]
    List {
        /// Maximum number of events to display.
        #[arg(long, default_value = "20")]
        limit: u32,
    },
    /// Browse the hash-chained journal across every run, newest first.
    ///
    /// Unlike `audit list` (the tool-invocation trail) and `audit show RUN`
    /// (one run), this reads the chained journal without needing a run id up
    /// front, so the audited register is reachable by browsing.
    #[command(name = "journal")]
    Journal {
        /// Maximum number of entries to display.
        #[arg(long, default_value = "20")]
        limit: u32,
        /// Number of entries to skip, newest first. Page through with it.
        #[arg(long, default_value_t = 0)]
        offset: u32,
    },
    /// Display audit statistics.
    Stats,
    /// Export the audit trail as JSON, up to `--limit` events.
    Export {
        /// Destination file (default: stdout).
        #[arg(long, value_name = "PATH")]
        output: Option<PathBuf>,
        /// Maximum number of events to include (default: 10000).
        #[arg(long, default_value_t = 10_000)]
        limit: u32,
    },
    /// Verify the audit journal's hash chains and signatures.
    ///
    /// With a RUN_ID, verifies that run's per-run chain. Without an argument,
    /// verifies the whole journal: the global chain across all runs (detecting
    /// interior deletion and whole-run deletion) and the head anchor (detecting
    /// truncation of the global tail).
    #[command(name = "verify")]
    Verify {
        /// Identifier of the run to verify. Omit to verify the whole journal.
        #[arg(value_name = "RUN_ID")]
        run_id: Option<String>,
    },
    /// Print the exportable head anchor of the global chain.
    ///
    /// Storing this off-machine is the only defense against truncation of the
    /// global tail once the signing key can be compromised.
    #[command(name = "anchor")]
    Anchor,
    /// Replay a captured run and detect divergences.
    ///
    /// Compares the replayed run against its captured trace across every
    /// category: LLM responses, tool outputs, and plan construction. Divergences
    /// are grouped by category in the human output and listed in the `--json`
    /// payload. `run` accepts a full run_id or an unambiguous prefix of at least
    /// 8 characters. Exit 0 = identical, exit 2 = diverged, exit 1 = any error
    /// (run not found, ambiguous prefix, incomplete trace, runtime unreachable).
    #[command(name = "replay")]
    Replay {
        /// Run identifier or unambiguous prefix.
        #[arg(value_name = "RUN")]
        run: String,
    },
    /// Show a run's full journal, including the model's LLM completions.
    ///
    /// Unlike `audit list`/`export` (the tool-only audit trail), this reads the
    /// hash-chained journal so the captured reasoning (prompts/responses) is
    /// readable. Accepts a run_id or a task_id (resolved to its run_id).
    #[command(name = "show")]
    Show {
        /// Run identifier, or a task_id that maps to one.
        #[arg(value_name = "RUN_OR_TASK")]
        run: String,
    },
}

/// Execute an `audit` subcommand.
///
/// Returns the process exit code.
pub async fn run(cmd: &AuditCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(default_socket_path);
    let client = RuntimeClient::new(socket_path);

    match cmd {
        AuditCommand::List { limit } => run_list(&client, *limit, json).await,
        AuditCommand::Journal { limit, offset } => {
            run_journal(&client, *limit, *offset, json).await
        }
        AuditCommand::Stats => run_stats(&client, json).await,
        AuditCommand::Export { output, limit } => {
            run_export(&client, output.as_deref(), *limit, json).await
        }
        AuditCommand::Verify { run_id } => match run_id {
            Some(rid) => run_verify(&client, rid, json).await,
            None => run_verify_journal(&client, json).await,
        },
        AuditCommand::Anchor => run_anchor(&client, json).await,
        AuditCommand::Replay { run } => run_replay(&client, run, json).await,
        AuditCommand::Show { run } => run_show(&client, run, json).await,
    }
}

/// Handle client errors uniformly.
fn handle_error(err: ClientError, json: bool) -> i32 {
    match err {
        ClientError::ConnectionRefused => crate::output::emit_error(
            json,
            exit_codes::RUNTIME_ERROR,
            "runtime not started (connection refused)",
        ),
        other => crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &other.to_string()),
    }
}

/// Handle HTTP server errors.
fn handle_server_error(status: u16, body: &str, json: bool) -> i32 {
    let error_msg = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
        .unwrap_or_else(|| format!("server error ({status})"));

    crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &error_msg.to_string())
}
#[cfg(test)]
mod tests {

    /// GIVEN a list response in the documented shape
    /// WHEN  its events are extracted
    /// THEN  the array comes back whole
    ///
    /// The export loop stops when a page comes back short, so mis-parsing a page
    /// as empty would end the export at the first request and write a file that
    /// looks complete.
    #[test]
    fn test_parse_events_reads_the_documented_shape() {
        // GIVEN / WHEN
        let got = parse_events(r#"{"events":[{"a":1},{"a":2}],"count":2}"#);

        // THEN
        assert_eq!(got.map(|v| v.len()), Some(2));
    }

    /// GIVEN a bare array, the other shape the endpoint has used
    /// WHEN  its events are extracted
    /// THEN  it is accepted too
    #[test]
    fn test_parse_events_accepts_a_bare_array() {
        // GIVEN / WHEN / THEN
        assert_eq!(parse_events(r#"[{"a":1}]"#).map(|v| v.len()), Some(1));
    }

    /// GIVEN a body that is not a list response
    /// WHEN  its events are extracted
    /// THEN  the failure is reported rather than read as an empty page
    ///
    /// This is the control that matters: returning `Some(vec![])` here would make
    /// the export loop exit cleanly on a malformed answer and report success.
    #[test]
    fn test_parse_events_rejects_an_unexpected_shape() {
        // GIVEN / WHEN / THEN
        assert!(parse_events(r#"{"error":"nope"}"#).is_none());
        assert!(parse_events("not json").is_none());
    }

    /// GIVEN the page-size arithmetic the export loop performs
    /// WHEN  the requested limit exceeds the server ceiling
    /// THEN  each page asks for at most the ceiling, and the last page asks only
    ///       for the remainder
    ///
    /// Asking for more than the ceiling is what made the old export silently
    /// short: the server clamped, the client believed it had everything.
    #[test]
    fn test_export_page_size_never_exceeds_the_server_ceiling() {
        // GIVEN
        let limit: u32 = 1200;
        let mut sizes = Vec::new();
        let mut collected: u32 = 0;

        // WHEN
        while collected < limit {
            let page = limit.saturating_sub(collected).min(SERVER_LIMIT_CAP);
            sizes.push(page);
            collected += page;
        }

        // THEN
        assert_eq!(sizes, vec![SERVER_LIMIT_CAP, SERVER_LIMIT_CAP, 200]);
        assert!(
            sizes.iter().all(|p| *p <= SERVER_LIMIT_CAP),
            "no page may exceed what the endpoint will actually serve"
        );
    }

    use super::replay::{group_divergences, print_divergences};
    use super::report::{parse_events, SERVER_LIMIT_CAP};
    use super::*;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(subcommand)]
        cmd: AuditCommand,
    }

    #[test]
    fn parses_list_default_limit() {
        let cli = TestCli::parse_from(["x", "list"]);
        match cli.cmd {
            AuditCommand::List { limit } => assert_eq!(limit, 20),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_list_with_limit() {
        let cli = TestCli::parse_from(["x", "list", "--limit", "100"]);
        match cli.cmd {
            AuditCommand::List { limit } => assert_eq!(limit, 100),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_stats() {
        let cli = TestCli::parse_from(["x", "stats"]);
        assert!(matches!(cli.cmd, AuditCommand::Stats));
    }

    #[test]
    fn parses_export_default_limit() {
        let cli = TestCli::parse_from(["x", "export"]);
        match cli.cmd {
            AuditCommand::Export { output, limit } => {
                assert!(output.is_none());
                assert_eq!(limit, 10_000);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_verify_with_run_id() {
        let cli = TestCli::parse_from(["x", "verify", "run-abc"]);
        match cli.cmd {
            AuditCommand::Verify { run_id } => assert_eq!(run_id.as_deref(), Some("run-abc")),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_verify_without_run_id() {
        // GIVEN the verify subcommand with no argument
        let cli = TestCli::parse_from(["x", "verify"]);
        // WHEN parsed
        // THEN the run id is absent (whole-journal verification)
        match cli.cmd {
            AuditCommand::Verify { run_id } => assert!(run_id.is_none()),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_anchor() {
        // GIVEN the anchor subcommand
        let cli = TestCli::parse_from(["x", "anchor"]);
        // WHEN parsed
        // THEN it is the Anchor variant
        assert!(matches!(cli.cmd, AuditCommand::Anchor));
    }

    #[test]
    fn parses_replay_with_run() {
        // GIVEN the replay subcommand with a run argument
        let cli = TestCli::parse_from(["x", "replay", "abc12345-run"]);
        // WHEN parsed
        // THEN the run is captured
        match cli.cmd {
            AuditCommand::Replay { run } => assert_eq!(run, "abc12345-run"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn replay_requires_run() {
        // GIVEN the replay subcommand with no run argument
        // WHEN parsing
        let result = TestCli::try_parse_from(["x", "replay"]);
        // THEN it is a usage error
        assert!(result.is_err());
    }

    #[test]
    fn groups_plan_and_input_divergences() {
        // GIVEN a divergence list with one plan_mutation and one ToolOutput
        let divergences = vec![
            serde_json::json!({
                "step_ordinal": 0, "kind": "ToolOutput",
                "expected": {"tool_name": "bash"}, "actual": {"tool_name": "python"},
            }),
            serde_json::json!({
                "step_ordinal": 1, "kind": "plan_mutation",
                "expected": {"kind": "modify_step"}, "actual": {"kind": "add_step"},
            }),
        ];

        // WHEN grouping by category
        let (plan, other) = group_divergences(&divergences);

        // THEN the plan divergence is isolated from the input divergences
        assert_eq!(plan.len(), 1);
        assert_eq!(other.len(), 1);
        assert_eq!(
            plan[0].get("kind").and_then(|v| v.as_str()),
            Some("plan_mutation")
        );
        assert_eq!(
            other[0].get("kind").and_then(|v| v.as_str()),
            Some("ToolOutput")
        );
    }

    #[test]
    fn print_divergences_includes_plan_section_without_panicking() {
        // GIVEN a diverged report carrying a plan and an LLM divergence
        let report = serde_json::json!({
            "status": "diverged",
            "run_id": "abc12345-run",
            "divergences": [
                {"step_ordinal": 0, "kind": "LlmCompletion",
                 "expected": {"a": 1}, "actual": {"a": 2}},
                {"step_ordinal": 2, "kind": "plan_mutation",
                 "expected": {"kind": "submit"}, "actual": {"kind": "approve"}},
            ],
        });

        // WHEN rendering the human output
        // THEN both categories render without a panic and the plan section is present
        print_divergences(&report, "abc12345-run");
        let (plan, other) = group_divergences(
            report
                .get("divergences")
                .and_then(|v| v.as_array())
                .expect("array"),
        );
        assert_eq!(plan.len(), 1);
        assert_eq!(other.len(), 1);
    }

    #[test]
    fn parses_export_with_output_and_limit() {
        let cli = TestCli::parse_from(["x", "export", "--output", "/tmp/a.json", "--limit", "50"]);
        match cli.cmd {
            AuditCommand::Export { output, limit } => {
                assert_eq!(output, Some(PathBuf::from("/tmp/a.json")));
                assert_eq!(limit, 50);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }
}
