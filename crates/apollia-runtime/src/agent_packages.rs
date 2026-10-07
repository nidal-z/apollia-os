//! The installed packages of the machine, as the library shows them.
//!
//! The listing and the detail live here so that the desktop running the
//! runtime in its own process and the daemon `apollia-os start` serve them
//! alike, under `/api/v1/agent-packages/`
//! (`crate::api::routes_agent_packages`). A package is installed from a
//! signed or unsigned `.apo` archive by [`crate::packages`], and removed by
//! [`crate::packages::uninstall_package`].

mod content;

#[cfg(test)]
mod tests;

use std::path::Path;

use apollia_tools::{InstalledPackage, PackageRepository, TrustedKeyRepository};
use serde::{Deserialize, Serialize};

pub use content::{
    AgentPackageAutomation, AgentPackageContent, AgentPackageCounts, AgentPackageItem,
    AgentPackageRequirement, AgentPackageSignature, AgentPackageTool,
};

/// Longest package or agent name accepted: it becomes a folder name.
const MAX_NAME_LEN: usize = 128;

/// Why an agent package operation did not complete.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AgentPackageError {
    /// No installed package carries this name.
    #[error("package '{0}' not found")]
    NotFound(String),
    /// A package or agent name that cannot name a folder.
    #[error("invalid name '{0}': empty, too long, or a path")]
    InvalidName(String),
    /// `agents.db` failed.
    #[error("database error: {0}")]
    Store(String),
}

/// An agent of a package.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AgentPackageAgent {
    /// Agent name, as the manifest recorded at installation gives it.
    pub name: String,
}

/// An installed package in the listing.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AgentPackageListItem {
    /// Package name.
    pub name: String,
    /// Installed version.
    pub version: String,
    /// Description from its manifest.
    pub description: String,
    /// Number of agents.
    pub agent_count: usize,
    /// Its agents.
    pub agents: Vec<AgentPackageAgent>,
    /// When it was installed.
    pub installed_at: String,
    /// Its folder.
    pub root_path: String,
    /// The folder is gone from the disk.
    pub root_missing: bool,
    /// Its publisher and the state of its signature.
    pub signature: AgentPackageSignature,
    /// Apollia ships it inside its binary: it is never uninstalled.
    pub embedded: bool,
    /// How many agents, workflows, tools and connections it holds.
    pub counts: AgentPackageCounts,
}

/// An installed package, in full.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AgentPackageDetail {
    /// Package name.
    pub name: String,
    /// Installed version.
    pub version: String,
    /// Description from its manifest.
    pub description: String,
    /// Author from its manifest.
    pub author: String,
    /// Its agents.
    pub agents: Vec<AgentPackageAgent>,
    /// When it was installed.
    pub installed_at: String,
    /// When it was last written.
    pub updated_at: String,
    /// Its folder.
    pub root_path: String,
    /// The folder is gone from the disk.
    pub root_missing: bool,
    /// The manifest as recorded.
    #[schema(value_type = Object)]
    pub manifest: serde_json::Value,
    /// Its publisher and the state of its signature.
    pub signature: AgentPackageSignature,
    /// Apollia ships it inside its binary: it is never uninstalled.
    pub embedded: bool,
    /// What it installed, by kind.
    pub content: AgentPackageContent,
    /// The automations that start one of its agents or workflows: they go
    /// with it. Filled by the route, which holds the trigger store.
    #[serde(default)]
    pub automations: Vec<AgentPackageAutomation>,
}

/// Refuses a name that cannot name one folder under the data directory.
pub(crate) fn check_name(name: &str) -> Result<(), AgentPackageError> {
    let bad = name.is_empty()
        || name.len() > MAX_NAME_LEN
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', '\0']);
    if bad {
        return Err(AgentPackageError::InvalidName(name.to_owned()));
    }
    Ok(())
}

/// The package store of `data_dir`.
pub(crate) fn open_packages(data_dir: &Path) -> Result<PackageRepository, AgentPackageError> {
    let db = apollia_core::paths::DataFile::Agents.path(data_dir);
    PackageRepository::open(&db).map_err(|e| AgentPackageError::Store(e.to_string()))
}

fn str_at<'a>(value: &'a serde_json::Value, key: &str) -> &'a str {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

fn recorded_agents(manifest: &serde_json::Value) -> Vec<AgentPackageAgent> {
    manifest
        .get("agents")
        .and_then(serde_json::Value::as_array)
        .map(|agents| {
            agents
                .iter()
                .map(|a| AgentPackageAgent {
                    name: str_at(a, "name").to_owned(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn recorded_manifest(pkg: &InstalledPackage) -> serde_json::Value {
    serde_json::from_str(&pkg.manifest_json).unwrap_or_default()
}

/// The keyring of trusted publishers of `data_dir`, when it can be read. A
/// keyring that cannot be read names no publisher: the listing still answers.
fn open_trusted_keys(data_dir: &Path) -> Option<TrustedKeyRepository> {
    let db = apollia_core::paths::DataFile::Agents.path(data_dir);
    TrustedKeyRepository::open(&db).ok()
}

fn store(error: apollia_tools::PackageRepositoryError) -> AgentPackageError {
    AgentPackageError::Store(error.to_string())
}

/// The publisher, the origin and the content of `pkg`.
fn described(
    pkg: &InstalledPackage,
    author: &str,
    packages: &PackageRepository,
    trusted_keys: Option<&TrustedKeyRepository>,
) -> Result<(AgentPackageSignature, bool, AgentPackageContent), AgentPackageError> {
    Ok((
        content::signature(pkg, author, packages, trusted_keys).map_err(store)?,
        packages.is_embedded(&pkg.name).map_err(store)?,
        content::content(pkg, packages).map_err(store)?,
    ))
}

/// The installed packages of `data_dir`.
///
/// # Errors
///
/// [`AgentPackageError::Store`] when `agents.db` cannot be read.
pub fn list(data_dir: &Path) -> Result<Vec<AgentPackageListItem>, AgentPackageError> {
    let repository = open_packages(data_dir)?;
    let trusted_keys = open_trusted_keys(data_dir);
    let packages = repository.list().map_err(store)?;
    packages
        .iter()
        .map(|pkg| {
            let manifest = recorded_manifest(pkg);
            let agents = recorded_agents(&manifest);
            let meta = manifest.get("package");
            let author = meta.map(|p| str_at(p, "author")).unwrap_or("");
            let (signature, embedded, content) =
                described(pkg, author, &repository, trusted_keys.as_ref())?;
            Ok(AgentPackageListItem {
                name: pkg.name.clone(),
                version: pkg.version.clone(),
                description: meta
                    .map(|p| str_at(p, "description"))
                    .unwrap_or("")
                    .to_owned(),
                agent_count: agents.len(),
                agents,
                installed_at: pkg.installed_at.clone(),
                root_path: pkg.root_path.to_string_lossy().to_string(),
                root_missing: !pkg.root_path.exists(),
                signature,
                embedded,
                counts: content.counts(),
            })
        })
        .collect()
}

/// The installed package `name`, in full. Its automations are left empty:
/// the caller that holds the trigger store fills them.
///
/// # Errors
///
/// [`AgentPackageError::NotFound`] when no package carries the name,
/// [`AgentPackageError::Store`] when `agents.db` cannot be read.
pub fn detail(data_dir: &Path, name: &str) -> Result<AgentPackageDetail, AgentPackageError> {
    check_name(name)?;
    let repository = open_packages(data_dir)?;
    let pkg = repository
        .get(name)
        .map_err(store)?
        .ok_or_else(|| AgentPackageError::NotFound(name.to_owned()))?;
    let manifest = recorded_manifest(&pkg);
    let meta = manifest.get("package");
    let author = meta.map(|p| str_at(p, "author")).unwrap_or("").to_owned();
    let trusted_keys = open_trusted_keys(data_dir);
    let (signature, embedded, content) =
        described(&pkg, &author, &repository, trusted_keys.as_ref())?;
    Ok(AgentPackageDetail {
        name: pkg.name.clone(),
        version: pkg.version.clone(),
        description: meta
            .map(|p| str_at(p, "description"))
            .unwrap_or("")
            .to_owned(),
        author,
        agents: recorded_agents(&manifest),
        installed_at: pkg.installed_at.clone(),
        updated_at: pkg.updated_at.clone(),
        root_path: pkg.root_path.to_string_lossy().to_string(),
        root_missing: !pkg.root_path.exists(),
        manifest,
        signature,
        embedded,
        content,
        automations: Vec::new(),
    })
}

/// The automations among `triggers`, each a trigger identifier with the agent
/// and the workflow it starts, that go with the package of `detail`: the rule
/// its uninstallation applies.
pub fn automations_of(
    detail: &AgentPackageDetail,
    triggers: impl IntoIterator<Item = (String, Option<String>, Option<String>)>,
) -> Vec<AgentPackageAutomation> {
    let agents: Vec<String> = detail.content.agents.iter().map(|a| a.id.clone()).collect();
    let workflows: Vec<String> = detail
        .content
        .workflows
        .iter()
        .map(|w| w.id.clone())
        .collect();
    triggers
        .into_iter()
        .filter(|(_, agent, workflow)| {
            crate::packages::starts_item_of(
                agent.as_deref(),
                workflow.as_deref(),
                Some(&detail.name),
                &agents,
                &workflows,
            )
        })
        .filter_map(|(id, agent, workflow)| {
            agent
                .or(workflow)
                .map(|target| AgentPackageAutomation { id, target })
        })
        .collect()
}
