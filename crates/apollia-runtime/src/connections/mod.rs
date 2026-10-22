//! One read model for everything plugged into this machine: a connection.
//!
//! A connection is a native connector account (Google, Microsoft), a mailbox
//! reached over IMAP, an MCP server, or a folder of this machine. Each kind
//! keeps its own store, which this module reads without migrating: `mcp.db`
//! for the MCP servers, the account index of [`apollia_auth`] for the native
//! accounts, `imap-accounts.json` for the mailboxes, `system.db` for the
//! folders (see [`folders`]).
//!
//! Two attributes are computed here and never entered by anyone:
//!
//! - [`is_remote`]: the connection sends data off this machine because it
//!   talks to a server over the network, not over a local process or a
//!   loopback address. A local server that calls the Internet itself is not
//!   remote; what it sends shows in the families of its tools.
//! - [`is_prechecked`]: a tool is offered checked when an agent is granted the
//!   connection only when it reads, and only when the connection comes from
//!   the curated selection or from an official publisher. The hints a server
//!   declares classify its tools, they never pre-authorise one.

pub mod agents;
mod assemble;
pub mod folder_import;
pub mod folders;
mod packages;
mod schema;
pub mod state;

pub use assemble::{assemble, ConnectionSources, NativeAccounts};
pub use folders::{folder_connection, folder_connection_id, folder_connections, folder_id_of};
pub use packages::package_connections;
pub use state::{
    lift_provider_block, native_provider_connection_id, record_admin_approval_required,
    spawn_state_recorder,
};

use apollia_core::{ConnectionReason, ConnectionStatus};
use apollia_mcp::Provenance;
use apollia_tools::{ActionFamily, FamilySource};
use serde::{Deserialize, Serialize};

/// The kind of thing a connection plugs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionKind {
    /// A native connector account: Google Workspace, Microsoft 365.
    Native,
    /// A mailbox reached over IMAP with an application password.
    Imap,
    /// An MCP server.
    Mcp,
    /// A folder of this machine.
    Folder,
}

/// Whether a connection can be used right now, in the detail its kind gives.
///
/// What its user reads is [`Connection::status`], one of four states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    /// Reachable, and no operation is failing.
    Connected,
    /// Reachable, but its operations fail.
    Degraded,
    /// The account must sign in again.
    NeedsReauth,
    /// Declared, but its server cannot be reached.
    Unavailable,
    /// Declared, and not started (disabled, or not yet connected).
    Stopped,
    /// Forbidden by the organisation policy since the last start: not started,
    /// its tools neither offered nor run, never deleted.
    ForbiddenByPolicy,
}

/// One tool a connection brings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTool {
    /// Tool identifier as the tool registry names it (`mcp:notion/search`,
    /// `conn:google:ada@example.com/gmail.send`).
    pub id: String,
    /// Action family of the tool.
    #[schema(value_type = String)]
    pub family: ActionFamily,
    /// Where the family comes from.
    #[schema(value_type = String)]
    pub family_source: FamilySource,
    /// Offered checked when the connection is granted to an agent.
    pub prechecked: bool,
}

/// One connection, whatever its kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    /// Stable identifier: `mcp:<server>`, `native:<provider>:<account>`,
    /// `imap:<address>`, `folder:<id>`.
    pub id: String,
    /// Kind of connection.
    pub kind: ConnectionKind,
    /// Name shown to the user: the server name, or the provider.
    pub label: String,
    /// Account the connection acts as, when it has one.
    pub account: Option<String>,
    /// Folder of this machine the connection opens, for a folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Where the connection was found before it was added.
    #[schema(value_type = String)]
    pub provenance: Provenance,
    /// Sends data off this machine. Computed, see [`is_remote`].
    pub remote: bool,
    /// Published by the service's own publisher.
    pub official: bool,
    /// Whether it can be used right now, in the detail its kind gives.
    pub state: ConnectionState,
    /// What its user reads: working, needs repair, paused, or blocked by the
    /// organisation, as last found (see [`state`]).
    #[schema(schema_with = schema::status_schema)]
    pub status: ConnectionStatus,
    /// Why it is not working, absent when it works.
    #[schema(schema_with = schema::reason_schema)]
    pub reason: Option<ConnectionReason>,
    /// When its state was last found, RFC 3339 in UTC; absent when it never
    /// was.
    pub checked_at: Option<String>,
    /// Name of the installed package that brought it, when one did.
    pub package: Option<String>,
    /// Tools the connection brings.
    pub tools: Vec<ConnectionTool>,
}

/// Network transports: a connection over one of them leaves this machine
/// unless its address is a loopback one.
const NETWORK_TRANSPORTS: [&str; 2] = ["streamable-http", "sse"];

/// `true` when and only when `transport` is a network transport and `url` is
/// not a loopback address.
///
/// An address that does not parse is remote: nothing proves it stays here.
#[must_use]
pub fn is_remote(transport: &str, url: Option<&str>) -> bool {
    if !NETWORK_TRANSPORTS.contains(&transport) {
        return false;
    }
    !url.is_some_and(apollia_core::net::is_loopback_host_str)
}

/// `true` when and only when the tool reads and its connection comes from
/// the curated selection or is official. Every other family, unclassified
/// included, is offered unchecked.
#[must_use]
pub fn is_prechecked(family: ActionFamily, provenance: Provenance, official: bool) -> bool {
    family == ActionFamily::Read && (provenance == Provenance::Selection || official)
}

/// Identifier of the connection of the mailbox at `email`.
#[must_use]
pub fn imap_connection_id(email: &str) -> String {
    format!("imap:{email}")
}

/// `true` unless `host` is a loopback address: a mailbox server on this
/// machine keeps the mail here.
#[must_use]
pub fn is_remote_host(host: &str) -> bool {
    // An `http` address, because only a special scheme parses its host as an
    // IP address; an IPv6 literal takes its brackets.
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    !apollia_core::net::is_loopback_host_str(&format!("http://{host}/"))
}

#[cfg(test)]
mod tests;
