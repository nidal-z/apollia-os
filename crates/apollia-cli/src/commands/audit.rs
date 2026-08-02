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
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let client = RuntimeClient::new(socket_path);

    match cmd {
        AuditCommand::List { limit } => run_list(&client, *limit, json).await,
        AuditCommand::Stats => run_stats(&client, json).await,
        AuditCommand::Export { output, limit } => {
            run_export(&client, output.as_deref(), *limit).await
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

/// `apollia-os audit show <run>`: print a run's journal (tool calls + LLM
/// completions). Accepts a run_id or a task_id (resolved to its run_id).
async fn run_show(client: &RuntimeClient, arg: &str, json: bool) -> i32 {
    // Resolve a task_id to its run_id if the argument is not a known run.
    let mut run = arg.to_string();
    let resp = match fetch_journal(client, &run).await {
        Some(r) => Some(r),
        None => {
            if let Some(rid) = resolve_task_to_run(client, arg).await {
                if rid != run {
                    run = rid;
                    fetch_journal(client, &run).await
                } else {
                    None
                }
            } else {
                None
            }
        }
    };

    let Some(body) = resp else {
        if json {
            println!("{}", serde_json::json!({"error": "not_found", "run": arg}));
        } else {
            eprintln!("no journal entries for '{arg}' (tried as run_id and task_id)");
        }
        return exit_codes::GENERAL_ERROR;
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
        return exit_codes::SUCCESS;
    }

    let entries = body
        .get("entries")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    println!("  Journal for run {run}  ({} entries)", entries.len());
    for e in &entries {
        let seq = e
            .get("seq")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let kind = e.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
        let signed = if e.get("signature").map(|v| !v.is_null()).unwrap_or(false) {
            "signed"
        } else {
            "unsigned"
        };
        let summary = journal_entry_summary(kind, e.get("payload"));
        println!("  [{seq:>3}] {kind:<20} {signed:<8} {summary}");
    }
    exit_codes::SUCCESS
}

/// Fetch a run's journal entries; `None` on 404 / error.
async fn fetch_journal(client: &RuntimeClient, run_id: &str) -> Option<serde_json::Value> {
    let resp = client
        .get(&format!("/api/v1/audit/journal/{run_id}"))
        .await
        .ok()?;
    if resp.status >= 400 {
        return None;
    }
    serde_json::from_str(&resp.body).ok()
}

/// One-line summary of a journal entry for the human view (the model's response
/// text for `llm_completion`, the tool name for tool calls).
fn journal_entry_summary(kind: &str, payload: Option<&serde_json::Value>) -> String {
    let Some(p) = payload else {
        return String::new();
    };
    match kind {
        "llm_completion" => {
            let content = p.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let one_line = content.replace('\n', " ");
            let trimmed: String = one_line.chars().take(80).collect();
            if one_line.chars().count() > 80 {
                format!("{trimmed}...")
            } else {
                trimmed
            }
        }
        _ => p
            .get("tool_name")
            .or_else(|| p.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    }
}

/// `apollia-os audit verify <run_id>`: verify a run's chain and signatures.
///
/// Exit 0 when the chain is intact, 1 when a link is broken or the run is
/// unknown, 2 when the runtime is not reachable.
/// Resolve a `task_id` to the `run_id` it belongs to via `GET /api/v1/tasks/{id}`.
///
/// Returns `None` when the id is unknown or carries no run_id (e.g. a task
/// submitted before run_id assignment).
async fn resolve_task_to_run(client: &RuntimeClient, id: &str) -> Option<String> {
    let resp = client.get(&format!("/api/v1/tasks/{id}")).await.ok()?;
    if resp.status >= 400 {
        return None;
    }
    let body: serde_json::Value = serde_json::from_str(&resp.body).ok()?;
    body.get("run_id")
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Outcome of a single verify request.
enum VerifyOutcome {
    /// A terminal result was produced; carries the process exit code.
    Done(i32),
    /// The run_id is unknown to the journal (candidate for task_id resolution).
    NotFound,
}

async fn run_verify(client: &RuntimeClient, arg: &str, json: bool) -> i32 {
    // First attempt: treat the argument as a run_id.
    match verify_once(client, arg, json).await {
        VerifyOutcome::Done(code) => code,
        VerifyOutcome::NotFound => {
            // Fall back: the argument may be a task_id. Resolve it to its run_id
            // and retry once (the journal is keyed by run_id, not task_id).
            if let Some(rid) = resolve_task_to_run(client, arg).await {
                if rid != arg {
                    if let VerifyOutcome::Done(code) = verify_once(client, &rid, json).await {
                        return code;
                    }
                }
            }
            if json {
                let output = serde_json::json!({"ok": false, "error": "not_found"});
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                );
            } else {
                eprintln!("not found in journal (tried as run_id and task_id): {arg}");
            }
            exit_codes::GENERAL_ERROR
        }
    }
}

/// Issue one verify request for `run_id` and interpret the response.
async fn verify_once(client: &RuntimeClient, run_id: &str, json: bool) -> VerifyOutcome {
    let uri = format!("/api/v1/audit/verify/{run_id}");
    let resp = match client.get(&uri).await {
        Ok(r) => r,
        Err(e) => return VerifyOutcome::Done(handle_error(e, json)),
    };

    if resp.status == 404 {
        return VerifyOutcome::NotFound;
    }
    if resp.status >= 400 {
        return VerifyOutcome::Done(handle_server_error(resp.status, &resp.body, json));
    }

    let report: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: invalid JSON response: {e}");
            return VerifyOutcome::Done(exit_codes::GENERAL_ERROR);
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
        VerifyOutcome::Done(exit_codes::SUCCESS)
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
        VerifyOutcome::Done(exit_codes::GENERAL_ERROR)
    }
}

/// `apollia-os audit verify` (no run): verify the whole journal.
///
/// Exit 0 when the global chain, every per-run chain, and the head anchor all
/// verify, 1 when a link is broken or the head anchor does not match, 2 when the
/// runtime is not reachable.
async fn run_verify_journal(client: &RuntimeClient, json: bool) -> i32 {
    let resp = match client.get("/api/v1/audit/verify").await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
    };
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
    let checked = report
        .get("entries_checked")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let runs = report
        .get("runs_checked")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if ok {
        if !json {
            println!("OK    whole journal  {checked} entries across {runs} runs verified");
        }
        exit_codes::SUCCESS
    } else {
        if !json {
            match report.get("first_break") {
                Some(brk) if !brk.is_null() => {
                    let seq = brk.get("global_seq").and_then(|v| v.as_u64()).unwrap_or(0);
                    let run = brk.get("run_id").and_then(|v| v.as_str()).unwrap_or("?");
                    let reason = brk
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    println!(
                        "FAIL  whole journal  broken link at global_seq={seq} run={run} ({reason})"
                    );
                }
                _ => {
                    println!(
                        "FAIL  whole journal  head anchor mismatch (tail truncation or rolled-back state)"
                    );
                }
            }
        }
        exit_codes::GENERAL_ERROR
    }
}

/// `apollia-os audit anchor`: print the exportable head anchor.
///
/// Exit 0 when an anchor is returned, 1 when the journal has no entries yet, 2
/// when the runtime is not reachable.
async fn run_anchor(client: &RuntimeClient, json: bool) -> i32 {
    let resp = match client.get("/api/v1/audit/anchor").await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
    };
    if resp.status == 404 {
        if json {
            let output = serde_json::json!({"error": "not_found"});
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_default()
            );
        } else {
            eprintln!("journal has no entries yet");
        }
        return exit_codes::GENERAL_ERROR;
    }
    if resp.status >= 400 {
        return handle_server_error(resp.status, &resp.body, json);
    }

    let anchor: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: invalid JSON response: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&anchor).unwrap_or_default()
        );
    } else {
        let seq = anchor
            .get("global_seq")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let hash = anchor
            .get("global_hash")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let key = anchor
            .get("key_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unsigned");
        let ts = anchor
            .get("updated_ts")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        println!("global_seq={seq}");
        println!("global_hash={hash}");
        println!("key_id={key}");
        println!("updated_ts={ts}");
    }
    exit_codes::SUCCESS
}

/// `apollia-os audit replay <run>`: replay a captured run and report divergence.
///
/// Exit 0 when the replay is identical, 2 when it diverges, 1 on any error
/// (run not found, ambiguous prefix, incomplete trace, runtime unreachable).
async fn run_replay(client: &RuntimeClient, run: &str, json: bool) -> i32 {
    let mut effective = run.to_string();
    let mut resp = match client
        .post(&format!("/api/v1/audit/replay/{effective}"), None)
        .await
    {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
    };
    // Fall back: the argument may be a task_id. Resolve it to its run_id and
    // retry once (the journal is keyed by run_id, not task_id).
    if resp.status == 404 {
        if let Some(rid) = resolve_task_to_run(client, run).await {
            if rid != run {
                effective = rid;
                resp = match client
                    .post(&format!("/api/v1/audit/replay/{effective}"), None)
                    .await
                {
                    Ok(r) => r,
                    Err(e) => return handle_error(e, json),
                };
            }
        }
    }
    let run = effective.as_str();

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

/// Print the divergence table on stdout (TTY mode), grouped by category.
///
/// Plan-construction divergences (`kind = "plan_mutation"`) are reported under a
/// dedicated section alongside the input categories (LLM, tools), so the operator
/// sees at a glance whether the plan itself drifted.
fn print_divergences(report: &serde_json::Value, run: &str) {
    let rid = report.get("run_id").and_then(|v| v.as_str()).unwrap_or(run);
    println!("replay: diverged  {}", short_uuid_prefix(rid));
    let Some(divergences) = report.get("divergences").and_then(|v| v.as_array()) else {
        return;
    };

    let (plan, other) = group_divergences(divergences);

    if !other.is_empty() {
        println!("  inputs ({} divergence(s)):", other.len());
        for divergence in &other {
            print_divergence_line(divergence);
        }
    }
    if !plan.is_empty() {
        println!("  plan ({} divergence(s)):", plan.len());
        for divergence in &plan {
            print_divergence_line(divergence);
        }
    }
}

/// Partition divergences into `(plan, other)` by category.
///
/// `plan` holds the plan-construction divergences (`kind = "plan_mutation"`);
/// `other` holds the input-replay divergences (LLM, tools).
fn group_divergences(
    divergences: &[serde_json::Value],
) -> (Vec<&serde_json::Value>, Vec<&serde_json::Value>) {
    divergences
        .iter()
        .partition(|d| d.get("kind").and_then(|v| v.as_str()) == Some("plan_mutation"))
}

/// Print one divergence row (step, kind, expected, actual).
fn print_divergence_line(divergence: &serde_json::Value) {
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
    println!("    step {step}  {kind}  expected={expected}  actual={actual}");
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

/// The server-side ceiling on `/api/v1/audit?limit=`, which clamps the query to
/// keep it bounded. Asking for more than this returns this many.
const SERVER_LIMIT_CAP: u32 = 500;

/// `apollia-os audit export`: dump the audit trail as JSON, bounded by `limit`.
///
/// Warns when the export comes back exactly at the limit. A caller archiving a
/// trail has no way of telling a complete export from a truncated one by looking
/// at the file, and an archive silently missing its oldest entries is worse than
/// one that is visibly partial.
async fn run_export(client: &RuntimeClient, output: Option<&std::path::Path>, limit: u32) -> i32 {
    let mut events: Vec<serde_json::Value> = Vec::new();
    let mut offset: u32 = 0;

    // Page until a short page comes back. The endpoint caps one query at
    // SERVER_LIMIT_CAP whatever is asked, so a single request can never be an
    // export: before `offset` existed, everything older than one page was
    // unreachable through the API at all, and the file written here was silently
    // the newest 500 events under whatever name the operator chose.
    loop {
        let page = limit
            .saturating_sub(events.len() as u32)
            .min(SERVER_LIMIT_CAP);
        if page == 0 {
            break;
        }
        let uri = format!("/api/v1/audit?limit={page}&offset={offset}");
        let resp = match client.get(&uri).await {
            Ok(r) if r.status < 400 => r,
            Ok(r) => {
                eprintln!("Error: HTTP {}: {}", r.status, r.body);
                return exit_codes::GENERAL_ERROR;
            }
            Err(ClientError::ConnectionRefused) => {
                eprintln!("Error: runtime not started");
                return exit_codes::RUNTIME_ERROR;
            }
            Err(e) => {
                eprintln!("Error: {e}");
                return exit_codes::GENERAL_ERROR;
            }
        };

        let batch = match parse_events(&resp.body) {
            Some(b) => b,
            None => {
                eprintln!("Error: unexpected response shape from /api/v1/audit");
                return exit_codes::GENERAL_ERROR;
            }
        };
        let got = batch.len() as u32;
        events.extend(batch);
        // A page shorter than requested is the end of the trail. Equality is not
        // a stop condition: a trail that happens to be an exact multiple of the
        // page size would end one page early.
        if got < page {
            break;
        }
        offset += got;
    }

    let count = events.len();
    let body = match serde_json::to_string_pretty(
        &serde_json::json!({ "events": events, "count": count }),
    ) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error serializing the export: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    if count as u32 == limit {
        eprintln!(
            "! export stopped at the --limit of {limit} events; older entries are \
             missing. Re-run with a higher --limit for a complete archive."
        );
    }

    match output {
        Some(path) => match std::fs::write(path, &body) {
            Ok(()) => {
                eprintln!(
                    "* wrote {count} events ({} bytes) to {}",
                    body.len(),
                    path.display()
                );
                exit_codes::SUCCESS
            }
            Err(e) => {
                eprintln!("Error writing {}: {e}", path.display());
                exit_codes::GENERAL_ERROR
            }
        },
        None => {
            println!("{body}");
            exit_codes::SUCCESS
        }
    }
}

/// Pulls the `events` array out of a list response, in either accepted shape.
fn parse_events(body: &str) -> Option<Vec<serde_json::Value>> {
    let parsed = serde_json::from_str::<serde_json::Value>(body).ok()?;
    parsed
        .get("events")
        .and_then(|v| v.as_array())
        .or_else(|| parsed.as_array())
        .cloned()
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
                    .or(Some(&v))
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
        "  {:<19} {:<24} {:<20} {:<8} {:<8} {:<10}",
        "TIMESTAMP", "AGENT", "TOOL", "STATUS", "MS", "RUN"
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
            // Full run_id (last column): it must be copy-pasteable into
            // `audit verify <run_id>`, so it is NOT truncated.
            let run = event
                .get("run_id")
                .and_then(|v| v.as_str())
                .unwrap_or("-")
                .to_string();
            println!(
                "  {:<19} {:<24} {:<20} {:<8} {:<8} {:<10}",
                ts, agent, tool, status, ms, run
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
