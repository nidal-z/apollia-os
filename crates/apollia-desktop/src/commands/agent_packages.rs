//! Tauri IPC commands for managing agent packages: list, detail, preview
//! (dry run), install, uninstall.
//!
//! The work lives in the runtime ([`apollia_runtime::agent_packages`]),
//! served under `/api/v1/agent-packages/`. Every command is a call to one of
//! those routes through [`RuntimeLink`], so it serves both arms the same way:
//! embedded, the runtime in this process; attached, the daemon, which listens
//! on this machine and reads the package folder the window picked. A refusal
//! reaches the interface as the runtime wrote it, the
//! `DEPS_CONFIRMATION_REQUIRED:<count>:<list>` the install dialog parses
//! included.

use std::time::Duration;

use apollia_runtime::agent_packages::{
    AgentPackageDetail, AgentPackageInstallReport, AgentPackageListItem, AgentPackagePreview,
    AgentPackageTriggerConfig,
};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use tauri::State;

use crate::attached::{segment, RuntimeLink};

pub mod uninstall;

/// The installed packages.
const INSTALLED: &str = "/api/v1/agent-packages/installed";

/// The dry run of a package folder.
const PREVIEW: &str = "/api/v1/agent-packages/preview";

/// How long an install may take on the daemon: it builds one virtualenv per
/// agent that declares Python dependencies and installs them.
const INSTALL_DEADLINE: Duration = Duration::from_secs(30 * 60);

/// `/api/v1/agent-packages/installed/{name}`, the name encoded.
fn installed_path(name: &str) -> String {
    format!("{INSTALLED}/{}", segment(name))
}

fn read<T: DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|e| format!("unreadable answer of the runtime: {e}"))
}

/// The installed packages.
pub(crate) async fn list(link: &RuntimeLink) -> Result<Vec<AgentPackageListItem>, String> {
    read(link.get_json(INSTALLED).await?)
}

/// The installed package `name`, in full.
pub(crate) async fn detail(link: &RuntimeLink, name: &str) -> Result<AgentPackageDetail, String> {
    read(link.get_json(&installed_path(name)).await?)
}

/// What the package folder `path` contains; nothing is written.
pub(crate) async fn preview(link: &RuntimeLink, path: &str) -> Result<AgentPackagePreview, String> {
    read(link.post_json(PREVIEW, &json!({ "path": path })).await?)
}

/// Installs the package folder `path`.
pub(crate) async fn install(
    link: &RuntimeLink,
    path: &str,
    trigger_configs: &[AgentPackageTriggerConfig],
    deps_confirmed: bool,
) -> Result<AgentPackageInstallReport, String> {
    let body = json!({
        "path": path,
        "trigger_configs": trigger_configs,
        "deps_confirmed": deps_confirmed,
    });
    read(
        link.post_json_within(INSTALLED, &body, INSTALL_DEADLINE)
            .await?,
    )
}

/// Dry run: parses `agent.toml` and validates the manifest without writing.
#[tauri::command]
pub async fn preview_agent_package(
    link: State<'_, RuntimeLink>,
    path: String,
) -> Result<AgentPackagePreview, String> {
    preview(link.inner(), &path).await
}

/// Lists all installed packages.
#[tauri::command]
pub async fn list_agent_packages(
    link: State<'_, RuntimeLink>,
) -> Result<Vec<AgentPackageListItem>, String> {
    list(link.inner()).await
}

/// Returns the full detail of a package.
#[tauri::command]
pub async fn get_agent_package_detail(
    link: State<'_, RuntimeLink>,
    name: String,
) -> Result<AgentPackageDetail, String> {
    detail(link.inner(), &name).await
}

/// Installs a package from a folder of this machine.
///
/// Python dependencies the user has not confirmed are refused with
/// `DEPS_CONFIRMATION_REQUIRED:<count>:<list>`, which the interface parses to
/// ask. Otherwise the runtime copies the folder, builds the virtualenvs,
/// imports each agent, records them and writes the triggers, everything
/// created removed on the first failure.
#[tauri::command]
pub async fn install_agent_package(
    link: State<'_, RuntimeLink>,
    path: String,
    trigger_configs: Vec<AgentPackageTriggerConfig>,
    deps_confirmed: bool,
) -> Result<AgentPackageInstallReport, String> {
    install(link.inner(), &path, &trigger_configs, deps_confirmed).await
}

/// Uninstalls the package `name` and everything it installed, through the
/// path the command line takes. The memory and the history of its agents are
/// erased only when asked. Returns what was removed and kept.
#[tauri::command]
pub async fn uninstall_agent_package(
    link: State<'_, RuntimeLink>,
    name: String,
    clear_memory: Option<bool>,
    clear_history: Option<bool>,
) -> Result<Value, String> {
    uninstall::package(link.inner(), &name, clear_memory, clear_history).await
}

#[cfg(test)]
mod tests;
