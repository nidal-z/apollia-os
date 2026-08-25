use super::*;

/// Formats the A2A agent list as a human-readable table.
///
/// Output columns: NAME, VERSION, STATUS, SKILLS (comma-separated skill IDs).
pub(in crate::commands::agent) fn format_a2a_agent_list(resp: &serde_json::Value) {
    let agents = resp.get("agents").and_then(|v| v.as_array());
    let list = match agents {
        None => {
            println!("No A2A-capable agents running.");
            return;
        }
        Some(v) if v.is_empty() => {
            println!("No A2A-capable agents running.");
            return;
        }
        Some(v) => v,
    };

    println!("  {:<24} {:<10} {:<10} SKILLS", "NAME", "VERSION", "STATUS");

    for agent in list {
        let name = agent.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let version = agent.get("version").and_then(|v| v.as_str()).unwrap_or("-");
        let state = agent.get("state").and_then(|v| v.as_str()).unwrap_or("?");
        let skills_label = agent
            .get("skills")
            .and_then(|v| v.as_array())
            .map(|skills| {
                skills
                    .iter()
                    .filter_map(|s| s.get("id").and_then(|id| id.as_str()))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let skills_display = if skills_label.is_empty() {
            "(none)".to_string()
        } else {
            skills_label
        };

        println!(
            "  {:<24} {:<10} {:<10} {}",
            name, version, state, skills_display
        );
    }
}

/// Render `agent info` from the local `agents.db` row when the runtime
/// registry returns 404 (e.g. disabled or not-yet-loaded agents).
pub(in crate::commands::agent) fn run_info_local_fallback(agent_id: &str, json: bool) -> i32 {
    match local_agent_detail(agent_id) {
        Some(detail) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&detail).unwrap_or_default()
                );
            } else {
                format_local_agent_detail(&detail);
            }
            exit_codes::SUCCESS
        }
        None => print_compact_error_and_exit(&format!("agent not found: {agent_id}"), json),
    }
}

/// Render the compact runtime-status snapshot produced by `agent status`.
pub(in crate::commands::agent) fn format_status_snapshot(
    resp: &serde_json::Value,
    agent_id: &str,
    json: bool,
) {
    let name = resp
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(agent_id);
    let state = resp
        .get("state")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let started_at = resp.get("started_at").and_then(|v| v.as_str());
    let last_activity = resp.get("last_activity_at").and_then(|v| v.as_str());
    let active_tasks = resp
        .get("active_tasks")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let completed_tasks = resp
        .get("completed_tasks")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if json {
        let body = serde_json::json!({
            "agent": name,
            "state": state,
            "active_tasks": active_tasks,
            "completed_tasks": completed_tasks,
            "started_at": started_at,
            "last_activity_at": last_activity,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
        return;
    }
    let glyph = match state {
        "active" | "idle" | "ready" => "*",
        "error" | "failed" => "x",
        _ => "?",
    };
    println!("  {glyph} {name}");
    println!("    state          : {state}");
    println!("    active tasks   : {active_tasks}");
    println!("    completed tasks: {completed_tasks}");
    if let Some(s) = started_at {
        println!("    started at     : {s}");
    }
    if let Some(s) = last_activity {
        println!("    last activity  : {s}");
    }
}

/// Load a detailed snapshot of an installed agent from `agents.db`, bypassing
/// the runtime registry. Returned as a JSON value so it can be either pretty-
/// printed via `format_local_agent_detail` or emitted verbatim with `--json`.
pub(in crate::commands::agent) fn local_agent_detail(agent_id: &str) -> Option<serde_json::Value> {
    let repo = open_repository()?;
    let entry = repo.get(agent_id).ok().flatten()?;
    let body = serde_json::json!({
        "agent_id": null,
        "name": entry.name,
        "version": entry.version,
        "state": if entry.enabled { "stopped" } else { "disabled" },
        "supports_a2a": entry.manifest.supports_a2a,
        "skills": entry.manifest.skills,
        "manifest": entry.manifest,
        "install_path": entry.install_path,
        "installed_at": entry.installed_at,
        "_source": "local agents.db (runtime registry has no live entry - agent is disabled or failed to load)",
    });
    Some(body)
}

/// Render the local-only agent detail in the same shape as the live runtime
/// view so operators don't have to learn two layouts. Skills come from the
/// manifest because they are unchanged across enable / disable transitions.
fn format_local_agent_detail(detail: &serde_json::Value) {
    let name = detail.get("name").and_then(|v| v.as_str()).unwrap_or("?");
    let version = detail
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let state = detail.get("state").and_then(|v| v.as_str()).unwrap_or("?");
    let installed_at = detail
        .get("installed_at")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let install_path = detail
        .get("install_path")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    println!("  Name         : {name}");
    println!("  Version      : {version}");
    println!("  State        : {state}  (from local agents.db; not running in registry)");
    println!("  Install path : {install_path}");
    println!("  Installed at : {installed_at}");
    if let Some(manifest) = detail.get("manifest") {
        if let Some(skills) = manifest.get("skills").and_then(|s| s.as_array()) {
            if !skills.is_empty() {
                println!("  Skills ({}) :", skills.len());
                for s in skills {
                    let id = s.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                    let n = s.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    println!("    - {id} ({n})");
                }
            }
        }
        if let Some(tools) = manifest.get("tools_required").and_then(|t| t.as_array()) {
            let names: Vec<&str> = tools.iter().filter_map(|v| v.as_str()).collect();
            if !names.is_empty() {
                println!("  Tools (required) : {}", names.join(", "));
            }
        }
    }
    println!(
        "  Hint         : run `apollia-os agent enable {name}` then `apollia-os agent start {name}` to load."
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// New commands (install/uninstall/enable/disable/update)
// ─────────────────────────────────────────────────────────────────────────────

/// The agent-code trust statement shown before every install.
///
/// Restates the v0.1.0 trust model: agent Python runs in-process with the full
/// rights of the current user and there is no OS sandbox around agent code, so
/// only audited agents should be installed.
pub(in crate::commands::agent) fn trust_banner_text() -> &'static str {
    "This agent runs Python in-process with the full rights of your user account: \
     filesystem, network, and credentials in your keyring. There is no OS sandbox \
     around agent code. Only install agents you have written or audited."
}

/// Prints the trust banner to stderr before an install proceeds.
///
/// Suppressed under `--json` so machine-readable output stays clean, matching the
/// existing operator-warning convention. The banner is informational, not a
/// prompt: it never blocks the install.
pub(in crate::commands::agent) fn print_trust_banner(json: bool) {
    if !json {
        eprintln!("Security notice: {}", trust_banner_text());
    }
}

/// Build a merged JSON array for `agent list --json`.
pub(in crate::commands::agent) fn build_list_json(
    installed: &[InstalledAgent],
    runtime: &Option<serde_json::Value>,
) -> serde_json::Value {
    let runtime_agents = runtime
        .as_ref()
        .and_then(|v| v.get("agents"))
        .and_then(|a| a.as_array());

    let mut entries: Vec<serde_json::Value> = Vec::new();

    for agent in installed {
        let runtime_entry = runtime_agents.and_then(|agents| {
            agents.iter().find(|a| {
                a.get("name")
                    .and_then(|n| n.as_str())
                    .is_some_and(|n| n == agent.name)
                    || a.get("manifest")
                        .and_then(|m| m.get("name"))
                        .and_then(|n| n.as_str())
                        .is_some_and(|n| n == agent.name)
            })
        });
        // Match `agent info` and the runtime shape: key is `state`, and the
        // runtime `agent_id` is surfaced so automation can map name -> id.
        let state = runtime_entry
            .and_then(|a| a.get("state").and_then(|s| s.as_str()))
            .unwrap_or("-");
        let agent_id = runtime_entry
            .and_then(|a| a.get("agent_id").cloned())
            .unwrap_or(serde_json::Value::Null);

        entries.push(serde_json::json!({
            "agent_id": agent_id,
            "name": agent.name,
            "version": agent.version,
            "state": state,
            "enabled": agent.enabled,
            "installed": true,
        }));
    }

    // Add runtime-only agents not in installed list.
    if let Some(agents) = runtime_agents {
        for agent in agents {
            let name = agent
                .get("name")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    agent
                        .get("manifest")
                        .and_then(|m| m.get("name"))
                        .and_then(|n| n.as_str())
                })
                .unwrap_or("?");

            let already_listed = installed.iter().any(|i| i.name == name);
            if !already_listed {
                let state = agent.get("state").and_then(|v| v.as_str()).unwrap_or("?");
                let agent_id = agent
                    .get("agent_id")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                entries.push(serde_json::json!({
                    "agent_id": agent_id,
                    "name": name,
                    "version": agent.get("manifest")
                        .and_then(|m| m.get("version"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("-"),
                    "state": state,
                    "enabled": serde_json::Value::Null,
                    "installed": false,
                }));
            }
        }
    }

    serde_json::json!({ "agents": entries })
}

/// Format an enriched agent list as a human-readable table.
pub(in crate::commands::agent) fn format_enriched_agent_list(
    installed: &[InstalledAgent],
    runtime: &Option<serde_json::Value>,
) {
    let runtime_agents = runtime
        .as_ref()
        .and_then(|v| v.get("agents"))
        .and_then(|a| a.as_array());

    println!(
        "  {:<24} {:<10} {:<12} {:<10} SOURCE",
        "NAME", "VERSION", "STATUS", "AUTO-LOAD"
    );

    let mut has_entries = false;

    for agent in installed {
        has_entries = true;
        let runtime_state = runtime_agents
            .and_then(|agents| {
                agents.iter().find(|a| {
                    a.get("name")
                        .and_then(|n| n.as_str())
                        .is_some_and(|n| n == agent.name)
                        || a.get("manifest")
                            .and_then(|m| m.get("name"))
                            .and_then(|n| n.as_str())
                            .is_some_and(|n| n == agent.name)
                })
            })
            .and_then(|a| a.get("state").and_then(|s| s.as_str()));

        // Combine "loaded in registry" with "enabled in repo" into a single
        // human-readable status so operators don't have to cross-reference
        // two columns. When the operator has explicitly disabled the agent
        // (enabled=false) we surface 'disabled' over the registry state so
        // it's clear the agent will not auto-start on the next boot, the
        // registry might still report 'stopped' transiently for a moment.
        let status = match (agent.enabled, runtime_state) {
            (false, _) => "disabled",
            (true, Some("stopped")) => "stopped",
            (true, Some(state)) => state, // active / degraded / initializing
            (true, None) => "stopped",
        };
        let auto_load = if agent.enabled { "yes" } else { "no" };

        println!(
            "  {:<24} {:<10} {:<12} {:<10} installed",
            agent.name, agent.version, status, auto_load
        );
    }

    // Add runtime-only agents not in installed list.
    if let Some(agents) = runtime_agents {
        for agent in agents {
            let name = agent
                .get("name")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    agent
                        .get("manifest")
                        .and_then(|m| m.get("name"))
                        .and_then(|n| n.as_str())
                })
                .unwrap_or("?");

            let already_listed = installed.iter().any(|i| i.name == name);
            if !already_listed {
                has_entries = true;
                let state = agent.get("state").and_then(|v| v.as_str()).unwrap_or("?");
                let version = agent
                    .get("manifest")
                    .and_then(|m| m.get("version"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("-");
                println!(
                    "  {:<24} {:<10} {:<12} {:<10} runtime-only",
                    name, version, state, "-"
                );
            }
        }
    }

    if !has_entries {
        println!("  (no agents registered or installed)");
    }
}

/// Format agent detail as human-readable text.
pub(in crate::commands::agent) fn format_agent_detail(resp: &serde_json::Value) {
    let agent_id = resp.get("agent_id").and_then(|v| v.as_str()).unwrap_or("?");
    let state = resp.get("state").and_then(|v| v.as_str()).unwrap_or("?");

    println!("  Agent     : {agent_id}");
    println!("  State     : {state}");

    if let Some(manifest) = resp.get("manifest") {
        let name = manifest.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let version = manifest
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let desc = manifest
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        println!("  Name      : {name}");
        println!("  Version   : {version}");
        if !desc.is_empty() {
            println!("  Desc      : {desc}");
        }
        if let Some(max) = manifest
            .get("max_concurrent_tasks")
            .and_then(|v| v.as_u64())
        {
            println!("  Max concurrency : {max}");
        }
    }
}

/// Handle client errors uniformly.
pub(in crate::commands::agent) fn handle_error(err: ClientError, json: bool) -> i32 {
    match err {
        ClientError::ConnectionRefused => crate::output::emit_error(
            json,
            exit_codes::RUNTIME_ERROR,
            "runtime not started (connection refused)",
        ),
        other => crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &other.to_string()),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Logs
// ─────────────────────────────────────────────────────────────────────────────

/// Print a single audit event as an aligned text row for `agent logs`.
pub(in crate::commands::agent) fn print_audit_event_row(e: &serde_json::Value) {
    println!("{}", format_audit_event_row(e));
}

/// Format a single audit event as an aligned text row for `agent logs`.
///
/// Reads the audit-event JSON shape returned by `GET /api/v1/agents/:id/logs`
/// (`started_at`, `tool_name`, `success`, `error_code`, `duration_ms`,
/// `task_id`), matching what `agent logs --json` emits.
pub(in crate::commands::agent) fn format_audit_event_row(e: &serde_json::Value) -> String {
    let ts = e.get("started_at").and_then(|v| v.as_str()).unwrap_or("?");
    let tool = e.get("tool_name").and_then(|v| v.as_str()).unwrap_or("?");
    let outcome = match e.get("success").and_then(|v| v.as_bool()) {
        Some(true) => "ok",
        Some(false) => e
            .get("error_code")
            .and_then(|v| v.as_str())
            .unwrap_or("FAILED"),
        None => "?",
    };
    let task = e.get("task_id").and_then(|v| v.as_str()).unwrap_or("-");
    match e.get("duration_ms").and_then(|v| v.as_u64()) {
        Some(ms) => format!("  {ts}  {tool:<32} {outcome:<8} {ms:>5}ms  task={task}"),
        None => format!("  {ts}  {tool:<32} {outcome:<8}          task={task}"),
    }
}
