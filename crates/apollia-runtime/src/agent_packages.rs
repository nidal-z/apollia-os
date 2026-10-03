//! Agent packages installed from a folder: an `agent.toml` and the `.py` it
//! declares, copied under `<data_dir>/agents/packages/<name>/`.
//!
//! The listing, the detail, the preview, the installation and the removal
//! live here so that the desktop running the runtime in its own process and
//! the daemon `apollia-os start` serve them alike, under
//! `/api/v1/agent-packages/` (`crate::api::routes_agent_packages`). The
//! signed `.apo` archives are [`crate::packages`]'.
//!
//! The installation copies the folder, builds one virtualenv per agent that
//! declares Python dependencies, imports each agent through the
//! [`AgentLoader`](crate::api::routes_agents::AgentLoader) with that
//! virtualenv on `sys.path`, records the agents and the package, and writes
//! the package's triggers. The removal undoes it, best effort.

mod install;
mod uninstall;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use apollia_tools::agent_package::{validate_manifest, PackageManifest};
use apollia_tools::{InstalledPackage, PackageRepository};
use serde::{Deserialize, Serialize};

pub use install::{install, InstallRequest, Installed};
pub use uninstall::uninstall;

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
    /// The `agent.toml` of the folder cannot be read or parsed.
    #[error("{0}")]
    Manifest(String),
    /// The package installs Python dependencies the caller has not confirmed.
    /// The text is the one the interface parses: `DEPS_CONFIRMATION_REQUIRED`,
    /// their count, then their list.
    #[error("DEPS_CONFIRMATION_REQUIRED:{}:{}", .packages.len(), .packages.join(","))]
    DepsConfirmationRequired {
        /// The requirements, as the package declares them.
        packages: Vec<String>,
    },
    /// The copy, a virtualenv, or the import of an agent failed; everything
    /// the installation created has been removed.
    #[error("{0}")]
    Install(String),
    /// `agents.db` failed.
    #[error("database error: {0}")]
    Store(String),
}

/// An agent of a package.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AgentPackageAgent {
    /// Agent name.
    pub name: String,
    /// `director`, `worker` or `assistant`.
    pub role: String,
    /// Entry point, relative to the package folder.
    pub entry: String,
}

/// A trigger a package declares, as the preview shows it.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AgentPackageTrigger {
    /// Trigger identifier.
    pub id: String,
    /// `cron`, `interval`, `oneshot`, `file_watch` or `webhook`.
    pub source_type: String,
    /// The agent it starts.
    pub agent: String,
    /// Cron expression (cron triggers).
    pub schedule: Option<String>,
    /// Interval (interval triggers).
    pub every: Option<String>,
    /// Watched path (file_watch triggers).
    pub path: Option<String>,
    /// A webhook without its secret: the user gives one at install.
    pub needs_config: bool,
    /// Whether it starts enabled.
    pub enabled: bool,
}

/// What the user gives a trigger at install, a webhook secret for now.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AgentPackageTriggerConfig {
    /// Trigger identifier.
    pub id: String,
    /// HMAC secret of a webhook.
    pub secret: Option<String>,
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
}

/// What a package folder contains, read without writing anything.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AgentPackagePreview {
    /// Package name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Description.
    pub description: String,
    /// Author.
    pub author: String,
    /// Its agents; empty when the manifest is invalid.
    pub agents: Vec<AgentPackageAgent>,
    /// Its triggers; empty when the manifest is invalid.
    pub triggers: Vec<AgentPackageTrigger>,
    /// Number of triggers.
    pub trigger_count: usize,
    /// The Python dependencies it installs, every agent together.
    pub pip_packages: Vec<String>,
    /// The manifest holds its invariants.
    pub valid: bool,
    /// Why it does not, when it does not.
    pub error: Option<String>,
}

/// What an installation did.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AgentPackageInstallReport {
    /// Package name.
    pub name: String,
    /// Installed version.
    pub version: String,
    /// Agents recorded.
    pub agent_count: usize,
    /// Triggers written.
    pub trigger_count: usize,
    /// Triggers refused, one line each.
    pub trigger_errors: Vec<String>,
}

/// The `[[triggers]]` of a manifest, read leniently: no semantic validation.
#[derive(Deserialize)]
struct RawTriggers {
    #[serde(default)]
    triggers: Vec<RawTrigger>,
}

#[derive(Deserialize)]
struct RawTrigger {
    id: String,
    #[serde(default)]
    agent: String,
    #[serde(default = "enabled_by_default")]
    enabled: bool,
    #[serde(default)]
    source: RawSource,
}

#[derive(Deserialize, Default)]
struct RawSource {
    #[serde(rename = "type", default)]
    kind: String,
    schedule: Option<String>,
    every: Option<String>,
    path: Option<String>,
    secret: Option<String>,
}

fn enabled_by_default() -> bool {
    true
}

fn trigger_previews(toml_str: &str) -> Vec<AgentPackageTrigger> {
    let Ok(raw) = toml::from_str::<RawTriggers>(toml_str) else {
        return Vec::new();
    };
    raw.triggers
        .into_iter()
        .map(|t| AgentPackageTrigger {
            needs_config: t.source.kind == "webhook"
                && t.source.secret.as_deref().unwrap_or("").is_empty(),
            id: t.id,
            source_type: if t.source.kind.is_empty() {
                "cron".to_owned()
            } else {
                t.source.kind
            },
            agent: t.agent,
            schedule: t.source.schedule,
            every: t.source.every,
            path: t.source.path,
            enabled: t.enabled,
        })
        .collect()
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

/// Where the installation copies the package `name`.
pub(crate) fn install_root(data_dir: &Path, name: &str) -> PathBuf {
    data_dir.join("agents").join("packages").join(name)
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
                    role: str_at(a, "role").to_owned(),
                    entry: str_at(a, "entry").to_owned(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn recorded_manifest(pkg: &InstalledPackage) -> serde_json::Value {
    serde_json::from_str(&pkg.manifest_json).unwrap_or_default()
}

/// The installed packages of `data_dir`.
///
/// # Errors
///
/// [`AgentPackageError::Store`] when `agents.db` cannot be read.
pub fn list(data_dir: &Path) -> Result<Vec<AgentPackageListItem>, AgentPackageError> {
    let packages = open_packages(data_dir)?
        .list()
        .map_err(|e| AgentPackageError::Store(e.to_string()))?;
    Ok(packages
        .iter()
        .map(|pkg| {
            let manifest = recorded_manifest(pkg);
            let agents = recorded_agents(&manifest);
            let description = manifest.get("package").map(|p| str_at(p, "description"));
            AgentPackageListItem {
                name: pkg.name.clone(),
                version: pkg.version.clone(),
                description: description.unwrap_or("").to_owned(),
                agent_count: agents.len(),
                agents,
                installed_at: pkg.installed_at.clone(),
                root_path: pkg.root_path.to_string_lossy().to_string(),
                root_missing: !pkg.root_path.exists(),
            }
        })
        .collect())
}

/// The installed package `name`, in full.
///
/// # Errors
///
/// [`AgentPackageError::NotFound`] when no package carries the name,
/// [`AgentPackageError::Store`] when `agents.db` cannot be read.
pub fn detail(data_dir: &Path, name: &str) -> Result<AgentPackageDetail, AgentPackageError> {
    check_name(name)?;
    let pkg = open_packages(data_dir)?
        .get(name)
        .map_err(|e| AgentPackageError::Store(e.to_string()))?
        .ok_or_else(|| AgentPackageError::NotFound(name.to_owned()))?;
    let manifest = recorded_manifest(&pkg);
    let meta = manifest.get("package");
    Ok(AgentPackageDetail {
        name: pkg.name.clone(),
        version: pkg.version.clone(),
        description: meta
            .map(|p| str_at(p, "description"))
            .unwrap_or("")
            .to_owned(),
        author: meta.map(|p| str_at(p, "author")).unwrap_or("").to_owned(),
        agents: recorded_agents(&manifest),
        installed_at: pkg.installed_at.clone(),
        updated_at: pkg.updated_at.clone(),
        root_path: pkg.root_path.to_string_lossy().to_string(),
        root_missing: !pkg.root_path.exists(),
        manifest,
    })
}

/// What the package folder `root` contains. Nothing is written.
///
/// A manifest that parses but breaks an invariant answers a preview marked
/// invalid, with the reason.
///
/// # Errors
///
/// [`AgentPackageError::Manifest`] when `agent.toml` cannot be read or parsed.
pub fn preview(root: &Path) -> Result<AgentPackagePreview, AgentPackageError> {
    let toml_str = std::fs::read_to_string(root.join("agent.toml"))
        .map_err(|e| AgentPackageError::Manifest(format!("cannot read agent.toml: {e}")))?;
    let manifest: PackageManifest = toml::from_str(&toml_str)
        .map_err(|e| AgentPackageError::Manifest(format!("invalid TOML: {e}")))?;

    if let Err(e) = validate_manifest(&manifest) {
        return Ok(AgentPackagePreview {
            name: manifest.package.name,
            version: manifest.package.version,
            description: manifest.package.description,
            author: manifest.package.author,
            agents: Vec::new(),
            triggers: Vec::new(),
            trigger_count: 0,
            pip_packages: Vec::new(),
            valid: false,
            error: Some(e.to_string()),
        });
    }

    let triggers = trigger_previews(&toml_str);
    let agents = manifest
        .agents
        .iter()
        .map(|a| AgentPackageAgent {
            name: a.name.clone(),
            role: a.role.clone(),
            entry: a.entry.clone(),
        })
        .collect();
    Ok(AgentPackagePreview {
        pip_packages: manifest.all_pip_packages(),
        name: manifest.package.name,
        version: manifest.package.version,
        description: manifest.package.description,
        author: manifest.package.author,
        agents,
        trigger_count: triggers.len(),
        triggers,
        valid: true,
        error: None,
    })
}
