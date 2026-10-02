//! Tauri IPC command for the connection list.
//!
//! Delegates to `GET /api/v1/connections` on the embedded runtime, so the
//! desktop and any other client read the same list.

use crate::attached::RuntimeLink;
use apollia_runtime::connections::Connection;
use tauri::State;

/// Every connection of this machine: native accounts, MCP servers, folders.
#[tauri::command]
pub async fn list_connections(state: State<'_, RuntimeLink>) -> Result<Vec<Connection>, String> {
    let json = state.get_json("/api/v1/connections").await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse connection list: {e}"))
}
