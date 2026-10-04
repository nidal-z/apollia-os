//! Uninstalling a package, or an agent installed on its own, through the
//! runtime's routes: the same path as the command line.
//!
//! The runtime removes the definitions, overrides, models, triggers,
//! environments, files and the MCP servers a package brought, takes the
//! agents out of its registry, records the uninstallation in the audit
//! journal and announces each removed agent on the bus. The memory and the
//! history of the agents are erased only when the caller asks.

use serde_json::Value;
use tauri::State;

use crate::attached::{segment, RuntimeLink};

/// The body both uninstall routes read.
fn erase(clear_memory: Option<bool>, clear_history: Option<bool>) -> Value {
    serde_json::json!({
        "clearMemory": clear_memory.unwrap_or(false),
        "clearHistory": clear_history.unwrap_or(false),
    })
}

/// Uninstalls the package `name` through
/// `POST /api/v1/packages/{name}/uninstall`. Returns what was removed and
/// kept.
pub(crate) async fn package(
    link: &RuntimeLink,
    name: &str,
    clear_memory: Option<bool>,
    clear_history: Option<bool>,
) -> Result<Value, String> {
    let path = format!("/api/v1/packages/{}/uninstall", segment(name));
    link.post_json(&path, &erase(clear_memory, clear_history))
        .await
}

/// Uninstalls the agent `name`, installed on its own, through
/// `POST /api/v1/agents/installed/{name}/uninstall`. Returns what was
/// removed and kept.
#[tauri::command]
pub async fn uninstall_agent(
    state: State<'_, RuntimeLink>,
    name: String,
    clear_memory: Option<bool>,
    clear_history: Option<bool>,
) -> Result<Value, String> {
    let path = format!("/api/v1/agents/installed/{}/uninstall", segment(&name));
    state
        .post_json(&path, &erase(clear_memory, clear_history))
        .await
}

#[cfg(test)]
mod tests {
    use super::erase;

    // GIVEN a caller that names no choice
    // WHEN the body of the uninstall route is built
    // THEN the memory and the history are kept
    #[test]
    fn test_memory_and_history_are_kept_unless_asked() {
        assert_eq!(
            erase(None, None),
            serde_json::json!({ "clearMemory": false, "clearHistory": false })
        );
        assert_eq!(
            erase(Some(true), Some(false)),
            serde_json::json!({ "clearMemory": true, "clearHistory": false })
        );
    }
}
