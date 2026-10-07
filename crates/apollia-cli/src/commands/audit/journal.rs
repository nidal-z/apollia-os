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

/// What one page of the journal is asked with: its bounds and its filters.
pub(super) struct JournalPage<'a> {
    pub(super) limit: u32,
    pub(super) offset: u32,
    pub(super) agents: &'a [String],
    pub(super) families: &'a [String],
    pub(super) natures: &'a [String],
    pub(super) results: &'a [String],
    pub(super) since: Option<&'a str>,
    pub(super) until: Option<&'a str>,
}

/// The page URI: each filter appears only when it is given, its values
/// encoded and joined by commas as the route reads them.
fn journal_page_uri(page: &JournalPage<'_>) -> String {
    let mut uri = format!(
        "/api/v1/audit/journal?limit={}&offset={}",
        page.limit, page.offset
    );
    let encode = crate::commands::model::urlencode;
    for (name, values) in [
        ("agents", page.agents),
        ("family", page.families),
        ("nature", page.natures),
        ("result", page.results),
    ] {
        if !values.is_empty() {
            let encoded: Vec<String> = values.iter().map(|v| encode(v)).collect();
            uri.push_str(&format!("&{name}={}", encoded.join(",")));
        }
    }
    for (name, value) in [("since", page.since), ("until", page.until)] {
        if let Some(value) = value {
            uri.push_str(&format!("&{name}={}", encode(value)));
        }
    }
    uri
}

/// The line `audit journal` prints for one entry, from its summary: when, who,
/// what, the tool, the target and the result. A part the summary does not have
/// is left out, so the line never shows a placeholder for it.
pub(in crate::commands) fn summary_line(entry: &serde_json::Value) -> String {
    let summary = entry.get("summary");
    let part = |key: &str| {
        summary
            .and_then(|s| s.get(key))
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
    };
    let mut line = part("ts").unwrap_or("?").to_owned();
    if let Some(agent) = part("agent") {
        line.push_str(&format!("  {agent}"));
    }
    let what = part("what")
        .or_else(|| entry.get("kind").and_then(|v| v.as_str()))
        .unwrap_or("?");
    line.push_str(&format!("  {what}"));
    if let Some(tool) = part("tool") {
        line.push_str(&format!(": {tool}"));
    }
    if let Some(family) = part("family") {
        line.push_str(&format!(" ({family})"));
    }
    if let Some(target) = part("target") {
        line.push_str(&format!(" on {target}"));
    }
    if let Some(outcome) = part("outcome") {
        line.push_str(&format!("  [{outcome}]"));
    }
    line
}

/// `apollia-os audit journal`: print a page of the chained journal across every
/// run, narrowed by the filters of `page`, newest global position first. Each
/// entry is printed as its summary, followed by its run, its position in that
/// run and whether it is signed, which is what `audit show` reads it whole by.
pub(super) async fn run_journal(client: &RuntimeClient, page: &JournalPage<'_>, json: bool) -> i32 {
    let uri = journal_page_uri(page);
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
        let seq = e
            .get("seq")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let signed = if e.get("signature").map(|v| !v.is_null()).unwrap_or(false) {
            "signed"
        } else {
            "unsigned"
        };
        println!("  {}  ({run} #{seq}, {signed})", summary_line(e));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn page<'a>(agents: &'a [String], families: &'a [String]) -> JournalPage<'a> {
        JournalPage {
            limit: 20,
            offset: 0,
            agents,
            families,
            natures: &[],
            results: &[],
            since: None,
            until: None,
        }
    }

    #[test]
    fn the_page_uri_names_the_agents_only_when_there_are_some() {
        // GIVEN no agent, then two agents
        let none: Vec<String> = Vec::new();
        let two = vec!["flux-quotes".to_string(), "flux invoices".to_string()];

        // WHEN the page URIs are built
        let all = journal_page_uri(&page(&none, &none));
        let filtered = journal_page_uri(&JournalPage {
            offset: 40,
            ..page(&two, &none)
        });

        // THEN the filter appears only for named agents, each one encoded
        assert_eq!(all, "/api/v1/audit/journal?limit=20&offset=0");
        assert_eq!(
            filtered,
            "/api/v1/audit/journal?limit=20&offset=40&agents=flux-quotes,flux%20invoices"
        );
    }

    #[test]
    fn the_page_uri_carries_every_filter_given() {
        // GIVEN one value or two for each filter, and a period
        let agents = vec!["quotes".to_string()];
        let families = vec!["read".to_string(), "send_external".to_string()];
        let natures = vec!["action".to_string()];
        let results = vec!["failure".to_string(), "refused".to_string()];

        // WHEN the page URI is built
        let uri = journal_page_uri(&JournalPage {
            natures: &natures,
            results: &results,
            since: Some("2026-10-01"),
            until: Some("2026-10-04T12:00:00+02:00"),
            ..page(&agents, &families)
        });

        // THEN each filter is there under the name the route reads, encoded
        assert_eq!(
            uri,
            "/api/v1/audit/journal?limit=20&offset=0&agents=quotes\
             &family=read,send_external&nature=action&result=failure,refused\
             &since=2026-10-01&until=2026-10-04T12%3A00%3A00%2B02%3A00"
        );
    }

    #[test]
    fn the_line_of_an_entry_is_its_summary_without_empty_parts() {
        // GIVEN a finished call with every part, and a policy entry with few
        let call = serde_json::json!({
            "kind": "tool_call_completed",
            "payload": { "content": "SECRET" },
            "summary": {
                "ts": "2026-10-04T09:00:00Z", "agent": "quotes", "nature": "action",
                "what": "Tool call finished", "tool": "file_write",
                "family": "write_or_delete", "target": "/home/u/a.md", "outcome": "success",
            },
        });
        let policy = serde_json::json!({
            "kind": "policy_changed",
            "summary": {
                "ts": "2026-10-04T09:00:00Z", "agent": null, "nature": "policy",
                "what": "Organisation policy changed", "tool": null, "family": null,
                "target": null, "outcome": "success",
            },
        });

        // WHEN their lines are written
        let call_line = summary_line(&call);
        let policy_line = summary_line(&policy);

        // THEN each reads as a sentence, and nothing of the payload is in it
        assert_eq!(
            call_line,
            "2026-10-04T09:00:00Z  quotes  Tool call finished: file_write \
             (write_or_delete) on /home/u/a.md  [success]"
        );
        assert_eq!(
            policy_line,
            "2026-10-04T09:00:00Z  Organisation policy changed  [success]"
        );
    }

    #[test]
    fn an_entry_without_a_summary_falls_back_to_its_kind() {
        // GIVEN an entry answered by a runtime that sends no summary
        let entry = serde_json::json!({ "kind": "tool_call_started", "ts": "x" });

        // WHEN its line is written
        let line = summary_line(&entry);

        // THEN the kind stands for what happened
        assert_eq!(line, "?  tool_call_started");
    }
}
