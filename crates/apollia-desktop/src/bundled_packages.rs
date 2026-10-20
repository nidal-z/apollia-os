//! The packages the desktop binary ships: the guide, the onboarding and the
//! agent workshop of Apollia.
//!
//! The guide is a declarative agent: a mission, examples and defaults, no
//! code. Its manifest and its knowledge base are embedded in the binary by
//! `include_str!`. The onboarding is a declarative agent too, with one tool
//! that records the profile facts of the first conversation; its manifest and
//! the source of its tool are embedded the same way. The agent workshop holds
//! no agent: it is one workflow, which turns the form of the Agents screen
//! into the sheet of an agent; its manifest and the source of the workflow are
//! embedded too.
//!
//! On start, unless the installed package is already the version the binary
//! carries with the same content (compared by the digest recorded at its
//! installation, so a fix shipped without a new version number still
//! reaches an installed package), the desktop assembles each into a `.apo`
//! archive under
//! `<data dir>/agents/bundled/` (for the guide, the knowledge appended to the
//! mission) and installs it through the same path as any package
//! ([`apollia_runtime::packages::install_embedded`]): unsigned and recognised
//! as embedded, recorded in the audit journal, never downgrading a newer
//! version, keeping the user's overrides and leaving a disabled agent
//! disabled.
//!
//! Both agents ran Python code in earlier versions. Before the agents that run
//! code are loaded, [`retire_code_agents`] turns each such row into the
//! declarative agent of its package, so the code is never loaded again and
//! the package installs over it as over its own agent.

use std::path::{Path, PathBuf};

use apollia_aip::package_tools::grant::{ONBOARDING_MODULE, ONBOARDING_PACKAGE, ONBOARDING_SOURCE};
use apollia_runtime::governance::{GovernanceContract, GovernanceHandle};
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

/// Identifier of the onboarding package, and of its one agent.
pub const ONBOARDING: &str = ONBOARDING_PACKAGE;

/// The manifest of the onboarding, its tool described.
const ONBOARDING_MANIFEST: &str =
    include_str!("../../../agents/system/onboarding-agent/apollia.toml");

/// The files of the onboarding package besides its manifest.
const ONBOARDING_FILES: [(&str, &str); 1] = [(ONBOARDING_MODULE, ONBOARDING_SOURCE)];

/// The manifest of the agent workshop, its workflow described.
const AGENT_BUILDER_MANIFEST: &str =
    include_str!("../../../agents/system/agent-builder/apollia.toml");

/// The files of the agent workshop besides its manifest.
const AGENT_BUILDER_FILES: [(&str, &str); 1] = [(
    "workflows/draft.py",
    include_str!("../../../agents/system/agent-builder/workflows/draft.py"),
)];

/// The agents earlier versions installed as code, now shipped as packages of
/// the same name.
const RETIRED_CODE_AGENTS: [&str; 2] = [GUIDE, ONBOARDING];

/// Heading under which the knowledge base follows the mission.
const KNOWLEDGE_HEADING: &str = "Knowledge base:";

/// Errors while assembling or installing a package the binary ships.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BundledPackageError {
    /// The embedded manifest is invalid.
    #[error(transparent)]
    Manifest(#[from] ApolliaManifestError),
    /// The embedded manifest declares no agent to carry the knowledge.
    #[error("the embedded guide manifest declares no agent")]
    NoAgent,
    /// The manifest names a file the binary does not carry.
    #[error("the embedded package file {0} is not a relative path inside the package")]
    File(String),
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

/// Turns the guide and the onboarding installed as Python code into the
/// declarative agents of their packages, keeping their overrides and whether
/// each is enabled. Called at boot, before the stored agents are registered.
pub fn retire_code_agents(repo: &AgentRepository) {
    for name in RETIRED_CODE_AGENTS {
        match repo.adopt_into_package(name, name) {
            Ok(true) => tracing::event!(
                tracing::Level::INFO,
                agent = %name,
                "bundled.agent.code_retired"
            ),
            Ok(false) => {}
            Err(error) => tracing::event!(
                tracing::Level::WARN,
                agent = %name,
                error = %error,
                "bundled.agent.code_retire_failed"
            ),
        }
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

/// The manifest of the onboarding as the binary ships it.
///
/// # Errors
///
/// [`BundledPackageError::Manifest`] for an embedded manifest the build should
/// never have let through.
pub fn onboarding_manifest() -> Result<ApolliaManifest, BundledPackageError> {
    Ok(apollia_manifest::parse(ONBOARDING_MANIFEST)?)
}

/// The manifest of the agent workshop as the binary ships it.
///
/// # Errors
///
/// [`BundledPackageError::Manifest`] for an embedded manifest the build should
/// never have let through.
pub fn agent_builder_manifest() -> Result<ApolliaManifest, BundledPackageError> {
    Ok(apollia_manifest::parse(AGENT_BUILDER_MANIFEST)?)
}

/// Assembles `manifest` and `files` (relative path, content) into
/// `<data_dir>/agents/bundled/<package>.apo` and returns its path.
fn assemble(
    data_dir: &Path,
    manifest: &ApolliaManifest,
    files: &[(&str, &str)],
) -> Result<PathBuf, BundledPackageError> {
    let name = manifest.package.name.as_str();
    let bundled = data_dir.join("agents").join("bundled");
    let staging = bundled.join(format!(".build-{name}-{}", uuid::Uuid::new_v4().simple()));
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| BundledPackageError::Io { path, source }
    };
    std::fs::create_dir_all(&staging).map_err(io(&staging))?;
    let manifest_path = staging.join(apollia_manifest::MANIFEST_FILE_NAME);
    let written = manifest
        .to_toml()
        .map_err(BundledPackageError::from)
        .and_then(|text| std::fs::write(&manifest_path, text).map_err(io(&manifest_path)))
        .and_then(|()| {
            files
                .iter()
                .try_for_each(|(relative, content)| write_file(&staging, relative, content))
        });
    let output = bundled.join(format!(
        "{name}.{}",
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
            "bundled.package.staging_cleanup_failed"
        );
    }
    built.map(|()| output)
}

/// Writes `content` at `relative` under `staging`, creating its folders.
fn write_file(staging: &Path, relative: &str, content: &str) -> Result<(), BundledPackageError> {
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(BundledPackageError::File(relative.to_owned()));
    }
    let path = staging.join(relative_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| BundledPackageError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    std::fs::write(&path, content).map_err(|source| BundledPackageError::Io { path, source })
}

/// A package the binary ships: its manifest and the files besides it.
struct Shipped {
    manifest: fn() -> Result<ApolliaManifest, BundledPackageError>,
    files: &'static [(&'static str, &'static str)],
}

impl Shipped {
    /// The content digest of the archive this package assembles into,
    /// computed from what the binary carries without assembling it.
    fn content_digest(&self, manifest: &ApolliaManifest) -> Result<String, BundledPackageError> {
        let text = manifest.to_toml()?;
        let files = std::iter::once((apollia_manifest::MANIFEST_FILE_NAME, text.as_bytes())).chain(
            self.files
                .iter()
                .map(|(name, content)| (*name, content.as_bytes())),
        );
        Ok(apollia_tools::apo_archive::content_digest(files)?)
    }
}

/// The packages the binary ships, in the order they are installed.
const SHIPPED: [Shipped; 3] = [
    Shipped {
        manifest: guide_manifest,
        files: &[],
    },
    Shipped {
        manifest: onboarding_manifest,
        files: &ONBOARDING_FILES,
    },
    Shipped {
        manifest: agent_builder_manifest,
        files: &AGENT_BUILDER_FILES,
    },
];

/// Installs `shipped` into the stores under `data_dir`, recording the
/// installation through `governance`. Nothing is assembled when the installed
/// package is already this version with this content.
async fn install_shipped(
    shipped: &Shipped,
    data_dir: &Path,
    governance: &dyn GovernanceContract,
) -> Result<EmbeddedOutcome, BundledPackageError> {
    let manifest = (shipped.manifest)()?;
    let digest = shipped.content_digest(&manifest)?;
    let package = &manifest.package;
    let db = apollia_core::paths::DataFile::Agents.path(data_dir);
    let current = PackageRepository::open(&db)
        .and_then(|packages| {
            let same_version = packages
                .get(&package.name)?
                .is_some_and(|p| p.version == package.version);
            let same_content =
                packages.content_digest(&package.name)?.as_deref() == Some(digest.as_str());
            Ok(same_version && same_content)
        })
        .map_err(|e| BundledPackageError::Store(e.to_string()))?;
    if current {
        return Ok(EmbeddedOutcome::Current {
            version: package.version.clone(),
        });
    }
    let archive = assemble(data_dir, &manifest, shipped.files)?;
    Ok(install_embedded(&archive, data_dir, governance).await?)
}

/// Installs the packages the binary ships, logging each outcome. Without an
/// audit journal nothing is installed: a package is never installed
/// unrecorded.
pub async fn install_bundled_packages(data_dir: &Path, governance: GovernanceHandle) {
    if !governance.journaled() {
        tracing::event!(
            tracing::Level::WARN,
            detail = "no audit journal, the packages the binary ships are not installed",
            "bundled.package.skipped"
        );
        return;
    }
    for shipped in &SHIPPED {
        let name = (shipped.manifest)()
            .map(|m| m.package.name)
            .unwrap_or_default();
        log_outcome(&name, install_shipped(shipped, data_dir, &governance).await);
    }
}

/// Logs what became of the shipped package `name`.
fn log_outcome(name: &str, outcome: Result<EmbeddedOutcome, BundledPackageError>) {
    match outcome {
        Ok(EmbeddedOutcome::Installed(report)) => tracing::event!(
            tracing::Level::INFO,
            package = %report.name,
            version = %report.version,
            previous = ?report.previous_version,
            "bundled.package.installed"
        ),
        Ok(EmbeddedOutcome::Current { version }) => tracing::event!(
            tracing::Level::DEBUG,
            package = %name,
            version = %version,
            "bundled.package.current"
        ),
        Ok(EmbeddedOutcome::Newer { installed, bundled }) => tracing::event!(
            tracing::Level::INFO,
            package = %name,
            installed = %installed,
            bundled = %bundled,
            "bundled.package.newer_kept"
        ),
        Err(error) => tracing::event!(
            tracing::Level::WARN,
            package = %name,
            error = %error,
            "bundled.package.install_failed"
        ),
    }
}

#[cfg(test)]
mod tests;
