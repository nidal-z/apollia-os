//! Installing a package Apollia ships inside its binary.
//!
//! An embedded package goes through the same preview and installation as any
//! other, with [`PackageOrigin::Embedded`]: it is unsigned and installs with no
//! notice about it, whatever the governance requires. On each start the
//! version the binary carries is compared with the installed one: the same
//! version is left alone, an older one is replaced, and a newer one is never
//! downgraded. Replacing keeps what the user set: the overrides of its agents
//! and whether each is enabled.

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
/// `data_dir`, unless the same or a newer version is installed. The
/// installation is recorded through `governance`.
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
    let incoming = ApoArchive::open(archive)?.manifest().package.clone();
    let db = apollia_core::paths::DataFile::Agents.path(data_dir);
    let packages = PackageRepository::open(&db)?;
    if let Some(installed) = packages.get(&incoming.name)? {
        if installed.version == incoming.version {
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
    commit(staged, &targets, EMBEDDED_ACTOR)
        .await
        .map(|report| EmbeddedOutcome::Installed(Box::new(report)))
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
