//! Tauri IPC commands for the connection list and the folder connections.
//!
//! Each delegates to the runtime this application drives
//! (`/api/v1/connections`), so the desktop and any other client read and
//! change the same list, embedded or attached. A folder is admitted, stored
//! and journaled by the runtime; a refusal comes back as its reason.

use crate::attached::RuntimeLink;
use apollia_runtime::connections::{folder_id_of, Connection};
use tauri::State;

/// Every connection of this machine: native accounts, MCP servers, folders.
#[tauri::command]
pub async fn list_connections(state: State<'_, RuntimeLink>) -> Result<Vec<Connection>, String> {
    let json = state.get_json("/api/v1/connections").await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse connection list: {e}"))
}

/// Add the folder at `path`, chosen with the system picker, as a connection.
#[tauri::command]
pub async fn add_folder_connection(
    state: State<'_, RuntimeLink>,
    path: String,
) -> Result<Connection, String> {
    let json = state
        .post_json("/api/v1/connections/folders", &add_folder_body(&path))
        .await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse the folder connection: {e}"))
}

/// Remove the folder connection `connection_id` (`folder:<id>`).
#[tauri::command]
pub async fn remove_folder_connection(
    state: State<'_, RuntimeLink>,
    connection_id: String,
) -> Result<Connection, String> {
    let json = state
        .delete_json(&remove_folder_path(&connection_id)?)
        .await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse the folder connection: {e}"))
}

fn add_folder_body(path: &str) -> serde_json::Value {
    serde_json::json!({ "path": path })
}

/// The route that removes the folder `connection_id` names.
fn remove_folder_path(connection_id: &str) -> Result<String, String> {
    let id = folder_id_of(connection_id)
        .ok_or_else(|| format!("'{connection_id}' is not a folder connection"))?;
    Ok(format!(
        "/api/v1/connections/folders/{}",
        crate::attached::segment(id)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_folder_connection_is_removed_by_its_stored_identifier() {
        // GIVEN the identifier of a folder connection, and of an MCP server
        // WHEN the removal route is built for each
        let folder = remove_folder_path("folder:4f1c");
        let server = remove_folder_path("mcp:notion");
        // THEN the folder's names its stored identifier, the server is refused
        assert_eq!(folder.unwrap(), "/api/v1/connections/folders/4f1c");
        assert!(server.is_err());
    }

    #[test]
    fn test_the_added_folder_is_sent_by_its_path() {
        // GIVEN a path the picker answered
        // WHEN the body is built
        let body = add_folder_body("/Users/ada/Clients");
        // THEN it carries the path as is
        assert_eq!(body, serde_json::json!({ "path": "/Users/ada/Clients" }));
    }
}
