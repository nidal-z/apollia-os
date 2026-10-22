//! What removing a connection takes away, before anything is taken.
//!
//! A removal answers the list of what it erased, and the same list without
//! erasing anything when it is only asked for. The list is made of
//! [`RemovedItem`]s: the configuration of the connection, the files installed
//! for it, its token, its secrets, the tools each agent was granted from it,
//! and the state kept for it.
//!
//! The tools of an agent are revoked from every list of tools the user wrote
//! for it: `access`, and any other override named after it (the tools given
//! on top of the default access, when the store keeps them apart). An agent
//! whose access nobody chose holds the default access, read against the
//! machine at each run: nothing is written for it, and a connection added
//! later is offered to it again as any new connection is. A package agent
//! that declares a tool of the connection is given a list without it, so the
//! same connection added again under the same name does not give it back.
//!
//! Pure: the caller reads the agents and the connection.

use apollia_tools::OverrideField;
use serde::{Deserialize, Serialize};

use super::agents::belongs_to;
use super::{Connection, ConnectionKind};

/// One thing a removal erases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemovedItem {
    /// What it is.
    pub kind: RemovedKind,
    /// Its name: the server, the folder of installed files, the secret key,
    /// the tool taken away, the source, the path of a OneDrive choice.
    pub name: String,
    /// The agent the tool is taken from, for an agent access.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

/// The kinds of things a removal erases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RemovedKind {
    /// The configuration of the connection, and its session when it runs.
    Configuration,
    /// The files installed for the connection (a server's packages).
    InstalledFiles,
    /// The token of an account or of a server signed in to.
    Token,
    /// A secret its configuration names.
    Secret,
    /// A tool of the connection granted to an agent.
    AgentAccess,
    /// A source made on a folder, with its index.
    Source,
    /// The state last found for the connection.
    State,
    /// A OneDrive folder or file chosen for a Microsoft account to read.
    #[serde(rename = "onedrive_choice")]
    OneDriveChoice,
}

impl RemovedItem {
    /// An item with no agent.
    #[must_use]
    pub fn new(kind: RemovedKind, name: impl Into<String>) -> Self {
        Self {
            kind,
            name: name.into(),
            agent: None,
        }
    }
}

/// The lists of tools the user wrote for one agent, and what it holds
/// otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLists {
    /// Identifier of the agent.
    pub id: String,
    /// Whether it holds the default access, nobody having chosen one.
    pub by_default: bool,
    /// Every override of a list of tools, by field.
    pub overrides: Vec<(OverrideField, Vec<String>)>,
    /// The access its definition declares, read when no `access` override
    /// replaces it.
    pub declared: Vec<String>,
}

/// One list of tools rewritten without the tools of the connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revocation {
    /// Identifier of the agent.
    pub agent: String,
    /// The field written.
    pub field: OverrideField,
    /// The list it keeps.
    pub kept: Vec<String>,
    /// The tools taken away, in the order of the list.
    pub revoked: Vec<String>,
}

/// Whether `field` holds a list of tools: `access`, and any field named
/// after it, such as the tools given on top of the default access.
#[must_use]
pub fn lists_tools(field: OverrideField) -> bool {
    let name = field.as_str();
    name == "access" || name.starts_with("access_")
}

/// Whether removing `connection` takes `tool` away. A mailbox tool reaches
/// every mailbox: it is taken away only with the last of them, when
/// `shared` says no other mailbox is left.
fn revoked_by(tool: &str, connection: &Connection, shared: bool) -> bool {
    if connection.kind == ConnectionKind::Imap && shared {
        return false;
    }
    belongs_to(tool, connection)
}

fn split(
    list: &[String],
    connection: &Connection,
    shared: bool,
) -> Option<(Vec<String>, Vec<String>)> {
    let (revoked, kept): (Vec<String>, Vec<String>) = list
        .iter()
        .cloned()
        .partition(|tool| revoked_by(tool, connection, shared));
    (!revoked.is_empty()).then_some((kept, revoked))
}

/// Every list of `agents` that holds a tool of `connection`, rewritten
/// without it. `shared` says other connections reach the same tools (other
/// mailboxes, for a mailbox).
#[must_use]
pub fn revocations(
    connection: &Connection,
    shared: bool,
    agents: &[AgentLists],
) -> Vec<Revocation> {
    let mut out = Vec::new();
    for agent in agents {
        for (field, list) in agent.overrides.iter().filter(|(f, _)| lists_tools(*f)) {
            if let Some((kept, revoked)) = split(list, connection, shared) {
                out.push(Revocation {
                    agent: agent.id.clone(),
                    field: *field,
                    kept,
                    revoked,
                });
            }
        }
        let access_overridden = agent
            .overrides
            .iter()
            .any(|(field, _)| *field == OverrideField::Access);
        if agent.by_default || access_overridden {
            continue;
        }
        if let Some((kept, revoked)) = split(&agent.declared, connection, shared) {
            out.push(Revocation {
                agent: agent.id.clone(),
                field: OverrideField::Access,
                kept,
                revoked,
            });
        }
    }
    out
}

/// The agent accesses `revocations` take away, as items.
#[must_use]
pub fn access_items(revocations: &[Revocation]) -> Vec<RemovedItem> {
    let mut out: Vec<RemovedItem> = Vec::new();
    for revocation in revocations {
        for tool in &revocation.revoked {
            let item = RemovedItem {
                kind: RemovedKind::AgentAccess,
                name: tool.clone(),
                agent: Some(revocation.agent.clone()),
            };
            if !out.contains(&item) {
                out.push(item);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
