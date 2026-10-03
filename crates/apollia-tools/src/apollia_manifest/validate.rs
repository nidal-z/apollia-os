//! The rules an `apollia.toml` must hold beyond its schema.

use std::collections::HashSet;

use super::{ApolliaManifest, ApolliaManifestError, PACKAGE_TOOL_PREFIX};

/// `true` when `value` matches `[a-z0-9_-]+`: the alphabet of a package name,
/// an item identifier, a trigger identifier, and an MCP server name (the same
/// rule `apollia_mcp::config::is_valid_server_name` applies).
pub fn is_valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

fn check_identifier(field: &str, value: &str) -> Result<(), ApolliaManifestError> {
    if is_valid_identifier(value) {
        Ok(())
    } else {
        Err(ApolliaManifestError::InvalidIdentifier {
            field: field.to_string(),
            value: value.to_string(),
        })
    }
}

impl ApolliaManifest {
    /// Check the rules the schema alone cannot express.
    ///
    /// - the package delivers at least one agent, one workflow or one tool;
    /// - the package name and every identifier match `[a-z0-9_-]+`;
    /// - `version` is semver and `sdk` a semver requirement;
    /// - agents, tools and workflows share one namespace without duplicates,
    ///   and trigger identifiers and MCP server names are unique too;
    /// - a default access `pkg:<package>/<tool>` names a tool of this package;
    /// - an MCP server name matches `[a-z0-9_-]+`;
    /// - a proposed trigger targets an agent or a workflow of this package.
    ///
    /// # Errors
    ///
    /// The variant of [`ApolliaManifestError`] naming the first rule broken.
    pub fn validate(&self) -> Result<(), ApolliaManifestError> {
        if self.agents.is_empty() && self.workflows.is_empty() && self.tools.is_empty() {
            return Err(ApolliaManifestError::EmptyPackage);
        }

        let package = &self.package;
        check_identifier("package.name", &package.name)?;
        if semver::Version::parse(&package.version).is_err() {
            return Err(ApolliaManifestError::InvalidVersion {
                value: package.version.clone(),
            });
        }
        if semver::VersionReq::parse(&package.sdk).is_err() {
            return Err(ApolliaManifestError::InvalidSdkRequirement {
                value: package.sdk.clone(),
            });
        }

        let items = self
            .agents
            .iter()
            .map(|a| ("agents.id", a.id.as_str()))
            .chain(self.tools.iter().map(|t| ("tools.name", t.name.as_str())))
            .chain(
                self.workflows
                    .iter()
                    .map(|w| ("workflows.name", w.name.as_str())),
            );
        unique(items)?;
        unique(self.triggers.iter().map(|t| ("triggers.id", t.id.as_str())))?;

        for server in &self.mcp_servers {
            if !is_valid_identifier(&server.name) {
                return Err(ApolliaManifestError::InvalidMcpServerName {
                    name: server.name.clone(),
                });
            }
        }
        unique(
            self.mcp_servers
                .iter()
                .map(|s| ("mcp_servers.name", s.name.as_str())),
        )?;

        self.check_package_access()?;
        self.check_trigger_targets()
    }

    fn check_package_access(&self) -> Result<(), ApolliaManifestError> {
        let tools: HashSet<&str> = self.tools.iter().map(|t| t.name.as_str()).collect();
        for agent in &self.agents {
            for access in &agent.defaults.access {
                let Some(rest) = access.strip_prefix(PACKAGE_TOOL_PREFIX) else {
                    continue;
                };
                let names_own_tool = rest
                    .split_once('/')
                    .is_some_and(|(pkg, tool)| pkg == self.package.name && tools.contains(tool));
                if !names_own_tool {
                    return Err(ApolliaManifestError::UnknownPackageTool {
                        agent: agent.id.clone(),
                        access: access.clone(),
                        package: self.package.name.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    fn check_trigger_targets(&self) -> Result<(), ApolliaManifestError> {
        let targets: HashSet<&str> = self
            .agents
            .iter()
            .map(|a| a.id.as_str())
            .chain(self.workflows.iter().map(|w| w.name.as_str()))
            .collect();
        for trigger in &self.triggers {
            if !targets.contains(trigger.target.as_str()) {
                return Err(ApolliaManifestError::UnknownTriggerTarget {
                    trigger: trigger.id.clone(),
                    target: trigger.target.clone(),
                });
            }
        }
        Ok(())
    }
}

/// Every value is a valid identifier and appears once.
fn unique<'a>(
    values: impl Iterator<Item = (&'static str, &'a str)>,
) -> Result<(), ApolliaManifestError> {
    let mut seen = HashSet::new();
    for (field, value) in values {
        check_identifier(field, value)?;
        if !seen.insert(value) {
            return Err(ApolliaManifestError::DuplicateId {
                id: value.to_string(),
            });
        }
    }
    Ok(())
}
