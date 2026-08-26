//! Tauri IPC commands for MCP server management.
//!
//! Delegates to the runtime REST API (via TCP) for CRUD operations on connected
//! servers, and directly to [`McpRegistryClient`] and [`SecretStore`] for
//! registry discovery and secret management.

use apollia_mcp::config::McpServerConfig;
use apollia_mcp::manager::{
    McpConnectionTestResult, McpResourceSummary, McpServerDetail, McpServerStatus, ProbeSpec,
};
use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::mcp::secret_store::SecretStore;

use super::{http_delete_json, http_get_json, http_patch_json, http_post_json, http_put_json};

/// The catalogue shapes live in `catalog`, the two registry fetches in
/// `discovery`, and the HTTP OAuth path in `oauth`.
pub mod catalog;
pub mod discovery;
pub mod oauth;

/// List all connected MCP servers with their status.
///
/// Delegates to `GET /api/v1/mcp/servers` on the embedded runtime.
/// Returns an empty list when no MCP servers are configured.
#[tauri::command]
pub async fn list_mcp_servers(
    state: State<'_, RuntimeHandle>,
) -> Result<Vec<McpServerStatus>, String> {
    let json = http_get_json(state.api_port, "/api/v1/mcp/servers").await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse server list: {e}"))
}

/// List MCP resources aggregated across every connected server.
///
/// Delegates to `GET /api/v1/mcp/resources` on the embedded runtime. Backs the
/// chat @-mention picker (user-initiative path). Returns an empty list when no
/// MCP server is connected.
#[tauri::command]
pub async fn list_mcp_resources(
    state: State<'_, RuntimeHandle>,
) -> Result<Vec<McpResourceSummary>, String> {
    let json = http_get_json(state.api_port, "/api/v1/mcp/resources").await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse resource list: {e}"))
}

/// Get detailed information for a single MCP server.
///
/// Delegates to `GET /api/v1/mcp/servers/{name}` on the embedded runtime.
/// Returns an error when the server is not found or MCP is not configured.
#[tauri::command]
pub async fn get_mcp_server_detail(
    state: State<'_, RuntimeHandle>,
    name: String,
) -> Result<McpServerDetail, String> {
    let path = format!("/api/v1/mcp/servers/{name}");
    let json = http_get_json(state.api_port, &path).await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse server detail: {e}"))
}

/// Add a new MCP server and persist its configuration to `mcp.toml`.
///
/// Delegates to `POST /api/v1/mcp/servers` on the embedded runtime. The server
/// process is spawned and the MCP handshake is performed before returning.
#[tauri::command]
pub async fn add_mcp_server(
    state: State<'_, RuntimeHandle>,
    config: McpServerConfig,
) -> Result<McpServerStatus, String> {
    let body =
        serde_json::to_value(&config).map_err(|e| format!("failed to serialize config: {e}"))?;
    let json = http_post_json(state.api_port, "/api/v1/mcp/servers", &body).await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse server status: {e}"))
}

/// Remove an MCP server, delete its configuration from `mcp.db`, and purge
/// the associated OAuth token from the keychain.
///
/// Delegates to `DELETE /api/v1/mcp/servers/{name}` on the embedded runtime
/// for the DB cleanup, then **idempotently** deletes the keychain entry at
/// `(apollia-mcp-oauth, <server_name>)` so a future reinstall doesn't
/// inherit a stale token that would silently fail with `invalid_grant`.
///
/// Keychain deletion errors are logged but not propagated: leaving an
/// orphan keychain entry is preferable to blocking the user from removing
/// a server they no longer want.
#[tauri::command]
pub async fn remove_mcp_server(
    state: State<'_, RuntimeHandle>,
    name: String,
) -> Result<(), String> {
    let path = format!("/api/v1/mcp/servers/{name}");
    http_delete_json(state.api_port, &path).await.map(|_| ())?;

    // Best-effort OAuth token cleanup. We don't know at this layer whether
    // the server used OAuth, so we always attempt the delete - it's a no-op
    // when no entry exists.
    if let Ok(store) = apollia_auth::select_secret_store() {
        if let Err(e) = apollia_auth::delete_mcp_token(&*store, &name) {
            tracing::warn!(
                server = %name,
                error = %e,
                detail = "the keychain entry may be orphaned",
                "mcp.oauth.token.purge.failed"
            );
        }
    }
    Ok(())
}

/// Return the raw persisted launch configuration of a server.
///
/// Delegates to `GET /api/v1/mcp/servers/{name}/raw_config`. Used by the
/// desktop "edit arguments" flow to seed the inline edit form with
/// the current command/args/env (placeholders, never resolved secrets).
#[tauri::command]
pub async fn get_mcp_server_raw_config(
    state: State<'_, RuntimeHandle>,
    name: String,
) -> Result<McpServerConfig, String> {
    let path = format!("/api/v1/mcp/servers/{name}/raw_config");
    let json = http_get_json(state.api_port, &path).await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse server config: {e}"))
}

/// Replace an MCP server's launch configuration and restart its session.
///
/// Delegates to `PUT /api/v1/mcp/servers/{name}/config` on the embedded
/// runtime, which performs a remove → add (restart) → persist cycle so the
/// new `command` / `args` / `env` / `transport` take effect immediately.
/// Used by the desktop "edit arguments" flow on the manage panel,
/// which lets the operator fix runtime parameters (e.g. allowed directories
/// for `@modelcontextprotocol/server-filesystem`) without re-running the
/// full install wizard.
///
/// The `config.name` field must equal `name`; the runtime rejects mismatches.
#[tauri::command]
pub async fn update_mcp_server_config(
    state: State<'_, RuntimeHandle>,
    name: String,
    config: McpServerConfig,
) -> Result<McpServerStatus, String> {
    let body =
        serde_json::to_value(&config).map_err(|e| format!("failed to serialize config: {e}"))?;
    let path = format!("/api/v1/mcp/servers/{name}/config");
    let json = http_put_json(state.api_port, &path, &body).await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse server status: {e}"))
}

/// Tagged response envelope for `test_mcp_connection`, mirroring the
/// runtime-side `McpConnectionTestResponse`.
///
/// The wizard dispatches its Auth step UI on `kind`:
/// - `success` → list tools, allow continue.
/// - `oauth_required` → switch to "Sign in with <provider>" mode and call
///   [`mcp_oauth_login`] when the user clicks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpConnectionTestResponse {
    Success {
        #[serde(flatten)]
        result: McpConnectionTestResult,
    },
    OauthRequired {
        /// Verbatim `WWW-Authenticate` header captured from the 401.
        www_authenticate: String,
    },
}

/// Test an MCP server configuration without persisting a session.
///
/// Delegates to `POST /api/v1/mcp/servers/test` on the embedded runtime.
/// Spawns an ephemeral process, performs the MCP handshake, then immediately
/// terminates the process without modifying `mcp.toml` or the tool registry.
///
/// Returns a tagged enum so the wizard can route Auth step UI without an
/// extra round-trip - `oauth_required` means the server returned 401 and the
/// MCP HTTP OAuth flow should be driven via [`mcp_oauth_login`].
#[tauri::command]
pub async fn test_mcp_connection(
    state: State<'_, RuntimeHandle>,
    config: McpServerConfig,
) -> Result<McpConnectionTestResponse, String> {
    let body =
        serde_json::to_value(&config).map_err(|e| format!("failed to serialize config: {e}"))?;
    let json = http_post_json(state.api_port, "/api/v1/mcp/servers/test", &body).await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse test result: {e}"))
}

/// Resolve the connector's declared read-only probe for an installed server.
///
/// Matches the server's persisted config against the bundled enrichments by
/// remote URL (the case that matters for OAuth connectors like Notion, whose
/// grant/workspace issues only surface on a real call). Returns `None` when the
/// server has no matching enrichment or no declared `health_probe`, in which
/// case the Test reports reachability only.
async fn resolve_health_probe(api_port: u16, name: &str) -> Option<ProbeSpec> {
    let path = format!("/api/v1/mcp/servers/{name}/raw_config");
    let json = http_get_json(api_port, &path).await.ok()?;
    let config: McpServerConfig = serde_json::from_value(json).ok()?;
    let enrichments = crate::mcp::enrichments::load_builtin_enrichments();
    let enrichment = enrichments
        .iter()
        .find(|e| config.url.is_some() && e.remote_url.as_deref() == config.url.as_deref())?;
    let probe = enrichment.health_probe.as_ref()?;
    Some(ProbeSpec {
        tool: probe.tool.clone(),
        args: probe.args.clone(),
    })
}

/// Test an already-installed MCP server.
///
/// Delegates to `POST /api/v1/mcp/servers/{name}/test` on the embedded runtime.
/// Re-handshakes the live session for reachability and, when the connector
/// declares a read-only `health_probe`, exercises real operational access. The
/// response `result.live_health` carries the verdict, so the UI can tell
/// "reachable" apart from "actually working".
#[tauri::command]
pub async fn test_mcp_live_server(
    state: State<'_, RuntimeHandle>,
    name: String,
) -> Result<McpConnectionTestResponse, String> {
    let probe = resolve_health_probe(state.api_port, &name).await;
    let path = format!("/api/v1/mcp/servers/{name}/test");
    let body = serde_json::json!({ "probe": probe });
    let json = http_post_json(state.api_port, &path, &body).await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse test result: {e}"))
}

/// Restart an MCP server session.
///
/// Delegates to `POST /api/v1/mcp/servers/{name}/restart` on the embedded
/// runtime. Stops the current session and spawns a new one using the original
/// configuration.
#[tauri::command]
pub async fn restart_mcp_server(
    state: State<'_, RuntimeHandle>,
    name: String,
) -> Result<McpServerStatus, String> {
    let path = format!("/api/v1/mcp/servers/{name}/restart");
    let json = http_post_json(state.api_port, &path, &serde_json::json!({})).await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse server status: {e}"))
}

/// Store a secret in the OS keychain for an MCP server environment variable.
///
/// The secret is stored under the composite key `"{server_name}:{env_var}"`.
#[tauri::command]
pub async fn store_mcp_secret(
    secret_store: State<'_, SecretStore>,
    server_name: String,
    env_var: String,
    value: String,
) -> Result<(), String> {
    let key = SecretStore::key_for(&server_name, &env_var);
    secret_store.store(&key, &value).map_err(|e| e.to_string())
}

/// Update the `requires_approval` flag for a running MCP server.
///
/// Applies the change in-memory immediately and persists it to `mcp.toml`.
/// The server session is not restarted; the flag takes effect on the next tool call.
/// Returns the updated server status.
#[tauri::command]
pub async fn set_mcp_server_approval(
    state: State<'_, RuntimeHandle>,
    name: String,
    requires_approval: bool,
) -> Result<McpServerStatus, String> {
    let body = serde_json::json!({ "requires_approval": requires_approval });
    let path = format!("/api/v1/mcp/servers/{name}/approval");
    let json = http_patch_json(state.api_port, &path, &body).await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse server status: {e}"))
}

/// Delete a secret from the OS keychain for an MCP server environment variable.
///
/// The secret is looked up under the composite key `"{server_name}:{env_var}"`.
#[tauri::command]
pub async fn delete_mcp_secret(
    secret_store: State<'_, SecretStore>,
    server_name: String,
    env_var: String,
) -> Result<(), String> {
    let key = SecretStore::key_for(&server_name, &env_var);
    secret_store.delete(&key).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use crate::mcp::secret_store::SecretStore;

    // ── store_mcp_secret uses the correct composite key ───────────────

    #[test]
    fn test_store_secret_delegates_to_secret_store() {
        // GIVEN a server name and env var name
        // WHEN the composite key is generated (as done inside store_mcp_secret)
        let key = SecretStore::key_for("notion", "NOTION_API_KEY");
        // THEN the key follows the "{server}:{env_var}" convention
        assert_eq!(key, "notion:NOTION_API_KEY");
    }

    // ── delete_mcp_secret uses the correct composite key ──────────────

    #[test]
    fn test_delete_secret_delegates_to_secret_store() {
        // GIVEN a server name and env var name
        // WHEN the composite key is generated (as done inside delete_mcp_secret)
        let key = SecretStore::key_for("slack", "SLACK_BOT_TOKEN");
        // THEN the key follows the "{server}:{env_var}" convention
        assert_eq!(key, "slack:SLACK_BOT_TOKEN");
    }
}
