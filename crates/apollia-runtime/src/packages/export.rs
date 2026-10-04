//! Exporting an agent created in the interface as a `.apo` package.
//!
//! The manifest is generated from the agent as it is in force: its
//! definition, then the user's overrides, with the model assigned to it. Those
//! current values become the defaults of the package, so the agent installed
//! from it on another machine reads the same card. The archive is then built
//! by the same builder `package build` uses, signed on request.
//!
//! What never enters the package, by construction rather than by filtering:
//! the archive holds `apollia.toml` and, when signed, the signature and its
//! public key, nothing else. The manifest is written from a fixed list of
//! fields of the agent, so no secret, no memory entry and no history reaches
//! it. The profile designated for the agent stays on this machine, and so does
//! the account of a connection.
//!
//! An access is kept, turned into a requirement, or removed:
//!
//! - a tool of Apollia and an MCP tool (`mcp:<server>/<tool>`) are kept; an
//!   MCP tool also requires a connection of the type of its server;
//! - a tool of a connected account (`conn:<provider>:<account>/<operation>`)
//!   names an account of this machine: it becomes a required connection of
//!   the type of its provider, and the person who installs the package grants
//!   their own account on the card;
//! - an access that reaches into another package, another agent or a
//!   workflow (`pkg:`, `agent:`, `workflow:`) is removed, since the package
//!   carries none of them, and listed in the report so the author sees it.

use std::collections::BTreeSet;
use std::path::Path;

use apollia_signing::{validate_publisher, SigningError, SigningKeyPair};
use apollia_tools::apo_archive::{
    build_from_dir, ApoError, BuildOptions, SignatureState, APO_EXTENSION,
};
use apollia_tools::apollia_manifest::{
    self, AgentDecl, AgentDefaults, ApolliaManifest, ModelDefaultsDecl, PackageSection,
    RequiredConnection, MANIFEST_FILE_NAME, PACKAGE_TOOL_PREFIX,
};
use apollia_tools::{is_built_in_agent, AgentOrigin, ResolvedAgent};

use crate::composition::{AGENT_ACCESS_PREFIX, WORKFLOW_ACCESS_PREFIX};
use crate::connectors_bridge::connection_tools::CONNECTION_TOOL_PREFIX;

/// Version written when the agent's own version is not semver: the agents
/// created in the interface are numbered `1`, `2`, ...
const FALLBACK_VERSION: &str = "1.0.0";

/// SDK requirement of an exported package: it carries no code, so any SDK
/// reads it.
const EXPORT_SDK_REQUIREMENT: &str = "*";

/// Prefix of an MCP tool access: `mcp:<server>/<tool>`.
const MCP_ACCESS_PREFIX: &str = "mcp:";

/// Why an agent cannot be exported.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ExportError {
    /// The agent came with a package: that package is what is shared.
    #[error("agent `{agent}` was installed from a package; only an agent created in the interface is exported")]
    NotInterface {
        /// The agent.
        agent: String,
    },
    /// The built-in agent Apollia is part of every installation.
    #[error("the built-in agent `{agent}` cannot be exported")]
    BuiltIn {
        /// The agent.
        agent: String,
    },
    /// The agent identifier cannot name a package.
    #[error("agent identifier `{agent}` cannot name a package: it must match [a-z0-9_-]+")]
    InvalidIdentifier {
        /// The agent.
        agent: String,
    },
    /// No publisher name was given and none is set.
    #[error("no publisher name: set `author.name` or give one for this export")]
    PublisherRequired,
    /// The publisher name cannot be written in the package.
    #[error("invalid publisher name: {0}")]
    InvalidPublisher(#[source] SigningError),
    /// The generated manifest does not validate.
    #[error("the generated manifest is invalid: {0}")]
    Manifest(#[from] apollia_manifest::ApolliaManifestError),
    /// The archive could not be built.
    #[error(transparent)]
    Archive(#[from] ApoError),
    /// A working file could not be written or read.
    #[error("{path}: {source}")]
    Io {
        /// The file or folder involved.
        path: std::path::PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
}

/// The manifest of an exported agent, and what the export changed on the way.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportPlan {
    /// The manifest written at the root of the archive.
    pub manifest: ApolliaManifest,
    /// Accesses left out of the package, in the order the agent lists them.
    pub removed_access: Vec<String>,
    /// Connection types the package requires, sorted.
    pub required_connections: Vec<String>,
}

/// An exported agent: the archive and its report.
#[derive(Debug, Clone)]
pub struct AgentExport {
    /// The bytes of the `.apo` archive.
    pub bytes: Vec<u8>,
    /// The file name the archive is offered under: `<id>-<version>.apo`.
    pub file_name: String,
    /// The publisher written in the manifest and the signature.
    pub publisher: String,
    /// Whether the archive is signed, and by which key.
    pub signature: SignatureState,
    /// Accesses left out of the package.
    pub removed_access: Vec<String>,
    /// Connection types the package requires.
    pub required_connections: Vec<String>,
}

/// The publisher an export is written under: `given` when it says something,
/// else the `author.name` setting.
///
/// # Errors
///
/// [`ExportError::PublisherRequired`] when neither names anyone, and
/// [`ExportError::InvalidPublisher`] when the name cannot be written on one
/// line of the signature format.
pub fn publisher_of(given: Option<&str>, setting: &str) -> Result<String, ExportError> {
    let name = given
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| setting.trim());
    if name.is_empty() {
        return Err(ExportError::PublisherRequired);
    }
    validate_publisher(name).map_err(ExportError::InvalidPublisher)?;
    Ok(name.to_owned())
}

/// What `access` becomes in the package.
enum AccessFate {
    /// Kept, with the connection type it requires, if any.
    Kept(Option<String>),
    /// Replaced by a required connection of this type.
    Required(String),
    /// Left out.
    Removed,
}

fn fate_of(access: &str) -> AccessFate {
    if let Some(rest) = access.strip_prefix(CONNECTION_TOOL_PREFIX) {
        return match rest.split_once(':') {
            Some((provider, _)) if !provider.is_empty() => {
                AccessFate::Required(provider.to_ascii_lowercase())
            }
            _ => AccessFate::Removed,
        };
    }
    if let Some(rest) = access.strip_prefix(MCP_ACCESS_PREFIX) {
        let server = rest.split_once('/').map_or(rest, |(server, _)| server);
        return AccessFate::Kept((!server.is_empty()).then(|| server.to_owned()));
    }
    if access.starts_with(PACKAGE_TOOL_PREFIX)
        || access.starts_with(AGENT_ACCESS_PREFIX)
        || access.starts_with(WORKFLOW_ACCESS_PREFIX)
    {
        return AccessFate::Removed;
    }
    AccessFate::Kept(None)
}

/// The model default of the package: the model assigned to the agent, else
/// the one its definition declares. A preset is a row of this machine and is
/// not carried.
fn model_default(agent: &ResolvedAgent) -> Option<ModelDefaultsDecl> {
    if let Some(model) = &agent.model {
        return Some(ModelDefaultsDecl {
            model: Some(model.model_id.clone()),
            preset: None,
            request: None,
        });
    }
    let declared = agent.definition.pointer("/defaults/model")?;
    let mut decl: ModelDefaultsDecl = serde_json::from_value(declared.clone()).ok()?;
    decl.preset = None;
    (decl.model.is_some() || decl.request.is_some()).then_some(decl)
}

/// The manifest of `agent`, published by `publisher`.
///
/// `agent` is the agent as its card shows it: when it reads by default, its
/// access is the reading family written out, so the package grants what the
/// card listed.
///
/// # Errors
///
/// [`ExportError::BuiltIn`], [`ExportError::NotInterface`] and
/// [`ExportError::InvalidIdentifier`] for an agent that cannot be exported;
/// [`ExportError::Manifest`] when the result does not validate.
pub fn export_plan(agent: &ResolvedAgent, publisher: &str) -> Result<ExportPlan, ExportError> {
    if is_built_in_agent(&agent.id) {
        return Err(ExportError::BuiltIn {
            agent: agent.id.clone(),
        });
    }
    if agent.origin != AgentOrigin::Interface {
        return Err(ExportError::NotInterface {
            agent: agent.id.clone(),
        });
    }
    if !apollia_manifest::is_valid_identifier(&agent.id) {
        return Err(ExportError::InvalidIdentifier {
            agent: agent.id.clone(),
        });
    }

    let mut access = Vec::new();
    let mut removed_access = Vec::new();
    let mut required = BTreeSet::new();
    for item in &agent.access {
        match fate_of(item) {
            AccessFate::Kept(requires) => {
                if !access.contains(item) {
                    access.push(item.clone());
                }
                required.extend(requires);
            }
            AccessFate::Required(kind) => {
                required.insert(kind);
            }
            AccessFate::Removed => removed_access.push(item.clone()),
        }
    }

    let mut manifest = ApolliaManifest {
        package: PackageSection {
            name: agent.id.clone(),
            version: agent.version.clone(),
            publisher: publisher.to_owned(),
            description: String::new(),
            sdk: EXPORT_SDK_REQUIREMENT.to_owned(),
        },
        agents: vec![AgentDecl {
            id: agent.id.clone(),
            name: agent.name.clone(),
            mission: agent.mission.clone(),
            examples: agent.examples.clone(),
            defaults: AgentDefaults {
                access,
                sources: agent.sources.clone(),
                model: model_default(agent),
                autonomy: Some(agent.autonomy),
            },
        }],
        tools: Vec::new(),
        workflows: Vec::new(),
        requires: required
            .iter()
            .map(|kind| RequiredConnection {
                kind: kind.clone(),
                reason: String::new(),
            })
            .collect(),
        mcp_servers: Vec::new(),
        pip: None,
        triggers: Vec::new(),
    };
    if let Err(apollia_manifest::ApolliaManifestError::InvalidVersion { .. }) = manifest.validate()
    {
        manifest.package.version = FALLBACK_VERSION.to_owned();
    }
    manifest.validate()?;
    Ok(ExportPlan {
        manifest,
        removed_access,
        required_connections: required.into_iter().collect(),
    })
}

/// Exports `agent` as an archive, signed with `key` when given.
///
/// The manifest is written to a working folder under `work_dir`, built into
/// an archive there, read back, and the folder removed.
///
/// # Errors
///
/// Those of [`export_plan`]; [`ExportError::Archive`] when the build fails;
/// [`ExportError::Io`] for the working folder.
pub fn export_agent(
    agent: &ResolvedAgent,
    publisher: &str,
    key: Option<&SigningKeyPair>,
    work_dir: &Path,
) -> Result<AgentExport, ExportError> {
    let plan = export_plan(agent, publisher)?;
    std::fs::create_dir_all(work_dir).map_err(|source| ExportError::Io {
        path: work_dir.to_path_buf(),
        source,
    })?;
    let work = tempfile::Builder::new()
        .prefix(".export-")
        .tempdir_in(work_dir)
        .map_err(|source| ExportError::Io {
            path: work_dir.to_path_buf(),
            source,
        })?;
    let package_dir = work.path().join("package");
    std::fs::create_dir_all(&package_dir).map_err(|source| ExportError::Io {
        path: package_dir.clone(),
        source,
    })?;
    let manifest_path = package_dir.join(MANIFEST_FILE_NAME);
    std::fs::write(&manifest_path, plan.manifest.to_toml()?).map_err(|source| ExportError::Io {
        path: manifest_path.clone(),
        source,
    })?;

    let file_name = format!(
        "{}-{}.{APO_EXTENSION}",
        plan.manifest.package.name, plan.manifest.package.version
    );
    let output = work.path().join(&file_name);
    let report = build_from_dir(
        &package_dir,
        &output,
        &BuildOptions {
            signing_key: key,
            ..BuildOptions::default()
        },
    )?;
    let bytes = std::fs::read(&output).map_err(|source| ExportError::Io {
        path: output.clone(),
        source,
    })?;
    Ok(AgentExport {
        bytes,
        file_name,
        publisher: plan.manifest.package.publisher,
        signature: report.signature,
        removed_access: plan.removed_access,
        required_connections: plan.required_connections,
    })
}
