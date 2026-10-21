//! Assemble the connection list from what each store answers.
//!
//! Pure: every input is read by the caller, so the rules are tested without a
//! runtime, a keychain or a server process.

use std::collections::{HashMap, HashSet};

use apollia_auth::ConnectorProvider;
use apollia_connectors::imap::{providers::provider_for_email, ImapAccount};
use apollia_core::{ConnectionStateRecord, ConnectionStatus, FolderConnection, McpHealth};
use apollia_mcp::classify::classify_mcp_tool;
use apollia_mcp::config::McpServerConfig;
use apollia_mcp::manager::McpServerStatus;
use apollia_mcp::tool_search::ToolIndexSnapshot;
use apollia_mcp::Provenance;
use apollia_tools::{FamilySource, ToolDescriptor};

use super::state::{kept_mcp_state, shown_state};
use super::{
    folder_connections, imap_connection_id, is_prechecked, is_remote, is_remote_host, Connection,
    ConnectionKind, ConnectionState, ConnectionTool,
};

/// Accounts connected for one native provider.
pub type NativeAccounts = (ConnectorProvider, Vec<String>);

/// Everything the list is assembled from.
#[derive(Debug, Default)]
pub struct ConnectionSources {
    /// Every MCP server declared in `mcp.db`, enabled or not.
    pub mcp_configs: Vec<McpServerConfig>,
    /// Status of every MCP server with a live session.
    pub mcp_statuses: Vec<McpServerStatus>,
    /// Every descriptor of the tool registry.
    pub descriptors: Vec<ToolDescriptor>,
    /// The tools each live MCP session indexed. In deferred loading none of
    /// them reaches the registry, so this is where a server's tools are read.
    pub mcp_index: Vec<ToolIndexSnapshot>,
    /// The MCP servers an installed package brought: the name of each, and
    /// the name of its package.
    pub package_servers: HashMap<String, String>,
    /// Connected accounts of each native provider.
    pub native_accounts: Vec<NativeAccounts>,
    /// Mailboxes connected over IMAP.
    pub imap_accounts: Vec<ImapAccount>,
    /// Connections the organisation policy forbade at boot, by identifier.
    pub forbidden: HashSet<String>,
    /// Folders of this machine added as connections.
    pub folders: Vec<FolderConnection>,
    /// The state each connection was last found in, by identifier.
    pub states: HashMap<String, ConnectionStateRecord>,
    /// Connections the user put in pause, by identifier.
    pub paused: HashSet<String>,
}

/// Assemble every connection: native accounts first, then mailboxes, then MCP
/// servers by name, then folders. A connection the policy forbade at boot is
/// marked so, whatever else its state would be. A connection in pause reads
/// paused, whatever state was kept for it before, and stopped in the detail
/// its kind gives. Each other one reads in the state it was last found in,
/// dated; one never found yet needs a check.
#[must_use]
pub fn assemble(sources: &ConnectionSources) -> Vec<Connection> {
    let mut out = Vec::new();
    for (provider, accounts) in &sources.native_accounts {
        out.extend(
            accounts
                .iter()
                .map(|account| native_connection(*provider, account)),
        );
    }
    out.extend(sources.imap_accounts.iter().map(imap_connection));
    out.extend(
        sources
            .mcp_configs
            .iter()
            .map(|config| mcp_connection(config, sources)),
    );
    out.extend(folder_connections(&sources.folders));
    for connection in &mut out {
        let forbidden = sources.forbidden.contains(&connection.id);
        if forbidden {
            connection.state = ConnectionState::ForbiddenByPolicy;
        }
        let live = connection
            .id
            .strip_prefix("mcp:")
            .and_then(|name| sources.mcp_statuses.iter().find(|s| s.name == name))
            .map(|s| &s.health);
        let shown = shown_state(&connection.id, &sources.states, forbidden, live);
        connection.status = shown.status;
        connection.reason = shown.reason;
        connection.checked_at = shown.checked_at;
        if !forbidden && sources.paused.contains(&connection.id) {
            connection.state = ConnectionState::Stopped;
            connection.status = ConnectionStatus::Paused;
            connection.reason = None;
            connection.checked_at = None;
        }
    }
    out
}

fn native_connection(provider: ConnectorProvider, account: &str) -> Connection {
    // A native connector is Apollia's own code: it counts as the selection.
    let provenance = Provenance::Selection;
    let official = false;
    let descriptors =
        crate::connectors_bridge::connection_tools::account_descriptors(provider, account);
    Connection {
        id: format!("native:{}:{account}", provider.id()),
        kind: ConnectionKind::Native,
        label: provider_label(provider).to_owned(),
        account: Some(account.to_owned()),
        path: None,
        provenance,
        remote: true,
        official,
        state: ConnectionState::Connected,
        status: ConnectionStatus::NeedsRepair,
        reason: Some(super::state::not_checked()),
        checked_at: None,
        package: None,
        tools: descriptors
            .iter()
            .map(|d| tool_from_descriptor(d, provenance, official))
            .collect(),
    }
}

fn imap_connection(account: &ImapAccount) -> Connection {
    // The IMAP client is Apollia's own code: it counts as the selection.
    let provenance = Provenance::Selection;
    let official = false;
    let label = provider_for_email(&account.email)
        .map_or_else(|| account.host.clone(), |p| p.name.to_owned());
    Connection {
        id: imap_connection_id(&account.email),
        kind: ConnectionKind::Imap,
        label,
        account: Some(account.email.clone()),
        path: None,
        provenance,
        remote: is_remote_host(&account.host),
        official,
        state: ConnectionState::Connected,
        status: ConnectionStatus::NeedsRepair,
        reason: Some(super::state::not_checked()),
        checked_at: None,
        package: None,
        tools: crate::connectors_bridge::imap_tool_descriptors()
            .iter()
            .map(|d| tool_from_descriptor(d, provenance, official))
            .collect(),
    }
}

fn provider_label(provider: ConnectorProvider) -> &'static str {
    match provider {
        ConnectorProvider::Google => "Google",
        ConnectorProvider::Microsoft => "Microsoft",
    }
}

fn mcp_connection(config: &McpServerConfig, sources: &ConnectionSources) -> Connection {
    let package = sources.package_servers.get(&config.name).cloned();
    let provenance = if package.is_some() {
        Provenance::Package
    } else {
        config
            .origin
            .as_ref()
            .map_or(Provenance::Custom, |origin| origin.provenance)
    };
    let official = config.origin.as_ref().is_some_and(|origin| origin.official);
    let status = sources.mcp_statuses.iter().find(|s| s.name == config.name);
    let id = format!("mcp:{}", config.name);
    let state = status.map_or_else(
        || kept_mcp_state(sources.states.get(&id)),
        |s| state_of(&s.health),
    );
    Connection {
        id,
        kind: ConnectionKind::Mcp,
        label: config.name.clone(),
        account: None,
        path: None,
        provenance,
        remote: is_remote(&config.transport, config.url.as_deref()),
        official,
        state,
        status: ConnectionStatus::NeedsRepair,
        reason: Some(super::state::not_checked()),
        checked_at: None,
        package,
        tools: mcp_tools(config, sources, provenance, official),
    }
}

fn state_of(health: &McpHealth) -> ConnectionState {
    match health {
        McpHealth::Healthy { .. } => ConnectionState::Connected,
        McpHealth::Degraded { .. } => ConnectionState::Degraded,
        McpHealth::NeedsReauth { .. } => ConnectionState::NeedsReauth,
        McpHealth::Unavailable { .. } => ConnectionState::Unavailable,
    }
}

/// The tools a server published, as registered; otherwise the tools its
/// selection entry classified, then every other tool its live session
/// indexed, unclassified. In deferred loading nothing is registered, so a
/// server with no selection entry would otherwise offer no tool to grant.
fn mcp_tools(
    config: &McpServerConfig,
    sources: &ConnectionSources,
    provenance: Provenance,
    official: bool,
) -> Vec<ConnectionTool> {
    let prefix = format!("mcp:{}/", config.name);
    let registered: Vec<ConnectionTool> = sources
        .descriptors
        .iter()
        .filter(|d| d.name.starts_with(&prefix))
        .map(|d| tool_from_descriptor(d, provenance, official))
        .collect();
    if !registered.is_empty() {
        return registered;
    }
    let mut tools: Vec<ConnectionTool> = config
        .tool_families
        .iter()
        .map(|(name, family)| ConnectionTool {
            id: format!("{prefix}{name}"),
            family: *family,
            family_source: FamilySource::Selection,
            prechecked: is_prechecked(*family, provenance, official),
        })
        .collect();
    let mut indexed: Vec<&ToolIndexSnapshot> = sources
        .mcp_index
        .iter()
        .filter(|e| {
            e.server_name == config.name && !config.tool_families.contains_key(&e.tool_name)
        })
        .collect();
    indexed.sort_by(|a, b| a.tool_name.cmp(&b.tool_name));
    indexed.dedup_by(|a, b| a.tool_name == b.tool_name);
    tools.extend(indexed.into_iter().map(|entry| {
        let (family, family_source) = classify_mcp_tool(None, None, config.has_account());
        ConnectionTool {
            id: format!("{prefix}{}", entry.tool_name),
            family,
            family_source,
            prechecked: is_prechecked(family, provenance, official),
        }
    }));
    tools
}

fn tool_from_descriptor(
    descriptor: &ToolDescriptor,
    provenance: Provenance,
    official: bool,
) -> ConnectionTool {
    ConnectionTool {
        id: descriptor.name.clone(),
        family: descriptor.family,
        family_source: descriptor.family_source,
        prechecked: is_prechecked(descriptor.family, provenance, official),
    }
}
