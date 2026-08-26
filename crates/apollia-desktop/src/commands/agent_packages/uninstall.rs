//! Removal of an agent package: database rows, runtime registrations, then the
//! files on disk. Best-effort at every step, for the reason the command's own
//! documentation gives.

use std::path::Path;
use std::sync::{Arc, Mutex};

use apollia_core::events::RuntimeEvent;
use apollia_runtime::embedded::RuntimeHandle;
use apollia_runtime::eventbus::EventBusSender;
use apollia_tools::{AgentRepository, PackageRepository};
use tauri::State;

use super::apollia_data_dir;

/// Uninstalls a package, its agents and its triggers.
///
/// Best-effort by design: if a step fails (DB out of sync, filesystem already
/// removed, no runtime registry entry), we keep purging the rest rather than
/// aborting. A partially failing uninstall leaves the user in a non-reinstallable
/// state. The error report is aggregated at the end of the function for logging.
///
/// Cleanup order:
/// 1. Snapshot of DB metadata (tolerant lookup: best-effort).
/// 2. Delete `installed_agents` (cascade).
/// 3. Delete `installed_packages` (cascade on `package_agents`).
/// 4. In-memory runtime deregistration + emit `AgentUninstalled`.
/// 5. Filesystem removal: `install_root` + per-agent worker venv.
/// 6. If no trace is found anywhere, return an error to tell the frontend there
///    was nothing to uninstall.
#[tauri::command]
pub async fn uninstall_agent_package(
    name: String,
    pkg_repo_state: State<'_, Arc<Mutex<PackageRepository>>>,
    agent_repo_state: State<'_, Arc<Mutex<AgentRepository>>>,
    runtime: State<'_, RuntimeHandle>,
    event_bus: State<'_, EventBusSender>,
) -> Result<(), String> {
    let data_dir = apollia_data_dir();
    let venvs_root = data_dir.join("venvs");
    let install_root_default = data_dir.join("agents").join("packages").join(&name);

    // ── Step 1: best-effort snapshot of DB metadata ──────────────────────────
    let (root_path, agent_names) = {
        let pkg_repo = pkg_repo_state.lock().map_err(|_| "repo lock poisoned")?;
        match pkg_repo.get(&name) {
            Ok(Some(pkg)) => {
                let agents = pkg_repo.list_agents_for_package(&name).unwrap_or_default();
                (pkg.root_path, agents)
            }
            _ => (install_root_default.clone(), Vec::new()),
        }
    };

    // Filesystem fallback: if the DB lists no agents but the install folder
    // exists, we can still proceed (at minimum purge install_root and any venv
    // that might correspond to it).
    let mut errors: Vec<String> = Vec::new();

    // ── Step 2: delete installed_agents rows (idempotent) ────────────────────
    {
        let agent_repo = agent_repo_state.lock().map_err(|_| "repo lock poisoned")?;
        for agent_name in &agent_names {
            if let Err(e) = agent_repo.delete(agent_name) {
                errors.push(format!("agent_repo.delete({agent_name}): {e}"));
            }
        }
        // Also try by package name; covers the case where a single agent
        // carries the package name (standalone workers).
        let _ = agent_repo.delete(&name);
    }

    // ── Step 3: delete installed_packages (cascade) ──────────────────────────
    {
        let pkg_repo = pkg_repo_state
            .lock()
            .map_err(|_| "pkg repo lock poisoned")?;
        if let Err(e) = pkg_repo.delete(&name) {
            errors.push(format!("pkg_repo.delete({name}): {e}"));
        }
    }

    // ── Step 4: unregister runtime + emit events ─────────────────────────────
    let names_to_unregister: Vec<String> = if agent_names.is_empty() {
        vec![name.clone()]
    } else {
        agent_names.clone()
    };
    unregister_runtime_agents(&runtime, &event_bus, &names_to_unregister).await;

    // ── Step 5: filesystem purge ─────────────────────────────────────────────
    purge_uninstall_filesystem(
        &root_path,
        &install_root_default,
        &venvs_root,
        &name,
        &names_to_unregister,
        &mut errors,
    );

    // ── Step 6: report ───────────────────────────────────────────────────────
    if !errors.is_empty() {
        tracing::warn!(
            package = %name,
            errors = ?errors,
            "package.uninstall.degraded"
        );
    }

    // If nothing was purged at all (neither DB, nor filesystem, nor venv), warn
    // the frontend. Otherwise the operation is treated as a success even when
    // partial; the user must be able to reinstall afterwards.
    let anything_purged = !agent_names.is_empty()
        || !root_path.exists()  // a remove_dir_all succeeded
        || !install_root_default.exists();
    if !anything_purged && errors.iter().any(|e| e.contains("pkg_repo.delete")) {
        return Err(format!("Package '{name}' not found in any registry"));
    }

    Ok(())
}

/// Step 4: unregister each agent from the runtime registry/router and emit an
/// `AgentUninstalled` event. Best-effort: failures are ignored (the agent may
/// already be gone from the in-memory registry).
async fn unregister_runtime_agents(
    runtime: &RuntimeHandle,
    event_bus: &EventBusSender,
    names_to_unregister: &[String],
) {
    for agent_name in names_to_unregister {
        if let Ok(Some(agent_id)) = runtime.registry_handle.find_by_name(agent_name).await {
            let _ = runtime
                .router_handle
                .unregister_coordinator(&agent_id)
                .await;
            let _ = runtime.registry_handle.unregister(agent_id.as_str()).await;
        }
        let _ = event_bus.send(RuntimeEvent::AgentUninstalled {
            name: agent_name.clone(),
        });
    }
}

/// Step 5: purge the package install root and per-agent / package venvs from
/// disk. Non-fatal `remove_dir_all` failures are aggregated into `errors`.
// Purge helper: the several disk roots plus the name/list/errors accumulator
// exceed 5 args by design; a struct here would not clarify the call site.
// REASON: internal helper taking the flattened uninstall context its one caller just computed.
#[allow(clippy::too_many_arguments)]
fn purge_uninstall_filesystem(
    root_path: &Path,
    install_root_default: &Path,
    venvs_root: &Path,
    name: &str,
    names_to_unregister: &[String],
    errors: &mut Vec<String>,
) {
    // 5a: package install_root (resolved from the DB, falling back to the default path).
    if root_path.exists() {
        if let Err(e) = std::fs::remove_dir_all(root_path) {
            errors.push(format!("remove_dir_all({}): {e}", root_path.display()));
        }
    }
    if install_root_default != root_path && install_root_default.exists() {
        let _ = std::fs::remove_dir_all(install_root_default);
    }

    // 5b: per-agent venvs at ~/.apollia/venvs/<agent_name>/
    // Critical: without this, a partially successful pip install would leave
    // installed packages that could shadow the new install.
    for agent_name in names_to_unregister {
        let agent_venv = venvs_root.join(agent_name);
        if agent_venv.exists() {
            if let Err(e) = std::fs::remove_dir_all(&agent_venv) {
                errors.push(format!("remove_dir_all({}): {e}", agent_venv.display()));
            }
        }
    }

    // 5c: the package's own venv (standalone worker case where agent name = pkg name)
    let pkg_venv = venvs_root.join(name);
    if pkg_venv.exists() && !names_to_unregister.iter().any(|n| n == name) {
        let _ = std::fs::remove_dir_all(&pkg_venv);
    }
}
