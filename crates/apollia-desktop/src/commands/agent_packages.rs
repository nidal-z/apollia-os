//! Tauri IPC commands for managing agent packages.
//!
//! Commands: list, detail, preview (dry-run), install, uninstall.
//! Heavy operations (file copies, Python duck-typing) run via `spawn_blocking`
//! so they do not block Tauri's async thread.
//!
//! The two long flows live in their own modules: `install` for the copy, the
//! virtualenvs and the trigger injection, `uninstall` for the reverse.

pub mod install;
pub mod uninstall;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use apollia_aip::package_loader::validate_manifest;
use apollia_tools::PackageRepository;
use serde::{Deserialize, Serialize};
use tauri::State;

// ─────────────────────────────────────────────────────────────────────────────
// Response types
// ─────────────────────────────────────────────────────────────────────────────

/// Summary of an agent within a package.
#[derive(Debug, Serialize)]
pub struct PackageAgentSummary {
    pub name: String,
    pub role: String,
    pub entry: String,
}

/// Preview of a trigger (dry-run, no strict validation).
#[derive(Debug, Serialize)]
pub struct TriggerPreview {
    pub id: String,
    pub source_type: String,
    pub agent: String,
    /// Cron expression (cron triggers).
    pub schedule: Option<String>,
    /// Interval (interval triggers).
    pub every: Option<String>,
    /// Watched path (file_watch triggers).
    pub path: Option<String>,
    /// Whether this trigger needs extra configuration (e.g. webhook secret).
    pub needs_config: bool,
    pub enabled: bool,
}

/// Configuration override applied at install time (e.g. webhook secret).
#[derive(Debug, Serialize, Deserialize)]
pub struct TriggerConfigOverride {
    pub id: String,
    /// HMAC secret for webhooks.
    pub secret: Option<String>,
}

/// Item in the list of installed packages.
#[derive(Debug, Serialize)]
pub struct AgentPackageListItem {
    pub name: String,
    pub version: String,
    pub description: String,
    pub agent_count: usize,
    pub agents: Vec<PackageAgentSummary>,
    pub installed_at: String,
    pub root_path: String,
    pub root_missing: bool,
}

/// Full detail of a package.
#[derive(Debug, Serialize)]
pub struct AgentPackageDetailView {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub agents: Vec<PackageAgentSummary>,
    pub installed_at: String,
    pub updated_at: String,
    pub root_path: String,
    pub root_missing: bool,
    pub manifest: serde_json::Value,
}

/// Result of a preview (dry-run with no writes).
#[derive(Debug, Serialize)]
pub struct PackagePreview {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub agents: Vec<PackageAgentSummary>,
    pub triggers: Vec<TriggerPreview>,
    pub trigger_count: usize,
    pub pip_packages: Vec<String>,
    pub valid: bool,
    pub error: Option<String>,
}

/// Result of an installation.
#[derive(Debug, Serialize)]
pub struct InstallPackageResponse {
    pub name: String,
    pub version: String,
    pub agent_count: usize,
    pub trigger_count: usize,
    pub trigger_errors: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Raw TOML helpers (lenient, no semantic validation, for preview)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct RawPreviewRoot {
    #[serde(default)]
    triggers: Vec<RawPreviewTrigger>,
}

#[derive(Deserialize)]
struct RawPreviewTrigger {
    id: String,
    #[serde(default)]
    agent: String,
    #[serde(default = "bool_true")]
    enabled: bool,
    #[serde(default)]
    source: RawPreviewSource,
}

#[derive(Deserialize, Default)]
struct RawPreviewSource {
    #[serde(rename = "type", default)]
    kind: String,
    schedule: Option<String>,
    every: Option<String>,
    path: Option<String>,
    secret: Option<String>,
}

fn bool_true() -> bool {
    true
}

fn parse_trigger_previews(toml_str: &str) -> Vec<TriggerPreview> {
    let Ok(raw) = toml::from_str::<RawPreviewRoot>(toml_str) else {
        return vec![];
    };
    raw.triggers
        .into_iter()
        .map(|t| {
            let needs_config =
                t.source.kind == "webhook" && t.source.secret.as_deref().unwrap_or("").is_empty();
            TriggerPreview {
                id: t.id,
                source_type: if t.source.kind.is_empty() {
                    "cron".to_string()
                } else {
                    t.source.kind
                },
                agent: t.agent,
                schedule: t.source.schedule,
                every: t.source.every,
                path: t.source.path,
                needs_config,
                enabled: t.enabled,
            }
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Dry-run: parses `agent.toml` and validates the manifests without writing to the DB.
#[tauri::command]
pub async fn preview_agent_package(path: String) -> Result<PackagePreview, String> {
    let root = PathBuf::from(&path);
    let toml_path = root.join("agent.toml");

    let toml_str =
        std::fs::read_to_string(&toml_path).map_err(|e| format!("cannot read agent.toml: {e}"))?;

    let manifest: apollia_aip::package_loader::PackageManifest =
        toml::from_str(&toml_str).map_err(|e| format!("invalid TOML: {e}"))?;

    if let Err(e) = validate_manifest(&manifest) {
        return Ok(PackagePreview {
            name: manifest.package.name.clone(),
            version: manifest.package.version.clone(),
            description: manifest.package.description.clone(),
            author: manifest.package.author.clone(),
            agents: vec![],
            triggers: vec![],
            trigger_count: 0,
            pip_packages: vec![],
            valid: false,
            error: Some(e.to_string()),
        });
    }

    let triggers = parse_trigger_previews(&toml_str);
    let trigger_count = triggers.len();

    // Aggregate pip packages from both [pip] top-level AND per-agent [[agents]].packages.
    // Both placements are supported; per-agent is the apollia-worker-forge convention.
    let pip_packages = manifest.all_pip_packages();

    let agents = manifest
        .agents
        .iter()
        .map(|a| PackageAgentSummary {
            name: a.name.clone(),
            role: a.role.clone(),
            entry: a.entry.clone(),
        })
        .collect();

    Ok(PackagePreview {
        name: manifest.package.name,
        version: manifest.package.version,
        description: manifest.package.description,
        author: manifest.package.author,
        agents,
        triggers,
        trigger_count,
        pip_packages,
        valid: true,
        error: None,
    })
}

/// Lists all installed packages.
#[tauri::command]
pub async fn list_agent_packages(
    pkg_repo_state: State<'_, Arc<Mutex<PackageRepository>>>,
) -> Result<Vec<AgentPackageListItem>, String> {
    let pkg_repo = pkg_repo_state.lock().map_err(|_| "repo lock poisoned")?;
    let packages = pkg_repo
        .list()
        .map_err(|e| format!("database error: {e}"))?;

    let mut items = Vec::with_capacity(packages.len());
    for pkg in &packages {
        let manifest: serde_json::Value =
            serde_json::from_str(&pkg.manifest_json).unwrap_or_default();

        let agents: Vec<PackageAgentSummary> = manifest
            .get("agents")
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|a| PackageAgentSummary {
                        name: a
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        role: a
                            .get("role")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        entry: a
                            .get("entry")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let agent_count = agents.len();

        let description = manifest
            .get("package")
            .and_then(|p| p.get("description"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        items.push(AgentPackageListItem {
            name: pkg.name.clone(),
            version: pkg.version.clone(),
            description,
            agent_count,
            agents,
            installed_at: pkg.installed_at.clone(),
            root_path: pkg.root_path.to_string_lossy().to_string(),
            root_missing: !pkg.root_path.exists(),
        });
    }
    Ok(items)
}

/// Returns the full detail of a package.
#[tauri::command]
pub async fn get_agent_package_detail(
    name: String,
    pkg_repo_state: State<'_, Arc<Mutex<PackageRepository>>>,
) -> Result<AgentPackageDetailView, String> {
    let pkg_repo = pkg_repo_state.lock().map_err(|_| "repo lock poisoned")?;
    let pkg = pkg_repo
        .get(&name)
        .map_err(|e| format!("database error: {e}"))?
        .ok_or_else(|| format!("Package '{name}' not found"))?;

    let manifest: serde_json::Value = serde_json::from_str(&pkg.manifest_json).unwrap_or_default();

    let agents = manifest
        .get("agents")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .map(|a| PackageAgentSummary {
                    name: a
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    role: a
                        .get("role")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    entry: a
                        .get("entry")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    let pkg_meta = manifest.get("package");
    let description = pkg_meta
        .and_then(|p| p.get("description"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let author = pkg_meta
        .and_then(|p| p.get("author"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(AgentPackageDetailView {
        name: pkg.name.clone(),
        version: pkg.version.clone(),
        description,
        author,
        agents,
        installed_at: pkg.installed_at.clone(),
        updated_at: pkg.updated_at.clone(),
        root_path: pkg.root_path.to_string_lossy().to_string(),
        root_missing: !pkg.root_path.exists(),
        manifest,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn apollia_data_dir() -> PathBuf {
    let home = apollia_core::paths::home_dir_or_temp()
        .display()
        .to_string();
    apollia_core::paths::data_dir_under(home)
}

/// Locate the `site-packages` directory(ies) of a per-agent virtualenv.
///
/// Delegates to the canonical helper in `apollia-tools` so install-flow,
/// runtime backend (`backend.rs`) and CLI (`commands/start.rs`) all read
/// the same venv layout.
fn find_venv_site_packages(venv_path: &Path) -> Vec<PathBuf> {
    // The canonical helper takes (base, agent_name) and reconstructs
    // `<base>/<agent_name>/venv/`. We already have the resolved venv path,
    // so derive (base, agent_name) by walking up two parents.
    let venv_dir = match venv_path.parent() {
        Some(p) => p,
        None => return Vec::new(),
    };
    let base = match venv_dir.parent() {
        Some(p) => p,
        None => return Vec::new(),
    };
    let agent_name = match venv_dir.file_name().and_then(|s| s.to_str()) {
        Some(s) => s,
        None => return Vec::new(),
    };
    apollia_tools::tools::python_executor::agent_venv_site_packages(base, agent_name)
}

/// Load + validate an agent's Python module with extra sys.path entries
/// (typically the venv's `site-packages`). Returns the parsed AgentManifest.
///
/// Combines duck-typing and manifest extraction in a single PyO3 import to
/// avoid loading the module twice.
fn load_agent_manifest_with_sys_paths(
    py_path: &Path,
    extra_sys_paths: &[PathBuf],
) -> Result<apollia_core::AgentManifest, String> {
    let module = apollia_aip::loader::load_agent_module_with_sys_paths(py_path, extra_sys_paths)
        .map_err(|e| e.to_string())?;
    let validated = apollia_aip::validator::validate_agent(&module).map_err(|e| e.to_string())?;
    Ok(validated.manifest)
}
