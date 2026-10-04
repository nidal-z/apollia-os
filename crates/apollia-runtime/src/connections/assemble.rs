//! Assemble the connection list from what each store answers.
//!
//! Pure: every input is read by the caller, so the rules are tested without a
//! runtime, a keychain or a server process.

use std::collections::HashSet;

use apollia_auth::ConnectorProvider;
use apollia_connectors::imap::{providers::provider_for_email, ImapAccount};
use apollia_core::FolderConnection;
use apollia_core::McpHealth;
use apollia_mcp::config::McpServerConfig;
use apollia_mcp::manager::McpServerStatus;
use apollia_mcp::Provenance;
use apollia_tools::{FamilySource, ToolDescriptor};

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
    /// Names of the MCP servers an installed package brought.
    pub package_servers: HashSet<String>,
    /// Connected accounts of each native provider.
    pub native_accounts: Vec<NativeAccounts>,
    /// Mailboxes connected over IMAP.
    pub imap_accounts: Vec<ImapAccount>,
    /// Folders of this machine added as connections.
    pub folders: Vec<FolderConnection>,
}

/// Assemble every connection: native accounts first, then mailboxes, then MCP
/// servers by name, then folders.
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
    let provenance = if sources.package_servers.contains(&config.name) {
        Provenance::Package
    } else {
        config
            .origin
            .map_or(Provenance::Custom, |origin| origin.provenance)
    };
    let official = config.origin.is_some_and(|origin| origin.official);
    let status = sources.mcp_statuses.iter().find(|s| s.name == config.name);
    Connection {
        id: format!("mcp:{}", config.name),
        kind: ConnectionKind::Mcp,
        label: config.name.clone(),
        account: None,
        path: None,
        provenance,
        remote: is_remote(&config.transport, config.url.as_deref()),
        official,
        state: status.map_or(ConnectionState::Stopped, |s| state_of(&s.health)),
        tools: mcp_tools(config, &sources.descriptors, provenance, official),
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

/// The tools a server published, as registered; for a server without a live
/// session, the tools its selection entry classified.
fn mcp_tools(
    config: &McpServerConfig,
    descriptors: &[ToolDescriptor],
    provenance: Provenance,
    official: bool,
) -> Vec<ConnectionTool> {
    let prefix = format!("mcp:{}/", config.name);
    let registered: Vec<ConnectionTool> = descriptors
        .iter()
        .filter(|d| d.name.starts_with(&prefix))
        .map(|d| tool_from_descriptor(d, provenance, official))
        .collect();
    if !registered.is_empty() {
        return registered;
    }
    config
        .tool_families
        .iter()
        .map(|(name, family)| ConnectionTool {
            id: format!("{prefix}{name}"),
            family: *family,
            family_source: FamilySource::Selection,
            prechecked: is_prechecked(*family, provenance, official),
        })
        .collect()
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
