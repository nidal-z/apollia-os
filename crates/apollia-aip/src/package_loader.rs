//! Loading and validation of an Agent Package from its directory.
//!
//! A package is described by an `agent.toml` at the directory root. This
//! module parses the manifest, validates each declared `.py` via the existing
//! AIP duck-typing, and returns an [`AgentPackage`] ready to be installed by
//! the runtime.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::loader::{load_agent_module, AIPLoaderError};

// Errors

/// Errors returned by [`load_package`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PackageLoaderError {
    #[error("agent.toml introuvable dans '{0}'")]
    ManifestNotFound(String),

    #[error("lecture agent.toml : {0}")]
    Io(#[from] std::io::Error),

    #[error("parsing agent.toml : {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("sérialisation JSON : {0}")]
    Json(#[from] serde_json::Error),

    #[error("agent '{name}' : entrée introuvable : {path}")]
    EntryNotFound { name: String, path: String },

    #[error("agent '{name}' : échec duck-typing AIP : {reason}")]
    DuckTypingFailed { name: String, reason: String },

    #[error("agent.toml invalide : {0}")]
    InvalidManifest(String),
}

// Manifest types (TOML deserialization)

/// Complete package manifest (`agent.toml`).
#[derive(Debug, Clone, Deserialize)]
pub struct PackageManifest {
    pub package: PackageMeta,
    #[serde(default)]
    pub agents: Vec<AgentEntry>,
    #[serde(default)]
    pub tools: Option<ToolsConfig>,
    #[serde(default)]
    pub triggers: Vec<toml::Value>,
    #[serde(default)]
    pub pip: Option<PipConfig>,
}

/// `[package]` section.
#[derive(Debug, Clone, Deserialize)]
pub struct PackageMeta {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
}

/// `[[agents]]` entry.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentEntry {
    pub name: String,
    pub entry: String,
    pub role: String,
    /// Per-agent pip packages. Combined with the top-level `[pip].packages`
    /// list at install time. Pins are recommended (`pkg==X.Y.Z`).
    #[serde(default)]
    pub packages: Vec<String>,
}

/// `[tools]` section.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct ToolsConfig {
    pub web: Option<WebToolConfig>,
}

/// Web tool config.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct WebToolConfig {
    pub enabled: bool,
    #[serde(default)]
    pub ssrf_guard: bool,
}

/// `[pip]` section.
#[derive(Debug, Clone, Deserialize)]
pub struct PipConfig {
    #[serde(default)]
    pub packages: Vec<String>,
}

// Output types

/// Role of an agent within the package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRole {
    Director,
    Worker,
    Assistant,
}

impl std::fmt::Display for AgentRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentRole::Director => write!(f, "director"),
            AgentRole::Worker => write!(f, "worker"),
            AgentRole::Assistant => write!(f, "assistant"),
        }
    }
}

impl AgentRole {
    /// Parse `role` from `[[agents]]`. The SDK exposes exactly three roles
    /// (mirroring the four scaffolding templates: react / conversational /
    /// orchestrated all share `assistant`-style structure, `worker` keeps its
    /// own). Reject every other value with an explicit error so malformed
    /// manifests don't silently install: a stale `role = "standalone"`
    /// coming from an older worker-forge template means the agent
    /// hasn't been updated to the canonical taxonomy and must be fixed at
    /// the source.
    fn parse(s: &str) -> Result<Self, PackageLoaderError> {
        match s {
            "director" => Ok(AgentRole::Director),
            "worker" => Ok(AgentRole::Worker),
            "assistant" => Ok(AgentRole::Assistant),
            other => Err(PackageLoaderError::InvalidManifest(format!(
                "rôle inconnu '{other}' - attendu : director | worker | assistant. \
                 (Agents @agent+@skill ⇒ worker ; @agent+@orchestrated ⇒ director ; \
                 @agent+@on_message ⇒ assistant.)"
            ))),
        }
    }
}

/// Package agent with its absolute path and parsed role.
#[derive(Debug, Clone)]
pub struct LoadedAgentEntry {
    pub name: String,
    pub entry: PathBuf,
    pub role: AgentRole,
}

/// Fully loaded and validated package.
#[derive(Debug)]
pub struct AgentPackage {
    pub manifest: PackageManifest,
    /// Contents of `agent.toml` serialized to JSON (for DB storage).
    pub manifest_json: String,
    /// Absolute path of the package root directory.
    pub root_path: PathBuf,
    pub agents: Vec<LoadedAgentEntry>,
}

// Public API

impl PackageManifest {
    /// Returns the union of pip packages declared in the top-level `[pip]`
    /// section and in any `[[agents]].packages` field, deduplicated while
    /// preserving first-seen order.
    pub fn all_pip_packages(&self) -> Vec<String> {
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut out: Vec<String> = Vec::new();
        if let Some(pip) = &self.pip {
            for p in &pip.packages {
                if seen.insert(p.clone()) {
                    out.push(p.clone());
                }
            }
        }
        for agent in &self.agents {
            for p in &agent.packages {
                if seen.insert(p.clone()) {
                    out.push(p.clone());
                }
            }
        }
        out
    }

    /// Returns the pip packages relevant for a specific agent: top-level
    /// `[pip].packages` plus that agent's own `packages` list (dedup, order
    /// preserved). Returns an empty vec if the agent name is unknown.
    pub fn agent_pip_packages(&self, agent_name: &str) -> Vec<String> {
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut out: Vec<String> = Vec::new();
        if let Some(pip) = &self.pip {
            for p in &pip.packages {
                if seen.insert(p.clone()) {
                    out.push(p.clone());
                }
            }
        }
        if let Some(agent) = self.agents.iter().find(|a| a.name == agent_name) {
            for p in &agent.packages {
                if seen.insert(p.clone()) {
                    out.push(p.clone());
                }
            }
        }
        out
    }
}

/// Loads and validates a package from its root directory.
///
/// 1. Reads and parses `agent.toml`
/// 2. Validates the manifest invariants (unique names, valid triggers)
/// 3. Duck-types each declared `.py` via [`load_agent_module`]
///
/// **Note**: this flow does not create a venv. If an agent declares pip
/// packages and imports them at the top level, duck-typing will fail. For the
/// production install flow, use [`load_manifest_only`] then call
/// [`duck_type_agent`] with the `site-packages` path of the created venv.
///
/// # Errors
///
/// Returns an error on the first detected violation (fail fast).
pub fn load_package(root: &Path) -> Result<AgentPackage, PackageLoaderError> {
    let pkg = load_manifest_only(root)?;
    for entry in &pkg.agents {
        load_agent_module(&entry.entry).map_err(|e: AIPLoaderError| {
            PackageLoaderError::DuckTypingFailed {
                name: entry.name.clone(),
                reason: e.to_string(),
            }
        })?;
    }
    Ok(pkg)
}

/// Parses and validates the manifest **without** duck-typing the `.py` files.
///
/// Pure step (no PyO3, no venv) used by the install flow to decide whether a
/// venv must be created and pip deps installed before duck-typing the modules.
pub fn load_manifest_only(root: &Path) -> Result<AgentPackage, PackageLoaderError> {
    let toml_path = root.join("agent.toml");
    if !toml_path.exists() {
        return Err(PackageLoaderError::ManifestNotFound(
            root.display().to_string(),
        ));
    }

    let toml_str = std::fs::read_to_string(&toml_path)?;
    let manifest: PackageManifest = toml::from_str(&toml_str)?;

    validate_manifest(&manifest)?;

    let manifest_json = serde_json::to_string(&manifest)?;

    let mut agents = Vec::with_capacity(manifest.agents.len());
    for entry in &manifest.agents {
        let abs_path = root.join(&entry.entry);
        if !abs_path.exists() {
            return Err(PackageLoaderError::EntryNotFound {
                name: entry.name.clone(),
                path: abs_path.display().to_string(),
            });
        }

        agents.push(LoadedAgentEntry {
            name: entry.name.clone(),
            entry: abs_path,
            role: AgentRole::parse(&entry.role)?,
        });
    }

    Ok(AgentPackage {
        manifest_json,
        manifest,
        root_path: root.to_path_buf(),
        agents,
    })
}

/// Duck-types a single agent `.py` file, with additional paths injected into
/// `sys.path` (e.g. a venv's `site-packages`).
///
/// Used by the install flow after venv creation to validate that an agent can
/// be imported with its pip packages installed.
pub fn duck_type_agent(path: &Path, extra_sys_paths: &[PathBuf]) -> Result<(), PackageLoaderError> {
    crate::loader::load_agent_module_with_sys_paths(path, extra_sys_paths)
        .map(|_| ())
        .map_err(|e: AIPLoaderError| PackageLoaderError::DuckTypingFailed {
            name: path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string(),
            reason: e.to_string(),
        })
}

/// Validates the manifest invariants without loading the `.py` files.
///
/// Used by `preview_agent_package` (dry-run without duck-typing).
pub fn validate_manifest(manifest: &PackageManifest) -> Result<(), PackageLoaderError> {
    if manifest.package.name.is_empty() {
        return Err(PackageLoaderError::InvalidManifest(
            "[package].name est vide".into(),
        ));
    }
    if manifest.package.version.is_empty() {
        return Err(PackageLoaderError::InvalidManifest(
            "[package].version est vide".into(),
        ));
    }
    if manifest.agents.is_empty() {
        return Err(PackageLoaderError::InvalidManifest(
            "aucun agent déclaré dans [[agents]]".into(),
        ));
    }

    // Unique agent names
    let mut seen = std::collections::HashSet::new();
    for a in &manifest.agents {
        if !seen.insert(&a.name) {
            return Err(PackageLoaderError::InvalidManifest(format!(
                "nom d'agent dupliqué : '{}'",
                a.name
            )));
        }
    }

    Ok(())
}

// JSON serialization of PackageManifest

impl serde::Serialize for PackageManifest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("PackageManifest", 5)?;
        s.serialize_field("package", &self.package)?;
        s.serialize_field("agents", &self.agents)?;
        s.serialize_field("tools", &self.tools)?;
        s.serialize_field("pip", &self.pip.as_ref().map(|p| &p.packages))?;
        s.end()
    }
}

impl serde::Serialize for PackageMeta {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("PackageMeta", 4)?;
        s.serialize_field("name", &self.name)?;
        s.serialize_field("version", &self.version)?;
        s.serialize_field("description", &self.description)?;
        s.serialize_field("author", &self.author)?;
        s.end()
    }
}

impl serde::Serialize for AgentEntry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AgentEntry", 4)?;
        s.serialize_field("name", &self.name)?;
        s.serialize_field("entry", &self.entry)?;
        s.serialize_field("role", &self.role)?;
        s.serialize_field("packages", &self.packages)?;
        s.end()
    }
}

impl serde::Serialize for PipConfig {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("PipConfig", 1)?;
        s.serialize_field("packages", &self.packages)?;
        s.end()
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_TOML: &str = r#"
[package]
name    = "test-pkg"
version = "1.0.0"

[[agents]]
name  = "my-agent"
entry = "agent.py"
role  = "director"
"#;

    const FULL_TOML: &str = r#"
[package]
name        = "veille-ia"
version     = "1.0.0"
description = "Veille IA/LLM"
author      = "Apollia OS"

[[agents]]
name  = "veille-ia-agent"
entry = "veille-ia-agent.py"
role  = "director"

[[agents]]
name  = "web-search-worker"
entry = "workers/web-search-worker.py"
role  = "worker"

[tools]
web = { enabled = true, ssrf_guard = true }

[[triggers]]
id             = "daily-veille-ia"
agent          = "veille-ia-agent"
enabled        = true
on_busy        = "skip"
input_template = "Génère la veille IA/LLM du jour"

[triggers.source]
type     = "cron"
schedule = "0 7 * * 1-5"

[pip]
packages = ["httpx>=0.27"]
"#;

    #[test]
    fn test_parse_minimal_toml() {
        // GIVEN a minimal TOML
        let manifest: PackageManifest = toml::from_str(MINIMAL_TOML).expect("parse");
        // THEN the fields are correct
        assert_eq!(manifest.package.name, "test-pkg");
        assert_eq!(manifest.agents.len(), 1);
        assert_eq!(manifest.agents[0].role, "director");
    }

    #[test]
    fn test_parse_full_toml() {
        // GIVEN a full TOML
        let manifest: PackageManifest = toml::from_str(FULL_TOML).expect("parse");
        // THEN
        assert_eq!(manifest.package.name, "veille-ia");
        assert_eq!(manifest.agents.len(), 2);
        assert!(manifest.tools.is_some());
        assert!(manifest.pip.is_some());
        assert_eq!(manifest.pip.unwrap().packages[0], "httpx>=0.27");
    }

    #[test]
    fn test_validate_manifest_ok() {
        // GIVEN a valid manifest
        let manifest: PackageManifest = toml::from_str(FULL_TOML).expect("parse");
        // WHEN we validate
        // THEN no error
        validate_manifest(&manifest).expect("valid");
    }

    #[test]
    fn test_validate_manifest_empty_agents() {
        // GIVEN a manifest with no agents
        let toml_str = r#"
[package]
name    = "empty-pkg"
version = "1.0.0"
"#;
        let manifest: PackageManifest = toml::from_str(toml_str).expect("parse");
        // WHEN we validate
        let result = validate_manifest(&manifest);
        // THEN error
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("aucun agent"));
    }

    #[test]
    fn test_validate_manifest_duplicate_agent_names() {
        // GIVEN two agents with the same name
        let toml_str = r#"
[package]
name    = "dup-pkg"
version = "1.0.0"

[[agents]]
name  = "same"
entry = "a.py"
role  = "director"

[[agents]]
name  = "same"
entry = "b.py"
role  = "worker"
"#;
        let manifest: PackageManifest = toml::from_str(toml_str).expect("parse");
        // WHEN we validate
        let result = validate_manifest(&manifest);
        // THEN duplication error
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("dupliqué"));
    }

    #[test]
    fn test_agent_role_parse_unknown() {
        // GIVEN an unknown role
        let result = AgentRole::parse("chef");
        // THEN error
        assert!(result.is_err());
    }

    #[test]
    fn test_manifest_json_serialization() {
        // GIVEN a parsed manifest
        let manifest: PackageManifest = toml::from_str(FULL_TOML).expect("parse");
        // WHEN we serialize to JSON
        let json = serde_json::to_string(&manifest).expect("serialize");
        // THEN valid JSON with the expected fields
        assert!(json.contains("veille-ia"));
        assert!(json.contains("director"));
    }
}
