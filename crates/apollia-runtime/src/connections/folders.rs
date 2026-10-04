//! A folder of this machine as a connection.
//!
//! The folder is kept in `system.db` by [`apollia_core::FolderConnectionStore`];
//! this module turns a stored folder into a [`Connection`]. A folder brings
//! the native file tools, with the families they declare: reading, listing
//! and searching read; writing and editing write or delete. A folder is
//! Apollia's own code, so it counts as the selection, and it never leaves the
//! machine. Each tool is granted to an agent for this folder alone, as
//! `conn:folder:<id>/<tool>`: the folders an agent's file tools reach are the
//! ones granted to it ([`apollia_tools::folder_access`]).

use apollia_core::FolderConnection;
use apollia_mcp::Provenance;
use apollia_tools::{ActionFamily, ToolDescriptor};

use super::{is_prechecked, Connection, ConnectionKind, ConnectionState, ConnectionTool};

/// Registry names of the native file tools a folder connection brings.
pub const FOLDER_TOOLS: [&str; 6] = [
    "file_read",
    "file_list",
    "file_glob",
    "file_grep",
    "file_write",
    "file_edit",
];

/// Prefix of the identifier of a folder connection.
const FOLDER_CONNECTION_PREFIX: &str = "folder:";

/// Identifier of the connection of the stored folder `id`.
#[must_use]
pub fn folder_connection_id(id: &str) -> String {
    format!("{FOLDER_CONNECTION_PREFIX}{id}")
}

/// The stored folder identifier a connection identifier names, if it names
/// a folder.
#[must_use]
pub fn folder_id_of(connection_id: &str) -> Option<&str> {
    connection_id
        .strip_prefix(FOLDER_CONNECTION_PREFIX)
        .filter(|id| !id.is_empty())
}

/// Descriptors of the native file tools a folder connection brings.
#[must_use]
pub fn folder_tool_descriptors() -> Vec<ToolDescriptor> {
    apollia_tools::tools::native_descriptors()
        .into_iter()
        .filter(|d| FOLDER_TOOLS.contains(&d.name.as_str()))
        .collect()
}

/// Families of the tools a folder connection brings, as the admission is
/// told them.
#[must_use]
pub fn folder_families() -> Vec<ActionFamily> {
    folder_tool_descriptors().iter().map(|d| d.family).collect()
}

/// The connection of the stored `folder`.
#[must_use]
pub fn folder_connection(folder: &FolderConnection) -> Connection {
    let provenance = Provenance::Selection;
    let official = false;
    Connection {
        id: folder_connection_id(&folder.id),
        kind: ConnectionKind::Folder,
        label: folder.label.clone(),
        account: None,
        path: Some(folder.path.clone()),
        provenance,
        remote: false,
        official,
        state: ConnectionState::Connected,
        tools: folder_tool_descriptors()
            .iter()
            .map(|d| ConnectionTool {
                id: apollia_tools::folder_access::folder_access_name(&folder.id, &d.name),
                family: d.family,
                family_source: d.family_source,
                prechecked: is_prechecked(d.family, provenance, official),
            })
            .collect(),
    }
}

/// The connections of every stored folder, in the order given.
#[must_use]
pub fn folder_connections(folders: &[FolderConnection]) -> Vec<Connection> {
    folders.iter().map(folder_connection).collect()
}
