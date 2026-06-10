//! `apollia-os audit` subcommands: query audit trail via the runtime API.
//!
//! Provides `list`, `stats`, `export`, and `verify` operations on the audit
//! trail and the hash-chained audit journal.

use std::path::PathBuf;

use clap::Subcommand;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

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
    /// Display audit statistics.
    Stats,
    /// Export the full audit trail as JSON.
    Export {
        /// Destination file (default: stdout).
        #[arg(long, value_name = "PATH")]
        output: Option<PathBuf>,
        /// Maximum number of events to include (default: 10000).
        #[arg(long, default_value_t = 10_000)]
        limit: u32,
    },
    /// Verify the hash chain and signatures of a run's audit journal.
    #[command(name = "verify")]
    Verify {
        /// Identifier of the run to verify.
        #[arg(value_name = "RUN_ID")]
        run_id: String,
    },
    /// Replay a captured run and detect divergences.
    ///
    /// `run` accepts a full run_id or an unambiguous prefix of at least 8
    /// characters. Exit 0 = identical, exit 2 = diverged, exit 1 = any error
    /// (run not found, ambiguous prefix, incomplete trace, runtime unreachable).
    #[command(name = "replay")]
    Replay {
        /// Run identifier or unambiguous prefix.
        #[arg(value_name = "RUN")]
        run: String,
    },
}

/// Execute an `audit` subcommand.
///
/// Returns the process exit code.
pub async fn run(cmd: &AuditCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let client = RuntimeClient::new(socket_path);

    match cmd {
        AuditCommand::List { limit } => run_list(&client, *limit, json).await,
        AuditCommand::Stats => run_stats(&client, json).await,
        AuditCommand::Export { output, limit } => {
            run_export(&client, output.as_deref(), *limit).await
        }
        AuditCommand::Verify { run_id } => run_verify(&client, run_id, json).await,
        AuditCommand::Replay { run } => run_replay(&client, run, json).await,
    }
}

/// `apollia-os audit verify <run_id>`: verify a run's chain and signatures.
///
/// Exit 0 when the chain is intact, 1 when a link is broken or the run is
/// unknown, 2 when the runtime is not reachable.
async fn run_verify(client: &RuntimeClient, run_id: &str, json: bool) -> i32 {
    let uri = format!("/api/v1/audit/verify/{run_id}");
    let resp = match client.get(&uri).await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
    };

    if resp.status == 404 {
        if json {
            let output = serde_json::json!({"ok": false, "error": "not_found"});
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_default()
            );
        } else {
            eprintln!("run_id not found in journal: {run_id}");
        }
        return exit_codes::GENERAL_ERROR;
    }
    if resp.status >= 400 {
        return handle_server_error(resp.status, &resp.body, json);
    }

    let report: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: invalid JSON response: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
    }

    let ok = report.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    if ok {
        let checked = report
            .get("entries_checked")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if !json {
            println!("OK    {run_id}  {checked} entries verified");
        }
        exit_codes::SUCCESS
    } else {
        if !json {
            let (seq, reason) = report
                .get("first_broken_link")
                .map(|link| {
                    let seq = link.get("seq").and_then(|v| v.as_u64()).unwrap_or(0);
                    let reason = link
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    (seq, reason)
                })
                .unwrap_or((0, "unknown".to_string()));
            println!("FAIL  {run_id}  broken link at seq={seq} ({reason})");
        }
        exit_codes::GENERAL_ERROR
    }
}

/// `apollia-os audit replay <run>`: replay a captured run and report divergence.
///
/// Exit 0 when the replay is identical, 2 when it diverges, 1 on any error
/// (run not found, ambiguous prefix, incomplete trace, runtime unreachable).
async fn run_replay(client: &RuntimeClient, run: &str, json: bool) -> i32 {
    let uri = format!("/api/v1/audit/replay/{run}");
    let resp = match client.post(&uri, None).await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
    };

    let report: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: invalid JSON response: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
    }

    match report.get("status").and_then(|v| v.as_str()) {
        Some("identical") => {
            if !json {
                let steps = report.get("steps").and_then(|v| v.as_u64()).unwrap_or(0);
                let rid = report.get("run_id").and_then(|v| v.as_str()).unwrap_or(run);
                println!(
                    "replay: identical ({steps} steps)  {}",
                    short_uuid_prefix(rid)
                );
            }
            exit_codes::SUCCESS
        }
        Some("diverged") => {
            if !json {
                print_divergences(&report, run);
            }
            // A determinism violation is surfaced as a runtime error (exit 2).
            exit_codes::RUNTIME_ERROR
        }
        _ => {
            if !json {
                print_replay_error(&report, run);
            }
            exit_codes::GENERAL_ERROR
        }
    }
}

/// Print the divergence table on stdout (TTY mode).
fn print_divergences(report: &serde_json::Value, run: &str) {
    let rid = report.get("run_id").and_then(|v| v.as_str()).unwrap_or(run);
    println!("replay: diverged  {}", short_uuid_prefix(rid));
    if let Some(divergences) = report.get("divergences").and_then(|v| v.as_array()) {
        for divergence in divergences {
            let step = divergence
                .get("step_ordinal")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let kind = divergence
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let expected = divergence.get("expected").cloned().unwrap_or_default();
            let actual = divergence.get("actual").cloned().unwrap_or_default();
            println!("  step {step}  {kind}  expected={expected}  actual={actual}");
        }
    }
}

/// Print a human-readable error line for a failed replay (TTY mode).
fn print_replay_error(report: &serde_json::Value, run: &str) {
    match report.get("code").and_then(|v| v.as_str()) {
        Some("run_not_found") => eprintln!("error: run not found: {run}"),
        Some("ambiguous_run_id") => {
            let candidates = report
                .get("candidates")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|c| c.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            eprintln!("error: ambiguous prefix '{run}', matches: [{candidates}]");
        }
        Some("incomplete_trace") => {
            let kind = report
                .get("missing_kind")
                .and_then(|v| v.as_str())
                .unwrap_or("entry");
            let step = report.get("step").and_then(|v| v.as_u64()).unwrap_or(0);
            eprintln!("error: incomplete trace for run {run}, missing {kind} at step {step}");
        }
        other => eprintln!("error: replay failed ({})", other.unwrap_or("unknown")),
    }
}

/// `apollia-os audit export`: dump the audit trail as JSON.
async fn run_export(client: &RuntimeClient, output: Option<&std::path::Path>, limit: u32) -> i32 {
    let uri = format!("/api/v1/audit?limit={limit}");
    match client.get(&uri).await {
        Ok(resp) if resp.status < 400 => match output {
            Some(path) => match std::fs::write(path, &resp.body) {
                Ok(()) => {
                    eprintln!("* wrote {} bytes to {}", resp.body.len(), path.display());
                    exit_codes::SUCCESS
                }
                Err(e) => {
                    eprintln!("Error writing {}: {e}", path.display());
                    exit_codes::GENERAL_ERROR
                }
            },
            None => {
                println!("{}", resp.body);
                exit_codes::SUCCESS
            }
        },
        Ok(resp) => {
            eprintln!("Error: HTTP {}: {}", resp.status, resp.body);
            exit_codes::GENERAL_ERROR
        }
        Err(ClientError::ConnectionRefused) => {
            eprintln!("Error: runtime not started");
            exit_codes::RUNTIME_ERROR
        }
        Err(e) => {
            eprintln!("Error: {e}");
            exit_codes::GENERAL_ERROR
        }
    }
}

/// `apollia-os audit list`: display recent audit events.
async fn run_list(client: &RuntimeClient, limit: u32, json: bool) -> i32 {
    let uri = format!("/api/v1/audit?limit={limit}");
    let resp = match client.get(&uri).await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
    };

    if resp.status >= 400 {
        return handle_server_error(resp.status, &resp.body, json);
    }

    let parsed: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: invalid JSON response: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&parsed).unwrap_or_default()
        );
    } else {
        // Best-effort agent_id → name lookup. We never block the audit
        // display on the agents endpoint: missing entries fall back to a
        // short UUID prefix in the formatter.
        let agent_names = client
            .list_agents()
            .await
            .ok()
            .and_then(|v| {
                v.get("agents")
                    .or_else(|| Some(&v))
                    .and_then(|x| x.as_array())
                    .cloned()
            })
            .map(|agents| {
                agents
                    .iter()
                    .filter_map(|a| {
                        let id = a.get("agent_id").or_else(|| a.get("id"))?.as_str()?;
                        let name = a.get("name")?.as_str()?;
                        Some((id.to_string(), name.to_string()))
                    })
                    .collect::<std::collections::HashMap<String, String>>()
            })
            .unwrap_or_default();
        format_audit_list(&parsed, &agent_names);
    }
    exit_codes::SUCCESS
}

/// `apollia-os audit stats`: display audit statistics.
async fn run_stats(client: &RuntimeClient, json: bool) -> i32 {
    let resp = match client.get("/api/v1/audit/stats").await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
    };

    if resp.status >= 400 {
        return handle_server_error(resp.status, &resp.body, json);
    }

    let parsed: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: invalid JSON response: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&parsed).unwrap_or_default()
        );
    } else {
        format_audit_stats(&parsed);
    }
    exit_codes::SUCCESS
}

/// Format audit events as a human-readable table.
///
/// `agent_names` is populated upstream from `GET /api/v1/agents`; unknown
/// IDs fall back to the first UUID segment so the column still aligns and
/// the operator can still copy-paste the value.
fn format_audit_list(
    resp: &serde_json::Value,
    agent_names: &std::collections::HashMap<String, String>,
) {
    let events = resp
        .get("events")
        .and_then(|e| e.as_array())
        .cloned()
        .unwrap_or_default();

    println!(
        "  {:<19} {:<24} {:<20} {:<8} {:<8}",
        "TIMESTAMP", "AGENT", "TOOL", "STATUS", "MS"
    );

    if events.is_empty() {
        println!("  (no audit events)");
    } else {
        for event in &events {
            // API field names: started_at (RFC3339), success (bool), duration_ms (u64)
            let ts = event
                .get("started_at")
                .and_then(|v| v.as_str())
                // Trim to 23 chars (drop sub-second precision) for compact display
                .map(|s| s.get(..19).unwrap_or(s))
                .unwrap_or("?");
            let agent_id = event
                .get("agent_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let agent = agent_names
                .get(agent_id)
                .cloned()
                .unwrap_or_else(|| short_uuid_prefix(agent_id));
            let agent = agent.as_str();
            let tool = event
                .get("tool_name")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let status = match event.get("success").and_then(|v| v.as_bool()) {
                Some(true) => "ok",
                Some(false) => "failed",
                None => "?",
            };
            let ms = event
                .get("duration_ms")
                .and_then(|v| v.as_u64())
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string());
            println!(
                "  {:<19} {:<24} {:<20} {:<8} {:<8}",
                ts, agent, tool, status, ms
            );
        }
    }
}

/// Shorten a UUID to its first hyphen-separated segment for display.
fn short_uuid_prefix(uuid: &str) -> String {
    uuid.split('-').next().unwrap_or(uuid).to_string()
}

/// Format audit stats as human-readable text.
fn format_audit_stats(resp: &serde_json::Value) {
    let total = resp
        .get("total_events")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let tools_used = resp
        .get("unique_tools")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let agents = resp
        .get("unique_agents")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    println!("  Total events  : {total}");
    println!("  Unique tools  : {tools_used}");
    println!("  Unique agents : {agents}");
}

/// Handle client errors uniformly.
fn handle_error(err: ClientError, json: bool) -> i32 {
    match err {
        ClientError::ConnectionRefused => {
            if json {
                let output =
                    serde_json::json!({"error": "runtime not started (connection refused)"});
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                );
            } else {
                eprintln!("Error: runtime not started (connection refused)");
            }
            exit_codes::RUNTIME_ERROR
        }
        other => {
            if json {
                let output = serde_json::json!({"error": other.to_string()});
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                );
            } else {
                eprintln!("Error: {other}");
            }
            exit_codes::GENERAL_ERROR
        }
    }
}

/// Handle HTTP server errors.
fn handle_server_error(status: u16, body: &str, json: bool) -> i32 {
    let error_msg = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
        .unwrap_or_else(|| format!("server error ({status})"));

    if json {
        let output = serde_json::json!({"error": error_msg});
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
    } else {
        eprintln!("Error: {error_msg}");
    }
    exit_codes::GENERAL_ERROR
}

#[cfg(test)]
mod tests {
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
            AuditCommand::Verify { run_id } => assert_eq!(run_id, "run-abc"),
            other => panic!("unexpected: {other:?}"),
        }
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
    fn verify_requires_run_id() {
        // GIVEN the verify subcommand with no run id
        // WHEN parsing
        let result = TestCli::try_parse_from(["x", "verify"]);
        // THEN it is a usage error
        assert!(result.is_err());
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
