//! Tauri IPC commands for the installed packages: list, detail, uninstall.
//!
//! The work lives in the runtime ([`apollia_runtime::agent_packages`]),
//! served under `/api/v1/agent-packages/`. Every command is a call to one of
//! those routes through [`RuntimeLink`], so it serves both arms the same way:
//! embedded, the runtime in this process; attached, the daemon. A package is
//! installed from a `.apo` archive, by the commands of `packages`.

use apollia_runtime::agent_packages::{AgentPackageDetail, AgentPackageListItem};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tauri::State;

use crate::attached::{segment, RuntimeLink};

pub mod uninstall;

/// The installed packages.
const INSTALLED: &str = "/api/v1/agent-packages/installed";

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
