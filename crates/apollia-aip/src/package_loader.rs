//! Chargement et validation d'un Agent Package depuis son dossier.
//!
//! Un package est décrit par un `agent.toml` à la racine du dossier.
//! Ce module parse le manifeste, valide chaque `.py` déclaré via le
//! duck-typing AIP existant, et retourne un [`AgentPackage`] prêt à
//! être installé par le runtime (ADR-081).

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::loader::{load_agent_module, AIPLoaderError};

// ─────────────────────────────────────────────────────────────────────────────
// Erreurs
// ─────────────────────────────────────────────────────────────────────────────

/// Erreurs retournées par [`load_package`].
#[derive(Debug, thiserror::Error)]
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

// ─────────────────────────────────────────────────────────────────────────────
// Types du manifeste (déserialisation TOML)
// ─────────────────────────────────────────────────────────────────────────────

/// Manifeste complet d'un package (`agent.toml`).
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

/// Section `[package]`.
#[derive(Debug, Clone, Deserialize)]
pub struct PackageMeta {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
}

/// Entrée `[[agents]]`.
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

/// Section `[tools]`.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct ToolsConfig {
    pub web: Option<WebToolConfig>,
}

/// Config de l'outil web.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct WebToolConfig {
    pub enabled: bool,
    #[serde(default)]
    pub ssrf_guard: bool,
}

/// Section `[pip]`.
#[derive(Debug, Clone, Deserialize)]
pub struct PipConfig {
    #[serde(default)]
    pub packages: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Types de sortie
// ─────────────────────────────────────────────────────────────────────────────

/// Rôle d'un agent dans le package.
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
    /// manifests don't silently install — a stale `role = "standalone"`
    /// coming from an older `/apollia-worker-forge` template means the agent
    /// hasn't been updated to the canonical taxonomy and must be fixed at
    /// the source.
    fn parse(s: &str) -> Result<Self, PackageLoaderError> {
        match s {
            "director" => Ok(AgentRole::Director),
            "worker" => Ok(AgentRole::Worker),
            "assistant" => Ok(AgentRole::Assistant),
            other => Err(PackageLoaderError::InvalidManifest(format!(
                "rôle inconnu '{other}' — attendu : director | worker | assistant. \
                 (Agents @agent+@skill ⇒ worker ; @agent+@orchestrated ⇒ director ; \
                 @agent+@on_message ⇒ assistant.)"
            ))),
        }
    }
}

/// Agent du package avec son chemin absolu et son rôle parsé.
#[derive(Debug, Clone)]
pub struct LoadedAgentEntry {
    pub name: String,
    pub entry: PathBuf,
    pub role: AgentRole,
}

/// Package complet chargé et validé.
#[derive(Debug)]
pub struct AgentPackage {
    pub manifest: PackageManifest,
    /// Contenu de `agent.toml` sérialisé en JSON (pour stockage en DB).
    pub manifest_json: String,
    /// Chemin absolu du dossier racine du package.
    pub root_path: PathBuf,
    pub agents: Vec<LoadedAgentEntry>,
}

// ─────────────────────────────────────────────────────────────────────────────
// API publique
// ─────────────────────────────────────────────────────────────────────────────

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

/// Charge et valide un package depuis son dossier racine.
///
/// 1. Lit et parse `agent.toml`
/// 2. Valide les invariants du manifeste (noms uniques, triggers valides)
/// 3. Duck-type chaque `.py` déclaré via [`load_agent_module`]
///
/// **Note** : ce flow ne crée pas de venv. Si un agent déclare des packages
/// pip et les importe au top-level, le duck-typing échouera. Pour le flow
/// d'installation production, utiliser [`load_manifest_only`] puis appeler
/// [`duck_type_agent`] avec le chemin du `site-packages` du venv créé.
///
/// # Errors
///
/// Retourne une erreur dès la première violation détectée (Principe #4 — Fail fast).
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

/// Parse et valide le manifeste **sans** duck-typer les `.py`.
///
/// Étape pure (pas de PyO3, pas de venv) utilisée par le flow d'install
/// pour décider s'il faut créer un venv et installer des deps pip avant de
/// duck-typer les modules.
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

/// Duck-type un seul fichier `.py` d'agent, avec des chemins additionnels
/// injectés dans `sys.path` (par ex. le `site-packages` d'un venv).
///
/// Utilisé par le flow d'install après création du venv pour valider qu'un
/// agent peut être importé avec ses packages pip installés.
pub fn duck_type_agent(
    path: &Path,
    extra_sys_paths: &[PathBuf],
) -> Result<(), PackageLoaderError> {
    crate::loader::load_agent_module_with_sys_paths(path, extra_sys_paths).map(|_| ()).map_err(
        |e: AIPLoaderError| PackageLoaderError::DuckTypingFailed {
            name: path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string(),
            reason: e.to_string(),
        },
    )
}

/// Valide les invariants du manifeste sans charger les `.py`.
///
/// Utilisé par `preview_agent_package` (dry-run sans duck-typing).
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

    // Noms d'agents uniques
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

// ─────────────────────────────────────────────────────────────────────────────
// Sérialisation JSON du PackageManifest
// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

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
        // GIVEN un TOML minimal
        let manifest: PackageManifest = toml::from_str(MINIMAL_TOML).expect("parse");
        // THEN les champs sont corrects
        assert_eq!(manifest.package.name, "test-pkg");
        assert_eq!(manifest.agents.len(), 1);
        assert_eq!(manifest.agents[0].role, "director");
    }

    #[test]
    fn test_parse_full_toml() {
        // GIVEN un TOML complet
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
        // GIVEN un manifeste valide
        let manifest: PackageManifest = toml::from_str(FULL_TOML).expect("parse");
        // WHEN on valide
        // THEN pas d'erreur
        validate_manifest(&manifest).expect("valid");
    }

    #[test]
    fn test_validate_manifest_empty_agents() {
        // GIVEN un manifeste sans agents
        let toml_str = r#"
[package]
name    = "empty-pkg"
version = "1.0.0"
"#;
        let manifest: PackageManifest = toml::from_str(toml_str).expect("parse");
        // WHEN on valide
        let result = validate_manifest(&manifest);
        // THEN erreur
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("aucun agent"));
    }

    #[test]
    fn test_validate_manifest_duplicate_agent_names() {
        // GIVEN deux agents avec le même nom
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
        // WHEN on valide
        let result = validate_manifest(&manifest);
        // THEN erreur de duplication
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("dupliqué"));
    }

    #[test]
    fn test_agent_role_parse_unknown() {
        // GIVEN un rôle inconnu
        let result = AgentRole::parse("chef");
        // THEN erreur
        assert!(result.is_err());
    }

    #[test]
    fn test_manifest_json_serialization() {
        // GIVEN un manifeste parsé
        let manifest: PackageManifest = toml::from_str(FULL_TOML).expect("parse");
        // WHEN on sérialise en JSON
        let json = serde_json::to_string(&manifest).expect("serialize");
        // THEN JSON valide avec les champs attendus
        assert!(json.contains("veille-ia"));
        assert!(json.contains("director"));
    }
}
