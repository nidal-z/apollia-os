//! Tauri IPC commands for MCP server management.
//!
//! Delegates to the runtime REST API for the servers, their secrets and the
//! registry, through [`RuntimeLink`], so both the embedded and the attached
//! modes are served.

use crate::attached::{segment, RuntimeLink};
use apollia_mcp::config::McpServerConfig;
use apollia_mcp::manager::{
    McpConnectionTestResult, McpResourceSummary, McpServerDetail, McpServerStatus, ProbeSpec,
};
use apollia_runtime::governance::{ConnectionEvent, ConnectionGovernance};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::mcp::npm_server::{self, NpmServerInstaller};
use crate::mcp::python_server::{self, PythonServerInstaller};

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
    state: State<'_, RuntimeLink>,
) -> Result<Vec<McpServerStatus>, String> {
    let json = state.get_json("/api/v1/mcp/servers").await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse server list: {e}"))
}

/// List MCP resources aggregated across every connected server.
///
/// Delegates to `GET /api/v1/mcp/resources` on the embedded runtime. Backs the
/// chat @-mention picker (user-initiative path). Returns an empty list when no
/// MCP server is connected.
#[tauri::command]
pub async fn list_mcp_resources(
    state: State<'_, RuntimeLink>,
) -> Result<Vec<McpResourceSummary>, String> {
    let json = state.get_json("/api/v1/mcp/resources").await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse resource list: {e}"))
}

/// Get detailed information for a single MCP server.
///
/// Delegates to `GET /api/v1/mcp/servers/{name}` on the embedded runtime.
/// Returns an error when the server is not found or MCP is not configured.
#[tauri::command]
pub async fn get_mcp_server_detail(
    state: State<'_, RuntimeLink>,
    name: String,
) -> Result<McpServerDetail, String> {
    let path = format!("/api/v1/mcp/servers/{name}");
    let json = state.get_json(&path).await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse server detail: {e}"))
}

/// Add a new MCP server and persist its configuration to `mcp.toml`.
///
/// Delegates to `POST /api/v1/mcp/servers` on the embedded runtime. The server
/// process is spawned and the MCP handshake is performed before returning.
///
/// A Python server declared as `uvx <package>` is installed first, into its
/// own virtualenv, and the persisted command names that venv's entry point;
/// an npm server declared as `npx <package>` is installed into its own
/// directory, and the persisted command runs its entry point on Node.js.
///
/// `warning_accepted` says the person accepted the warning shown before the
/// addition; it is journaled first, whatever the addition answers after.
#[tauri::command]
pub async fn add_mcp_server(
    state: State<'_, RuntimeLink>,
    config: McpServerConfig,
    warning_accepted: Option<bool>,
) -> Result<McpServerStatus, String> {
    if warning_accepted == Some(true) {
        if let RuntimeLink::Embedded(runtime) = state.inner() {
            record_warning_accepted(&runtime.connection_governance, &config.name);
        }
    }
    let config = install_local_server(&state, config).await?;
    let body =
        serde_json::to_value(&config).map_err(|e| format!("failed to serialize config: {e}"))?;
    let json = state.post_json("/api/v1/mcp/servers", &body).await?;
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
pub async fn remove_mcp_server(state: State<'_, RuntimeLink>, name: String) -> Result<(), String> {
    let path = format!("/api/v1/mcp/servers/{name}");
    state.delete_json(&path).await.map(|_| ())?;

    // The connection's own directory (a Python server's venv, an npm
    // server's packages) goes with it.
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
    state: State<'_, RuntimeLink>,
    name: String,
) -> Result<McpServerConfig, String> {
    let path = format!("/api/v1/mcp/servers/{name}/raw_config");
    let json = state.get_json(&path).await?;
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
    state: State<'_, RuntimeLink>,
    name: String,
    config: McpServerConfig,
) -> Result<McpServerStatus, String> {
    let config = install_local_server(&state, config).await?;
    let body =
        serde_json::to_value(&config).map_err(|e| format!("failed to serialize config: {e}"))?;
    let path = format!("/api/v1/mcp/servers/{name}/config");
    let json = state.put_json(&path, &body).await?;
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
    state: State<'_, RuntimeLink>,
    config: McpServerConfig,
) -> Result<McpConnectionTestResponse, String> {
    let config = install_local_server(&state, config).await?;
    let body =
        serde_json::to_value(&config).map_err(|e| format!("failed to serialize config: {e}"))?;
    let json = state.post_json("/api/v1/mcp/servers/test", &body).await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse test result: {e}"))
}

/// Give a configuration the tool classification of the curated entry it was
/// made from and the origin it is added under, then install a `uvx` server on
/// the bundled interpreter or an `npx` server on the engine store's Node.js,
/// and rewrite its command; any other configuration is otherwise unchanged.
///
/// The classification is read before the install, which rewrites the command
/// into the path of an installed entry point. The test before adding goes
/// through here too: it is the first launch, so what it installs is what the
/// addition then finds and reuses.
async fn install_local_server(
    link: &RuntimeLink,
    config: McpServerConfig,
) -> Result<McpServerConfig, String> {
    let enrichments = crate::mcp::load_builtin_enrichments();
    let config = crate::mcp::enrichments::with_selection_classification(config, &enrichments);
    let config = crate::mcp::origin::with_origin(config, &enrichments);
    if npm_server::needs_install(&config) {
        ensure_node(link).await?;
        let installer = NpmServerInstaller::new(python_server::connections_dir());
        return npm_server::prepare(config, &enrichments, &installer)
            .await
            .map_err(|e| e.to_string());
    }
    let installer = PythonServerInstaller::new(python_server::connections_dir());
    python_server::prepare(config, &enrichments, &installer)
        .await
        .map_err(|e| e.to_string())
}

/// Who the connection journal names for a change made from this window.
const DESKTOP_ACTOR: &str = "desktop";

/// Journal that the warning shown before adding the server `name` was
/// accepted. The connection is named as the connection list names it.
fn record_warning_accepted(governance: &ConnectionGovernance, name: &str) {
    governance.record_or_warn(&ConnectionEvent::WarningAccepted {
        connection_id: format!("mcp:{name}"),
        actor: DESKTOP_ACTOR.to_owned(),
    });
}

/// Where Node.js stands in the engine store: absent, downloading with the
/// bytes received, ready, or failed with its reason.
///
/// Delegates to `GET /api/v1/engines/nodejs`. The window reads it while an
/// addition that needs Node.js waits for its download, to show the progress.
#[tauri::command]
pub async fn node_runtime_state(
    state: State<'_, RuntimeLink>,
) -> Result<serde_json::Value, String> {
    state.get_json("/api/v1/engines/nodejs").await
}

/// Longest wait for the engine store to download Node.js.
const NODE_INSTALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

/// Make sure the engine store holds Node.js, asking the runtime to download it
/// when it does not and waiting for the outcome.
async fn ensure_node(link: &RuntimeLink) -> Result<(), String> {
    let installed = || {
        apollia_mcp::transport::node::default_engines_root()
            .as_deref()
            .and_then(apollia_mcp::transport::node::installed_node)
            .is_some()
    };
    if installed() {
        return Ok(());
    }
    let started = std::time::Instant::now();
    let mut state = link
        .post_json("/api/v1/engines/nodejs", &serde_json::json!({}))
        .await?;
    loop {
        match state.get("state").and_then(serde_json::Value::as_str) {
            Some("ready") => return Ok(()),
            Some("failed") => {
                let error = state
                    .get("error")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("unknown error");
                return Err(format!("Node.js could not be installed: {error}"));
            }
            Some("absent") | None => {
                return Err("Node.js is required and its download did not start".into());
            }
            _ => {}
        }
        if started.elapsed() > NODE_INSTALL_TIMEOUT {
            return Err("Node.js is still downloading; try again once it is installed".into());
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        state = link.get_json("/api/v1/engines/nodejs").await?;
    }
}

/// Resolve the connector's declared read-only probe for an installed server.
///
/// Matches the server's persisted config against the bundled enrichments by
/// remote URL (the case that matters for OAuth connectors like Notion, whose
/// grant/workspace issues only surface on a real call). Returns `None` when the
/// server has no matching enrichment or no declared `health_probe`, in which
/// case the Test reports reachability only.
async fn resolve_health_probe(link: &RuntimeLink, name: &str) -> Option<ProbeSpec> {
    let path = format!("/api/v1/mcp/servers/{name}/raw_config");
    let json = link.get_json(&path).await.ok()?;
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
    state: State<'_, RuntimeLink>,
    name: String,
) -> Result<McpConnectionTestResponse, String> {
    let probe = resolve_health_probe(&state, &name).await;
    let path = format!("/api/v1/mcp/servers/{name}/test");
    let body = serde_json::json!({ "probe": probe });
    let json = state.post_json(&path, &body).await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse test result: {e}"))
}

/// Restart an MCP server session.
///
/// Delegates to `POST /api/v1/mcp/servers/{name}/restart` on the embedded
/// runtime. Stops the current session and spawns a new one using the original
/// configuration.
#[tauri::command]
pub async fn restart_mcp_server(
    state: State<'_, RuntimeLink>,
    name: String,
) -> Result<McpServerStatus, String> {
    let path = format!("/api/v1/mcp/servers/{name}/restart");
    let json = state.post_json(&path, &serde_json::json!({})).await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse server status: {e}"))
}

/// Store a secret for an MCP server environment variable.
///
/// Delegates to `PUT /api/v1/mcp/servers/{name}/secrets/{key}` on the runtime
/// this application drives. The runtime writes it to the configured
/// [`apollia_auth::SecretStore`] (OS keychain by default, encrypted files when
/// `APOLLIA_TOKEN_STORAGE=file`), the store it resolves
/// `${APOLLIA_SECRET:KEY}` placeholders from, and journals that it was set.
#[tauri::command]
pub async fn store_mcp_secret(
    state: State<'_, RuntimeLink>,
    server_name: String,
    env_var: String,
    value: String,
) -> Result<(), String> {
    put_mcp_secret(&state, &server_name, &env_var, &value).await
}

/// Delete the secret of an MCP server environment variable. Idempotent.
///
/// Delegates to `DELETE /api/v1/mcp/servers/{name}/secrets/{key}`.
#[tauri::command]
pub async fn delete_mcp_secret(
    state: State<'_, RuntimeLink>,
    server_name: String,
    env_var: String,
) -> Result<(), String> {
    remove_mcp_secret(&state, &server_name, &env_var).await
}

/// The route of the secret `env_var` of server `server_name`, each segment
/// encoded.
fn mcp_secret_path(server_name: &str, env_var: &str) -> String {
    format!(
        "/api/v1/mcp/servers/{}/secrets/{}",
        segment(server_name),
        segment(env_var)
    )
}

/// Puts the secret through the runtime's route.
pub(crate) async fn put_mcp_secret(
    link: &RuntimeLink,
    server_name: &str,
    env_var: &str,
    value: &str,
) -> Result<(), String> {
    let body = serde_json::json!({ "value": value });
    link.put_json(&mcp_secret_path(server_name, env_var), &body)
        .await
        .map(|_| ())
}

/// Removes the secret through the runtime's route.
pub(crate) async fn remove_mcp_secret(
    link: &RuntimeLink,
    server_name: &str,
    env_var: &str,
) -> Result<(), String> {
    link.delete_json(&mcp_secret_path(server_name, env_var))
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Accepting the warning before an addition is journaled on the connection.
    #[tokio::test]
    async fn test_accepted_warning_is_journaled_on_the_connection() {
        // GIVEN governance over a journal
        let dir = tempfile::TempDir::new().unwrap();
        let journal = apollia_runtime::audit_journal::AuditJournalHandle::open(
            &dir.path().join("journal.db"),
        )
        .await
        .unwrap();
        let governance = ConnectionGovernance::without_policy(Some(journal.clone()));

        // WHEN the warning shown before adding the server `notion` is accepted
        record_warning_accepted(&governance, "notion");

        // THEN the connection's run holds one acceptance, by the desktop
        let entries = journal
            .query_run(&apollia_runtime::governance::connection_run_id(
                "mcp:notion",
            ))
            .await;
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].kind,
            apollia_runtime::audit_journal::JournalEntryKind::ConnectionWarningAccepted
        );
        assert_eq!(entries[0].payload["event"], "warning_accepted");
        assert_eq!(entries[0].payload["actor"], "desktop");
        journal.shutdown().await;
    }
}
