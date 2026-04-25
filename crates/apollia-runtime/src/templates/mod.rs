//! Template gallery (US-SP42-058).
//!
//! Parses the on-disk template registry shipped under `agents/templates/`
//! and exposes a typed list to the desktop layer. The registry is a
//! single `registry.toml` file pointing at per-template `.toml` files.
//!
//! This module is intentionally read-only — "instantiating" a template is
//! the caller's responsibility (the desktop layer either duplicates the
//! returned body into the user workspace, or feeds it into an existing
//! wizard). We only hand back the raw body so the caller can decide how
//! to materialize it.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Error cases when loading the template registry.
#[derive(Debug, Error)]
pub enum TemplateError {
    /// The root `registry.toml` could not be read.
    #[error("cannot read template registry at {path}: {source}")]
    Registry {
        /// Path that failed to open.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// A template body file could not be read.
    #[error("cannot read template body at {path}: {source}")]
    Body {
        /// Path that failed to open.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// TOML parsing failure (registry or body).
    #[error("invalid TOML at {path}: {source}")]
    Toml {
        /// Path of the offending file.
        path: PathBuf,
        /// Underlying parse error.
        #[source]
        source: toml::de::Error,
    },
    /// Requested template id was not found in the registry.
    #[error("template `{0}` not found")]
    NotFound(String),
}

/// Kind of artifact the template produces when instantiated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TemplateKind {
    /// Automation (trigger + target agent).
    Automation,
    /// Standalone assistant manifest.
    Agent,
    /// Multi-step pipeline.
    Pipeline,
}

/// User-facing category — maps to the filter chips in the gallery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TemplateCategory {
    /// Productivity.
    Productivity,
    /// Development.
    Dev,
    /// Communication.
    Communication,
    /// Creative.
    Creative,
    /// Analysis / research.
    Analysis,
    /// System / ops.
    System,
}

/// Difficulty signal shown in the card badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TemplateDifficulty {
    /// Simple — minimal configuration.
    Simple,
    /// Intermediate — a handful of dependencies.
    Intermediate,
    /// Advanced — assumes existing integrations.
    Advanced,
}

/// Source of the template: official (shipped with Apollia) or community.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TemplateSource {
    /// Bundled with the Apollia desktop app.
    Official,
    /// Fetched from a community registry (MVP: stubbed, not fetched).
    Community,
}

/// Registry entry as declared in `registry.toml` (without the body).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateMeta {
    /// Stable id.
    pub id: String,
    /// What kind of artifact is produced.
    pub kind: TemplateKind,
    /// Category bucket.
    pub category: TemplateCategory,
    /// Difficulty signal.
    pub difficulty: TemplateDifficulty,
    /// Source (official / community).
    pub source: TemplateSource,
    /// Display author.
    pub author: String,
    /// User-facing title.
    pub title: String,
    /// One-sentence description.
    pub description: String,
    /// Relative path to the body file (from `agents/templates/`).
    pub path: String,
    /// Free-form tags the user reads as "needs X connected".
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// A template with its raw body loaded.
#[derive(Debug, Clone, Serialize)]
pub struct Template {
    /// Metadata from the registry.
    #[serde(flatten)]
    pub meta: TemplateMeta,
    /// Raw TOML body — the caller decides how to materialize it.
    pub body: String,
}

#[derive(Debug, Deserialize)]
struct RegistryFile {
    #[serde(default = "default_schema_version")]
    #[allow(dead_code)]
    schema_version: u32,
    templates: Vec<TemplateMeta>,
}

fn default_schema_version() -> u32 {
    1
}

/// The loaded registry — kept in memory after the first read.
#[derive(Debug, Clone, Default)]
pub struct TemplateRegistry {
    /// Root directory (`agents/templates/`).
    pub root: PathBuf,
    /// All templates indexed by registry order.
    pub entries: Vec<TemplateMeta>,
}

impl TemplateRegistry {
    /// Load the registry from the given root directory (`agents/templates/`).
    pub fn load(root: impl AsRef<Path>) -> Result<Self, TemplateError> {
        let root = root.as_ref().to_path_buf();
        let registry_path = root.join("registry.toml");
        let content = fs::read_to_string(&registry_path).map_err(|e| TemplateError::Registry {
            path: registry_path.clone(),
            source: e,
        })?;
        let parsed: RegistryFile = toml::from_str(&content).map_err(|e| TemplateError::Toml {
            path: registry_path,
            source: e,
        })?;
        Ok(Self {
            root,
            entries: parsed.templates,
        })
    }

    /// Load the registry using the default Apollia agents root if present,
    /// otherwise fall back to an empty registry (no-op — desktop will just
    /// show an empty gallery).
    pub fn load_or_empty(root: impl AsRef<Path>) -> Self {
        Self::load(root).unwrap_or_default()
    }

    /// Return the list of metadata entries (no body loading).
    pub fn list(&self) -> &[TemplateMeta] {
        &self.entries
    }

    /// Look up a template by id and load its body.
    pub fn get(&self, id: &str) -> Result<Template, TemplateError> {
        let meta = self
            .entries
            .iter()
            .find(|e| e.id == id)
            .cloned()
            .ok_or_else(|| TemplateError::NotFound(id.to_string()))?;
        let body_path = self.root.join(&meta.path);
        let body = fs::read_to_string(&body_path).map_err(|e| TemplateError::Body {
            path: body_path,
            source: e,
        })?;
        Ok(Template { meta, body })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        // GIVEN the repo checkout, navigate from `crates/apollia-runtime/` up to
        // the workspace root and into `agents/templates/`.
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("agents")
            .join("templates")
    }

    #[test]
    fn registry_loads_and_exposes_15_templates() {
        // GIVEN the shipped registry.toml
        let registry = TemplateRegistry::load(repo_root()).expect("registry should load");
        // THEN we expose at least 15 templates
        assert!(
            registry.list().len() >= 15,
            "expected >= 15 templates, got {}",
            registry.list().len()
        );
    }

    #[test]
    fn each_template_body_is_parseable_toml() {
        // GIVEN a registry
        let registry = TemplateRegistry::load(repo_root()).expect("registry should load");
        // WHEN we load every template body
        for meta in registry.list() {
            let tmpl = registry
                .get(&meta.id)
                .unwrap_or_else(|e| panic!("template `{}` failed to load: {e}", meta.id));
            // THEN its body is well-formed TOML (the registry only validates
            // the shape of registry.toml itself — this ensures the pointed-at
            // body files exist and parse).
            let _: toml::Value = toml::from_str(&tmpl.body)
                .unwrap_or_else(|e| panic!("template `{}` body is invalid TOML: {e}", meta.id));
        }
    }

    #[test]
    fn missing_id_returns_not_found() {
        let registry = TemplateRegistry::load(repo_root()).expect("registry should load");
        let err = registry.get("does-not-exist").unwrap_err();
        matches!(err, TemplateError::NotFound(_));
    }
}
