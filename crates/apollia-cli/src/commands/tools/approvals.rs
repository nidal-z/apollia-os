//! `tools approvals`: the human-in-the-loop queue, pending and resolved.

use std::path::PathBuf;

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;

use super::ToolsApprovalsCmd;

// ─── Approvals (HITL queue) ──────────────────────────────────────────────────

/// Dispatch `tools approvals <verb>` to the runtime client.
pub(super) async fn run_approvals(
    socket: Option<PathBuf>,
    cmd: &ToolsApprovalsCmd,
    json: bool,
) -> i32 {
    let socket_path = socket.unwrap_or_else(default_socket_path);
    let client = RuntimeClient::new(socket_path);
    match cmd {
        ToolsApprovalsCmd::Pending => run_approvals_pending(&client, json).await,
        ToolsApprovalsCmd::Resolved { days, limit } => {
            run_approvals_resolved(&client, *days, *limit, json).await
        }
    }
}

/// Emit a non-success HTTP response (status >= 400) and return its exit code.
pub(super) fn emit_approvals_http_error(resp: &crate::client::RawResponse, json: bool) -> i32 {
    crate::output::emit_error(
        json,
        exit_codes::GENERAL_ERROR,
        &format!("HTTP {}: {}", resp.status, resp.body),
    )
}

/// Emit a client transport error and return its exit code.
pub(super) fn emit_approvals_client_error(err: &ClientError, json: bool) -> i32 {
    match err {
        ClientError::ConnectionRefused => crate::output::emit_error(
            json,
            exit_codes::RUNTIME_ERROR,
            "runtime not started (connection refused)",
        ),
        e => crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string()),
    }
}

pub(super) async fn run_approvals_pending(client: &RuntimeClient, json: bool) -> i32 {
    match client.get("/api/v1/approvals/pending").await {
        Ok(resp) if resp.status < 400 => {
            if json {
                println!("{}", resp.body);
            } else {
                match serde_json::from_str::<serde_json::Value>(&resp.body) {
                    Ok(v) => print_pending_human(&v),
                    Err(_) => println!("{}", resp.body),
                }
            }
            exit_codes::SUCCESS
        }
        Ok(resp) => emit_approvals_http_error(&resp, json),
        Err(e) => emit_approvals_client_error(&e, json),
    }
}

pub(super) async fn run_approvals_resolved(
    client: &RuntimeClient,
    days: u32,
    limit: u32,
    json: bool,
) -> i32 {
    let uri = format!("/api/v1/approvals/resolved?days={days}&limit={limit}");
    match client.get(&uri).await {
        Ok(resp) if resp.status < 400 => {
            if json {
                println!("{}", resp.body);
            } else {
                match serde_json::from_str::<serde_json::Value>(&resp.body) {
                    Ok(v) => print_resolved_human(&v),
                    Err(_) => println!("{}", resp.body),
                }
            }
            exit_codes::SUCCESS
        }
        Ok(resp) => emit_approvals_http_error(&resp, json),
        Err(e) => emit_approvals_client_error(&e, json),
    }
}

pub(super) fn print_pending_human(v: &serde_json::Value) {
    let arr = v
        .get("pending")
        .or_else(|| v.get("approvals"))
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_else(|| v.as_array().cloned().unwrap_or_default());
    if arr.is_empty() {
        println!("  (no pending approvals)");
        return;
    }
    println!("  {:<24} {:<24} {:<24} REQUESTED", "TASK", "TOOL", "AGENT");
    for item in &arr {
        let task_id = item.get("task_id").and_then(|x| x.as_str()).unwrap_or("?");
        let tool = item
            .get("tool")
            .or_else(|| item.get("tool_name"))
            .and_then(|x| x.as_str())
            .unwrap_or("?");
        let agent = item
            .get("agent")
            .or_else(|| item.get("agent_id"))
            .and_then(|x| x.as_str())
            .unwrap_or("?");
        let when = item
            .get("requested_at")
            .or_else(|| item.get("created_at"))
            .and_then(|x| x.as_str())
            .unwrap_or("?");
        println!("  {task_id:<24} {tool:<24} {agent:<24} {when}");
    }
}

pub(super) fn print_resolved_human(v: &serde_json::Value) {
    let arr = v
        .get("resolved")
        .or_else(|| v.get("approvals"))
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_else(|| v.as_array().cloned().unwrap_or_default());
    if arr.is_empty() {
        println!("  (no resolved approvals in window)");
        return;
    }
    println!(
        "  {:<24} {:<10} {:<24} RESOLVED",
        "TASK", "DECISION", "TOOL"
    );
    for item in &arr {
        let task_id = item.get("task_id").and_then(|x| x.as_str()).unwrap_or("?");
        let decision = item
            .get("decision")
            .or_else(|| item.get("status"))
            .and_then(|x| x.as_str())
            .unwrap_or("?");
        let tool = item.get("tool").and_then(|x| x.as_str()).unwrap_or("?");
        let when = item
            .get("resolved_at")
            .and_then(|x| x.as_str())
            .unwrap_or("?");
        println!("  {task_id:<24} {decision:<10} {tool:<24} {when}");
    }
}
