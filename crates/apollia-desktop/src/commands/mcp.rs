//! Tauri IPC commands for MCP server management.
//!
//! Delegates to the runtime REST API (via TCP) for CRUD operations on connected
//! servers, and directly to [`McpRegistryClient`] and the configured
//! [`apollia_auth::SecretStore`] for registry discovery and secret management.

use apollia_auth::SecretStore;
use apollia_mcp::config::{McpServerConfig, MCP_STATIC_SECRET_SERVICE};
use apollia_mcp::manager::{
    McpConnectionTestResult, McpResourceSummary, McpServerDetail, McpServerStatus, ProbeSpec,
};
use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::mcp::python_server::{self, PythonServerInstaller};

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
///
/// A Python server declared as `uvx <package>` is installed first, into its
/// own virtualenv, and the persisted command names that venv's entry point.
#[tauri::command]
pub async fn add_mcp_server(
    state: State<'_, RuntimeHandle>,
    config: McpServerConfig,
) -> Result<McpServerStatus, String> {
    let config = install_python_server(config).await?;
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

    // The connection's own directory (a Python server's venv) goes with it.
    if let Err(e) = python_server::remove_connection_dir(&python_server::connections_dir(), &name) {
        tracing::warn!(
            server = %name,
            error = %e,
            detail = "the connection directory may be orphaned",
            "mcp.connection_dir.purge.failed"
        );
    }

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
    let config = install_python_server(config).await?;
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
    let config = install_python_server(config).await?;
    let body =
        serde_json::to_value(&config).map_err(|e| format!("failed to serialize config: {e}"))?;
    let json = http_post_json(state.api_port, "/api/v1/mcp/servers/test", &body).await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse test result: {e}"))
}

/// Give a configuration the tool classification of the curated entry it was
/// made from and the origin it is added under, then install a `uvx` server on the bundled interpreter and
/// rewrite its command; any other configuration is otherwise unchanged.
///
/// The classification is read before the install, which rewrites a `uvx`
/// command into the path of its entry point. The test before adding goes
/// through here too: it is the first launch, so the venv it builds is the one
/// the addition then finds and reuses.
async fn install_python_server(config: McpServerConfig) -> Result<McpServerConfig, String> {
    let installer = PythonServerInstaller::new(python_server::connections_dir());
    let enrichments = crate::mcp::load_builtin_enrichments();
    let config = crate::mcp::enrichments::with_selection_classification(config, &enrichments);
    let config = crate::mcp::origin::with_origin(config, &enrichments);
    python_server::prepare(config, &enrichments, &installer)
        .await
        .map_err(|e| e.to_string())
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

/// Store a secret for an MCP server environment variable.
///
/// The secret goes through the configured [`apollia_auth::SecretStore`]
/// (OS keychain by default, encrypted files when `APOLLIA_TOKEN_STORAGE=file`),
/// the store the runtime resolves `${APOLLIA_SECRET:KEY}` placeholders from.
#[tauri::command]
pub async fn store_mcp_secret(
    server_name: String,
    env_var: String,
    value: String,
) -> Result<(), String> {
    let store = open_secret_store()?;
    put_mcp_secret(&*store, &server_name, &env_var, &value)
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

/// Delete the secret of an MCP server environment variable. Idempotent.
#[tauri::command]
pub async fn delete_mcp_secret(server_name: String, env_var: String) -> Result<(), String> {
    let store = open_secret_store()?;
    remove_mcp_secret(&*store, &server_name, &env_var)
}

fn open_secret_store() -> Result<Box<dyn SecretStore>, String> {
    apollia_auth::select_secret_store().map_err(|e| format!("secret store unavailable: {e}"))
}

/// Store key of a static MCP secret: `"{server_name}:{env_var}"`, the key
/// `apollia_mcp::config` resolves `${APOLLIA_SECRET:KEY}` with, under the
/// [`MCP_STATIC_SECRET_SERVICE`] service.
fn mcp_secret_key(server_name: &str, env_var: &str) -> String {
    format!("{server_name}:{env_var}")
}

fn put_mcp_secret(
    store: &dyn SecretStore,
    server_name: &str,
    env_var: &str,
    value: &str,
) -> Result<(), String> {
    store
        .set(
            MCP_STATIC_SECRET_SERVICE,
            &mcp_secret_key(server_name, env_var),
            value,
        )
        .map_err(|e| e.to_string())
}

fn remove_mcp_secret(
    store: &dyn SecretStore,
    server_name: &str,
    env_var: &str,
) -> Result<(), String> {
    store
        .delete(
            MCP_STATIC_SECRET_SERVICE,
            &mcp_secret_key(server_name, env_var),
        )
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn age_store(dir: &std::path::Path) -> apollia_auth::AgeFileSecretStore {
        apollia_auth::AgeFileSecretStore::new(dir.to_path_buf(), "test-pass".into())
            .expect("age store")
    }

    #[test]
    fn test_secret_key_and_service_match_the_keychain_names() {
        // GIVEN the names the desktop keychain wrapper wrote under before it
        // was folded into the shared store: service "apollia-mcp", account
        // "{server}:{env_var}"
        // WHEN the store names are produced for the same inputs
        // THEN they are identical, so no keychain entry has to be migrated
        assert_eq!(MCP_STATIC_SECRET_SERVICE, "apollia-mcp");
        assert_eq!(
            mcp_secret_key("notion", "NOTION_API_KEY"),
            "notion:NOTION_API_KEY"
        );
        assert_eq!(mcp_secret_key("", "KEY"), ":KEY");
    }

    #[test]
    fn test_put_read_remove_through_the_shared_store_in_age_file_mode() {
        // GIVEN the encrypted-file store on a throwaway directory
        let dir = tempfile::tempdir().expect("tempdir");
        let store = age_store(dir.path());

        // WHEN the desktop command body stores a secret
        put_mcp_secret(&store, "notion", "NOTION_API_KEY", "secret-1").expect("put");
        // THEN the runtime resolver's slot holds it
        assert_eq!(
            age_store(dir.path())
                .get(MCP_STATIC_SECRET_SERVICE, "notion:NOTION_API_KEY")
                .expect("get")
                .as_deref(),
            Some("secret-1")
        );

        // WHEN the desktop command body removes it, twice
        remove_mcp_secret(&store, "notion", "NOTION_API_KEY").expect("remove");
        remove_mcp_secret(&store, "notion", "NOTION_API_KEY").expect("remove again");
        // THEN the slot is empty and the second removal did not fail
        assert!(store
            .get(MCP_STATIC_SECRET_SERVICE, "notion:NOTION_API_KEY")
            .expect("get")
            .is_none());
    }
}
