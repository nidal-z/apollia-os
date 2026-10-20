//! Installing a package Apollia ships inside its binary.
//!
//! An embedded package goes through the same preview and installation as any
//! other, with [`PackageOrigin::Embedded`]: it is unsigned and installs with no
//! notice about it, whatever the governance requires. On each start the
//! version and the content the binary carries are compared with the installed
//! ones: the same version with the same content is left alone, an older
//! version, or the same version with another content, is replaced, and a
//! newer version is never downgraded. The content is compared by its digest
//! ([`apollia_tools::apo_archive::content_digest`]), recorded at each
//! embedded installation; a package installed before the digest was recorded
//! is replaced once. Replacing keeps what the user set: the overrides of its
//! agents and whether each is enabled.

use std::path::Path;

use apollia_mcp::config::McpServerConfig;
use apollia_tools::apo_archive::ApoArchive;
use apollia_tools::{AgentRepository, PackageRepository, TrustedKeyRepository};

use crate::governance::GovernanceContract;
use crate::packages::{
    commit, prepare, CommitTargets, InstallOptions, InstallRefusal, InstallReport, McpServerAdder,
    PackageError, PackageOrigin, PreviewSources,
};

/// Who the journal names for an embedded installation.
pub const EMBEDDED_ACTOR: &str = "apollia";

/// What became of an embedded package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbeddedOutcome {
    /// The package was installed, or replaced an older version.
    Installed(Box<InstallReport>),
    /// The installed version is the one the binary carries.
    Current {
        /// That version.
        version: String,
    },
    /// The installed version is newer than the one the binary carries, and is
    /// kept.
    Newer {
        /// Installed version.
        installed: String,
        /// Version the binary carries.
        bundled: String,
    },
}

/// Installs the embedded package at `archive` into the stores under
/// `data_dir`, unless a newer version is installed, or the same version with
/// the same content. The installation is recorded through `governance`, and
/// the content digest of the archive with the package.
///
/// # Errors
///
/// [`PackageError`] when the archive cannot be read, a store cannot be opened,
/// or the installation fails. An embedded package brings no MCP server: one
/// that does is refused with [`PackageError::McpUnavailable`].
pub async fn install_embedded(
    archive: &Path,
    data_dir: &Path,
    governance: &dyn GovernanceContract,
) -> Result<EmbeddedOutcome, PackageError> {
    let mut opened = ApoArchive::open(archive)?;
    let incoming = opened.manifest().package.clone();
    let digest = opened.content_digest()?;
    drop(opened);
    let db = apollia_core::paths::DataFile::Agents.path(data_dir);
    let packages = PackageRepository::open(&db)?;
    if let Some(installed) = packages.get(&incoming.name)? {
        let same_content =
            packages.content_digest(&incoming.name)?.as_deref() == Some(digest.as_str());
        if installed.version == incoming.version && same_content {
            // An installation made before the mark existed is marked here.
            packages.set_embedded(&incoming.name, true)?;
            return Ok(EmbeddedOutcome::Current {
                version: installed.version,
            });
        }
    }
    let agents = AgentRepository::open(&db)?;
    let trusted_keys = TrustedKeyRepository::open(&db)?;
    let sources = PreviewSources {
        governance,
        packages: &packages,
        agents: &agents,
        trusted_keys: &trusted_keys,
        connections: &[],
        tools: &[],
    };
    let options = InstallOptions {
        deps_confirmed: true,
        changes_confirmed: true,
        allow_downgrade: false,
        actor: EMBEDDED_ACTOR.to_owned(),
        origin: PackageOrigin::Embedded,
    };
    let staged = match prepare(archive, &options, &sources, data_dir) {
        Ok(staged) => staged,
        Err(PackageError::Refused(InstallRefusal::Downgrade {
            installed,
            incoming,
        })) => {
            return Ok(EmbeddedOutcome::Newer {
                installed,
                bundled: incoming,
            })
        }
        Err(error) => return Err(error),
    };
    let venvs = data_dir.join("venvs");
    let targets = CommitTargets::<NoMcp> {
        packages: &packages,
        agents: &agents,
        governance,
        mcp: None,
        venvs: &venvs,
    };
    let report = commit(staged, &targets, EMBEDDED_ACTOR).await?;
    packages.set_content_digest(&report.name, &digest)?;
    Ok(EmbeddedOutcome::Installed(Box::new(report)))
}

/// Stands for the MCP manager an embedded package never needs.
struct NoMcp;

impl McpServerAdder for NoMcp {
    async fn add(&self, config: McpServerConfig) -> Result<(), String> {
        Err(format!(
            "an embedded package brings no MCP server, `{}` refused",
            config.name
        ))
    }

    async fn remove(&self, _name: &str) -> Result<(), String> {
        Ok(())
    }
}
