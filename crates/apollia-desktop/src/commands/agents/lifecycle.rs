//! The command that changes an installed agent: its update, restart
//! included, a route of the runtime.
//!
//! There is no start, stop, enable or disable here: an installed agent is
//! always available, loaded by its first task and released once idle. An
//! agent arrives with a package, and the uninstall goes through the runtime,
//! with the packages (`agent_packages::uninstall`).

use tauri::State;

use super::UpdateAgentResponse;
use crate::attached::{segment, RuntimeLink};

/// The route of an installed agent on the runtime.
fn installed_path(name: &str) -> String {
    format!("/api/v1/agents/installed/{}", segment(name))
}

/// Updates an installed agent with a new Python file.
///
/// The runtime validates the new module, replaces the installed one, records
/// the new version and cycles the running instance when there is one
/// (`PUT /api/v1/agents/installed/{name}`). The answer says which of the two
/// versions now serves tasks, through its `restart_outcome`.
#[tauri::command]
pub async fn update_agent(
    name: String,
    path: String,
    link: State<'_, RuntimeLink>,
) -> Result<UpdateAgentResponse, String> {
    update_on(link.inner(), &name, &path).await
}

/// The body of [`update_agent`], on either arm of the seam.
pub(crate) async fn update_on(
    link: &RuntimeLink,
    name: &str,
    path: &str,
) -> Result<UpdateAgentResponse, String> {
    let body = serde_json::json!({ "path": path });
    let answer = link.put_json(&installed_path(name), &body).await?;
    serde_json::from_value(answer).map_err(|e| format!("invalid update answer: {e}"))
}

#[cfg(test)]
mod attached_tests;
