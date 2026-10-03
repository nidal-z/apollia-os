//! Removal of a package: its rows, its agents in the runtime, then its files.
//!
//! Best effort at every step: a removal that stops half way leaves a package
//! that cannot be installed again, so a failing step is logged and the next
//! one runs.

use std::path::Path;

use apollia_core::events::RuntimeEvent;
use apollia_tools::AgentRepository;

use super::{check_name, install_root, open_packages, AgentPackageError};
use crate::coordinator::ExecutionBackend;
use crate::eventbus::EventBusSender;
use crate::registry::AgentRegistryHandle;
use crate::router::TaskRouterHandle;

/// Uninstalls the package `name`, its agents and their virtualenvs.
///
/// In order: read which agents the package owns; delete their rows, then the
/// package's; take each agent out of the registry and the router and
/// announce `AgentUninstalled`; remove the package folder and the
/// virtualenvs.
///
/// # Errors
///
/// [`AgentPackageError::InvalidName`] for a name that is a path,
/// [`AgentPackageError::Store`] when `agents.db` cannot be opened,
/// [`AgentPackageError::NotFound`] when nothing of the package was found
/// anywhere.
pub async fn uninstall<B: ExecutionBackend + Clone>(
    data_dir: &Path,
    registry: &AgentRegistryHandle,
    router: &TaskRouterHandle<B>,
    events: &EventBusSender,
    name: &str,
) -> Result<(), AgentPackageError> {
    check_name(name)?;
    let venvs = data_dir.join("venvs");
    let default_root = install_root(data_dir, name);
    let packages = open_packages(data_dir)?;

    let (root, agent_names) = match packages.get(name) {
        Ok(Some(pkg)) => {
            let agents = packages.list_agents_for_package(name).unwrap_or_default();
            (pkg.root_path, agents)
        }
        _ => (default_root.clone(), Vec::new()),
    };

    let mut errors: Vec<String> = Vec::new();
    let db = apollia_core::paths::DataFile::Agents.path(data_dir);
    match AgentRepository::open(&db) {
        Ok(agents) => {
            for agent in &agent_names {
                if let Err(e) = agents.delete(agent) {
                    errors.push(format!("agent_repo.delete({agent}): {e}"));
                }
            }
            // A standalone worker is one agent carrying the package's name.
            let _ = agents.delete(name);
        }
        Err(e) => errors.push(format!("agent_repo.open: {e}")),
    }
    let package_deleted = packages.delete(name);
    if let Err(e) = &package_deleted {
        errors.push(format!("pkg_repo.delete({name}): {e}"));
    }

    let unregistered = if agent_names.is_empty() {
        vec![name.to_owned()]
    } else {
        agent_names.clone()
    };
    for agent in &unregistered {
        if let Ok(Some(id)) = registry.find_by_name(agent).await {
            let _ = router.forget_coordinator(&id).await;
            let _ = registry.unregister(id.as_str()).await;
        }
        let _ = events.send(RuntimeEvent::AgentUninstalled {
            name: agent.clone(),
        });
    }

    errors.extend(purge_files(
        &root,
        &default_root,
        &venvs,
        name,
        &unregistered,
    ));

    if !errors.is_empty() {
        tracing::event!(
            tracing::Level::WARN,
            package = %name,
            errors = ?errors,
            "package.uninstall.degraded"
        );
    }
    let anything_purged = !agent_names.is_empty() || !root.exists() || !default_root.exists();
    if !anything_purged && package_deleted.is_err() {
        return Err(AgentPackageError::NotFound(name.to_owned()));
    }
    Ok(())
}

/// Removes the package folder, recorded or default, and the virtualenv of
/// each agent and of the package itself. Returns why a removal failed.
fn purge_files(
    root: &Path,
    default_root: &Path,
    venvs: &Path,
    name: &str,
    agents: &[String],
) -> Vec<String> {
    let mut errors = Vec::new();
    if root.exists() {
        if let Err(e) = std::fs::remove_dir_all(root) {
            errors.push(format!("remove_dir_all({}): {e}", root.display()));
        }
    }
    if default_root != root && default_root.exists() {
        let _ = std::fs::remove_dir_all(default_root);
    }
    // A virtualenv left behind by a half-finished install would shadow the
    // packages of the next one.
    for agent in agents {
        let venv = venvs.join(agent);
        if venv.exists() {
            if let Err(e) = std::fs::remove_dir_all(&venv) {
                errors.push(format!("remove_dir_all({}): {e}", venv.display()));
            }
        }
    }
    let own = venvs.join(name);
    if own.exists() && !agents.iter().any(|a| a == name) {
        let _ = std::fs::remove_dir_all(&own);
    }
    errors
}
