//! Tauri IPC commands for agent management.
//!
//! Each command delegates to the embedded runtime handles for runtime agents
//! (list/start/stop) and to the [`AgentRepository`] + `AgentLoader`
//! for persistence operations (install/uninstall/enable/disable/update).
//! The registry, the install, the start and the stop also serve a desktop
//! attached to a daemon, through the routes of `/api/v1/agents`.

use std::path::PathBuf;
use std::sync::Arc;

use apollia_runtime::api::routes_agents::installed::merge_agent_views;
use apollia_tools::{AgentRepository, InstalledAgent};
use serde::Serialize;
use tauri::{Manager, State};

use super::http_post_json;
use crate::attached::RuntimeLink;

/// The state-changing commands live in `lifecycle`, the agent-to-agent mailbox
/// view in `messages`.
pub mod lifecycle;
pub mod messages;

// ─────────────────────────────────────────────────────────────────────────────
// Response types
// ─────────────────────────────────────────────────────────────────────────────

/// Response to an agent install. The daemon answers the same body.
pub use apollia_runtime::api::routes_agents::installed::InstallAgentResponse;

/// One agent of the machine, installed agents and running ones merged. The
/// daemon answers the same view, built by the same function.
pub use apollia_runtime::api::routes_agents::installed::InstalledAgentView as AgentListItem;

/// What happened to the live runtime instance while an agent's module was
/// replaced.
///
/// The embedded interpreter keeps the imported module in memory, so writing a
/// new `.py` over the installed one changes nothing for an agent that is
/// already running. The replacement therefore stops and restarts it, and this
/// value is what lets the window say which of the two versions is now serving
/// tasks instead of announcing a success it cannot vouch for.
pub mod restart_outcome {
    /// No live runtime entry: the new module loads at the next start.
    pub const NOT_RUNNING: &str = "not_running";
    /// Stopped and started again on the new module.
    pub const RESTARTED: &str = "restarted";
    /// The stop was refused: the previous module is still running.
    pub const STOP_FAILED: &str = "stop_failed";
    /// Stopped, but it did not come back up: nothing is running.
    pub const START_FAILED: &str = "start_failed";
}

/// Response to an agent update.
///
/// Distinct from [`InstallAgentResponse`] because an update has an outcome an
/// install cannot have: a runtime instance that had to be cycled.
#[derive(Debug, Serialize)]
pub struct UpdateAgentResponse {
    /// Unique name of the updated agent.
    pub name: String,
    /// Semver version read from the new module's manifest.
    pub version: String,
    /// Install path on disk.
    pub install_path: String,
    /// One of the [`restart_outcome`] constants.
    pub restart_outcome: String,
    /// Raw cause when the stop or the start failed, `None` otherwise.
    pub restart_error: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Resolves the `~/.apollia/` directory.
fn apollia_data_dir() -> PathBuf {
    let home = apollia_core::paths::home_dir_or_temp()
        .display()
        .to_string();
    apollia_core::paths::data_dir_under(home)
}

/// Current timestamp in RFC 3339.
fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// The repository of `agents.db`, managed only when the runtime runs in this
/// process.
fn agent_repository(
    app: &tauri::AppHandle,
) -> Result<State<'_, Arc<std::sync::Mutex<AgentRepository>>>, String> {
    app.try_state()
        .ok_or_else(|| "agent repository not available".to_string())
}

/// Every row of `agents.db`, read off the async threads.
async fn installed_agents(app: &tauri::AppHandle) -> Result<Vec<InstalledAgent>, String> {
    let repo = Arc::clone(agent_repository(app)?.inner());
    tokio::task::spawn_blocking(move || {
        let repo = repo.lock().map_err(|e| format!("mutex poisoned: {e}"))?;
        repo.list().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

// ─────────────────────────────────────────────────────────────────────────────
// Existing commands
// ─────────────────────────────────────────────────────────────────────────────

/// Lists all agents, merging installed agents and runtime agents.
///
/// Installed agents (persisted in `agents.db`) are enriched with their runtime
/// state when registered. Runtime-only agents (registered through
/// `POST /api/v1/agents`) also appear in the list. Attached to a daemon, the
/// daemon builds the same view.
#[tauri::command]
pub async fn list_agents(
    link: State<'_, RuntimeLink>,
    app: tauri::AppHandle,
) -> Result<Vec<AgentListItem>, String> {
    let runtime = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => {
            return Ok(remote.get("/api/v1/agents/installed").await?);
        }
    };
    let installed = installed_agents(&app).await?;
    let runtime_entries = runtime
        .registry_handle
        .list_agents()
        .await
        .map_err(|e| e.to_string())?;
    Ok(merge_agent_views(&installed, &runtime_entries))
}

/// Lightweight snapshot of the live status of all agents.
///
/// Projects the runtime state onto the 4 statuses exposed to the QuickPicker
/// (`online` / `busy` / `offline` / `error`), read from the same view as
/// [`list_agents`] so both modes answer it.
///
/// Mapping:
/// - an installed agent switched off from the command line → `"offline"`
/// - `degraded` → `"error"` (last run failed)
/// - `initializing` → `"busy"`
/// - anything else → `"online"`: an agent is loaded by its first task, so
///   one stopped or not loaded yet still answers
///
/// The stopped and stopping states never reach the window.
#[derive(Debug, Serialize)]
pub struct AgentStatusSnapshot {
    /// Unique agent name.
    pub name: String,
    /// Status normalized for the UI: `online` | `busy` | `offline` | `error`.
    pub status: String,
}

#[tauri::command]
pub async fn agent_status_snapshot(
    link: State<'_, RuntimeLink>,
    app: tauri::AppHandle,
) -> Result<Vec<AgentStatusSnapshot>, String> {
    Ok(list_agents(link, app)
        .await?
        .into_iter()
        .map(|item| AgentStatusSnapshot {
            status: ui_status(&item).to_string(),
            name: item.name,
        })
        .collect())
}

fn ui_status(item: &AgentListItem) -> &'static str {
    if item.installed_at.is_some() && !item.enabled {
        return "offline";
    }
    match item.runtime_status.as_deref() {
        Some("degraded") => "error",
        Some("initializing") => "busy",
        _ => "online",
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn item(enabled: bool, installed: bool, runtime_status: Option<&str>) -> AgentListItem {
        AgentListItem {
            id: None,
            name: "mailer".to_string(),
            version: "1.0.0".to_string(),
            enabled,
            runtime_status: runtime_status.map(str::to_string),
            installed_at: installed.then(|| "2026-10-02T10:00:00Z".to_string()),
            description: None,
            tags: vec![],
            tools_required: vec![],
            tools_optional: vec![],
            execution_mode: None,
            install_path: None,
            supports_a2a: false,
            skills: vec![],
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
        }
    }

    #[test]
    fn test_the_picker_never_reads_a_stopped_agent_as_offline() {
        // GIVEN an enabled installed agent in every runtime state the view
        // carries, including stopped, stopping and not loaded yet, and one
        // switched off from the command line
        let states = [
            Some("active"),
            Some("stopping"),
            Some("stopped"),
            None,
            Some("degraded"),
            Some("initializing"),
        ];

        // WHEN each is projected for the QuickPicker
        let projected: Vec<&str> = states
            .iter()
            .map(|state| ui_status(&item(true, true, *state)))
            .collect();
        let switched_off = ui_status(&item(false, true, None));

        // THEN an agent loaded on demand is online whether loaded or not, a
        // degraded one errors, a starting one is busy, and only the switched
        // off one is offline
        assert_eq!(
            projected,
            vec!["online", "online", "online", "online", "error", "busy"]
        );
        assert_eq!(switched_off, "offline");
    }

    #[test]
    fn test_install_agent_response_serialization() {
        // GIVEN an InstallAgentResponse
        let resp = InstallAgentResponse {
            name: "mon-agent".to_string(),
            version: "1.0.0".to_string(),
            install_path: "/home/user/.apollia/agents/mon-agent/agent.py".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&resp).expect("serialize");

        // THEN all fields are present
        assert_eq!(json["name"], "mon-agent");
        assert_eq!(json["version"], "1.0.0");
        assert!(json["install_path"]
            .as_str()
            .is_some_and(|p| p.contains("mon-agent")));
    }

    // AgentListItem includes installed fields
    #[test]
    fn test_agent_list_item_includes_installed_fields() {
        // GIVEN an AgentListItem with all fields populated
        let item = AgentListItem {
            id: Some("uuid-123".to_string()),
            name: "hello-agent".to_string(),
            version: "0.2.0".to_string(),
            enabled: true,
            runtime_status: Some("active".to_string()),
            installed_at: Some("2026-03-17T10:00:00Z".to_string()),
            description: Some("A test agent".to_string()),
            tags: vec!["test".to_string(), "demo".to_string()],
            tools_required: vec!["bash".to_string()],
            tools_optional: vec!["file_io".to_string()],
            execution_mode: Some("auto".to_string()),
            install_path: Some("/home/user/.apollia/agents/hello-agent/agent.py".to_string()),
            supports_a2a: true,
            skills: vec![],
            agent_type: Some("assistant".to_string()),
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&item).expect("serialize");

        // THEN all fields are present with correct types
        assert_eq!(json["id"], "uuid-123");
        assert_eq!(json["name"], "hello-agent");
        assert_eq!(json["version"], "0.2.0");
        assert_eq!(json["enabled"], true);
        assert_eq!(json["runtime_status"], "active");
        assert_eq!(json["installed_at"], "2026-03-17T10:00:00Z");
        assert_eq!(json["description"], "A test agent");
        assert_eq!(json["tags"], serde_json::json!(["test", "demo"]));
        assert_eq!(json["tools_required"], serde_json::json!(["bash"]));
        assert_eq!(json["execution_mode"], "auto");
    }

    // AgentListItem with no runtime (installed but not loaded)
    #[test]
    fn test_agent_list_item_without_runtime() {
        // GIVEN an installed agent not currently loaded in runtime
        let item = AgentListItem {
            id: None,
            name: "disabled-agent".to_string(),
            version: "1.0.0".to_string(),
            enabled: false,
            runtime_status: None,
            installed_at: Some("2026-03-17T09:00:00Z".to_string()),
            description: None,
            tags: vec![],
            tools_required: vec![],
            tools_optional: vec![],
            execution_mode: Some("direct".to_string()),
            install_path: Some("/home/user/.apollia/agents/disabled-agent/agent.py".to_string()),
            supports_a2a: false,
            skills: vec![],
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&item).expect("serialize");

        // THEN optional fields are null
        assert!(json["id"].is_null());
        assert!(json["runtime_status"].is_null());
        assert_eq!(json["enabled"], false);
        assert!(json["description"].is_null());
        assert_eq!(json["tags"], serde_json::json!([]));
    }

    // Runtime-only agent (not installed)
    #[test]
    fn test_agent_list_item_runtime_only() {
        // GIVEN a runtime-only agent (not persisted)
        let item = AgentListItem {
            id: Some("uuid-456".to_string()),
            name: "ephemeral-agent".to_string(),
            version: "0.1.0".to_string(),
            enabled: true,
            runtime_status: Some("active".to_string()),
            installed_at: None,
            description: Some("Ephemeral agent".to_string()),
            tags: vec![],
            tools_required: vec![],
            tools_optional: vec![],
            execution_mode: Some("auto".to_string()),
            install_path: None,
            supports_a2a: false,
            skills: vec![],
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&item).expect("serialize");

        // THEN installed_at is null
        assert!(json["installed_at"].is_null());
        assert_eq!(json["runtime_status"], "active");
        assert!(json["install_path"].is_null());
    }

    #[test]
    fn test_update_agent_response_reports_a_completed_restart() {
        // GIVEN an update that cycled the running agent
        let resp = UpdateAgentResponse {
            name: "mon-agent".to_string(),
            version: "2.0.0".to_string(),
            install_path: "/home/user/.apollia/agents/mon-agent/agent.py".to_string(),
            restart_outcome: restart_outcome::RESTARTED.to_string(),
            restart_error: None,
        };

        // WHEN serialized for the window
        let json = serde_json::to_value(&resp).expect("serialize");

        // THEN the outcome is explicit and no cause is carried
        assert_eq!(json["restart_outcome"], "restarted");
        assert!(json["restart_error"].is_null());
        assert_eq!(json["version"], "2.0.0");
    }

    #[test]
    fn test_update_agent_response_keeps_the_restart_cause() {
        // GIVEN an update whose restart failed after the stop
        let resp = UpdateAgentResponse {
            name: "mon-agent".to_string(),
            version: "2.0.0".to_string(),
            install_path: "/home/user/.apollia/agents/mon-agent/agent.py".to_string(),
            restart_outcome: restart_outcome::START_FAILED.to_string(),
            restart_error: Some("tool resolution failed: missing bash".to_string()),
        };

        // WHEN serialized for the window
        let json = serde_json::to_value(&resp).expect("serialize");

        // THEN the raw cause survives the boundary, it is what the operator acts on
        assert_eq!(json["restart_outcome"], "start_failed");
        assert_eq!(
            json["restart_error"],
            "tool resolution failed: missing bash"
        );
    }

    #[test]
    fn test_restart_outcome_constants_are_distinct() {
        // GIVEN the four outcomes the window switches on
        let all = [
            restart_outcome::NOT_RUNNING,
            restart_outcome::RESTARTED,
            restart_outcome::STOP_FAILED,
            restart_outcome::START_FAILED,
        ];

        // WHEN collected into a set
        let unique: std::collections::HashSet<&str> = all.iter().copied().collect();

        // THEN none collapses onto another
        assert_eq!(unique.len(), all.len());
    }
}
