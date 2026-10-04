//! The commands that change an agent's state: install and update. The
//! update, restart included, is a route of the runtime.
//!
//! There is no start, stop, enable or disable here: an installed agent is
//! always available, loaded by its first task and released once idle. The
//! install makes it available at once. The uninstall goes through the
//! runtime, with the packages (`agent_packages::uninstall`).

use std::sync::Arc;

use apollia_core::EventBusSender;
use apollia_runtime::api::routes_agents::installed::place_agent;
use apollia_runtime::api::routes_agents::AgentLoader;
use tauri::{Manager, State};

use super::{
    agent_repository, apollia_data_dir, http_post_json, InstallAgentResponse, UpdateAgentResponse,
};
use crate::attached::{segment, RuntimeLink};

/// Registers the agent at `path` with the runtime, through
/// `POST /api/v1/agents`: loading Python requires `AgentLoader` and
/// `BackendFactory`, which are not on `RuntimeHandle`. Returns its `AgentId`.
async fn start_agent_inner(api_port: u16, path: &str) -> Result<String, String> {
    let body = serde_json::json!({ "agent_path": path });
    agent_id_of(&http_post_json(api_port, "/api/v1/agents", &body).await?)
}

/// Registers a freshly installed agent with the embedded runtime, so that a
/// task reaches it without anyone starting it. A failure is reported and
/// leaves the install in place: the agent becomes available at the next
/// launch.
pub(crate) async fn make_available(api_port: u16, name: &str, install_path: &std::path::Path) {
    let path = install_path.to_string_lossy();
    if let Err(e) = start_agent_inner(api_port, &path).await {
        tracing::warn!(agent = %name, cause = %e, "agent.install.register_failed");
    }
}

/// The `agent_id` of a start answer.
fn agent_id_of(resp: &serde_json::Value) -> Result<String, String> {
    resp.get("agent_id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| "missing agent_id in response".to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Commandes de persistance
// ─────────────────────────────────────────────────────────────────────────────

/// Installs an agent permanently from a Python file.
///
/// Validates the Python module via `AgentLoader`, copies the file to
/// `~/.apollia/agents/<name>/agent.py`, persists the entry in `agents.db`, and
/// makes the agent available in the runtime at once: nobody starts it.
/// Emits a `RuntimeEvent::AgentInstalled` on the EventBus. Attached to a
/// daemon, the daemon validates, copies, records and registers it.
#[tauri::command]
pub async fn install_agent(
    path: String,
    link: State<'_, RuntimeLink>,
    app: tauri::AppHandle,
) -> Result<InstallAgentResponse, String> {
    let runtime = match link.inner() {
        RuntimeLink::Attached(remote) => {
            let body = serde_json::json!({ "path": path });
            let installed: InstallAgentResponse =
                remote.post("/api/v1/agents/installed", &body).await?;
            let body = serde_json::json!({ "agent_path": installed.install_path });
            if let Err(e) = remote
                .post::<serde_json::Value>("/api/v1/agents", &body)
                .await
            {
                tracing::warn!(agent = %installed.name, cause = %e, "agent.install.register_failed");
            }
            return Ok(installed);
        }
        RuntimeLink::Embedded(runtime) => runtime,
    };
    let loader = app
        .try_state::<Arc<dyn AgentLoader>>()
        .ok_or_else(|| "agent loader not available".to_string())?;
    let repo = agent_repository(&app)?;
    let agent =
        place_agent(loader.as_ref(), &apollia_data_dir(), &path).map_err(|e| e.to_string())?;

    let agent_for_db = agent.clone();
    let repo_clone = Arc::clone(&repo);
    tokio::task::spawn_blocking(move || {
        let repo = repo_clone
            .lock()
            .map_err(|e| format!("mutex poisoned: {e}"))?;
        repo.save(&agent_for_db)
            .map_err(|e| format!("failed to save to database: {e}"))
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))??;

    make_available(runtime.api_port, &agent.name, &agent.install_path).await;

    if let Some(event_bus) = app.try_state::<EventBusSender>() {
        let _ = event_bus.send(apollia_core::events::RuntimeEvent::AgentInstalled {
            name: agent.name.clone(),
            version: agent.version.clone(),
        });
    }

    Ok(InstallAgentResponse {
        name: agent.name,
        version: agent.version,
        install_path: agent.install_path.to_string_lossy().to_string(),
    })
}

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
