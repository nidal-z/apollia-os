//! The packages the desktop binary ships: the guide of Apollia.
//!
//! The guide is a declarative agent: a mission, examples and defaults, no
//! code. Its manifest and its knowledge base are embedded in the binary by
//! `include_str!`. On start, unless the installed guide is already the version
//! the binary carries, the desktop assembles them into a `.apo` archive under
//! `<data dir>/agents/bundled/`, the knowledge appended to the mission, and
//! installs it through the same path as any package
//! ([`apollia_runtime::packages::install_embedded`]): unsigned and recognised
//! as embedded, recorded in the audit journal, never downgrading a newer
//! version, keeping the user's overrides and leaving a disabled guide
//! disabled.
//!
//! A guide installed by an earlier version ran Python code. Before the agents
//! that run code are loaded, [`retire_code_guide`] turns that row into the
//! declarative agent of the guide package, so the code is never loaded again
//! and the package installs over it as over its own agent.

use std::path::{Path, PathBuf};

use apollia_runtime::audit_journal::AuditJournalHandle;
use apollia_runtime::governance::NoPolicyGovernance;
use apollia_runtime::packages::{install_embedded, EmbeddedOutcome, PackageError};
use apollia_tools::apo_archive::{build_from_dir, ApoError, BuildOptions};
use apollia_tools::apollia_manifest::{self, ApolliaManifest, ApolliaManifestError};
use apollia_tools::{AgentRepository, PackageRepository};

/// Identifier of the guide package, and of its one agent.
pub const GUIDE: &str = "apollia-guide";

/// The manifest of the guide, before its knowledge is appended.
const GUIDE_MANIFEST: &str = include_str!("../../../agents/system/apollia-guide/apollia.toml");

/// The knowledge base of the guide, appended to its mission in this order.
const GUIDE_KNOWLEDGE: [&str; 2] = [
    include_str!("../../../agents/system/apollia-guide/knowledge/capabilities.md"),
    include_str!("../../../agents/system/apollia-guide/knowledge/tutorials.md"),
];

/// Heading under which the knowledge base follows the mission.
const KNOWLEDGE_HEADING: &str = "Knowledge base:";

/// Errors while assembling or installing a package the binary ships.
#[derive(Debug, thiserror::Error)]
pub enum BundledPackageError {
    /// The embedded manifest is invalid.
    #[error(transparent)]
    Manifest(#[from] ApolliaManifestError),
    /// The embedded manifest declares no agent to carry the knowledge.
    #[error("the embedded guide manifest declares no agent")]
    NoAgent,
    /// The archive could not be built.
    #[error(transparent)]
    Archive(#[from] ApoError),
    /// The package could not be installed.
    #[error(transparent)]
    Install(#[from] PackageError),
    /// A store could not be read.
    #[error("package store: {0}")]
    Store(String),
    /// A file operation failed.
    #[error("{path}: {source}")]
    Io {
        /// The file or folder involved.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
}

/// Turns a guide installed as Python code into the declarative agent of the
/// guide package, keeping its overrides and whether it is enabled. Called at
/// boot, before the agents that run code are loaded.
pub fn retire_code_guide(repo: &AgentRepository) {
    match repo.adopt_into_package(GUIDE, GUIDE) {
        Ok(true) => tracing::event!(
            tracing::Level::INFO,
            agent = %GUIDE,
            "bundled.guide.code_retired"
        ),
        Ok(false) => {}
        Err(error) => tracing::event!(
            tracing::Level::WARN,
            agent = %GUIDE,
            error = %error,
            "bundled.guide.code_retire_failed"
        ),
    }
}

/// The manifest of the guide as the binary ships it: the embedded manifest,
/// with the knowledge base appended to the mission of its agent.
///
/// # Errors
///
/// [`BundledPackageError::Manifest`] or [`BundledPackageError::NoAgent`] for an
/// embedded manifest the build should never have let through.
pub fn guide_manifest() -> Result<ApolliaManifest, BundledPackageError> {
    let mut manifest = apollia_manifest::parse(GUIDE_MANIFEST)?;
    let agent = manifest
        .agents
        .first_mut()
        .ok_or(BundledPackageError::NoAgent)?;
    let mut mission = agent.mission.trim_end().to_owned();
    mission.push_str("\n\n");
    mission.push_str(KNOWLEDGE_HEADING);
    for knowledge in GUIDE_KNOWLEDGE {
        mission.push_str("\n\n");
        mission.push_str(knowledge.trim_end());
    }
    mission.push('\n');
    agent.mission = mission;
    Ok(manifest)
}

/// Assembles the guide into `<data_dir>/agents/bundled/apollia-guide.apo` and
/// returns its path.
///
/// # Errors
///
/// [`BundledPackageError`] when the manifest is invalid or the archive cannot
/// be written.
pub fn assemble_guide(data_dir: &Path) -> Result<PathBuf, BundledPackageError> {
    let manifest = guide_manifest()?;
    let bundled = data_dir.join("agents").join("bundled");
    let staging = bundled.join(format!(".build-{GUIDE}-{}", uuid::Uuid::new_v4().simple()));
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| BundledPackageError::Io { path, source }
    };
    std::fs::create_dir_all(&staging).map_err(io(&staging))?;
    let manifest_path = staging.join(apollia_manifest::MANIFEST_FILE_NAME);
    let written = manifest
        .to_toml()
        .map_err(BundledPackageError::from)
        .and_then(|text| std::fs::write(&manifest_path, text).map_err(io(&manifest_path)));
    let output = bundled.join(format!(
        "{GUIDE}.{}",
        apollia_tools::apo_archive::APO_EXTENSION
    ));
    let built = written.and_then(|()| {
        build_from_dir(&staging, &output, &BuildOptions::default())
            .map(|_| ())
            .map_err(BundledPackageError::from)
    });
    if let Err(error) = std::fs::remove_dir_all(&staging) {
        tracing::event!(
            tracing::Level::WARN,
            path = %staging.display(),
            error = %error,
            "bundled.guide.staging_cleanup_failed"
        );
    }
    built.map(|()| output)
}

/// Installs the guide the binary ships into the stores under `data_dir`,
/// recording the installation through `journal`. Nothing is assembled when
/// the installed guide is already this version.
///
/// # Errors
///
/// [`BundledPackageError`] when the guide cannot be assembled or installed.
pub async fn install_guide(
    data_dir: &Path,
    journal: AuditJournalHandle,
) -> Result<EmbeddedOutcome, BundledPackageError> {
    let version = guide_manifest()?.package.version;
    let db = apollia_core::paths::DataFile::Agents.path(data_dir);
    let installed = PackageRepository::open(&db)
        .and_then(|packages| packages.get(GUIDE))
        .map_err(|e| BundledPackageError::Store(e.to_string()))?;
    if installed.as_ref().is_some_and(|p| p.version == version) {
        return Ok(EmbeddedOutcome::Current { version });
    }
    let archive = assemble_guide(data_dir)?;
    let governance = NoPolicyGovernance::new(journal);
    Ok(install_embedded(&archive, data_dir, &governance).await?)
}

/// Installs the packages the binary ships, logging the outcome. Without an
/// audit journal nothing is installed: a package is never installed
/// unrecorded.
pub async fn install_bundled_packages(data_dir: &Path, journal: Option<AuditJournalHandle>) {
    let Some(journal) = journal else {
        tracing::event!(
            tracing::Level::WARN,
            package = %GUIDE,
            detail = "no audit journal, the guide is not installed",
            "bundled.package.skipped"
        );
        return;
    };
    match install_guide(data_dir, journal).await {
        Ok(EmbeddedOutcome::Installed(report)) => tracing::event!(
            tracing::Level::INFO,
            package = %report.name,
            version = %report.version,
            previous = ?report.previous_version,
            "bundled.package.installed"
        ),
        Ok(EmbeddedOutcome::Current { version }) => tracing::event!(
            tracing::Level::DEBUG,
            package = %GUIDE,
            version = %version,
            "bundled.package.current"
        ),
        Ok(EmbeddedOutcome::Newer { installed, bundled }) => tracing::event!(
            tracing::Level::INFO,
            package = %GUIDE,
            installed = %installed,
            bundled = %bundled,
            "bundled.package.newer_kept"
        ),
        Err(error) => tracing::event!(
            tracing::Level::WARN,
            package = %GUIDE,
            error = %error,
            "bundled.package.install_failed"
        ),
    }
}

#[cfg(test)]
mod tests;
