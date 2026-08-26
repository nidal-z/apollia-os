//! `audit anchor` and `audit replay`, and the divergence reporting.

use crate::client::RuntimeClient;
use crate::exit_codes;
use crate::note;

use super::journal::resolve_task_to_run;
use super::report::short_uuid_prefix;
use super::{handle_error, handle_server_error};

/// `apollia-os audit anchor`: print the exportable head anchor.
///
/// Exit 0 when an anchor is returned, 1 when the journal has no entries yet, 2
/// when the runtime is not reachable.
pub(super) async fn run_anchor(client: &RuntimeClient, json: bool) -> i32 {
    let resp = match client.get("/api/v1/audit/anchor").await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
    };
    if resp.status == 404 {
        return crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            "journal has no entries yet",
        );
    }
    if resp.status >= 400 {
        return handle_server_error(resp.status, &resp.body, json);
    }

    let anchor: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            return crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!("invalid JSON response: {e}"),
            );
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
pub(super) async fn run_replay(client: &RuntimeClient, run: &str, json: bool) -> i32 {
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
            return crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!("invalid JSON response: {e}"),
            );
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
pub(super) fn print_divergences(report: &serde_json::Value, run: &str) {
    let rid = report.get("run_id").and_then(|v| v.as_str()).unwrap_or(run);
    println!("replay: diverged  {}", short_uuid_prefix(rid));
    let Some(divergences) = report.get("divergences").and_then(|v| v.as_array()) else {
        return;
    };

    let (plan, other) = group_divergences(divergences);

    if !other.is_empty() {
        note!("  inputs ({} divergence(s)):", other.len());
        for divergence in &other {
            print_divergence_line(divergence);
        }
    }
    if !plan.is_empty() {
        note!("  plan ({} divergence(s)):", plan.len());
        for divergence in &plan {
            print_divergence_line(divergence);
        }
    }
}

/// Partition divergences into `(plan, other)` by category.
///
/// `plan` holds the plan-construction divergences (`kind = "plan_mutation"`);
/// `other` holds the input-replay divergences (LLM, tools).
pub(super) fn group_divergences(
    divergences: &[serde_json::Value],
) -> (Vec<&serde_json::Value>, Vec<&serde_json::Value>) {
    divergences
        .iter()
        .partition(|d| d.get("kind").and_then(|v| v.as_str()) == Some("plan_mutation"))
}

/// Print one divergence row (step, kind, expected, actual).
pub(super) fn print_divergence_line(divergence: &serde_json::Value) {
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
pub(super) fn print_replay_error(report: &serde_json::Value, run: &str) {
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
