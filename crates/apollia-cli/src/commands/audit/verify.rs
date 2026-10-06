//! `audit verify` and `audit verify-journal`: signature and chain checks.

use crate::client::RuntimeClient;
use crate::exit_codes;

use super::journal::resolve_task_to_run;
use super::{handle_error, handle_server_error};

/// Outcome of a single verify request.
pub(super) enum VerifyOutcome {
    /// A terminal result was produced; carries the process exit code.
    Done(i32),
    /// The run_id is unknown to the journal (candidate for task_id resolution).
    NotFound,
}

pub(super) async fn run_verify(client: &RuntimeClient, arg: &str, json: bool) -> i32 {
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
pub(super) async fn verify_once(client: &RuntimeClient, run_id: &str, json: bool) -> VerifyOutcome {
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
            return VerifyOutcome::Done(crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!("invalid JSON response: {e}"),
            ));
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

/// The line that says where the verification started, for a journal whose
/// beginning a retention purge removed. `None` for a journal verified from
/// its origin.
fn start_line(report: &serde_json::Value) -> Option<String> {
    let start = report.get("start").filter(|start| !start.is_null())?;
    let global_seq = start.get("global_seq").and_then(|v| v.as_u64())?;
    let ts = start.get("ts").and_then(|v| v.as_str()).unwrap_or("?");
    let purged_at = start
        .get("purged_at")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    Some(format!(
        "      verified from global_seq={global_seq} ({ts}): earlier entries were removed \
         by the retention of the organisation policy on {purged_at}"
    ))
}

/// `apollia-os audit verify` (no run): verify the whole journal.
///
/// A journal whose beginning the retention of the organisation policy removed
/// is verified from the purge entry, and the output says from where.
///
/// Exit 0 when the global chain, every per-run chain, and the head anchor all
/// verify, 1 when a link is broken or the head anchor does not match, 2 when the
/// runtime is not reachable.
pub(super) async fn run_verify_journal(client: &RuntimeClient, json: bool) -> i32 {
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
            if let Some(line) = start_line(&report) {
                println!("{line}");
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_purged_journal_says_where_its_verification_starts() {
        // GIVEN the report of a journal whose beginning a purge removed, and
        // the report of one verified from its origin
        let purged = serde_json::json!({
            "ok": true,
            "start": {
                "global_seq": 42,
                "ts": "2026-06-01T08:00:00Z",
                "purged_at": "2026-06-30T00:00:00Z",
            },
        });
        let whole = serde_json::json!({ "ok": true, "start": null });

        // WHEN the start line of each is asked
        let line = start_line(&purged);

        // THEN the purged one names the position, its date and the purge
        let line = line.expect("a start line");
        assert!(line.contains("global_seq=42"), "{line}");
        assert!(line.contains("2026-06-01T08:00:00Z"), "{line}");
        assert!(line.contains("2026-06-30T00:00:00Z"), "{line}");
        // AND the other has none, as does a report from an older runtime
        assert_eq!(start_line(&whole), None);
        assert_eq!(start_line(&serde_json::json!({ "ok": true })), None);
    }
}
