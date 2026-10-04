//! Uninstalling a package, or an agent installed on its own.
//!
//! One path for every caller. Always removed: the definitions of the agents
//! and their overrides, the model assigned to each, the triggers that start
//! them or a workflow of the package, the package environment and the agent environments, the files, and
//! the MCP servers the package brought (read from what it installed). The
//! connections the package required stay: other packages and the user may
//! use them. At the user's choice, the memory of the agents and their history
//! (tasks, batches, workflow runs) are erased or kept. Each uninstallation is
//! recorded in the audit journal.
//!
//! Apollia is part of the product and is never uninstalled, and neither is a
//! package Apollia ships inside its binary; an agent that belongs to an
//! installed package goes with its package.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use apollia_core::paths::DataFile;
use apollia_core::LlmBackendRepository;
use apollia_tools::apollia_manifest::ApolliaManifest;
use apollia_tools::{
    is_built_in_agent, AgentRepository, PackageItemKind, PackageRepository, TaskRepository,
};
use apollia_triggers::TriggerDefinitionRepository;
use serde::Serialize;

use crate::governance::{GovernanceContract, GovernanceError, GovernanceEvent};
use crate::packages::McpServerAdder;

/// What the user chose to erase rather than keep.
#[derive(Debug, Clone, Copy, Default)]
pub struct UninstallOptions {
    /// Erase the memory of the agents.
    pub clear_memory: bool,
    /// Erase the history of the agents and workflows: tasks, batches,
    /// workflow runs.
    pub clear_history: bool,
}

/// Where an uninstallation removes things.
pub struct UninstallTargets<'a, M: McpServerAdder> {
    /// The installed packages.
    pub packages: &'a PackageRepository,
    /// The stored agents and their overrides.
    pub agents: &'a AgentRepository,
    /// Where the uninstallation is recorded.
    pub governance: &'a dyn GovernanceContract,
    /// The MCP manager, when one runs.
    pub mcp: Option<&'a M>,
    /// The trigger definitions, when the store is open.
    pub triggers: Option<&'a Mutex<TriggerDefinitionRepository>>,
    /// The model assigned to each agent, when the store is open.
    pub models: Option<&'a Mutex<LlmBackendRepository>>,
    /// The data directory: files, environments, memory and history.
    pub data_dir: &'a Path,
}

/// What an uninstallation removed, and what it kept.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UninstallReport {
    /// The package, or the agent uninstalled on its own.
    pub name: String,
    /// The version removed.
    pub version: String,
    /// Agents removed.
    pub agents: Vec<String>,
    /// Workflows removed.
    pub workflows: Vec<String>,
    /// Python tools removed.
    pub tools: Vec<String>,
    /// Triggers removed.
    pub triggers_removed: Vec<String>,
    /// MCP servers the package brought, removed.
    pub mcp_servers_removed: Vec<String>,
    /// Connections the package required, kept.
    pub connections_kept: Vec<String>,
    /// The memory of the agents was erased.
    pub memory_cleared: bool,
    /// The history was erased.
    pub history_cleared: bool,
    /// Tasks erased with the history.
    pub tasks_removed: u64,
    /// Batches erased with the history.
    pub batches_removed: u64,
    /// Workflow runs erased with the history.
    pub workflow_runs_removed: u64,
    /// What could not be removed, and why. The uninstallation goes on past
    /// such a step, so that the package can be installed again.
    pub leftovers: Vec<String>,
}

/// Errors of an uninstallation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UninstallError {
    /// No package, or no agent, of this name is installed.
    #[error("`{0}` is not installed")]
    NotFound(String),
    /// Apollia, or a package it ships inside its binary, is part of the
    /// product.
    #[error("`{0}` is part of Apollia and cannot be uninstalled")]
    BuiltIn(String),
    /// The agent goes with the package that installed it.
    #[error("`{agent}` belongs to the package `{package}`: uninstall the package")]
    OwnedByPackage {
        /// The agent.
        agent: String,
        /// Its package.
        package: String,
    },
    /// A store of `agents.db` or `hitl.db` failed.
    #[error("store: {0}")]
    Store(String),
    /// The uninstallation could not be recorded in the audit journal.
    #[error("cannot record the uninstallation: {0}")]
    Journal(#[from] GovernanceError),
}

impl From<apollia_tools::PackageRepositoryError> for UninstallError {
    fn from(error: apollia_tools::PackageRepositoryError) -> Self {
        UninstallError::Store(error.to_string())
    }
}

impl From<apollia_tools::AgentRepositoryError> for UninstallError {
    fn from(error: apollia_tools::AgentRepositoryError) -> Self {
        UninstallError::Store(error.to_string())
    }
}

/// What one uninstallation covers.
struct Scope {
    report: UninstallReport,
    /// The package, when a package is uninstalled.
    package: Option<String>,
    /// Folders to remove: the files and the environments.
    folders: Vec<PathBuf>,
}

/// Uninstalls the package `name`, everything it installed, and, as
/// `options` asks, the memory and history of its agents and workflows.
///
/// # Errors
///
/// [`UninstallError::NotFound`] when no such package is installed,
/// [`UninstallError::BuiltIn`] when it carries Apollia or is shipped inside
/// the binary, [`UninstallError::Store`] or [`UninstallError::Journal`].
pub async fn uninstall_package<M: McpServerAdder>(
    name: &str,
    options: UninstallOptions,
    targets: &UninstallTargets<'_, M>,
    actor: &str,
) -> Result<UninstallReport, UninstallError> {
    let package = targets
        .packages
        .get(name)?
        .ok_or_else(|| UninstallError::NotFound(name.to_owned()))?;
    if targets.packages.is_embedded(name)? {
        return Err(UninstallError::BuiltIn(name.to_owned()));
    }
    let mut report = UninstallReport {
        name: name.to_owned(),
        version: package.version.clone(),
        ..UninstallReport::default()
    };
    let mut servers = Vec::new();
    for item in targets.packages.list_items(name)? {
        match item.kind {
            PackageItemKind::Agent => report.agents.push(item.item_id),
            PackageItemKind::Workflow => report.workflows.push(item.item_id),
            PackageItemKind::Tool => report.tools.push(item.item_id),
            PackageItemKind::Connection => servers.push(item.item_id),
        }
    }
    if let Some(agent) = report.agents.iter().find(|a| is_built_in_agent(a)) {
        return Err(UninstallError::BuiltIn(agent.clone()));
    }
    report.connections_kept = serde_json::from_str::<ApolliaManifest>(&package.manifest_json)
        .map(|m| m.requires.into_iter().map(|r| r.kind).collect())
        .unwrap_or_default();

    let venvs = targets.data_dir.join("venvs");
    let mut folders = vec![package.root_path.clone(), venvs.join(name)];
    folders.extend(report.agents.iter().map(|agent| venvs.join(agent)));
    let mut scope = Scope {
        report,
        package: Some(name.to_owned()),
        folders,
    };
    remove_servers(&mut scope.report, servers, targets.mcp).await;
    remove(&mut scope, options, targets).await?;

    targets
        .governance
        .record(&GovernanceEvent::PackageUninstalled {
            package_id: name.to_owned(),
            version: package.version,
            memory_cleared: options.clear_memory,
            history_cleared: options.clear_history,
            actor: actor.to_owned(),
        })?;
    record_agents(&scope.report, options, targets.governance, actor)?;
    Ok(finish(scope))
}

/// Uninstalls the agent `name`, installed on its own, and, as `options`
/// asks, its memory and history.
///
/// # Errors
///
/// [`UninstallError::BuiltIn`] for Apollia, [`UninstallError::NotFound`]
/// when no such agent is stored, [`UninstallError::OwnedByPackage`] when an
/// installed package owns it, [`UninstallError::Store`] or
/// [`UninstallError::Journal`].
pub async fn uninstall_agent<M: McpServerAdder>(
    name: &str,
    options: UninstallOptions,
    targets: &UninstallTargets<'_, M>,
    actor: &str,
) -> Result<UninstallReport, UninstallError> {
    if is_built_in_agent(name) {
        return Err(UninstallError::BuiltIn(name.to_owned()));
    }
    let stored = targets
        .agents
        .stored(name)?
        .ok_or_else(|| UninstallError::NotFound(name.to_owned()))?;
    if let Some(package) = stored.package.as_deref() {
        let owned = targets
            .packages
            .list_agents_for_package(package)?
            .iter()
            .any(|agent| agent == name);
        if owned && targets.packages.get(package)?.is_some() {
            return Err(UninstallError::OwnedByPackage {
                agent: name.to_owned(),
                package: package.to_owned(),
            });
        }
    }

    let mut folders = vec![targets.data_dir.join("venvs").join(name)];
    if let Some(code) = targets.agents.get(name)? {
        if let Some(folder) = code.install_path.parent() {
            folders.push(folder.to_path_buf());
        }
    }
    let mut scope = Scope {
        report: UninstallReport {
            name: name.to_owned(),
            version: stored.version,
            agents: vec![name.to_owned()],
            ..UninstallReport::default()
        },
        package: None,
        folders,
    };
    remove(&mut scope, options, targets).await?;
    record_agents(&scope.report, options, targets.governance, actor)?;
    Ok(finish(scope))
}

/// The steps common to a package and an agent, after what is specific to
/// each has been gathered into `scope`.
async fn remove<M: McpServerAdder>(
    scope: &mut Scope,
    options: UninstallOptions,
    targets: &UninstallTargets<'_, M>,
) -> Result<(), UninstallError> {
    let report = &mut scope.report;
    if let Some(triggers) = targets.triggers {
        remove_triggers(report, scope.package.as_deref(), triggers);
    }
    if let Some(models) = targets.models {
        let models = models.lock().unwrap_or_else(PoisonError::into_inner);
        for agent in &report.agents {
            if let Err(e) = models.clear_agent_model(agent) {
                report.leftovers.push(format!("model of `{agent}`: {e}"));
            }
        }
    }
    for agent in &report.agents {
        targets.agents.delete(agent)?;
    }
    if let Some(package) = &scope.package {
        targets.packages.delete(package)?;
    }
    if options.clear_memory {
        clear_memory(report, targets.data_dir);
        report.memory_cleared = true;
    }
    if options.clear_history {
        clear_history(report, scope.package.as_deref(), targets.data_dir).await?;
        report.history_cleared = true;
    }
    for folder in &scope.folders {
        remove_folder(report, folder, targets.data_dir);
    }
    Ok(())
}

/// Whether the trigger that starts `agent` or `workflow` goes with a package
/// named `package` whose agents are `agents` and whose workflows are
/// `workflows`: a workflow target is named `<package>/<workflow>`. The
/// uninstallation removes these triggers, and the detail of an installed
/// package lists them beforehand, so both read this one rule.
pub(crate) fn starts_item_of(
    agent: Option<&str>,
    workflow: Option<&str>,
    package: Option<&str>,
    agents: &[String],
    workflows: &[String],
) -> bool {
    let by_agent = agent.is_some_and(|agent| agents.iter().any(|a| a == agent));
    let by_workflow = match (workflow, package) {
        (Some(target), Some(package)) => target
            .strip_prefix(package)
            .and_then(|rest| rest.strip_prefix('/'))
            .is_some_and(|name| workflows.iter().any(|w| w == name)),
        _ => false,
    };
    by_agent || by_workflow
}

/// Removes the triggers that start one of the agents of `report`, or one of
/// the workflows of `package`, named `<package>/<workflow>`.
fn remove_triggers(
    report: &mut UninstallReport,
    package: Option<&str>,
    triggers: &Mutex<TriggerDefinitionRepository>,
) {
    let triggers = triggers.lock().unwrap_or_else(PoisonError::into_inner);
    let rows = match triggers.list() {
        Ok(rows) => rows,
        Err(e) => {
            report.leftovers.push(format!("triggers: {e}"));
            return;
        }
    };
    for row in rows {
        let targeted = starts_item_of(
            row.agent.as_deref(),
            row.workflow.as_deref(),
            package,
            &report.agents,
            &report.workflows,
        );
        if !targeted {
            continue;
        }
        match triggers.delete(&row.id) {
            Ok(()) => report.triggers_removed.push(row.id),
            Err(e) => report.leftovers.push(format!("trigger `{}`: {e}", row.id)),
        }
    }
}

/// Removes the MCP servers the package brought.
async fn remove_servers<M: McpServerAdder>(
    report: &mut UninstallReport,
    servers: Vec<String>,
    mcp: Option<&M>,
) {
    if servers.is_empty() {
        return;
    }
    let Some(mcp) = mcp else {
        report.leftovers.extend(
            servers
                .iter()
                .map(|s| format!("MCP server `{s}`: MCP is not running")),
        );
        return;
    };
    for server in servers {
        match mcp.remove(&server).await {
            Ok(()) => report.mcp_servers_removed.push(server),
            Err(e) => report.leftovers.push(format!("MCP server `{server}`: {e}")),
        }
    }
}

/// Erases the memory of each agent: the namespace of its name, in its file.
fn clear_memory(report: &mut UninstallReport, data_dir: &Path) {
    let memory = data_dir.join("memory");
    for agent in &report.agents {
        let path = memory.join(format!("{agent}.db"));
        if !path.exists() {
            continue;
        }
        let cleared = apollia_memory::store::MemoryStore::open(&path).and_then(|store| {
            store.clear_episodic(agent)?;
            store.clear_semantic(agent)?;
            store.clear_procedural(agent)
        });
        if let Err(e) = cleared {
            report.leftovers.push(format!("memory of `{agent}`: {e}"));
        }
    }
}

/// Erases the tasks and batches of the agents and the runs of the
/// workflows of `package`.
async fn clear_history(
    report: &mut UninstallReport,
    package: Option<&str>,
    data_dir: &Path,
) -> Result<(), UninstallError> {
    let path = DataFile::Hitl.path(data_dir);
    if !path.exists() {
        return Ok(());
    }
    let tasks = TaskRepository::open(&path)
        .await
        .map_err(|e| UninstallError::Store(e.to_string()))?;
    let workflows = match package {
        Some(package) => report
            .workflows
            .iter()
            .map(|w| format!("{package}/{w}"))
            .collect(),
        None => Vec::new(),
    };
    let removed = tasks
        .remove_history(report.agents.clone(), workflows)
        .await
        .map_err(|e| UninstallError::Store(e.to_string()))?;
    report.tasks_removed = removed.tasks;
    report.batches_removed = removed.batches;
    report.workflow_runs_removed = removed.workflow_runs;
    Ok(())
}

/// Removes `folder` when it lies in the data directory; a folder elsewhere is
/// not Apollia's to remove and is reported.
fn remove_folder(report: &mut UninstallReport, folder: &Path, data_dir: &Path) {
    if !folder.exists() {
        return;
    }
    if !folder.starts_with(data_dir) || folder == data_dir {
        report.leftovers.push(format!(
            "{}: outside the data directory, left in place",
            folder.display()
        ));
        return;
    }
    if let Err(e) = std::fs::remove_dir_all(folder) {
        report.leftovers.push(format!("{}: {e}", folder.display()));
    }
}

/// Records the uninstallation of each agent of `report`.
fn record_agents(
    report: &UninstallReport,
    options: UninstallOptions,
    governance: &dyn GovernanceContract,
    actor: &str,
) -> Result<(), UninstallError> {
    for agent in &report.agents {
        governance.record(&GovernanceEvent::Uninstalled {
            agent_id: agent.clone(),
            version: report.version.clone(),
            memory_cleared: options.clear_memory,
            history_cleared: options.clear_history,
            actor: actor.to_owned(),
        })?;
    }
    Ok(())
}

fn finish(scope: Scope) -> UninstallReport {
    let report = scope.report;
    tracing::event!(
        tracing::Level::INFO,
        name = %report.name,
        agents = report.agents.len(),
        triggers = report.triggers_removed.len(),
        servers = report.mcp_servers_removed.len(),
        memory_cleared = report.memory_cleared,
        history_cleared = report.history_cleared,
        leftovers = report.leftovers.len(),
        "package.uninstalled"
    );
    report
}
