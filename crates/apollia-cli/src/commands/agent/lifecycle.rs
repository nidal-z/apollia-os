use super::*;

/// `apollia-os agent list`: display all agents (installed + runtime).
///
/// When `supports_a2a` is `true`, fetches from `/api/v1/a2a/agents` instead
/// and displays only A2A-capable agents with their skill descriptors.
/// Under `--quiet`, only agent names are printed, one per line.
pub(in crate::commands::agent) async fn run_list(
    client: &RuntimeClient,
    supports_a2a: bool,
    json: bool,
) -> i32 {
    if supports_a2a {
        return run_list_a2a(client, json).await;
    }

    // Fetch installed agents from local DB.
    let installed = open_repository()
        .and_then(|repo| repo.list().ok())
        .unwrap_or_default();

    // Fetch runtime agents (may fail if runtime not running).
    let runtime_agents = client.list_agents().await.ok();

    // --json takes priority over --quiet (machine-readable output is always complete).
    if json {
        let output = build_list_json(&installed, &runtime_agents);
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
    } else if crate::output::is_quiet() {
        // Quiet mode: emit only agent names, one per line.
        for agent in &installed {
            println!("{}", agent.name);
        }
    } else {
        format_enriched_agent_list(&installed, &runtime_agents);
    }
    exit_codes::SUCCESS
}

/// `apollia-os agent list --supports-a2a`: display A2A-capable agents with skills.
async fn run_list_a2a(client: &RuntimeClient, json: bool) -> i32 {
    match client.list_a2a_agents().await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                format_a2a_agent_list(&resp);
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os agent start <path-or-name>`: register / re-load an agent.
///
/// Accepts either a filesystem path to a `.py` (legacy mode for ad-hoc
/// agents) or the **name** of an already installed agent. In the latter case
/// we look it up in `agents.db` and forward its `install_path` to the runtime,
/// so operators don't have to remember where every package landed inside
/// `~/.apollia/agents/`.
pub(in crate::commands::agent) async fn run_start(
    client: &RuntimeClient,
    arg: &str,
    json: bool,
) -> i32 {
    let resolved_path = if looks_like_file_path(arg) {
        arg.to_string()
    } else {
        match open_repository().and_then(|r| r.get(arg).ok().flatten()) {
            Some(entry) => entry.install_path.to_string_lossy().to_string(),
            None => {
                let msg = format!(
                    "agent '{arg}' not installed (and not a file path); install it first via \
                     `apollia-os agent install <path>` or pass a `.py` path"
                );
                return crate::output::emit_error(
                    json,
                    exit_codes::GENERAL_ERROR,
                    &msg.to_string(),
                );
            }
        }
    };
    match client.start_agent(&resolved_path).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                let agent_id = resp.get("agent_id").and_then(|v| v.as_str()).unwrap_or("?");
                let state = resp.get("state").and_then(|v| v.as_str()).unwrap_or("?");
                println!("Agent {agent_id} started ({state})");
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// Agent offered as the example in [`file_path_hint`].
///
/// Must name an agent this repository ships under `agents/`. An operator who
/// mistypes a path reads the hint and copies the example verbatim; a name
/// nothing answers to lands them on `agent not found`, with nothing telling
/// them the example was the wrong part.
pub(in crate::commands::agent) const HINT_EXAMPLE_AGENT: &str = "apollia-guide";

/// Sub-command a [`file_path_hint`] tells the operator to retype.
///
/// A closed set, so every verb the hint can spell out is one the parser
/// accepts. Spelling a verb `apollia-os agent` rejects turns the recovery path
/// into a second failure.
#[derive(Debug, Clone, Copy)]
pub(in crate::commands::agent) enum HintVerb {
    Stop,
    Show,
}

impl HintVerb {
    /// Every variant, so the crate's tests can check them all against the parser.
    #[cfg(test)]
    pub(in crate::commands::agent) const ALL: [Self; 2] = [Self::Stop, Self::Show];

    pub(in crate::commands::agent) fn as_str(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Show => "show",
        }
    }
}

/// Error rendered when an agent sub-command receives what looks like a file
/// path instead of a name or UUID.
pub(in crate::commands::agent) fn file_path_hint(verb: HintVerb, arg: &str) -> String {
    let verb = verb.as_str();
    format!(
        "'{arg}' looks like a file path - use the agent name or UUID instead\n\
         Hint: apollia-os agent {verb} <name|uuid>  (e.g. apollia-os agent {verb} {HINT_EXAMPLE_AGENT})"
    )
}

/// `apollia-os agent stop <id>`: stop a running agent.
pub(in crate::commands::agent) async fn run_stop(
    client: &RuntimeClient,
    agent_id: &str,
    json: bool,
) -> i32 {
    if looks_like_file_path(agent_id) {
        let msg = file_path_hint(HintVerb::Stop, agent_id);
        return crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &msg.to_string());
    }
    match client.stop_agent(agent_id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("Agent {agent_id} stopped");
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os agent show <id>`: display agent detail.
pub(in crate::commands::agent) async fn run_info(
    client: &RuntimeClient,
    agent_id: &str,
    json: bool,
) -> i32 {
    if looks_like_file_path(agent_id) {
        let msg = file_path_hint(HintVerb::Show, agent_id);
        return print_compact_error_and_exit(&msg, json);
    }
    match client.get_agent(agent_id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                format_agent_detail(&resp);
            }
            exit_codes::SUCCESS
        }
        // Disabled / not-yet-loaded agents are absent from the runtime
        // registry but still present in `agents.db`. Fall back to the local
        // repository so `apollia-os agent show` works on every installed
        // agent regardless of its runtime state.
        Err(ClientError::ServerError { status: 404, .. }) => {
            run_info_local_fallback(agent_id, json)
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os agent status <id>`: compact runtime-status snapshot.
pub(in crate::commands::agent) async fn run_status(
    client: &RuntimeClient,
    agent_id: &str,
    json: bool,
) -> i32 {
    match client.get_agent(agent_id).await {
        Ok(resp) => {
            format_status_snapshot(&resp, agent_id, json);
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, .. }) => {
            print_compact_error_and_exit(&format!("agent not found: {agent_id}"), json)
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os agent messages <id>`: list in-memory A2A messages.
pub(in crate::commands::agent) async fn run_messages(
    client: &RuntimeClient,
    agent_id: &str,
    limit: u32,
    json: bool,
) -> i32 {
    let limit_opt = if limit == 0 { None } else { Some(limit) };
    match client.list_agent_messages(agent_id, limit_opt).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
                return exit_codes::SUCCESS;
            }
            let empty: Vec<serde_json::Value> = Vec::new();
            let messages = resp
                .get("messages")
                .and_then(|v| v.as_array())
                .unwrap_or(&empty);
            if messages.is_empty() {
                println!("No A2A messages for {agent_id}.");
                return exit_codes::SUCCESS;
            }
            println!(
                "  A2A messages for {agent_id} ({} returned, limit {limit}):",
                messages.len()
            );
            println!("  {:<24} {:<22} PAYLOAD", "FROM", "SENT_AT");
            for m in messages {
                let from = m.get("from_agent").and_then(|v| v.as_str()).unwrap_or("?");
                let sent_at = m.get("sent_at").and_then(|v| v.as_str()).unwrap_or("?");
                let payload_str = m
                    .get("payload")
                    .map(|p| serde_json::to_string(p).unwrap_or_default())
                    .unwrap_or_default();
                let preview: String = payload_str.chars().take(60).collect();
                println!("  {from:<24} {sent_at:<22} {preview}");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError {
            status: 503, body, ..
        }) => {
            let msg = format!("agent mailbox unavailable: {body}");
            crate::output::emit_error(json, exit_codes::RUNTIME_ERROR, &msg.to_string())
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os agent logs <id> [--last N] [--follow]`: display recent
/// activity for an agent.
///
/// Until a dedicated agent stderr persistence layer ships (v0.1.1+), the
/// runtime does not expose `/api/v1/agents/:id/logs`. We fall back to the
/// audit trail: every tool invocation attributed to `<agent_id>` is fetched
/// from `GET /api/v1/audit?limit=…` and rendered as one timestamped line per
/// event. This gives operators the "what did this agent do recently?" view
/// that `agent logs` should answer.
///
/// With `--follow`, opens the SSE stream at
/// `GET /api/v1/agents/{id}/logs/stream` (server-side route; when absent,
/// the CLI prints a clear "not implemented" message and exits 1).
pub(in crate::commands::agent) async fn run_logs(
    client: &RuntimeClient,
    agent_id: &str,
    last: u32,
    follow: bool,
    json: bool,
) -> i32 {
    if looks_like_file_path(agent_id) {
        let msg = format!(
            "'{agent_id}' looks like a file path - use the agent name or UUID instead\n\
             Hint: apollia-os agent logs <name|uuid>"
        );
        return print_compact_error_and_exit(&msg, json);
    }

    if follow {
        return run_logs_follow(client, agent_id, json).await;
    }

    // Validate the agent exists before falling back to the audit trail, so a
    // typo'd name reports not-found (exit 1) like the rest of the `agent`
    // family rather than silently printing "no recent activity" with exit 0.
    match client.get_agent(agent_id).await {
        Ok(_) => {}
        Err(ClientError::ServerError { status: 404, .. }) => {
            // Disabled / not-yet-loaded agents are absent from the runtime
            // registry but still installed; accept them via the local repo.
            if local_agent_detail(agent_id).is_none() {
                return print_compact_error_and_exit(&format!("agent not found: {agent_id}"), json);
            }
        }
        Err(e) => return handle_error(e, json),
    }

    // Fetch a generous slice of recent audit events. We cap at 500
    // (runtime hard limit) and client-side filter on agent_id, then keep
    // the last `last` matches. This is O(500) per call, fine for an
    // interactive CLI; a dedicated route would be faster but is out of
    // scope.
    let fetch_limit = 500u32;
    let uri = format!("/api/v1/audit?limit={fetch_limit}");
    let resp = match client.get(&uri).await {
        Ok(r) if r.status < 400 => r,
        Ok(r) => {
            let msg = format!("HTTP {} from /audit: {}", r.status, r.body);
            return print_compact_error_and_exit(&msg, json);
        }
        Err(e) => return handle_error(e, json),
    };
    let body: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            return print_compact_error_and_exit(&format!("invalid audit JSON: {e}"), json);
        }
    };

    let empty: Vec<serde_json::Value> = Vec::new();
    let events: Vec<&serde_json::Value> = body
        .get("events")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty)
        .iter()
        .filter(|e| {
            e.get("agent_id")
                .and_then(|v| v.as_str())
                .map(|s| s == agent_id)
                .unwrap_or(false)
        })
        .take(last as usize)
        .collect();

    if json {
        let array: Vec<serde_json::Value> = events.iter().map(|e| (*e).clone()).collect();
        let out = serde_json::json!({
            "agent_id": agent_id,
            "events": array,
            "source": "audit-trail (no dedicated agent log channel yet)",
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
        return exit_codes::SUCCESS;
    }

    if events.is_empty() {
        println!("(no recent activity for {agent_id} in the audit trail)");
        return exit_codes::SUCCESS;
    }

    println!(
        "  Recent audit events for {agent_id} ({} of last {fetch_limit}):",
        events.len()
    );
    for e in &events {
        print_audit_event_row(e);
    }
    exit_codes::SUCCESS
}

/// `apollia-os agent logs <id> --follow`: live stream placeholder.
///
/// The runtime does not yet register `GET /api/v1/agents/:id/logs/stream`
/// (no dedicated agent stderr persistence layer ships in v0.1.0; the
/// audit fallback used by `agent logs` covers the structured tool-call
/// view). Rather than fail silently by hitting a missing endpoint, the
/// CLI returns a clear, actionable error pointing at the same `--last`
/// path and the v0.1.1 roadmap entry.
async fn run_logs_follow(_client: &RuntimeClient, agent_id: &str, json: bool) -> i32 {
    let msg = format!(
        "agent logs --follow is not yet implemented; run `apollia-os agent logs \
         {agent_id} --last 20` for the latest activity"
    );
    crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &msg)
}

// ─────────────────────────────────────────────────────────────────────────────
// Validate
// ─────────────────────────────────────────────────────────────────────────────
