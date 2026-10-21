//! Which agents hold the tools of a connection, and what granting a
//! connection's tools adds to an agent's access.
//!
//! An access names a tool of a connection in one of these shapes:
//!
//! - `mcp:<server>/<tool>` for an MCP server;
//! - `conn:<provider>:<account>/<operation>` for a native account, or the
//!   bare operation (`gmail.send`) a package declares, which the caller
//!   resolves against the connected accounts before it gets here;
//! - `conn:folder:<id>/<tool>` for a folder;
//! - `imap.list_folders`, `imap.search`, `imap.read` for a mailbox. These three
//!   are common to every mailbox: a call names its mailbox in its input, so an
//!   agent that holds them reaches every connected mailbox. Each mailbox lists
//!   that agent, and says which other connections the same tools reach.
//!
//! Pure: the caller reads the agents and the connections.

use serde::{Deserialize, Serialize};

use super::{Connection, ConnectionKind};

/// An agent's access as the runtime grants it: default access written in,
/// bare connector operations resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentAccessView {
    /// Identifier of the agent.
    pub id: String,
    /// Display name of the agent.
    pub name: String,
    /// Whether the agent is active.
    pub enabled: bool,
    /// Whether the agent holds the default access, nobody having chosen one.
    pub by_default: bool,
    /// The tools granted.
    pub access: Vec<String>,
}

/// One agent that holds at least one tool of a connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionAgent {
    /// Identifier of the agent.
    pub id: String,
    /// Display name of the agent.
    pub name: String,
    /// Whether the agent is active.
    pub enabled: bool,
    /// Whether the agent holds these tools through the default access, nobody
    /// having chosen its access.
    pub by_default: bool,
    /// The tools of the connection the agent holds, in the order of its
    /// access.
    pub tools: Vec<String>,
}

/// The agents that hold the tools of one connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionAgents {
    /// Identifier of the connection.
    pub connection: String,
    /// Every agent holding at least one of its tools, by identifier.
    pub agents: Vec<ConnectionAgent>,
    /// Other connections its tools reach as well, by identifier: the other
    /// mailboxes, for a mailbox, since the mailbox tools are common to all of
    /// them. Empty for every other kind.
    pub shared_with: Vec<String>,
}

/// The prefix every tool of `connection` is named under, `None` for a mailbox,
/// whose tools carry no prefix.
#[must_use]
pub fn tool_prefix(connection: &Connection) -> Option<String> {
    match connection.kind {
        ConnectionKind::Mcp => Some(format!("{}/", connection.id)),
        ConnectionKind::Native => connection
            .id
            .strip_prefix("native:")
            .map(|rest| format!("conn:{rest}/")),
        ConnectionKind::Folder => super::folder_id_of(&connection.id).map(|id| {
            format!(
                "{}{id}/",
                apollia_tools::folder_access::FOLDER_ACCESS_PREFIX
            )
        }),
        ConnectionKind::Imap => None,
    }
}

/// Whether the access `access_id` is a tool of `connection`: one it lists, or
/// one named under its prefix (a stopped server may list none).
#[must_use]
pub fn belongs_to(access_id: &str, connection: &Connection) -> bool {
    connection.tools.iter().any(|tool| tool.id == access_id)
        || tool_prefix(connection).is_some_and(|prefix| access_id.starts_with(&prefix))
}

/// The agents among `agents` that hold at least one tool of `connection`, and
/// the connections among `all` its tools reach as well.
#[must_use]
pub fn holders(
    connection: &Connection,
    all: &[Connection],
    agents: &[AgentAccessView],
) -> ConnectionAgents {
    let mut held: Vec<ConnectionAgent> = agents
        .iter()
        .filter_map(|agent| {
            let tools: Vec<String> = agent
                .access
                .iter()
                .filter(|access| belongs_to(access, connection))
                .cloned()
                .collect();
            (!tools.is_empty()).then(|| ConnectionAgent {
                id: agent.id.clone(),
                name: agent.name.clone(),
                enabled: agent.enabled,
                by_default: agent.by_default,
                tools,
            })
        })
        .collect();
    held.sort_by(|a, b| a.id.cmp(&b.id));
    let shared_with = all
        .iter()
        .filter(|other| other.id != connection.id)
        .filter(|other| {
            other
                .tools
                .iter()
                .any(|tool| connection.tools.iter().any(|own| own.id == tool.id))
        })
        .map(|other| other.id.clone())
        .collect();
    ConnectionAgents {
        connection: connection.id.clone(),
        agents: held,
        shared_with,
    }
}

/// The tools among `tools` that are not tools of `connection`.
#[must_use]
pub fn foreign_tools<'a>(connection: &Connection, tools: &'a [String]) -> Vec<&'a str> {
    tools
        .iter()
        .filter(|tool| !belongs_to(tool, connection))
        .map(String::as_str)
        .collect()
}

/// `access` with every tool of `tools` it lacks appended, in order; `None`
/// when it already holds them all.
#[must_use]
pub fn with_granted(access: &[String], tools: &[String]) -> Option<Vec<String>> {
    let mut out = access.to_vec();
    for tool in tools {
        if !out.contains(tool) {
            out.push(tool.clone());
        }
    }
    (out.len() > access.len()).then_some(out)
}

#[cfg(test)]
mod tests;
