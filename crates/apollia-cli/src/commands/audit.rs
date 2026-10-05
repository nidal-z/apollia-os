//! `apollia-os audit` subcommands: query audit trail via the runtime API.
//!
//! Provides `list` and `stats` on the audit trail, and `journal`, `export`,
//! `show`, `verify`, `anchor` and `replay` on the hash-chained audit journal.

use std::path::PathBuf;

use clap::Subcommand;

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;

mod export;
mod journal;
mod replay;
mod report;
mod verify;

pub use export::JournalExportFormat;
use export::{run_export, JournalExport};
use journal::{run_journal, run_show, JournalPage};
use replay::{run_anchor, run_replay};
use report::{run_list, run_stats};
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
    /// front, so the audited register is reachable by browsing. Each entry is
    /// printed as its summary: when, who, what, on what, with which result,
    /// and never the content of a call. The filters add up, and `--limit` and
    /// `--offset` count the entries that pass them.
    #[command(name = "journal")]
    Journal {
        /// Maximum number of entries to display.
        #[arg(long, default_value = "20")]
        limit: u32,
        /// Number of entries to skip, newest first. Page through with it.
        #[arg(long, default_value_t = 0)]
        offset: u32,
        /// Keep what names this agent, and the runs of its tasks. Repeat it
        /// for several agents.
        #[arg(long = "agent", value_name = "AGENT")]
        agents: Vec<String>,
        /// Keep this action family: read, read_personal, write_or_delete,
        /// send_external, execute_code. Repeat it for several.
        #[arg(long = "family", value_name = "FAMILY")]
        families: Vec<String>,
        /// Keep this nature: action, access_change, policy, system. Repeat it
        /// for several.
        #[arg(long = "nature", value_name = "NATURE")]
        natures: Vec<String>,
        /// Keep this result: success, failure, refused. Repeat it for several.
        #[arg(long = "result", value_name = "RESULT")]
        results: Vec<String>,
        /// Start of the period, included: YYYY-MM-DD (UTC) or an RFC 3339
        /// instant.
        #[arg(long, value_name = "DATE")]
        since: Option<String>,
        /// End of the period, excluded: YYYY-MM-DD (UTC) or an RFC 3339
        /// instant.
        #[arg(long, value_name = "DATE")]
        until: Option<String>,
    },
    /// Display audit statistics.
    Stats,
    /// Export the hash-chained journal to a file, as CSV or JSON.
    ///
    /// The file holds every entry that passes the filters, newest first, each
    /// as its summary: when, who, what, on what, with which result, and its
    /// place in the chain. `--detail` adds the payload of each entry and the
    /// arguments and output of each tool call, which may hold personal data.
    /// The filters are those of `audit journal`. The file is written on this
    /// machine, and the export is itself recorded in the journal.
    Export {
        /// Format of the file.
        #[arg(long, value_enum)]
        format: JournalExportFormat,
        /// Add the detail of each entry: its payload, and the arguments and
        /// output of its tool call. May hold personal data.
        #[arg(long)]
        detail: bool,
        /// Keep what names this agent, and the runs of its tasks. Repeat it
        /// for several agents.
        #[arg(long = "agent", value_name = "AGENT")]
        agents: Vec<String>,
        /// Keep this action family: read, read_personal, write_or_delete,
        /// send_external, execute_code. Repeat it for several.
        #[arg(long = "family", value_name = "FAMILY")]
        families: Vec<String>,
        /// Keep this nature: action, access_change, policy, system. Repeat it
        /// for several.
        #[arg(long = "nature", value_name = "NATURE")]
        natures: Vec<String>,
        /// Keep this result: success, failure, refused. Repeat it for several.
        #[arg(long = "result", value_name = "RESULT")]
        results: Vec<String>,
        /// Start of the period, included: YYYY-MM-DD (UTC) or an RFC 3339
        /// instant.
        #[arg(long, value_name = "DATE")]
        since: Option<String>,
        /// End of the period, excluded: YYYY-MM-DD (UTC) or an RFC 3339
        /// instant.
        #[arg(long, value_name = "DATE")]
        until: Option<String>,
        /// File the export is written to.
        #[arg(long, value_name = "PATH")]
        output: PathBuf,
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
    /// Unlike `audit list` (the tool-only audit trail), this reads the
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
        AuditCommand::Journal {
            limit,
            offset,
            agents,
            families,
            natures,
            results,
            since,
            until,
        } => {
            let page = JournalPage {
                limit: *limit,
                offset: *offset,
                agents,
                families,
                natures,
                results,
                since: since.as_deref(),
                until: until.as_deref(),
            };
            run_journal(&client, &page, json).await
        }
        AuditCommand::Stats => run_stats(&client, json).await,
        AuditCommand::Export {
            format,
            detail,
            agents,
            families,
            natures,
            results,
            since,
            until,
            output,
        } => {
            let export = JournalExport {
                format: *format,
                detail: *detail,
                agents,
                families,
                natures,
                results,
                since: since.as_deref(),
                until: until.as_deref(),
            };
            run_export(&client, &export, output, json).await
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

    use super::replay::{group_divergences, print_divergences};
    use super::*;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(subcommand)]
        cmd: AuditCommand,
    }

    #[test]
    fn parses_list_default_limit() {
        // GIVEN "audit list", with no --limit
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "list"]);
        // THEN the limit falls back to twenty
        match cli.cmd {
            AuditCommand::List { limit } => assert_eq!(limit, 20),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_list_with_limit() {
        // GIVEN the same listing carrying --limit 100
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "list", "--limit", "100"]);
        // THEN the limit given wins over the default
        match cli.cmd {
            AuditCommand::List { limit } => assert_eq!(limit, 100),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_stats() {
        // GIVEN "audit stats"
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "stats"]);
        // THEN the stats subcommand is selected
        assert!(matches!(cli.cmd, AuditCommand::Stats));
    }

    #[test]
    fn parses_export_with_its_format_and_its_output() {
        // GIVEN "audit export" with a format and a file, and nothing else
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "export", "--format", "csv", "--output", "/tmp/j.csv"]);
        // THEN the export is a summary of the whole journal, written to that file
        match cli.cmd {
            AuditCommand::Export {
                format,
                detail,
                agents,
                families,
                natures,
                results,
                since,
                until,
                output,
            } => {
                assert_eq!(format, JournalExportFormat::Csv);
                assert!(!detail);
                assert!(agents.is_empty() && families.is_empty());
                assert!(natures.is_empty() && results.is_empty());
                assert!(since.is_none() && until.is_none());
                assert_eq!(output, PathBuf::from("/tmp/j.csv"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_export_with_the_detail_and_every_filter() {
        // GIVEN a JSON export with the detail and every filter of the journal
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from([
            "x",
            "export",
            "--format",
            "json",
            "--detail",
            "--agent",
            "quotes",
            "--agent",
            "invoices",
            "--family",
            "read",
            "--nature",
            "action",
            "--result",
            "refused",
            "--since",
            "2026-10-01",
            "--until",
            "2026-10-04",
            "--output",
            "j.json",
        ]);
        // THEN each one is captured
        match cli.cmd {
            AuditCommand::Export {
                format,
                detail,
                agents,
                families,
                natures,
                results,
                since,
                until,
                output,
            } => {
                assert_eq!(format, JournalExportFormat::Json);
                assert!(detail);
                assert_eq!(agents, ["quotes", "invoices"]);
                assert_eq!(families, ["read"]);
                assert_eq!(natures, ["action"]);
                assert_eq!(results, ["refused"]);
                assert_eq!(since.as_deref(), Some("2026-10-01"));
                assert_eq!(until.as_deref(), Some("2026-10-04"));
                assert_eq!(output, PathBuf::from("j.json"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn export_requires_a_known_format_and_an_output() {
        // GIVEN an export with no format, one with no file, one in a format
        // the journal is not written in, and the options of the former export
        // WHEN clap parses each argument line
        let no_format = TestCli::try_parse_from(["x", "export", "--output", "j.csv"]);
        let no_output = TestCli::try_parse_from(["x", "export", "--format", "csv"]);
        let pdf = TestCli::try_parse_from(["x", "export", "--format", "pdf", "--output", "j"]);
        let limit = TestCli::try_parse_from([
            "x", "export", "--format", "csv", "--output", "j.csv", "--limit", "100",
        ]);
        // THEN each is a usage error
        assert!(no_format.is_err());
        assert!(no_output.is_err());
        assert!(pdf.is_err());
        assert!(limit.is_err());
    }

    #[test]
    fn parses_verify_with_run_id() {
        // GIVEN "audit verify run-abc"
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "verify", "run-abc"]);
        // THEN the run identifier is captured, which narrows the verification to that run
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
}
