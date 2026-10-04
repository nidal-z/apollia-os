//! Installing or updating a `.apo` package, after its preview.

use std::future::Future;
use std::path::{Path, PathBuf};

use apollia_mcp::config::McpServerConfig;
use apollia_mcp::package_server::config_from_package;
use apollia_tools::apo_archive::ApoArchive;
use apollia_tools::apollia_manifest::ApolliaManifest;
use apollia_tools::{
    AgentOrigin, AgentRepository, InstalledPackage, PackageItemKind, PackageRepository,
    PackageSignature, StoredAgent,
};
use serde::Serialize;

use crate::governance::{GovernanceContract, GovernanceEvent};
use crate::packages::preview::{
    build, PackageNotice, PackageOrigin, PackagePreview, PreviewSources, SignaturePreview,
    SignatureStatus,
};
use crate::packages::PackageError;

/// What the caller has confirmed, and who it is.
#[derive(Debug, Clone, Default)]
pub struct InstallOptions {
    /// The user accepted the Python dependencies the package installs.
    pub deps_confirmed: bool,
    /// The user accepted what an update adds (accesses, required connections,
    /// MCP servers).
    pub changes_confirmed: bool,
    /// Install a version older than the installed one.
    pub allow_downgrade: bool,
    /// Who installs, as the journal names it.
    pub actor: String,
    /// Where the package comes from; only the caller can say it is embedded.
    pub origin: PackageOrigin,
}

/// Adds and removes the MCP servers a package brings, through the same path
/// as `POST /api/v1/mcp/servers` and `DELETE /api/v1/mcp/servers/{name}`.
pub trait McpServerAdder: Send + Sync {
    /// Adds the server `config` describes, after its admission.
    fn add(&self, config: McpServerConfig) -> impl Future<Output = Result<(), String>> + Send;
    /// Removes the server `name`; used to undo an installation that failed.
    fn remove(&self, name: &str) -> impl Future<Output = Result<(), String>> + Send;
}

/// A package checked and extracted into a staging folder, waiting for
/// [`commit`].
#[derive(Debug)]
pub struct StagedInstall {
    preview: PackagePreview,
    manifest: ApolliaManifest,
    staging: PathBuf,
    root: PathBuf,
    previous: Option<InstalledPackage>,
    /// Where the package comes from, as the caller of [`prepare`] said.
    origin: PackageOrigin,
}

impl StagedInstall {
    /// The preview the installation was checked against.
    pub fn preview(&self) -> &PackagePreview {
        &self.preview
    }

    /// Removes the staging folder without installing anything.
    pub fn discard(self) {
        remove_dir_quietly(&self.staging);
    }
}

/// What was installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstallReport {
    /// Package identifier.
    pub name: String,
    /// Installed version.
    pub version: String,
    /// Version it replaced, for an update.
    pub previous_version: Option<String>,
    /// Publisher and signature, as recorded.
    pub signature: SignaturePreview,
    /// Agents installed.
    pub agents: Vec<String>,
    /// Workflows installed.
    pub workflows: Vec<String>,
    /// Python tools installed.
    pub tools: Vec<String>,
    /// MCP servers added.
    pub mcp_servers_added: Vec<String>,
    /// What the user should know, as the preview said.
    pub notices: Vec<PackageNotice>,
}

/// Where [`commit`] writes.
pub struct CommitTargets<'a, M: McpServerAdder> {
    /// The installed packages.
    pub packages: &'a PackageRepository,
    /// The stored agents.
    pub agents: &'a AgentRepository,
    /// Where the installation is recorded.
    pub governance: &'a dyn GovernanceContract,
    /// The MCP manager, when one runs.
    pub mcp: Option<&'a M>,
    /// Folder of the package environments (`<data dir>/venvs`).
    pub venvs: &'a Path,
}

/// Previews the package at `path`, refuses what the preview refuses or what
/// the caller has not confirmed, and extracts it into a staging folder under
/// `<data_dir>/agents/packages/`.
///
/// # Errors
///
/// [`PackageError::Refused`] for an invalid signature, a missing required
/// signature, a version older than the installed one without
/// `allow_downgrade`, or an agent identifier another package owns;
/// [`PackageError::DepsConfirmationRequired`] and
/// [`PackageError::ChangesConfirmationRequired`] for a missing confirmation;
/// [`PackageError::Archive`] or [`PackageError::Io`] when the archive cannot
/// be read or extracted.
pub fn prepare(
    path: &Path,
    options: &InstallOptions,
    sources: &PreviewSources<'_>,
    data_dir: &Path,
) -> Result<StagedInstall, PackageError> {
    let mut archive = ApoArchive::open(path)?;
    let signature = match archive.signature_state() {
        Ok(state) => Ok(state),
        Err(apollia_tools::apo_archive::ApoError::InvalidSignature { reason }) => Err(reason),
        Err(other) => return Err(other.into()),
    };
    let manifest = archive.manifest().clone();
    let preview = build(&manifest, signature, sources, options.origin)?;

    match &preview.refusal {
        Some(crate::packages::InstallRefusal::Downgrade { .. }) if options.allow_downgrade => {}
        Some(refusal) => return Err(PackageError::Refused(refusal.clone())),
        None => {}
    }
    if !preview.pip.is_empty() && !options.deps_confirmed {
        return Err(PackageError::DepsConfirmationRequired {
            packages: preview.pip.clone(),
        });
    }
    if let Some(diff) = &preview.update {
        if diff.adds_anything() && !options.changes_confirmed {
            return Err(PackageError::ChangesConfirmationRequired);
        }
    }

    let packages_dir = data_dir.join("agents").join("packages");
    std::fs::create_dir_all(&packages_dir).map_err(|source| PackageError::Io {
        path: packages_dir.clone(),
        source,
    })?;
    let staging = packages_dir.join(format!(
        ".staging-{}-{}",
        manifest.package.name,
        uuid::Uuid::new_v4().simple()
    ));
    archive.extract_to(&staging)?;
    let previous = sources.packages.get(&manifest.package.name)?;
    Ok(StagedInstall {
        root: packages_dir.join(&manifest.package.name),
        preview,
        manifest,
        staging,
        previous,
        origin: options.origin,
    })
}

/// Installs `staged`: the package environment, the MCP servers it brings,
/// its files, its definitions and content in `agents.db`, then the journal
/// entries.
///
/// On a failure before the definitions are written, the previous files of
/// the package and the servers added by this call are put back as they
/// were.
///
/// # Errors
///
/// [`PackageError::McpUnavailable`] when the package brings a server and no
/// MCP manager runs, [`PackageError::McpServer`] when one is refused,
/// [`PackageError::Environment`] when `pip` fails, [`PackageError::Io`],
/// [`PackageError::Store`], or [`PackageError::Journal`].
pub async fn commit<M: McpServerAdder>(
    staged: StagedInstall,
    targets: &CommitTargets<'_, M>,
    actor: &str,
) -> Result<InstallReport, PackageError> {
    let name = staged.manifest.package.name.clone();
    let servers_to_add: Vec<_> = staged
        .manifest
        .mcp_servers
        .iter()
        .filter(|server| {
            !staged
                .preview
                .mcp_servers
                .iter()
                .any(|p| p.name == server.name && p.already_present)
        })
        .cloned()
        .collect();
    let mcp = match (servers_to_add.is_empty(), targets.mcp) {
        (true, mcp) => mcp,
        (false, Some(mcp)) => Some(mcp),
        (false, None) => {
            staged.discard();
            return Err(PackageError::McpUnavailable);
        }
    };

    if !staged.preview.pip.is_empty() {
        if let Err(e) = build_environment(&name, &staged.preview.pip, targets.venvs).await {
            staged.discard();
            return Err(e);
        }
    }

    let backup = match swap_in(&staged) {
        Ok(backup) => backup,
        Err(e) => {
            staged.discard();
            return Err(e);
        }
    };

    let mut added: Vec<String> = Vec::new();
    if let Some(mcp) = mcp {
        for server in &servers_to_add {
            if let Err(message) = mcp.add(config_from_package(server)).await {
                for done in &added {
                    let _ = mcp.remove(done).await;
                }
                swap_back(&staged.root, backup.as_deref());
                return Err(PackageError::McpServer {
                    name: server.name.clone(),
                    message,
                });
            }
            added.push(server.name.clone());
        }
    }

    let report = write_records(&staged, targets, &added)?;
    record(&staged, targets.governance, actor)?;
    if let Some(backup) = backup {
        remove_dir_quietly(&backup);
    }
    tracing::event!(
        tracing::Level::INFO,
        package = %report.name,
        version = %report.version,
        signed = staged.preview.signature.status == SignatureStatus::Signed,
        "package.installed"
    );
    Ok(report)
}

async fn build_environment(name: &str, pip: &[String], venvs: &Path) -> Result<(), PackageError> {
    let executor = apollia_tools::tools::python_executor::PythonExecutor::new(name, venvs)
        .map_err(|e| PackageError::Environment(e.to_string()))?;
    executor
        .setup_venv(pip)
        .await
        .map_err(|e| PackageError::Environment(e.to_string()))
}

/// Moves the installed files of the package aside, if any, and the staged
/// files in their place. Returns where the previous files went.
fn swap_in(staged: &StagedInstall) -> Result<Option<PathBuf>, PackageError> {
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| PackageError::Io { path, source }
    };
    let backup = if staged.root.exists() {
        let backup = staged.root.with_file_name(format!(
            ".previous-{}-{}",
            staged.manifest.package.name,
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::rename(&staged.root, &backup).map_err(io(&staged.root))?;
        Some(backup)
    } else {
        None
    };
    if let Err(e) = std::fs::rename(&staged.staging, &staged.root) {
        swap_back(&staged.root, backup.as_deref());
        return Err(io(&staged.staging)(e));
    }
    Ok(backup)
}

/// Puts the previous files of the package back at `root`.
fn swap_back(root: &Path, backup: Option<&Path>) {
    remove_dir_quietly(root);
    if let Some(backup) = backup {
        if let Err(e) = std::fs::rename(backup, root) {
            tracing::event!(
                tracing::Level::WARN,
                root = %root.display(),
                error = %e,
                "package.install.restore_failed"
            );
        }
    }
}

fn remove_dir_quietly(path: &Path) {
    if path.exists() {
        if let Err(e) = std::fs::remove_dir_all(path) {
            tracing::event!(
                tracing::Level::WARN,
                path = %path.display(),
                error = %e,
                "package.install.cleanup_failed"
            );
        }
    }
}

fn write_records<M: McpServerAdder>(
    staged: &StagedInstall,
    targets: &CommitTargets<'_, M>,
    added: &[String],
) -> Result<InstallReport, PackageError> {
    let manifest = &staged.manifest;
    let package = &manifest.package;
    let now = chrono::Utc::now().to_rfc3339();
    let manifest_json = serde_json::to_string(manifest)
        .map_err(|e| PackageError::Store(format!("cannot serialise the manifest: {e}")))?;

    // Servers this package brought before and still declares stay its own.
    let previously_brought: Vec<String> = targets
        .packages
        .list_items(&package.name)?
        .into_iter()
        .filter(|item| item.kind == PackageItemKind::Connection)
        .map(|item| item.item_id)
        .collect();

    targets.packages.save(&InstalledPackage {
        name: package.name.clone(),
        version: package.version.clone(),
        root_path: staged.root.clone(),
        manifest_json,
        installed_at: staged
            .previous
            .as_ref()
            .map_or_else(|| now.clone(), |p| p.installed_at.clone()),
        updated_at: now.clone(),
    })?;
    let signature = &staged.preview.signature;
    targets.packages.set_signature(
        &package.name,
        &PackageSignature {
            publisher: signature.declared_publisher.clone(),
            key_fingerprint: signature.key_fingerprint.clone(),
            signed: signature.status == SignatureStatus::Signed,
        },
    )?;
    targets
        .packages
        .set_embedded(&package.name, staged.origin == PackageOrigin::Embedded)?;
    targets.packages.clear_items(&package.name)?;

    for agent in &manifest.agents {
        let definition = serde_json::to_value(agent)
            .map_err(|e| PackageError::Store(format!("cannot serialise agent: {e}")))?;
        targets.agents.save_definition(&StoredAgent {
            name: agent.id.clone(),
            origin: AgentOrigin::Package,
            package: Some(package.name.clone()),
            version: package.version.clone(),
            definition,
            enabled: true,
            installed_at: now.clone(),
            updated_at: now.clone(),
        })?;
        targets.packages.link_agent(&package.name, &agent.id)?;
    }
    for workflow in &manifest.workflows {
        targets
            .packages
            .link_item(&package.name, PackageItemKind::Workflow, &workflow.name)?;
    }
    for tool in &manifest.tools {
        targets
            .packages
            .link_item(&package.name, PackageItemKind::Tool, &tool.name)?;
    }
    for server in &manifest.mcp_servers {
        if added.contains(&server.name) || previously_brought.contains(&server.name) {
            targets
                .packages
                .link_item(&package.name, PackageItemKind::Connection, &server.name)?;
        }
    }

    Ok(InstallReport {
        name: package.name.clone(),
        version: package.version.clone(),
        previous_version: staged.previous.as_ref().map(|p| p.version.clone()),
        signature: signature.clone(),
        agents: manifest.agents.iter().map(|a| a.id.clone()).collect(),
        workflows: manifest.workflows.iter().map(|w| w.name.clone()).collect(),
        tools: manifest.tools.iter().map(|t| t.name.clone()).collect(),
        mcp_servers_added: added.to_vec(),
        notices: staged.preview.notices.clone(),
    })
}

/// Records the installation of the package, then of each of its agents.
fn record(
    staged: &StagedInstall,
    governance: &dyn GovernanceContract,
    actor: &str,
) -> Result<(), PackageError> {
    let package = &staged.manifest.package;
    let signature = &staged.preview.signature;
    let signed = signature.status == SignatureStatus::Signed;
    let event = match &staged.previous {
        Some(previous) => GovernanceEvent::PackageUpdated {
            package_id: package.name.clone(),
            from_version: previous.version.clone(),
            to_version: package.version.clone(),
            publisher: signature.declared_publisher.clone(),
            key_fingerprint: signature.key_fingerprint.clone(),
            signed,
            actor: actor.to_owned(),
        },
        None => GovernanceEvent::PackageInstalled {
            package_id: package.name.clone(),
            version: package.version.clone(),
            publisher: signature.declared_publisher.clone(),
            key_fingerprint: signature.key_fingerprint.clone(),
            signed,
            actor: actor.to_owned(),
        },
    };
    governance.record(&event)?;
    let previous_agents: Vec<String> = staged
        .previous
        .as_ref()
        .and_then(|p| serde_json::from_str::<ApolliaManifest>(&p.manifest_json).ok())
        .map(|m| m.agents.into_iter().map(|a| a.id).collect())
        .unwrap_or_default();
    for agent in &staged.manifest.agents {
        let event = match &staged.previous {
            Some(previous) if previous_agents.contains(&agent.id) => GovernanceEvent::Updated {
                agent_id: agent.id.clone(),
                from_version: previous.version.clone(),
                to_version: package.version.clone(),
                signed,
                actor: actor.to_owned(),
            },
            _ => GovernanceEvent::Installed {
                agent_id: agent.id.clone(),
                version: package.version.clone(),
                signed,
                actor: actor.to_owned(),
            },
        };
        governance.record(&event)?;
    }
    Ok(())
}
