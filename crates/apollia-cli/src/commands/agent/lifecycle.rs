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

    // The stored agents of the local store.
    let installed = stored_agents();

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

/// `apollia-os agent start <agent_id>`: make a stored agent available again.
///
/// An agent is available as soon as it is installed and enabled; the verb
/// brings back one that `agent stop` took out of the runtime inventory.
pub(in crate::commands::agent) async fn run_start(
    client: &RuntimeClient,
    arg: &str,
    json: bool,
) -> i32 {
    if looks_like_file_path(arg) {
        // A Python file is not an agent: an agent arrives with a package.
        let msg = format!(
            "{}\nAn agent is installed with its package: apollia-os package install <file.apo>",
            file_path_hint(HintVerb::Start, arg)
        );
        return crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &msg);
    }
    match client.start_agent(arg).await {
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
    Start,
    Stop,
    Show,
}

impl HintVerb {
    /// Every variant, so the crate's tests can check them all against the parser.
    #[cfg(test)]
    pub(in crate::commands::agent) const ALL: [Self; 3] = [Self::Start, Self::Stop, Self::Show];

    pub(in crate::commands::agent) fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
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
    // The definition comes from agents.db through the runtime; an agent the
    // registry has never loaded may still have one, and a runtime agent may
    // have none (a module started by path).
    let definition = client.get_agent_definition(agent_id).await.ok();
    match client.get_agent(agent_id).await {
        Ok(mut resp) => {
            if let Some(map) = resp.as_object_mut() {
                map.insert(
                    "definition".into(),
                    definition.clone().unwrap_or(serde_json::Value::Null),
                );
            }
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                format_agent_detail(&resp);
                if let Some(definition) = &definition {
                    print_definition_section(definition);
                }
            }
            exit_codes::SUCCESS
        }
        // Disabled / not-yet-loaded agents are absent from the runtime
        // registry but still present in `agents.db`. Fall back to the local
        // repository so `apollia-os agent show` works on every installed
        // agent regardless of its runtime state.
        Err(ClientError::ServerError { status: 404, .. }) => {
            run_info_local_fallback(agent_id, definition, json)
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
/// The runtime keeps no log channel per agent. The command reads the audit
/// journal instead: the actions that name `<agent_id>` and the runs of its
/// tasks, from `GET /api/v1/audit/journal?agents=...&nature=action`, each
/// printed as its summary. This is the "what did this agent do recently?"
/// view that `agent logs` answers.
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

    // Validate the agent exists before reading the audit journal, so a
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

    // The journal narrows the page to what names the agent and to the runs of
    // its tasks, so the `last` entries asked for are the last of that agent.
    let uri = agent_logs_uri(agent_id, last);
    let resp = match client.get(&uri).await {
        Ok(r) if r.status < 400 => r,
        Ok(r) => {
            let msg = format!("HTTP {} from the audit journal: {}", r.status, r.body);
            return print_compact_error_and_exit(&msg, json);
        }
        Err(e) => return handle_error(e, json),
    };
    let body: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            return print_compact_error_and_exit(&format!("invalid journal JSON: {e}"), json);
        }
    };

    let empty: Vec<serde_json::Value> = Vec::new();
    let events = body
        .get("entries")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);

    if json {
        let out = serde_json::json!({
            "agent_id": agent_id,
            "events": events,
            "source": "audit journal",
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
        return exit_codes::SUCCESS;
    }

    if events.is_empty() {
        println!("(no recent activity for {agent_id} in the audit journal)");
        return exit_codes::SUCCESS;
    }

    println!(
        "  Recent actions of {agent_id} in the audit journal ({}):",
        events.len()
    );
    for e in events {
        println!("  {}", crate::commands::audit::journal_summary_line(e));
    }
    exit_codes::SUCCESS
}

/// The page of the audit journal `agent logs` reads: the last `last` actions
/// that name `agent_id` or belong to the runs of its tasks.
pub(in crate::commands::agent) fn agent_logs_uri(agent_id: &str, last: u32) -> String {
    format!(
        "/api/v1/audit/journal?limit={last}&offset=0&agents={}&nature=action",
        crate::commands::model::urlencode(agent_id)
    )
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
