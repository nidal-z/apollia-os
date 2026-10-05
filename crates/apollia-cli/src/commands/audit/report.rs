//! `audit list` and `audit stats`, and their rendering.

use crate::client::RuntimeClient;
use crate::exit_codes;

use super::{handle_error, handle_server_error};

/// `apollia-os audit list`: display recent audit events.
pub(super) async fn run_list(client: &RuntimeClient, limit: u32, json: bool) -> i32 {
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
pub(super) async fn run_stats(client: &RuntimeClient, json: bool) -> i32 {
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
pub(super) fn format_audit_list(
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
pub(super) fn short_uuid_prefix(uuid: &str) -> String {
    uuid.split('-').next().unwrap_or(uuid).to_string()
}

/// Format audit stats as human-readable text.
pub(super) fn format_audit_stats(resp: &serde_json::Value) {
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
