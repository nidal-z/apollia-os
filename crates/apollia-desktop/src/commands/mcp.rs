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
use apollia_mcp::transport::node::{
    MachineNode, MachineNodeMissing, MachineNodeSearch, MIN_MACHINE_NODE_MAJOR,
};

/// The catalogue shapes live in `catalog`, the two registry fetches in
/// `discovery`, and the HTTP OAuth path in `oauth`.
pub mod catalog;
pub mod discovery;
pub mod oauth;

/// Longest wait for the addition or the test of a server: the runtime starts
/// it and waits for its handshake, which a first `npx` or `uvx` launch can
/// stretch well past the usual deadline of a request. The embedded arm sets
/// no deadline on these; the attached arm waits this long.
const MCP_HANDSHAKE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(120);

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
    let json = state
        .post_json_within("/api/v1/mcp/servers", &body, MCP_HANDSHAKE_DEADLINE)
        .await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse server status: {e}"))
}

/// Remove an MCP server and everything it leaves behind.
///
/// Delegates to `DELETE /api/v1/connections/mcp:{name}`, which stops its
/// session when there is one, deletes its configuration from `mcp.db`, the
/// files installed for it, its sign-in token and the secrets its
/// configuration names, and revokes the tools agents were granted from it.
/// A server that never started is removed all the same.
#[tauri::command]
pub async fn remove_mcp_server(state: State<'_, RuntimeLink>, name: String) -> Result<(), String> {
    state
        .delete_json(&remove_connection_path(&name))
        .await
        .map(|_| ())
}

/// The route that removes the connection of MCP server `name`.
fn remove_connection_path(name: &str) -> String {
    format!("/api/v1/connections/mcp:{name}")
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
    let json = state
        .post_json_within("/api/v1/mcp/servers/test", &body, MCP_HANDSHAKE_DEADLINE)
        .await?;
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

/// Make sure a Node.js is there for a server that needs one: the engine
/// store's, asking the runtime to download it when it does not hold it and
/// waiting for the outcome, or, when the channel cannot provide it, the one
/// installed on the machine.
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
    let channel = node_from_channel(link).await;
    let machine = MachineNodeSearch::from_env();
    let source = tokio::task::spawn_blocking(move || settle_node(channel, || machine.find()))
        .await
        .map_err(|e| format!("the search for Node.js stopped: {e}"))??;
    if let NodeSource::Machine {
        node,
        channel_error,
    } = source
    {
        tracing::event!(
            tracing::Level::WARN,
            channel_error = %channel_error,
            node = %node.install.node.display(),
            version = %node.version,
            "mcp.node.machine_fallback"
        );
    }
    Ok(())
}

/// Ask the runtime to download Node.js into the engine store and wait for
/// the outcome; the error says why the channel could not provide it.
async fn node_from_channel(link: &RuntimeLink) -> Result<(), String> {
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
                return Err(error.to_owned());
            }
            Some("absent") | None => {
                return Err("its download did not start".into());
            }
            _ => {}
        }
        if started.elapsed() > NODE_INSTALL_TIMEOUT {
            return Err("its download is still running".into());
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        state = link.get_json("/api/v1/engines/nodejs").await?;
    }
}

/// Start of the error an addition gives when no Node.js is available, the
/// code the window reads to say it in the language of the person; the rest
/// is the detail, in English.
const NODE_UNAVAILABLE_PREFIX: &str = "node_unavailable: ";

/// The help page that tells how to give Apollia a Node.js.
const NODE_HELP_URL: &str =
    "https://docs.apollia.fr/operator-help/integrations/connect-an-mcp-server#nodejs";

/// Where the Node.js of an addition comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
enum NodeSource {
    /// The engine store's, verified by Apollia.
    Store,
    /// The one installed on the machine, used because the channel could not
    /// provide Apollia's, for the reason `channel_error` gives.
    Machine {
        node: MachineNode,
        channel_error: String,
    },
}

/// Decide which Node.js an addition runs on, from what the channel answered:
/// the store's when the channel provided it, otherwise the one `find_machine`
/// finds on the machine (asked only then), and when there is none, an error
/// that starts with [`NODE_UNAVAILABLE_PREFIX`] and says what to do.
fn settle_node(
    channel: Result<(), String>,
    find_machine: impl FnOnce() -> Result<MachineNode, MachineNodeMissing>,
) -> Result<NodeSource, String> {
    let channel_error = match channel {
        Ok(()) => return Ok(NodeSource::Store),
        Err(e) => e,
    };
    match find_machine() {
        Ok(node) => Ok(NodeSource::Machine {
            node,
            channel_error,
        }),
        Err(missing) => {
            tracing::event!(
                tracing::Level::WARN,
                channel_error = %channel_error,
                machine = %missing,
                "mcp.node.unavailable"
            );
            Err(format!(
                "{NODE_UNAVAILABLE_PREFIX}This service needs Node.js. Apollia could not provide its own \
                 ({channel_error}), and {missing}. Install Node.js \
                 {MIN_MACHINE_NODE_MAJOR} or later from https://nodejs.org, or wait \
                 until Apollia publishes it, then add the service again. Help: \
                 {NODE_HELP_URL}"
            ))
        }
    }
}

/// Resolve the connector's declared read-only probe for an installed server.
///
/// Matches the server's persisted config against the bundled enrichments by
/// remote URL (the case that matters for OAuth connectors like Notion, whose
/// grant/workspace issues only surface on a real call). Returns `None` when the
/// server has no matching enrichment or no declared `health_probe`, in which
/// case the test or the check reports reachability only.
pub(crate) async fn resolve_health_probe(link: &RuntimeLink, name: &str) -> Option<ProbeSpec> {
    health_probe_of(&raw_config(link, name).await?)
}

/// The persisted configuration of server `name`, `None` when it cannot be read.
async fn raw_config(link: &RuntimeLink, name: &str) -> Option<McpServerConfig> {
    let path = format!("/api/v1/mcp/servers/{name}/raw_config");
    let json = link.get_json(&path).await.ok()?;
    serde_json::from_value(json).ok()
}

/// The read-only probe the bundled enrichment of `config` declares.
fn health_probe_of(config: &McpServerConfig) -> Option<ProbeSpec> {
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
/// response `result.live_health` carries the verdict and `tools` the tools the
/// session lists, which the trial after an addition reports.
///
/// For a server launched by `node` or `npx`, the answer also says which
/// Node.js ran it (`node_source`): the engine store's, or the machine's when
/// the store holds none, the order its launch resolves them in.
#[tauri::command]
pub async fn test_mcp_live_server(
    state: State<'_, RuntimeLink>,
    name: String,
) -> Result<McpLiveTestResponse, String> {
    let config = raw_config(&state, &name).await;
    let probe = config.as_ref().and_then(health_probe_of);
    let node_source = config.as_ref().and_then(|c| {
        let store_holds_node = apollia_mcp::transport::node::default_engines_root()
            .as_deref()
            .and_then(apollia_mcp::transport::node::installed_node)
            .is_some();
        node_source_of(&c.command, store_holds_node)
    });
    let path = format!("/api/v1/mcp/servers/{name}/test");
    let body = serde_json::json!({ "probe": probe });
    let json = state.post_json(&path, &body).await?;
    let response =
        serde_json::from_value(json).map_err(|e| format!("failed to parse test result: {e}"))?;
    Ok(McpLiveTestResponse {
        response,
        node_source,
    })
}

/// Which Node.js runs a server, as the trial after an addition says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeOrigin {
    /// The engine store's, verified by Apollia.
    Store,
    /// The one installed on the machine, the store holding none.
    Machine,
}

/// The answer of [`test_mcp_live_server`]: the test, and for a server that
/// runs on Node.js, which one.
#[derive(Debug, Clone, Serialize)]
pub struct McpLiveTestResponse {
    #[serde(flatten)]
    pub response: McpConnectionTestResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_source: Option<NodeOrigin>,
}

/// The Node.js a server launched by `command` runs on, `None` when it does
/// not run on Node.js.
fn node_source_of(command: &str, store_holds_node: bool) -> Option<NodeOrigin> {
    if !apollia_mcp::transport::node::runs_on_node(command) {
        return None;
    }
    Some(if store_holds_node {
        NodeOrigin::Store
    } else {
        NodeOrigin::Machine
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    // Removing a server goes through the removal of its connection, which
    // erases what the server leaves behind.
    #[test]
    fn test_removing_a_server_removes_its_connection() {
        // GIVEN the MCP server `notion`
        let name = "notion";
        // WHEN the route its removal calls is built
        let path = remove_connection_path(name);
        // THEN it is the removal of the connection `mcp:notion`
        assert_eq!(path, "/api/v1/connections/mcp:notion");
    }

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

    /// A Node.js 22 of the machine, as the search would answer it; nothing
    /// is written or run.
    fn machine_node(version: &str) -> MachineNode {
        MachineNode {
            install: apollia_mcp::transport::node::NodeInstall {
                node: "/opt/homebrew/opt/node@22/bin/node".into(),
                npx_cli: Some(
                    "/opt/homebrew/opt/node@22/lib/node_modules/npm/bin/npx-cli.js".into(),
                ),
            },
            version: version.to_owned(),
        }
    }

    /// The search of a machine that holds no Node.js Apollia accepts, having
    /// seen `rejected`.
    fn missing(rejected: Vec<apollia_mcp::transport::node::RejectedNode>) -> MachineNodeMissing {
        MachineNodeMissing { rejected }
    }

    const CHANNEL_404: &str =
        "https://github.com/Apollia-OS/apollia-os/releases/download/engines-stable/engines.json answered HTTP 404";

    // A channel that answers 404 leaves the addition to the Node.js of the
    // machine, when one is installed.
    #[test]
    fn test_a_channel_in_404_falls_back_on_the_node_of_the_machine() {
        // GIVEN a channel that could not provide Node.js, and a search that
        //       finds a Node.js 22 on the machine
        let found = machine_node("v22.11.0");

        // WHEN the addition settles which Node.js it runs on
        let source = settle_node(Err(CHANNEL_404.to_owned()), || Ok(found.clone())).unwrap();

        // THEN it runs on the machine's, and keeps why the channel failed
        let expected = NodeSource::Machine {
            node: found,
            channel_error: CHANNEL_404.to_owned(),
        };
        assert_eq!(source, expected);
    }

    // The channel's Node.js is preferred: when it is there, the machine is
    // not searched.
    #[test]
    fn test_a_channel_that_provides_node_is_preferred() {
        // GIVEN a channel that provided Node.js, and a machine holding another
        let mut searched = false;

        // WHEN the addition settles which Node.js it runs on
        let source = settle_node(Ok(()), || {
            searched = true;
            Ok(machine_node("v24.1.0"))
        })
        .unwrap();

        // THEN it is the store's, and the machine was not searched
        assert_eq!(source, NodeSource::Store);
        assert!(!searched);
    }

    // A machine whose only Node.js is too old is not a fallback.
    #[test]
    fn test_a_node_too_old_on_the_machine_is_no_fallback() {
        // GIVEN a channel in 404, and a search that left out a Node.js 18
        let rejected = missing(vec![apollia_mcp::transport::node::RejectedNode {
            node: "/usr/local/bin/node".into(),
            reason: "v18.19.0, older than the 22 release Apollia needs".to_owned(),
        }]);

        // WHEN the addition settles which Node.js it runs on
        let message = settle_node(Err(CHANNEL_404.to_owned()), || Err(rejected)).unwrap_err();

        // THEN it is refused under the stable code, and names the old release
        assert!(message.starts_with(NODE_UNAVAILABLE_PREFIX), "{message}");
        assert!(message.contains("v18.19.0"), "{message}");
        assert!(message.contains("Node.js 22 or later"), "{message}");
    }

    // Without any Node.js, the error carries the stable code the window
    // reads, then says plainly what to do.
    #[test]
    fn test_without_any_node_the_message_says_what_to_do() {
        // GIVEN a channel in 404, and a machine without Node.js
        let none = missing(Vec::new());

        // WHEN the addition settles which Node.js it runs on
        let message = settle_node(Err(CHANNEL_404.to_owned()), || Err(none)).unwrap_err();

        // THEN the message starts with the code, says to install Node.js or
        //      wait for the channel, keeps the channel's reason, and links
        //      the help page
        assert!(
            message.starts_with("node_unavailable: This service needs Node.js"),
            "{message}"
        );
        assert!(message.contains("Install Node.js 22 or later"), "{message}");
        assert!(
            message.contains("wait until Apollia publishes it"),
            "{message}"
        );
        assert!(message.contains("HTTP 404"), "{message}");
        assert!(
            message.contains("no Node.js was found on this machine"),
            "{message}"
        );
        assert!(message.contains(NODE_HELP_URL), "{message}");
        assert!(!message.contains("could not be installed"), "{message}");
    }

    // The trial of a server launched by Node.js says which one ran it, in
    // the order its launch resolves them.
    #[test]
    fn test_the_trial_says_which_node_runs_the_server() {
        // GIVEN servers launched by `node`, `npx`, and by another command
        // WHEN the Node.js each runs on is read, the store holding one or not
        let cases = [
            node_source_of("node", true),
            node_source_of("node", false),
            node_source_of("npx", false),
            node_source_of("/usr/bin/python3", false),
            node_source_of("/opt/homebrew/bin/node", false),
        ];

        // THEN the store's when it holds one, the machine's otherwise, and
        //      nothing for a command that does not resolve to Node.js
        assert_eq!(
            cases,
            [
                Some(NodeOrigin::Store),
                Some(NodeOrigin::Machine),
                Some(NodeOrigin::Machine),
                None,
                None,
            ]
        );
    }

    // The answer of the live test keeps the shape the window reads, with the
    // source of Node.js beside it.
    #[test]
    fn test_the_live_test_answer_carries_the_node_source() {
        // GIVEN a successful test of a server that ran on the machine's Node.js
        let response: McpConnectionTestResponse = serde_json::from_value(serde_json::json!({
            "kind": "success",
            "server_info": "browser",
            "protocol_version": "2025-06-18",
            "tools": [],
            "test_duration_ms": 3
        }))
        .unwrap();
        let answer = McpLiveTestResponse {
            response,
            node_source: Some(NodeOrigin::Machine),
        };

        // WHEN it is sent to the window
        let json = serde_json::to_value(&answer).unwrap();

        // THEN it is the test as before, tagged, with `node_source` beside it
        assert_eq!(json["kind"], "success");
        assert_eq!(json["server_info"], "browser");
        assert_eq!(json["node_source"], "machine");
    }
}
