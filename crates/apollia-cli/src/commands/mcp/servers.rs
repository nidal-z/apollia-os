//! Server CRUD against the runtime: add, remove, show, test, restart, update.

use crate::client::{ClientError, RuntimeClient};
use crate::exit_codes;
use crate::note;

use super::handle_client_error;

// ─── Runtime-based CRUD handlers ──────────────────────────────────────────────

/// Server definition fields shared by the `add` and `update` runtime handlers.
pub(super) struct ServerSpec<'a> {
    pub(super) name: &'a str,
    pub(super) command: Option<&'a str>,
    pub(super) url: Option<&'a str>,
    pub(super) require_approval: bool,
}

/// `apollia-os mcp add <name>`: add an MCP server to the runtime.
pub(super) async fn run_add(client: &RuntimeClient, spec: ServerSpec<'_>, json: bool) -> i32 {
    let ServerSpec {
        name,
        command,
        url,
        require_approval,
    } = spec;
    let mut body = serde_json::json!({
        "name": name,
        "requires_approval": require_approval,
    });
    if let Some(cmd) = command {
        body["command"] = serde_json::Value::String(cmd.to_string());
    }
    if let Some(u) = url {
        body["url"] = serde_json::Value::String(u.to_string());
    }

    match client.add_mcp_server(&body).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                note!("✔ MCP server '{name}' added to the runtime");
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os mcp remove <name>`: remove an MCP server from the runtime.
pub(super) async fn run_remove(
    client: &RuntimeClient,
    name: &str,
    confirm: bool,
    json: bool,
) -> i32 {
    // Existence check BEFORE the confirm gate: a missing server must report
    // not-found rather than prompting for a `--confirm` that can never succeed.
    if let Err(e) = client.get_mcp_server_detail(name).await {
        return match e {
            ClientError::ServerError { status: 404, .. } => crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!("MCP server '{name}' not found"),
            ),
            other => handle_client_error(other, json),
        };
    }

    if !confirm {
        return crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("use --confirm to remove server '{name}' without prompt"),
        );
    }

    match client.remove_mcp_server(name).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                note!("✔ MCP server '{name}' removed from the runtime");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, .. }) => crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("MCP server '{name}' not found"),
        ),
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os mcp get <name>`: show the details of an MCP server.
pub(super) async fn run_get_server(client: &RuntimeClient, name: &str, json: bool) -> i32 {
    match client.get_mcp_server_detail(name).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                let config = resp.get("config");
                let status = resp.get("status");
                let transport = config
                    .and_then(|c| c.get("transport"))
                    .and_then(|v| v.as_str())
                    .or_else(|| {
                        status
                            .and_then(|s| s.get("transport"))
                            .and_then(|v| v.as_str())
                    })
                    .unwrap_or("?");
                let connected = status
                    .and_then(|s| s.get("connected"))
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                let health = status
                    .and_then(|s| s.get("health"))
                    .and_then(|h| h.get("state"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let tools = resp.get("tools").and_then(|v| v.as_array());
                println!("  Server    : {name}");
                println!("  Transport : {transport}");
                println!("  Connected : {}", if connected { "yes" } else { "no" });
                println!("  Health    : {health}");
                println!("  Tools     : {}", tools.map(Vec::len).unwrap_or(0));
                if let Some(arr) = tools {
                    for t in arr {
                        if let Some(n) = t.get("name").and_then(|v| v.as_str()) {
                            println!("    - {n}");
                        }
                    }
                }
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, .. }) => crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("MCP server '{name}' not found"),
        ),
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os mcp test <name>`: re-handshake an already-installed MCP server.
///
/// Routes to the name-based, existence-aware endpoint so a missing server
/// reports a clean not-found (exit 1) instead of leaking a raw deserialization
/// error from the ephemeral-config route.
pub(super) async fn run_test_connection(client: &RuntimeClient, target: &str, json: bool) -> i32 {
    match client.test_live_mcp_server(target).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                match resp.get("kind").and_then(|v| v.as_str()) {
                    Some("success") => {
                        let tools = resp
                            .get("result")
                            .and_then(|r| r.get("tools"))
                            .and_then(|t| t.as_array())
                            .map(|a| a.len())
                            .unwrap_or(0);
                        let latency = resp
                            .get("result")
                            .and_then(|r| r.get("test_duration_ms"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        println!("✔ MCP server '{target}' reachable ({tools} tools, {latency}ms)");
                    }
                    Some("oauth_required") => {
                        println!(
                            "✗ MCP server '{target}' requires authentication (run the OAuth flow)"
                        );
                    }
                    _ => println!("✗ MCP server '{target}' test returned an unexpected response"),
                }
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, .. }) => crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("MCP server '{target}' not found"),
        ),
        Err(e) => handle_client_error(e, json),
    }
}

/// `apollia-os mcp restart <name>`: restart an MCP server.
pub(super) async fn run_restart_server(client: &RuntimeClient, name: &str, json: bool) -> i32 {
    match client.restart_mcp_server(name).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                note!("✔ MCP server '{name}' restarted");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, .. }) => crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("MCP server '{name}' not found"),
        ),
        Err(e) => handle_client_error(e, json),
    }
}

/// Patch fields forwarded to the `update` runtime handler.
pub(super) struct ServerPatch<'a> {
    pub(super) name: &'a str,
    pub(super) command: Option<&'a str>,
    pub(super) url: Option<&'a str>,
    pub(super) require_approval: Option<bool>,
}

/// `apollia-os mcp update <name>`: patch a server configuration.
///
/// Fails when no patch field is supplied; otherwise forwards a partial body to
/// `PUT /api/v1/mcp/servers/{name}/config`. The runtime merges with the
/// existing stored definition.
pub(super) async fn run_update_server(
    client: &RuntimeClient,
    patch: ServerPatch<'_>,
    json: bool,
) -> i32 {
    let ServerPatch {
        name,
        command,
        url,
        require_approval,
    } = patch;
    if command.is_none() && url.is_none() && require_approval.is_none() {
        return crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            "provide at least one of --command, --url, --require-approval",
        );
    }

    let mut body = serde_json::Map::new();
    if let Some(c) = command {
        body.insert(
            "command".to_string(),
            serde_json::Value::String(c.to_string()),
        );
    }
    if let Some(u) = url {
        body.insert("url".to_string(), serde_json::Value::String(u.to_string()));
    }
    if let Some(req) = require_approval {
        body.insert(
            "requires_approval".to_string(),
            serde_json::Value::Bool(req),
        );
    }

    match client
        .update_mcp_server_config(name, &serde_json::Value::Object(body))
        .await
    {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("* MCP server '{name}' updated");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, .. }) => crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("MCP server '{name}' not found"),
        ),
        Err(e) => handle_client_error(e, json),
    }
}

/// Prints the body of a non-error raw-config response (pretty JSON or raw).
pub(super) fn print_raw_config_body(body: String, json: bool) {
    // Body is JSON already: pretty-print when --json, raw otherwise.
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) else {
        println!("{body}");
        return;
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    } else {
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or(body));
    }
}

/// `apollia-os mcp raw-config <name>`: read the persisted launch definition.
pub(super) async fn run_get_raw_config(client: &RuntimeClient, name: &str, json: bool) -> i32 {
    let uri = format!("/api/v1/mcp/servers/{name}/raw_config");
    let resp = match client.get(&uri).await {
        Ok(resp) => resp,
        Err(e) => return handle_client_error(e, json),
    };

    if resp.status >= 400 {
        return crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &resp.body);
    }

    print_raw_config_body(resp.body, json);
    exit_codes::SUCCESS
}
