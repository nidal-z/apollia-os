//! What governance reads about each tool before it decides anything about it.
//!
//! For each tool: its action family and where that family comes from, the
//! connection that brings it, where that connection was found, whether it
//! sends data off this machine, and whether it runs a program on this machine
//! (an MCP server over `stdio`). A tool Apollia ships itself has no
//! connection: it is neither remote nor a local program launched for it.

use std::collections::HashSet;

use apollia_mcp::Provenance;
use apollia_tools::{ActionFamily, FamilySource};
use serde::{Deserialize, Serialize};

use crate::connections::{assemble, ConnectionKind, ConnectionSources};

/// Transport of an MCP server launched as a local process.
const STDIO_TRANSPORT: &str = "stdio";

/// The attributes of one tool, as governance reads them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolAttributes {
    /// Tool identifier as the tool registry names it.
    pub tool_id: String,
    /// Action family of the tool.
    pub family: ActionFamily,
    /// Where the family comes from.
    pub family_source: FamilySource,
    /// Connection that brings the tool; `None` for a tool Apollia ships.
    pub connection_id: Option<String>,
    /// Where that connection was found; `None` for a tool Apollia ships.
    pub provenance: Option<Provenance>,
    /// The connection sends data off this machine.
    pub remote: bool,
    /// The connection runs a program on this machine (an MCP server over
    /// `stdio`).
    pub runs_local_code: bool,
}

/// The attributes of every tool `sources` knows: the tools of each
/// connection, then every registered tool no connection claims.
///
/// A connector tool shared by two accounts of the same provider appears once
/// per account, under each connection.
#[must_use]
pub fn tool_attributes(sources: &ConnectionSources) -> Vec<ToolAttributes> {
    let stdio_servers: HashSet<String> = sources
        .mcp_configs
        .iter()
        .filter(|c| c.transport == STDIO_TRANSPORT)
        .map(|c| format!("mcp:{}", c.name))
        .collect();
    let mut out = Vec::new();
    let mut claimed = HashSet::new();
    for connection in assemble(sources) {
        let runs_local_code =
            connection.kind == ConnectionKind::Mcp && stdio_servers.contains(&connection.id);
        for tool in &connection.tools {
            claimed.insert(tool.id.clone());
            out.push(ToolAttributes {
                tool_id: tool.id.clone(),
                family: tool.family,
                family_source: tool.family_source,
                connection_id: Some(connection.id.clone()),
                provenance: Some(connection.provenance),
                remote: connection.remote,
                runs_local_code,
            });
        }
    }
    out.extend(
        sources
            .descriptors
            .iter()
            .filter(|d| !claimed.contains(&d.name))
            .map(|d| ToolAttributes {
                tool_id: d.name.clone(),
                family: d.family,
                family_source: d.family_source,
                connection_id: None,
                provenance: None,
                remote: false,
                runs_local_code: false,
            }),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_mcp::config::McpServerConfig;
    use apollia_mcp::ConnectionOrigin;
    use apollia_tools::ToolDescriptor;

    fn descriptor(name: &str, family: ActionFamily, source: FamilySource) -> ToolDescriptor {
        let mut d = crate::connectors_bridge::google_tool_descriptors()[0].clone();
        d.name = name.to_owned();
        d.family = family;
        d.family_source = source;
        d
    }

    fn config(value: serde_json::Value) -> McpServerConfig {
        serde_json::from_value(value).expect("config")
    }

    // A native tool, a tool of a remote MCP server and a tool of a stdio MCP
    // server each carry the attributes governance reads.
    #[test]
    fn test_native_remote_and_stdio_tools_carry_their_attributes() {
        // GIVEN a native tool, a remote server from the registry and a stdio
        // server from the selection, each with one registered tool
        let mut remote = config(serde_json::json!({
            "name": "notion",
            "transport": "streamable-http",
            "url": "https://mcp.notion.com/mcp",
        }));
        remote.origin = Some(ConnectionOrigin {
            provenance: Provenance::Registry,
            official: true,
        });
        let mut local = config(serde_json::json!({ "name": "files", "command": "npx" }));
        local.origin = Some(ConnectionOrigin {
            provenance: Provenance::Selection,
            official: false,
        });
        let sources = ConnectionSources {
            mcp_configs: vec![remote, local],
            descriptors: vec![
                descriptor("web_search", ActionFamily::Read, FamilySource::Builtin),
                descriptor(
                    "mcp:notion/create_page",
                    ActionFamily::WriteOrDelete,
                    FamilySource::Server,
                ),
                descriptor(
                    "mcp:files/read_file",
                    ActionFamily::Read,
                    FamilySource::Selection,
                ),
            ],
            ..ConnectionSources::default()
        };

        // WHEN the attributes are read
        let attributes = tool_attributes(&sources);

        // THEN each tool is described once, with its connection's attributes
        let find = |id: &str| {
            attributes
                .iter()
                .find(|a| a.tool_id == id)
                .cloned()
                .expect("every registered tool is described")
        };
        assert_eq!(attributes.len(), 3);
        assert_eq!(
            find("web_search"),
            ToolAttributes {
                tool_id: "web_search".into(),
                family: ActionFamily::Read,
                family_source: FamilySource::Builtin,
                connection_id: None,
                provenance: None,
                remote: false,
                runs_local_code: false,
            }
        );
        assert_eq!(
            find("mcp:notion/create_page"),
            ToolAttributes {
                tool_id: "mcp:notion/create_page".into(),
                family: ActionFamily::WriteOrDelete,
                family_source: FamilySource::Server,
                connection_id: Some("mcp:notion".into()),
                provenance: Some(Provenance::Registry),
                remote: true,
                runs_local_code: false,
            }
        );
        assert_eq!(
            find("mcp:files/read_file"),
            ToolAttributes {
                tool_id: "mcp:files/read_file".into(),
                family: ActionFamily::Read,
                family_source: FamilySource::Selection,
                connection_id: Some("mcp:files".into()),
                provenance: Some(Provenance::Selection),
                remote: false,
                runs_local_code: true,
            }
        );
    }

    // A native connector tool belongs to the account's connection, remote.
    #[test]
    fn test_connector_tool_belongs_to_its_account() {
        // GIVEN one Google account and the Google descriptors registered
        let google = crate::connectors_bridge::google_tool_descriptors();
        let first = google[0].name.clone();
        let sources = ConnectionSources {
            descriptors: google,
            native_accounts: vec![(
                apollia_auth::ConnectorProvider::Google,
                vec!["ada@example.com".into()],
            )],
            ..ConnectionSources::default()
        };

        // WHEN the attributes are read
        let attributes = tool_attributes(&sources);

        // THEN the connector tool is described under the account, remote, and
        // never a second time as a tool without a connection
        let entries: Vec<&ToolAttributes> =
            attributes.iter().filter(|a| a.tool_id == first).collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].connection_id.as_deref(),
            Some("native:google:ada@example.com")
        );
        assert!(entries[0].remote);
        assert!(!entries[0].runs_local_code);
    }
}
