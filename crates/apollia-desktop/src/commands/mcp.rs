//! Tauri IPC commands for MCP server management.
//!
//! Delegates to the runtime REST API (via TCP) for CRUD operations on connected
//! servers, and directly to [`McpRegistryClient`] and [`SecretStore`] for
//! registry discovery and secret management.

use apollia_mcp::config::McpServerConfig;
use apollia_mcp::manager::{McpConnectionTestResult, McpServerDetail, McpServerStatus};
use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::mcp::registry_client::{
    McpRegistryClient, RegistryIcon, RegistryPackage, RegistryRepository, RegistryServer,
};
use crate::mcp::secret_store::SecretStore;

use super::{http_delete_json, http_get_json, http_post_json};

/// Flattened view of a registry server entry for the catalogue UI.
///
/// Removes the `server` / `_meta` nesting of [`RegistryServer`] and exposes
/// all relevant metadata at the top level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryServerView {
    /// Package identifier (e.g. `@notionhq/notion-mcp-server`).
    pub name: String,
    /// Human-readable display name.
    pub title: Option<String>,
    /// Short description of the server's capabilities.
    pub description: Option<String>,
    /// Semantic version of this registry entry.
    pub version: String,
    /// Documentation or product website URL.
    pub website_url: Option<String>,
    /// Installable packages (npm, pip, …).
    pub packages: Option<Vec<RegistryPackage>>,
    /// Icon assets for display in the catalogue.
    pub icons: Option<Vec<RegistryIcon>>,
    /// Source code repository reference.
    pub repository: Option<RegistryRepository>,
}

impl From<RegistryServer> for RegistryServerView {
    fn from(s: RegistryServer) -> Self {
        Self {
            name: s.server.name,
            title: s.server.title,
            description: s.server.description,
            version: s.server.version,
            website_url: s.server.website_url,
            packages: s.server.packages,
            icons: s.server.icons,
            repository: s.server.repository,
        }
    }
}

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

/// Remove an MCP server and delete its configuration from `mcp.toml`.
///
/// Delegates to `DELETE /api/v1/mcp/servers/{name}` on the embedded runtime.
#[tauri::command]
pub async fn remove_mcp_server(
    state: State<'_, RuntimeHandle>,
    name: String,
) -> Result<(), String> {
    let path = format!("/api/v1/mcp/servers/{name}");
    http_delete_json(state.api_port, &path).await.map(|_| ())
}

/// Test an MCP server configuration without persisting a session.
///
/// Delegates to `POST /api/v1/mcp/servers/test` on the embedded runtime.
/// Spawns an ephemeral process, performs the MCP handshake, then immediately
/// terminates the process without modifying `mcp.toml` or the tool registry.
#[tauri::command]
pub async fn test_mcp_connection(
    state: State<'_, RuntimeHandle>,
    config: McpServerConfig,
) -> Result<McpConnectionTestResult, String> {
    let body =
        serde_json::to_value(&config).map_err(|e| format!("failed to serialize config: {e}"))?;
    let json = http_post_json(state.api_port, "/api/v1/mcp/servers/test", &body).await?;
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

/// Fetch MCP Registry servers with local cache and offline fallback.
///
/// Queries the official MCP registry for available servers, updating the local
/// disk cache on success. Falls back to cached data when the registry is
/// unreachable.
#[tauri::command]
pub async fn fetch_mcp_registry(
    registry: State<'_, McpRegistryClient>,
    search: Option<String>,
) -> Result<Vec<RegistryServerView>, String> {
    registry
        .fetch_servers(search.as_deref())
        .await
        .map(|servers| servers.into_iter().map(RegistryServerView::from).collect())
        .map_err(|e| e.to_string())
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
    use super::*;
    use crate::mcp::registry_client::RegistryServer;
    use crate::mcp::secret_store::SecretStore;

    // ── AC-4 : RegistryServerView flattens RegistryServer correctly ──────────

    #[test]
    fn test_ac4_registry_server_view_from_maps_all_fields() {
        // GIVEN a RegistryServer with nested server detail
        let raw = serde_json::json!({
            "server": {
                "name": "notion",
                "title": "Notion",
                "description": "Read and write Notion pages",
                "version": "1.0.0",
                "repository": null,
                "websiteUrl": "https://notion.so",
                "packages": null,
                "icons": null
            },
            "_meta": null
        });
        let server: RegistryServer = serde_json::from_value(raw).unwrap();

        // WHEN converted to a view
        let view = RegistryServerView::from(server);

        // THEN all fields are lifted to the top level and meta is dropped
        assert_eq!(view.name, "notion");
        assert_eq!(view.title.as_deref(), Some("Notion"));
        assert_eq!(view.version, "1.0.0");
        assert_eq!(view.website_url.as_deref(), Some("https://notion.so"));
        assert!(view.packages.is_none());
    }

    #[test]
    fn test_ac4_registry_server_view_from_sqlite() {
        // GIVEN a RegistryServer for a server without website or packages
        let raw = serde_json::json!({
            "server": {
                "name": "@modelcontextprotocol/server-sqlite",
                "title": "SQLite",
                "description": "Query local SQLite databases",
                "version": "0.3.0",
                "repository": null,
                "websiteUrl": null,
                "packages": null,
                "icons": null
            },
            "_meta": null
        });
        let server: RegistryServer = serde_json::from_value(raw).unwrap();

        // WHEN converted to a view
        let view = RegistryServerView::from(server);

        // THEN optional fields are None and required fields are present
        assert_eq!(view.name, "@modelcontextprotocol/server-sqlite");
        assert_eq!(view.version, "0.3.0");
        assert!(view.website_url.is_none());
        assert!(view.icons.is_none());
    }

    // ── AC-5 : store_mcp_secret uses the correct composite key ───────────────

    #[test]
    fn test_ac5_store_secret_delegates_to_secret_store() {
        // GIVEN a server name and env var name
        // WHEN the composite key is generated (as done inside store_mcp_secret)
        let key = SecretStore::key_for("notion", "NOTION_API_KEY");
        // THEN the key follows the "{server}:{env_var}" convention
        assert_eq!(key, "notion:NOTION_API_KEY");
    }

    // ── AC-5 : delete_mcp_secret uses the correct composite key ──────────────

    #[test]
    fn test_ac5_delete_secret_delegates_to_secret_store() {
        // GIVEN a server name and env var name
        // WHEN the composite key is generated (as done inside delete_mcp_secret)
        let key = SecretStore::key_for("slack", "SLACK_BOT_TOKEN");
        // THEN the key follows the "{server}:{env_var}" convention
        assert_eq!(key, "slack:SLACK_BOT_TOKEN");
    }
}
