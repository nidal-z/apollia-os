//! Tauri IPC command for the connection list.
//!
//! Delegates to `GET /api/v1/connections` on the embedded runtime, so the
//! desktop and any other client read the same list.

use apollia_runtime::connections::Connection;
use apollia_runtime::embedded::RuntimeHandle;
use tauri::State;

use super::http_get_json;

/// Every connection of this machine: native accounts, MCP servers, folders.
#[tauri::command]
pub async fn list_connections(state: State<'_, RuntimeHandle>) -> Result<Vec<Connection>, String> {
    let json = http_get_json(state.api_port, "/api/v1/connections").await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse connection list: {e}"))
}
