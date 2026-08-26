//! `audit show` and `audit journal`: read the journal of a run.

use crate::client::RuntimeClient;
use crate::exit_codes;

use super::{handle_error, handle_server_error};

/// `apollia-os audit show <run>`: print a run's journal (tool calls + LLM
/// completions). Accepts a run_id or a task_id (resolved to its run_id).
pub(super) async fn run_show(client: &RuntimeClient, arg: &str, json: bool) -> i32 {
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
        return crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("no journal entries for '{arg}' (tried as run_id and task_id)"),
        );
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

/// `apollia-os audit journal`: print a page of the chained journal across every
/// run, newest global position first.
pub(super) async fn run_journal(
    client: &RuntimeClient,
    limit: u32,
    offset: u32,
    json: bool,
) -> i32 {
    let uri = format!("/api/v1/audit/journal?limit={limit}&offset={offset}");
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
            serde_json::to_string_pretty(&parsed).unwrap_or_default()
        );
        return exit_codes::SUCCESS;
    }

    let entries = parsed
        .get("entries")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    if entries.is_empty() {
        println!("  No journal entries.");
        return exit_codes::SUCCESS;
    }

    println!("  Audit journal  ({} entries)", entries.len());
    for e in &entries {
        let run = e.get("run_id").and_then(|v| v.as_str()).unwrap_or("?");
        let run_short: String = run.chars().take(8).collect();
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
        println!("  {run_short:<8} [{seq:>3}] {kind:<20} {signed:<8} {summary}");
    }
    exit_codes::SUCCESS
}

/// Fetch a run's journal entries; `None` on 404 / error.
pub(super) async fn fetch_journal(
    client: &RuntimeClient,
    run_id: &str,
) -> Option<serde_json::Value> {
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
pub(super) fn journal_entry_summary(kind: &str, payload: Option<&serde_json::Value>) -> String {
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
pub(super) async fn resolve_task_to_run(client: &RuntimeClient, id: &str) -> Option<String> {
    let resp = client.get(&format!("/api/v1/tasks/{id}")).await.ok()?;
    if resp.status >= 400 {
        return None;
    }
    let body: serde_json::Value = serde_json::from_str(&resp.body).ok()?;
    body.get("run_id")
        .and_then(|v| v.as_str())
        .map(String::from)
}
