//! The dry-run preview of a `.apo` package: nothing is written.

use std::collections::BTreeSet;
use std::path::Path;

use apollia_tools::apo_archive::{ApoArchive, ApoError, SignatureState};
use apollia_tools::apollia_manifest::{self, AgentDecl, ApolliaManifest, TriggerSource};
use apollia_tools::{
    AgentOrigin, AgentRepository, PackageRepository, ToolDescriptor, TrustedKeyRepository,
};
use serde::Serialize;

use crate::connections::{Connection, ConnectionKind};
use crate::governance::{DeclaredAccess, GovernanceContract, InstallSubject};
use crate::packages::preview_access::{access_family, declared_access, family_name};
use crate::packages::PackageError;

/// What the preview reads besides the archive.
pub struct PreviewSources<'a> {
    /// Answers which accesses an agent holds and whether a signature is
    /// required.
    pub governance: &'a dyn GovernanceContract,
    /// The installed packages.
    pub packages: &'a PackageRepository,
    /// The stored agents, to find an identifier another package already
    /// uses.
    pub agents: &'a AgentRepository,
    /// The keyring of trusted publisher keys.
    pub trusted_keys: &'a TrustedKeyRepository,
    /// The connections of this machine.
    pub connections: &'a [Connection],
    /// The tools the registry holds, to say what a granted tool permits.
    pub tools: &'a [ToolDescriptor],
}

/// State of the signature of a package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SignatureStatus {
    /// The package carries no signature.
    Unsigned,
    /// The signature holds over every entry of the package.
    Signed,
    /// The package carries a signature that does not hold.
    Invalid,
}

/// The publisher and the signature, as the preview shows them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignaturePreview {
    /// State of the signature.
    pub status: SignatureStatus,
    /// Name to show for the publisher: the label of the keyring when the key
    /// is trusted, otherwise the name the package declares.
    pub publisher: String,
    /// Publisher name the package declares. Not verified.
    pub declared_publisher: String,
    /// SHA-256 fingerprint of the signing key, when the signature holds.
    pub key_fingerprint: Option<String>,
    /// The signing public key, 64 hexadecimal characters, when the signature
    /// holds: what trusting the publisher imports into the keyring.
    pub public_key: Option<String>,
    /// The signing key is in the keyring.
    pub trusted: bool,
    /// Why the signature does not hold, when it does not.
    pub reason: Option<String>,
}

/// One access an agent asks for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccessPreview {
    /// Identifier of the access: a tool name, `mcp:<server>/<tool>`, or
    /// `pkg:<package>/<tool>`.
    pub access_id: String,
    /// What the access lets the agent do.
    pub permits: String,
    /// Using it is sensitive.
    pub sensitive: bool,
    /// Action family: `read`, `read_personal`, `write_or_delete`,
    /// `send_external`, `execute_code`, or `unclassified` when nothing
    /// classifies it (a tool of a connection not added yet, an agent).
    pub family: String,
}

/// One agent of the package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentPreview {
    /// Identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// What the agent is for.
    pub mission: String,
    /// Example requests.
    pub examples: Vec<String>,
    /// Accesses the agent asks for, as governance resolves them.
    pub access: Vec<AccessPreview>,
}

/// One Python tool of the package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ItemPreview {
    /// Identifier, unique in the package.
    pub name: String,
    /// What it does.
    pub description: String,
    /// Action family: `read`, `read_personal`, `write_or_delete`,
    /// `send_external` or `execute_code`.
    pub family: String,
    /// Its family is sensitive.
    pub sensitive: bool,
}

/// One workflow of the package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowPreview {
    /// Identifier, unique in the package.
    pub name: String,
    /// What it does.
    pub description: String,
    /// Action family of the most sensitive thing it does.
    pub family: String,
    /// Its family is sensitive.
    pub sensitive: bool,
    /// Names of its steps, in order.
    pub steps: Vec<String>,
}

/// A connection the package requires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequiredConnectionPreview {
    /// Connection type, as the package names it.
    #[serde(rename = "type")]
    pub kind: String,
    /// Why the package needs it.
    pub reason: String,
    /// A connection of that type exists on this machine.
    pub present: bool,
    /// The package itself brings an MCP server of that name.
    pub brought_by_package: bool,
}

/// An MCP server the package brings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BroughtServerPreview {
    /// Server name.
    pub name: String,
    /// `stdio`, `streamable-http` or `sse`.
    pub transport: String,
    /// Executable spawned, for `stdio`.
    pub command: String,
    /// Address, for a network transport.
    pub url: Option<String>,
    /// The server sends data off this machine.
    pub remote: bool,
    /// A connection of that name already exists; it is left as it is.
    pub already_present: bool,
}

/// A trigger the package proposes. Installing does not arm it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TriggerPreview {
    /// Identifier.
    pub id: String,
    /// Agent or workflow it starts.
    pub target: String,
    /// `cron`, `interval`, `file_watch` or `webhook`.
    pub source: String,
}

/// An access the update grants that the installed version did not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AddedAccess {
    /// Agent that gains it.
    pub agent_id: String,
    /// Identifier of the access.
    pub access_id: String,
    /// What it lets the agent do.
    pub permits: String,
    /// Using it is sensitive.
    pub sensitive: bool,
    /// Action family: `read`, `read_personal`, `write_or_delete`,
    /// `send_external`, `execute_code`, or `unclassified`.
    pub family: String,
}

/// What an update changes, against the installed version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDiff {
    /// Installed version.
    pub from_version: String,
    /// Incoming version.
    pub to_version: String,
    /// The incoming version is older than the installed one.
    pub downgrade: bool,
    /// Accesses the incoming package defaults grant that the installed ones
    /// did not.
    pub added_access: Vec<AddedAccess>,
    /// Connection types required by the incoming version only.
    pub added_requires: Vec<String>,
    /// MCP servers brought by the incoming version only.
    pub added_mcp_servers: Vec<String>,
}

impl UpdateDiff {
    /// The update adds an access, a required connection or an MCP server.
    pub fn adds_anything(&self) -> bool {
        !self.added_access.is_empty()
            || !self.added_requires.is_empty()
            || !self.added_mcp_servers.is_empty()
    }
}

/// Something the user should know before installing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum PackageNotice {
    /// The package is not signed: nothing says who made it or that it was
    /// not altered.
    Unsigned,
    /// The signing key is not in the keyring: the publisher name is declared,
    /// not verified.
    UntrustedPublisher {
        /// Fingerprint of the signing key.
        fingerprint: String,
    },
    /// A connection the package requires is missing.
    MissingConnection {
        /// Connection type.
        #[serde(rename = "type")]
        kind: String,
    },
    /// An MCP server of that name already exists and is left as it is.
    ServerAlreadyPresent {
        /// Server name.
        name: String,
    },
}

/// Why the package cannot be installed as it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum InstallRefusal {
    /// The signature does not hold over the content.
    InvalidSignature {
        /// Why.
        reason: String,
    },
    /// Governance requires a signed package and this one is not signed.
    SignatureRequired,
    /// Governance requires a signed package, the organisation policy names the
    /// publishers it trusts, and the key that signed this one is not among
    /// them.
    PublisherNotTrusted {
        /// Fingerprint of the signing key.
        fingerprint: String,
    },
    /// The incoming version is older than the installed one.
    Downgrade {
        /// Installed version.
        installed: String,
        /// Incoming version.
        incoming: String,
    },
    /// An agent of the package has the identifier of an agent installed
    /// otherwise.
    AgentTaken {
        /// The identifier.
        agent: String,
        /// The package that owns it, `None` for an agent created in the
        /// interface.
        owner: Option<String>,
    },
}

impl std::fmt::Display for InstallRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallRefusal::InvalidSignature { reason } => {
                write!(f, "the signature of the package does not hold: {reason}")
            }
            InstallRefusal::SignatureRequired => {
                write!(f, "a signed package is required and this one is not signed")
            }
            InstallRefusal::PublisherNotTrusted { fingerprint } => write!(
                f,
                "the organisation policy does not trust the key {fingerprint} that signed this package"
            ),
            InstallRefusal::Downgrade {
                installed,
                incoming,
            } => write!(
                f,
                "version {incoming} is older than the installed {installed}; allow the downgrade to install it"
            ),
            InstallRefusal::AgentTaken { agent, owner } => match owner {
                Some(owner) => write!(f, "agent `{agent}` is already installed by package `{owner}`"),
                None => write!(f, "agent `{agent}` already exists, created in the interface"),
            },
        }
    }
}

/// Everything the user sees before installing a package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackagePreview {
    /// Package identifier.
    pub name: String,
    /// Incoming version.
    pub version: String,
    /// What the package delivers.
    pub description: String,
    /// SDK requirement the package was built against.
    pub sdk: String,
    /// Publisher and signature.
    pub signature: SignaturePreview,
    /// Governance requires a signed package.
    pub signature_required: bool,
    /// Agents.
    pub agents: Vec<AgentPreview>,
    /// Workflows.
    pub workflows: Vec<WorkflowPreview>,
    /// Python tools.
    pub tools: Vec<ItemPreview>,
    /// Connections the package requires.
    pub requires: Vec<RequiredConnectionPreview>,
    /// MCP servers the package brings.
    pub mcp_servers: Vec<BroughtServerPreview>,
    /// Python requirements installed in the package environment.
    pub pip: Vec<String>,
    /// Triggers the package proposes.
    pub triggers: Vec<TriggerPreview>,
    /// Installed version, when the package is installed.
    pub installed_version: Option<String>,
    /// What the incoming version adds, when the package is installed.
    pub update: Option<UpdateDiff>,
    /// What the user should know.
    pub notices: Vec<PackageNotice>,
    /// Why installing it as it is would be refused.
    pub refusal: Option<InstallRefusal>,
}

/// Previews the package at `path`. Nothing is written.
///
/// # Errors
///
/// [`PackageError::Archive`] when the archive cannot be read or its manifest
/// is invalid, [`PackageError::Store`] when a store cannot be read. A
/// signature that does not hold is not an error: the preview reports it and
/// sets the refusal.
pub fn preview(path: &Path, sources: &PreviewSources<'_>) -> Result<PackagePreview, PackageError> {
    let mut archive = ApoArchive::open(path)?;
    let signature_state = match archive.signature_state() {
        Ok(state) => Ok(state),
        Err(ApoError::InvalidSignature { reason }) => Err(reason),
        Err(other) => return Err(other.into()),
    };
    let public_key = match signature_state {
        Ok(SignatureState::Signed { .. }) => archive.signing_public_key()?,
        _ => None,
    };
    let manifest = archive.manifest().clone();
    let mut preview = build(&manifest, signature_state, sources, PackageOrigin::File)?;
    preview.signature.public_key = public_key;
    Ok(preview)
}

/// Where a package comes from, as the caller knows it. The archive itself
/// never says: nothing in a file can make it [`PackageOrigin::Embedded`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PackageOrigin {
    /// A file the user or a host hands over.
    #[default]
    File,
    /// A package Apollia ships inside its binary, such as its guide. It is not
    /// signed: the binary carries it, and the binary is what the distribution
    /// signature covers. It installs with no notice about its signature, and a
    /// policy that requires a signature does not refuse it.
    Embedded,
    /// An example Apollia ships inside its binary and proposes. The binary
    /// carries it like an embedded package, so its signature is treated the
    /// same; it is installed only when the user chooses it, and uninstalls
    /// like any package.
    Example,
}

impl PackageOrigin {
    /// The binary carries the package: it is not signed, and is neither
    /// flagged nor refused for it.
    pub fn shipped(self) -> bool {
        matches!(self, PackageOrigin::Embedded | PackageOrigin::Example)
    }
}

/// Builds the preview of `manifest` whose signature reads as `signature`
/// (`Err` holding the reason when it does not hold), for a package of
/// `origin`.
pub(crate) fn build(
    manifest: &ApolliaManifest,
    signature: Result<SignatureState, String>,
    sources: &PreviewSources<'_>,
    origin: PackageOrigin,
) -> Result<PackagePreview, PackageError> {
    let embedded = origin.shipped();
    let package = &manifest.package;
    let signature = signature_preview(&package.publisher, signature, sources)?;
    let signature_required = sources.governance.signature_required(&InstallSubject {
        package_id: &package.name,
        publisher: Some(&package.publisher),
    });

    let agents: Vec<AgentPreview> = manifest
        .agents
        .iter()
        .map(|agent| agent_preview(manifest, agent, sources))
        .collect();
    let tools = manifest
        .tools
        .iter()
        .map(|tool| ItemPreview {
            name: tool.name.clone(),
            description: tool.description.clone(),
            family: family_name(tool.action).to_owned(),
            sensitive: tool.is_sensitive(),
        })
        .collect();
    let workflows = manifest
        .workflows
        .iter()
        .map(|workflow| WorkflowPreview {
            name: workflow.name.clone(),
            description: workflow.description.clone(),
            family: family_name(workflow.action).to_owned(),
            sensitive: workflow.is_sensitive(),
            steps: workflow.steps.clone(),
        })
        .collect();
    let requires: Vec<RequiredConnectionPreview> = manifest
        .requires
        .iter()
        .map(|required| RequiredConnectionPreview {
            kind: required.kind.clone(),
            reason: required.reason.clone(),
            present: sources
                .connections
                .iter()
                .any(|c| connection_matches(c, &required.kind)),
            brought_by_package: manifest
                .mcp_servers
                .iter()
                .any(|s| s.name.eq_ignore_ascii_case(&required.kind)),
        })
        .collect();
    let mcp_servers: Vec<BroughtServerPreview> = manifest
        .mcp_servers
        .iter()
        .map(|server| BroughtServerPreview {
            name: server.name.clone(),
            transport: server.transport.clone(),
            command: server.command.clone(),
            url: server.url.clone(),
            remote: crate::connections::is_remote(&server.transport, server.url.as_deref()),
            already_present: mcp_server_exists(sources.connections, &server.name),
        })
        .collect();
    let triggers = manifest
        .triggers
        .iter()
        .map(|trigger| TriggerPreview {
            id: trigger.id.clone(),
            target: trigger.target.clone(),
            source: trigger_source_name(&trigger.source).to_owned(),
        })
        .collect();

    let installed = sources.packages.get(&package.name)?;
    let update = installed
        .as_ref()
        .map(|installed| update_diff(installed, manifest, &agents));

    let mut notices = Vec::new();
    match signature.status {
        SignatureStatus::Unsigned if !embedded => notices.push(PackageNotice::Unsigned),
        SignatureStatus::Signed if !signature.trusted => {
            if let Some(fingerprint) = &signature.key_fingerprint {
                notices.push(PackageNotice::UntrustedPublisher {
                    fingerprint: fingerprint.clone(),
                });
            }
        }
        _ => {}
    }
    for required in &requires {
        if !required.present && !required.brought_by_package {
            notices.push(PackageNotice::MissingConnection {
                kind: required.kind.clone(),
            });
        }
    }
    for server in &mcp_servers {
        if server.already_present {
            notices.push(PackageNotice::ServerAlreadyPresent {
                name: server.name.clone(),
            });
        }
    }

    let refusal = refusal(
        manifest,
        &signature,
        signature_required && !embedded,
        &update,
        sources,
    )?;

    Ok(PackagePreview {
        name: package.name.clone(),
        version: package.version.clone(),
        description: package.description.clone(),
        sdk: package.sdk.clone(),
        signature,
        signature_required,
        agents,
        workflows,
        tools,
        requires,
        mcp_servers,
        pip: manifest
            .pip
            .as_ref()
            .map(|pip| pip.packages.clone())
            .unwrap_or_default(),
        triggers,
        installed_version: installed.map(|p| p.version),
        update,
        notices,
        refusal,
    })
}

/// `true` when `connection` is of the type a package names `kind`.
///
/// The type of a connection is the segment that follows its kind in its
/// identifier: the provider of a native account (`native:google:<account>`
/// is `google`), the server name of an MCP server (`mcp:notion` is
/// `notion`), and `imap` for a mailbox. The comparison ignores case.
pub fn connection_matches(connection: &Connection, kind: &str) -> bool {
    let connection_type = match connection.kind {
        ConnectionKind::Imap => "imap",
        ConnectionKind::Folder => "folder",
        ConnectionKind::Native | ConnectionKind::Mcp => {
            connection.id.split(':').nth(1).unwrap_or_default()
        }
    };
    connection_type.eq_ignore_ascii_case(kind)
}

fn mcp_server_exists(connections: &[Connection], name: &str) -> bool {
    connections
        .iter()
        .any(|c| c.kind == ConnectionKind::Mcp && c.id == format!("mcp:{name}"))
}

fn signature_preview(
    declared: &str,
    state: Result<SignatureState, String>,
    sources: &PreviewSources<'_>,
) -> Result<SignaturePreview, PackageError> {
    Ok(match state {
        Ok(SignatureState::Unsigned) => SignaturePreview {
            status: SignatureStatus::Unsigned,
            publisher: declared.to_owned(),
            declared_publisher: declared.to_owned(),
            key_fingerprint: None,
            public_key: None,
            trusted: false,
            reason: None,
        },
        Ok(SignatureState::Signed {
            key_fingerprint, ..
        }) => {
            let trusted = sources.trusted_keys.get(&key_fingerprint)?;
            SignaturePreview {
                status: SignatureStatus::Signed,
                publisher: trusted
                    .as_ref()
                    .filter(|key| !key.label.is_empty())
                    .map_or_else(|| declared.to_owned(), |key| key.label.clone()),
                declared_publisher: declared.to_owned(),
                key_fingerprint: Some(key_fingerprint),
                public_key: None,
                trusted: trusted.is_some(),
                reason: None,
            }
        }
        Err(reason) => SignaturePreview {
            status: SignatureStatus::Invalid,
            publisher: declared.to_owned(),
            declared_publisher: declared.to_owned(),
            key_fingerprint: None,
            public_key: None,
            trusted: false,
            reason: Some(reason),
        },
    })
}

fn agent_preview(
    manifest: &ApolliaManifest,
    agent: &AgentDecl,
    sources: &PreviewSources<'_>,
) -> AgentPreview {
    let declared: Vec<DeclaredAccess> = agent
        .defaults
        .access
        .iter()
        .map(|access_id| declared_access(manifest, access_id, sources.tools))
        .collect();
    let access = sources
        .governance
        .effective_accesses(&agent.id, &declared)
        .into_iter()
        .map(|access| AccessPreview {
            family: access_family(manifest, &access.access_id, sources.tools),
            access_id: access.access_id,
            permits: access.permits,
            sensitive: access.sensitive,
        })
        .collect();
    AgentPreview {
        id: agent.id.clone(),
        name: agent.name.clone(),
        mission: agent.mission.clone(),
        examples: agent.examples.clone(),
        access,
    }
}

fn update_diff(
    installed: &apollia_tools::InstalledPackage,
    incoming: &ApolliaManifest,
    agents: &[AgentPreview],
) -> UpdateDiff {
    // A package installed from an `agent.toml` folder has no `apollia.toml`
    // manifest to compare with: everything the incoming one declares counts
    // as added.
    let previous = serde_json::from_str::<ApolliaManifest>(&installed.manifest_json).ok();
    let previous_access = |agent_id: &str| -> BTreeSet<String> {
        previous
            .as_ref()
            .and_then(|m| m.agents.iter().find(|a| a.id == agent_id))
            .map(|a| a.defaults.access.iter().cloned().collect())
            .unwrap_or_default()
    };
    let mut added_access = Vec::new();
    for agent in agents {
        let before = previous_access(&agent.id);
        for access in &agent.access {
            if !before.contains(&access.access_id) {
                added_access.push(AddedAccess {
                    agent_id: agent.id.clone(),
                    access_id: access.access_id.clone(),
                    permits: access.permits.clone(),
                    sensitive: access.sensitive,
                    family: access.family.clone(),
                });
            }
        }
    }
    let previous_requires: BTreeSet<&str> = previous
        .iter()
        .flat_map(|m| m.requires.iter().map(|r| r.kind.as_str()))
        .collect();
    let previous_servers: BTreeSet<&str> = previous
        .iter()
        .flat_map(|m| m.mcp_servers.iter().map(|s| s.name.as_str()))
        .collect();
    UpdateDiff {
        from_version: installed.version.clone(),
        to_version: incoming.package.version.clone(),
        downgrade: apollia_manifest::is_older_version(
            &incoming.package.version,
            &installed.version,
        ),
        added_access,
        added_requires: incoming
            .requires
            .iter()
            .filter(|r| !previous_requires.contains(r.kind.as_str()))
            .map(|r| r.kind.clone())
            .collect(),
        added_mcp_servers: incoming
            .mcp_servers
            .iter()
            .filter(|s| !previous_servers.contains(s.name.as_str()))
            .map(|s| s.name.clone())
            .collect(),
    }
}

fn refusal(
    manifest: &ApolliaManifest,
    signature: &SignaturePreview,
    signature_required: bool,
    update: &Option<UpdateDiff>,
    sources: &PreviewSources<'_>,
) -> Result<Option<InstallRefusal>, PackageError> {
    if let (SignatureStatus::Invalid, Some(reason)) = (signature.status, &signature.reason) {
        return Ok(Some(InstallRefusal::InvalidSignature {
            reason: reason.clone(),
        }));
    }
    if signature.status == SignatureStatus::Unsigned && signature_required {
        return Ok(Some(InstallRefusal::SignatureRequired));
    }
    if let (SignatureStatus::Signed, Some(fingerprint)) =
        (signature.status, &signature.key_fingerprint)
    {
        if signature_required && !signature.trusted && sources.trusted_keys.restricted_to_policy() {
            return Ok(Some(InstallRefusal::PublisherNotTrusted {
                fingerprint: fingerprint.clone(),
            }));
        }
    }
    for agent in &manifest.agents {
        if let Some(stored) = sources.agents.stored(&agent.id)? {
            let owned_here = stored.origin == AgentOrigin::Package
                && stored.package.as_deref() == Some(manifest.package.name.as_str());
            if !owned_here {
                return Ok(Some(InstallRefusal::AgentTaken {
                    agent: agent.id.clone(),
                    owner: stored.package,
                }));
            }
        }
    }
    if let Some(diff) = update {
        if diff.downgrade {
            return Ok(Some(InstallRefusal::Downgrade {
                installed: diff.from_version.clone(),
                incoming: diff.to_version.clone(),
            }));
        }
    }
    Ok(None)
}

fn trigger_source_name(source: &TriggerSource) -> &'static str {
    match source {
        TriggerSource::Cron { .. } => "cron",
        TriggerSource::Interval { .. } => "interval",
        TriggerSource::FileWatch { .. } => "file_watch",
        TriggerSource::Webhook {} => "webhook",
    }
}
