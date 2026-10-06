//! The example packages Apollia proposes.
//!
//! An example is a declarative package the binary carries: the files of one
//! folder of `agents/starters/`, embedded by the build script. Nothing of it
//! is installed until the user chooses it. It is then previewed like any
//! package, assembled into a `.apo` archive under
//! `<data dir>/agents/bundled/`, and installed through the same path, with
//! [`PackageOrigin::Example`]: unsigned and neither flagged nor refused for
//! it, since the binary carries it. Once installed it belongs to the user and
//! uninstalls like any package.

use std::path::{Component, Path, PathBuf};

use apollia_tools::apo_archive::{build_from_dir, BuildOptions, SignatureState, APO_EXTENSION};
use apollia_tools::apollia_manifest::{self, ApolliaManifest, MANIFEST_FILE_NAME};
use apollia_tools::PackageRepository;
use serde::Serialize;

use crate::connections::Connection;
use crate::packages::preview::build;
use crate::packages::{
    connection_matches, PackageError, PackageOrigin, PackagePreview, PreviewSources,
    RequiredConnectionPreview,
};

/// The files of one example, as the build embedded them.
#[derive(Debug, Clone, Copy)]
pub struct ExampleSource {
    /// Name of its folder.
    pub folder: &'static str,
    /// Its files: path inside the package with `/` separators, and content.
    pub files: &'static [(&'static str, &'static [u8])],
}

/// The examples of `agents/starters/`.
const SHIPPED: &[ExampleSource] = include!(concat!(env!("OUT_DIR"), "/example_packages.rs"));

/// The packages of `tests/fixtures/example-packages/`, embedded the same way.
#[cfg(test)]
pub(crate) const FIXTURE: &[ExampleSource] =
    include!(concat!(env!("OUT_DIR"), "/example_packages_fixture.rs"));

/// The examples this binary proposes.
#[cfg(not(test))]
pub fn proposed_examples() -> &'static [ExampleSource] {
    SHIPPED
}

/// The examples the tests propose: the packages of the fixture folder, so the
/// routes are exercised whatever `agents/starters/` holds.
#[cfg(test)]
pub fn proposed_examples() -> &'static [ExampleSource] {
    FIXTURE
}

/// The examples of `agents/starters/`, whatever the tests propose.
#[cfg(test)]
pub(crate) fn shipped_examples() -> &'static [ExampleSource] {
    SHIPPED
}

/// One agent of an example.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExampleAgent {
    /// Agent identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// What the agent does.
    pub mission: String,
}

/// An example as the application proposes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExamplePackage {
    /// Package identifier.
    pub name: String,
    /// Version the binary carries.
    pub version: String,
    /// What the package delivers.
    pub description: String,
    /// Its agents, each with its mission.
    pub agents: Vec<ExampleAgent>,
    /// The connections it requires, present or missing on this machine.
    pub requires: Vec<RequiredConnectionPreview>,
    /// Installed version, when the user installed it.
    pub installed_version: Option<String>,
}

impl ExampleSource {
    /// The manifest of the example.
    ///
    /// # Errors
    ///
    /// [`PackageError::Example`] when the folder carries no manifest, or one
    /// that is not valid.
    pub fn manifest(&self) -> Result<ApolliaManifest, PackageError> {
        let bytes = self
            .files
            .iter()
            .find(|(path, _)| *path == MANIFEST_FILE_NAME)
            .map(|(_, bytes)| *bytes)
            .ok_or_else(|| self.unusable(format!("no {MANIFEST_FILE_NAME}")))?;
        let text = std::str::from_utf8(bytes).map_err(|e| self.unusable(e))?;
        apollia_manifest::parse(text).map_err(|e| self.unusable(e))
    }

    /// Assembles the example into `<data_dir>/agents/bundled/<folder>.apo`,
    /// unsigned, and returns the path of the archive.
    ///
    /// # Errors
    ///
    /// [`PackageError::Example`] for a file path that leaves the package,
    /// [`PackageError::Io`] when a file cannot be written,
    /// [`PackageError::Archive`] when the archive cannot be built.
    pub fn assemble(&self, data_dir: &Path) -> Result<PathBuf, PackageError> {
        let bundled = data_dir.join("agents").join("bundled");
        let staging = bundled.join(format!(
            ".build-{}-{}",
            self.folder,
            uuid::Uuid::new_v4().simple()
        ));
        let output = bundled.join(format!("{}.{APO_EXTENSION}", self.folder));
        let built = self.write_files(&staging).and_then(|()| {
            build_from_dir(&staging, &output, &BuildOptions::default())
                .map(|_| ())
                .map_err(PackageError::from)
        });
        if let Err(error) = std::fs::remove_dir_all(&staging) {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::event!(
                    tracing::Level::WARN,
                    path = %staging.display(),
                    error = %error,
                    "packages.example.staging_cleanup_failed"
                );
            }
        }
        built.map(|()| output)
    }

    fn write_files(&self, staging: &Path) -> Result<(), PackageError> {
        for (relative, content) in self.files {
            let relative_path = Path::new(relative);
            if relative_path
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
            {
                return Err(self.unusable(format!("`{relative}` is not a path inside the package")));
            }
            let path = staging.join(relative_path);
            let parent = path.parent().unwrap_or(staging);
            std::fs::create_dir_all(parent).map_err(|source| PackageError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
            std::fs::write(&path, content).map_err(|source| PackageError::Io { path, source })?;
        }
        Ok(())
    }

    fn unusable(&self, reason: impl std::fmt::Display) -> PackageError {
        PackageError::Example {
            name: self.folder.to_owned(),
            reason: reason.to_string(),
        }
    }
}

/// The example of `examples` whose package is named `name`, with its
/// manifest. An example whose manifest is not valid is never found.
pub fn find_example<'a>(
    examples: &'a [ExampleSource],
    name: &str,
) -> Option<(&'a ExampleSource, ApolliaManifest)> {
    examples.iter().find_map(|example| {
        let manifest = example.manifest().ok()?;
        (manifest.package.name == name).then_some((example, manifest))
    })
}

/// What the application proposes: each example of `examples` with its agents,
/// the connections it requires as `connections` answer them, and the version
/// `packages` holds when it is installed. An example whose manifest is not
/// valid is left out and logged.
///
/// # Errors
///
/// [`PackageError::Store`] when the installed packages cannot be read.
pub fn list_examples(
    examples: &[ExampleSource],
    packages: &PackageRepository,
    connections: &[Connection],
) -> Result<Vec<ExamplePackage>, PackageError> {
    let mut proposed = Vec::with_capacity(examples.len());
    for example in examples {
        let manifest = match example.manifest() {
            Ok(manifest) => manifest,
            Err(error) => {
                tracing::event!(
                    tracing::Level::WARN,
                    example = %example.folder,
                    error = %error,
                    "packages.example.unusable"
                );
                continue;
            }
        };
        let installed_version = packages.get(&manifest.package.name)?.map(|p| p.version);
        proposed.push(ExamplePackage {
            name: manifest.package.name,
            version: manifest.package.version,
            description: manifest.package.description,
            agents: manifest
                .agents
                .into_iter()
                .map(|agent| ExampleAgent {
                    id: agent.id,
                    name: agent.name,
                    mission: agent.mission,
                })
                .collect(),
            requires: manifest
                .requires
                .into_iter()
                .map(|required| RequiredConnectionPreview {
                    present: connections
                        .iter()
                        .any(|c| connection_matches(c, &required.kind)),
                    brought_by_package: manifest
                        .mcp_servers
                        .iter()
                        .any(|s| s.name.eq_ignore_ascii_case(&required.kind)),
                    kind: required.kind,
                    reason: required.reason,
                })
                .collect(),
            installed_version,
        });
    }
    Ok(proposed)
}

/// Previews the example whose manifest is `manifest`. Nothing is written, and
/// no archive is assembled.
///
/// # Errors
///
/// [`PackageError::Store`] when a store cannot be read.
pub fn preview_example(
    manifest: &ApolliaManifest,
    sources: &PreviewSources<'_>,
) -> Result<PackagePreview, PackageError> {
    build(
        manifest,
        Ok(SignatureState::Unsigned),
        sources,
        PackageOrigin::Example,
    )
}
