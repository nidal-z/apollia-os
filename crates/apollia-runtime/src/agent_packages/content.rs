//! What an installed package holds, and who published it, as the library of
//! installed packages shows it.
//!
//! The content is what the store links to the package (`package_items`):
//! its agents, workflows, tools and the connections it brought. The manifest
//! recorded at installation describes each item; an item the manifest does
//! not describe, as in a package installed from a folder, is listed by its
//! identifier alone. The connections a package requires are read from the
//! manifest: they belong to the user and stay when the package goes.

use apollia_tools::apollia_manifest::ApolliaManifest;
use apollia_tools::{
    InstalledPackage, PackageItemKind, PackageRepository, PackageRepositoryError,
    TrustedKeyRepository,
};
use serde::{Deserialize, Serialize};

/// Longest description of an agent in the content of a package, in
/// characters: the first line of its mission, cut there.
const AGENT_SUMMARY_CHARS: usize = 160;

/// Who published an installed package, as recorded at its installation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AgentPackageSignature {
    /// Name to show for the publisher: the label of the keyring when the key
    /// that signed the package is trusted, otherwise the name the package
    /// declares, which nothing verifies.
    pub publisher: String,
    /// The package carried a signature that held.
    pub signed: bool,
    /// The key that signed it is in the keyring of trusted publishers.
    pub trusted: bool,
    /// SHA-256 fingerprint of the signing key, when the package is signed.
    pub key_fingerprint: Option<String>,
}

/// An agent or a workflow of a package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AgentPackageItem {
    /// Identifier: the agent id, or the workflow name inside its package.
    pub id: String,
    /// Display name.
    pub name: String,
    /// What it is for, in one line; empty when the manifest does not say.
    pub description: String,
}

/// A Python tool of a package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AgentPackageTool {
    /// Tool name inside its package.
    pub name: String,
    /// What it does; empty when the manifest does not say.
    pub description: String,
    /// Action family: `read`, `read_personal`, `write_or_delete`,
    /// `send_external` or `execute_code`; empty when the manifest does not
    /// describe the tool.
    pub family: String,
}

/// A connection a package requires. It belongs to the user and stays when
/// the package is uninstalled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AgentPackageRequirement {
    /// Connection type, as the package names it.
    #[serde(rename = "type")]
    pub kind: String,
    /// Why the package needs it.
    pub reason: String,
}

/// What a package installed, by kind.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AgentPackageContent {
    /// Its agents.
    pub agents: Vec<AgentPackageItem>,
    /// Its workflows.
    pub workflows: Vec<AgentPackageItem>,
    /// Its Python tools.
    pub tools: Vec<AgentPackageTool>,
    /// The connections it brought: they go with it.
    pub connections_brought: Vec<String>,
    /// The connections it requires: they stay.
    pub connections_required: Vec<AgentPackageRequirement>,
}

/// How many items of each kind a package installed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AgentPackageCounts {
    /// Agents.
    pub agents: usize,
    /// Workflows.
    pub workflows: usize,
    /// Python tools.
    pub tools: usize,
    /// Connections, brought and required together.
    pub connections: usize,
}

impl AgentPackageContent {
    /// The number of items of each kind.
    pub fn counts(&self) -> AgentPackageCounts {
        AgentPackageCounts {
            agents: self.agents.len(),
            workflows: self.workflows.len(),
            tools: self.tools.len(),
            connections: self.connections_brought.len() + self.connections_required.len(),
        }
    }
}

/// An automation that starts an agent or a workflow of a package: it goes
/// with the package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AgentPackageAutomation {
    /// Trigger identifier.
    pub id: String,
    /// The agent, or the workflow as `<package>/<workflow>`, it starts.
    pub target: String,
}

/// The first line of `mission`, cut at [`AGENT_SUMMARY_CHARS`] characters.
fn summary(mission: &str) -> String {
    let line = mission
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    if line.chars().count() <= AGENT_SUMMARY_CHARS {
        return line.to_owned();
    }
    let cut: String = line.chars().take(AGENT_SUMMARY_CHARS).collect();
    format!("{}…", cut.trim_end())
}

/// What `package` installed, as `packages` links it and its manifest
/// describes it.
///
/// # Errors
///
/// [`PackageRepositoryError`] when the store cannot be read.
pub(crate) fn content(
    package: &InstalledPackage,
    packages: &PackageRepository,
) -> Result<AgentPackageContent, PackageRepositoryError> {
    let manifest = serde_json::from_str::<ApolliaManifest>(&package.manifest_json).ok();
    let mut content = AgentPackageContent::default();
    for item in packages.list_items(&package.name)? {
        let id = item.item_id;
        match item.kind {
            PackageItemKind::Agent => {
                let described = manifest
                    .as_ref()
                    .and_then(|m| m.agents.iter().find(|a| a.id == id));
                content.agents.push(AgentPackageItem {
                    name: described.map_or_else(|| id.clone(), |a| a.name.clone()),
                    description: described.map(|a| summary(&a.mission)).unwrap_or_default(),
                    id,
                });
            }
            PackageItemKind::Workflow => {
                let described = manifest
                    .as_ref()
                    .and_then(|m| m.workflows.iter().find(|w| w.name == id));
                content.workflows.push(AgentPackageItem {
                    name: id.clone(),
                    description: described.map(|w| w.description.clone()).unwrap_or_default(),
                    id,
                });
            }
            PackageItemKind::Tool => {
                let described = manifest
                    .as_ref()
                    .and_then(|m| m.tools.iter().find(|t| t.name == id));
                content.tools.push(AgentPackageTool {
                    description: described.map(|t| t.description.clone()).unwrap_or_default(),
                    family: described
                        .map(|t| crate::packages::family_name(t.action).to_owned())
                        .unwrap_or_default(),
                    name: id,
                });
            }
            PackageItemKind::Connection => content.connections_brought.push(id),
        }
    }
    if let Some(manifest) = manifest {
        content.connections_required = manifest
            .requires
            .into_iter()
            .map(|r| AgentPackageRequirement {
                kind: r.kind,
                reason: r.reason,
            })
            .collect();
    }
    Ok(content)
}

/// Who published `package`: what its installation recorded, the publisher
/// named by the keyring when its key is trusted now. A package installed
/// from a folder recorded no publisher; `author` is then the name its
/// manifest gives.
///
/// # Errors
///
/// [`PackageRepositoryError`] when the store cannot be read.
pub(crate) fn signature(
    package: &InstalledPackage,
    author: &str,
    packages: &PackageRepository,
    trusted_keys: Option<&TrustedKeyRepository>,
) -> Result<AgentPackageSignature, PackageRepositoryError> {
    let recorded = packages.signature(&package.name)?.unwrap_or_default();
    let key = match (&recorded.key_fingerprint, trusted_keys) {
        (Some(fingerprint), Some(keys)) if recorded.signed => keys.get(fingerprint).ok().flatten(),
        _ => None,
    };
    let declared = if recorded.publisher.is_empty() {
        author.to_owned()
    } else {
        recorded.publisher
    };
    Ok(AgentPackageSignature {
        publisher: key
            .as_ref()
            .filter(|key| !key.label.is_empty())
            .map_or(declared, |key| key.label.clone()),
        signed: recorded.signed,
        trusted: key.is_some(),
        key_fingerprint: recorded.key_fingerprint,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN a mission of several lines, a long one, and an empty one
    // WHEN each is summarised
    // THEN the first line is kept, cut at the limit, and nothing stays nothing
    #[test]
    fn test_the_summary_of_a_mission_is_its_first_line_cut_at_the_limit() {
        let several = "\n  Track the leads.  \nNever send anything.";
        // A character of two bytes: the cut counts characters, not bytes.
        let long = "\u{e9}".repeat(AGENT_SUMMARY_CHARS + 20);

        let short = summary(several);
        let cut = summary(&long);
        let none = summary("  \n ");

        assert_eq!(short, "Track the leads.");
        assert_eq!(cut.chars().count(), AGENT_SUMMARY_CHARS + 1);
        assert!(cut.ends_with('…'));
        assert_eq!(none, "");
    }
}
