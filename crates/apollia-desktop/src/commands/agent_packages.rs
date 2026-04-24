//! Tauri IPC commands pour la gestion des agent packages.
//!
//! Commandes : list, detail, preview (dry-run), install, uninstall.
//! Les opérations lourdes (copie de fichiers, duck-typing Python) sont exécutées
//! via `spawn_blocking` pour ne pas bloquer le thread async de Tauri.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use apollia_aip::package_loader::{load_package, validate_manifest, PackageLoaderError};
use apollia_tools::{AgentRepository, InstalledAgent, InstalledPackage, PackageRepository};
use apollia_triggers::{
    definition_repository::TriggerDefinitionRepository, parse_triggers_from_toml_str, OnBusy,
    OnBusyPolicy, TriggerDefinitionRow, TriggerSourceConfig,
};
use serde::Serialize;
use tauri::State;

// ─────────────────────────────────────────────────────────────────────────────
// Types de réponse
// ─────────────────────────────────────────────────────────────────────────────

/// Résumé d'un agent dans un package.
#[derive(Debug, Serialize)]
pub struct PackageAgentSummary {
    pub name: String,
    pub role: String,
    pub entry: String,
}

/// Élément de la liste des packages installés.
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

/// Détail complet d'un package.
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

/// Résultat d'un preview (dry-run sans écriture).
#[derive(Debug, Serialize)]
pub struct PackagePreview {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub agents: Vec<PackageAgentSummary>,
    pub trigger_count: usize,
    pub pip_packages: Vec<String>,
    pub valid: bool,
    pub error: Option<String>,
}

/// Résultat d'une installation.
#[derive(Debug, Serialize)]
pub struct InstallPackageResponse {
    pub name: String,
    pub version: String,
    pub agent_count: usize,
    pub trigger_count: usize,
}

// ─────────────────────────────────────────────────────────────────────────────
// Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Dry-run : parse `agent.toml` + valide les manifestes sans écrire en DB.
#[tauri::command]
pub async fn preview_agent_package(path: String) -> Result<PackagePreview, String> {
    let root = PathBuf::from(&path);
    let toml_path = root.join("agent.toml");

    let toml_str = std::fs::read_to_string(&toml_path)
        .map_err(|e| format!("cannot read agent.toml: {e}"))?;

    let manifest: apollia_aip::package_loader::PackageManifest =
        toml::from_str(&toml_str).map_err(|e| format!("invalid TOML: {e}"))?;

    if let Err(e) = validate_manifest(&manifest) {
        return Ok(PackagePreview {
            name: manifest.package.name.clone(),
            version: manifest.package.version.clone(),
            description: manifest.package.description.clone(),
            author: manifest.package.author.clone(),
            agents: vec![],
            trigger_count: 0,
            pip_packages: vec![],
            valid: false,
            error: Some(e.to_string()),
        });
    }

    let trigger_count = parse_triggers_from_toml_str(&toml_str)
        .map(|v| v.len())
        .unwrap_or(0);

    let pip_packages = manifest
        .pip
        .as_ref()
        .map(|p| p.packages.clone())
        .unwrap_or_default();

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
        trigger_count,
        pip_packages,
        valid: true,
        error: None,
    })
}

/// Installe un package depuis un chemin local.
#[tauri::command]
pub async fn install_agent_package(
    path: String,
    pkg_repo_state: State<'_, Arc<Mutex<PackageRepository>>>,
    agent_repo_state: State<'_, Arc<Mutex<AgentRepository>>>,
) -> Result<InstallPackageResponse, String> {
    let root = PathBuf::from(&path);
    let data_dir = apollia_data_dir();

    // Validate + duck-type in blocking thread (PyO3 operations).
    let root_clone = root.clone();
    let pkg = tokio::task::spawn_blocking(move || load_package(&root_clone))
        .await
        .map_err(|e| format!("spawn error: {e}"))?
        .map_err(|e| format!("package validation failed: {e}"))?;

    let pkg_name = pkg.manifest.package.name.clone();
    let pkg_version = pkg.manifest.package.version.clone();
    let install_root = data_dir.join("agents").join("packages").join(&pkg_name);

    // Copy directory.
    copy_dir_all(&root, &install_root)
        .map_err(|e| format!("failed to copy package: {e}"))?;

    let now = chrono::Utc::now().to_rfc3339();
    let mut agent_count = 0;

    {
        let agent_repo = agent_repo_state.lock().map_err(|_| "repo lock poisoned")?;
        for entry in &pkg.agents {
            let installed_entry = install_root.join(
                entry.entry.strip_prefix(&root).unwrap_or(&entry.entry),
            );
            let agent = InstalledAgent {
                name: entry.name.clone(),
                version: pkg_version.clone(),
                install_path: installed_entry.clone(),
                source_path: installed_entry,
                manifest: apollia_core::AgentManifest {
                    name: entry.name.clone(),
                    version: pkg_version.clone(),
                    description: format!("Part of package {}", pkg_name),
                    tools_required: vec![],
                    tools_optional: vec![],
                    supports_streaming: false,
                    supports_a2a: false,
                    memory_namespace: None,
                    shared_memory_namespaces: vec![],
                    max_concurrent_tasks: 1,
                    step_budget: None,
                    network_allowlist: None,
                    dangerous_tools_allowed: false,
                    tags: vec![],
                    skills: vec![],
                    execution_mode: "auto".to_string(),
                    system_prompt: None,
                    tools_requiring_approval: vec![],
                    llm_backend: None,
                    packages: vec![],
                    memory_config: None,
                    agent_type: None,
                    examples: vec![],
                    limitations: vec![],
                    setup_notes: None,
                },
                enabled: true,
                installed_at: now.clone(),
                updated_at: now.clone(),
            };
            agent_repo.save(&agent).map_err(|e| format!("failed to save agent '{}': {e}", entry.name))?;
            agent_count += 1;
        }
    }

    {
        let pkg_repo = pkg_repo_state.lock().map_err(|_| "pkg repo lock poisoned")?;
        let installed_pkg = InstalledPackage {
            name: pkg_name.clone(),
            version: pkg_version.clone(),
            root_path: install_root.clone(),
            manifest_json: pkg.manifest_json.clone(),
            installed_at: now.clone(),
            updated_at: now.clone(),
        };
        pkg_repo.save(&installed_pkg).map_err(|e| format!("failed to save package: {e}"))?;
        for entry in &pkg.agents {
            pkg_repo.link_agent(&pkg_name, &entry.name)
                .map_err(|e| format!("failed to link agent: {e}"))?;
        }
    }

    // Inject triggers.
    let toml_str = std::fs::read_to_string(root.join("agent.toml"))
        .map_err(|e| format!("cannot read agent.toml: {e}"))?;
    let trigger_count = inject_package_triggers(&data_dir, &toml_str).unwrap_or(0);

    Ok(InstallPackageResponse {
        name: pkg_name,
        version: pkg_version,
        agent_count,
        trigger_count,
    })
}

/// Liste tous les packages installés.
#[tauri::command]
pub async fn list_agent_packages(
    pkg_repo_state: State<'_, Arc<Mutex<PackageRepository>>>,
) -> Result<Vec<AgentPackageListItem>, String> {
    let pkg_repo = pkg_repo_state.lock().map_err(|_| "repo lock poisoned")?;
    let packages = pkg_repo.list().map_err(|e| format!("database error: {e}"))?;

    let mut items = Vec::with_capacity(packages.len());
    for pkg in &packages {
        let agent_names = pkg_repo.list_agents_for_package(&pkg.name).unwrap_or_default();
        let agent_count = agent_names.len();

        let manifest: serde_json::Value =
            serde_json::from_str(&pkg.manifest_json).unwrap_or_default();

        let agents = manifest
            .get("agents")
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|a| PackageAgentSummary {
                        name: a.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        role: a.get("role").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        entry: a.get("entry").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();

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

/// Retourne le détail complet d'un package.
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

    let manifest: serde_json::Value =
        serde_json::from_str(&pkg.manifest_json).unwrap_or_default();

    let agents = manifest
        .get("agents")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .map(|a| PackageAgentSummary {
                    name: a.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    role: a.get("role").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    entry: a.get("entry").and_then(|v| v.as_str()).unwrap_or("").to_string(),
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

/// Désinstalle un package, ses agents et ses triggers.
#[tauri::command]
pub async fn uninstall_agent_package(
    name: String,
    pkg_repo_state: State<'_, Arc<Mutex<PackageRepository>>>,
    agent_repo_state: State<'_, Arc<Mutex<AgentRepository>>>,
) -> Result<(), String> {
    let (root_path, agent_names) = {
        let pkg_repo = pkg_repo_state.lock().map_err(|_| "repo lock poisoned")?;
        let pkg = pkg_repo
            .get(&name)
            .map_err(|e| format!("database error: {e}"))?
            .ok_or_else(|| format!("Package '{name}' not found"))?;
        let agents = pkg_repo.list_agents_for_package(&name).unwrap_or_default();
        (pkg.root_path, agents)
    };

    {
        let agent_repo = agent_repo_state.lock().map_err(|_| "repo lock poisoned")?;
        for agent_name in &agent_names {
            let _ = agent_repo.delete(agent_name);
        }
    }

    {
        let pkg_repo = pkg_repo_state.lock().map_err(|_| "repo lock poisoned")?;
        pkg_repo.delete(&name).map_err(|e| format!("failed to delete package: {e}"))?;
    }

    let _ = std::fs::remove_dir_all(&root_path);
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn apollia_data_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".apollia")
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn inject_package_triggers(data_dir: &Path, toml_str: &str) -> Result<usize, String> {
    let trigger_defs = parse_triggers_from_toml_str(toml_str)
        .map_err(|e| format!("trigger parse error: {e}"))?;

    if trigger_defs.is_empty() {
        return Ok(0);
    }

    let triggers_db = data_dir.join("triggers.db");
    let repo = TriggerDefinitionRepository::open(&triggers_db)
        .map_err(|e| format!("cannot open triggers.db: {e}"))?;

    let mut count = 0;
    for def in &trigger_defs {
        let row = trigger_def_to_row(def);
        let _ = repo.delete(&def.id);
        repo.insert(&row)
            .map_err(|e| format!("failed to insert trigger '{}': {e}", def.id))?;
        count += 1;
    }
    Ok(count)
}

fn trigger_def_to_row(def: &apollia_triggers::TriggerDefinition) -> TriggerDefinitionRow {
    let (source_type, source_config) = match &def.source {
        TriggerSourceConfig::Cron { schedule } => (
            "cron".to_string(),
            serde_json::json!({"schedule": schedule}),
        ),
        TriggerSourceConfig::Interval { every } => (
            "interval".to_string(),
            serde_json::json!({"every": every}),
        ),
        TriggerSourceConfig::Oneshot { fire_at } => (
            "oneshot".to_string(),
            serde_json::json!({"fire_at": fire_at.to_rfc3339()}),
        ),
        TriggerSourceConfig::FileWatch {
            path,
            events,
            follow_symlinks,
            exclude_patterns,
        } => (
            "file_watch".to_string(),
            serde_json::json!({
                "path": path.to_string_lossy(),
                "events": events,
                "follow_symlinks": follow_symlinks,
                "exclude_patterns": exclude_patterns,
            }),
        ),
        TriggerSourceConfig::Webhook { secret } => (
            "webhook".to_string(),
            serde_json::json!({"secret": secret}),
        ),
    };

    let on_busy = match &def.on_busy {
        OnBusyPolicy::Skip => OnBusy::Drop,
        OnBusyPolicy::Queue { .. } | OnBusyPolicy::Block => OnBusy::Queue,
    };

    TriggerDefinitionRow {
        id: def.id.clone(),
        agent: if def.agent.is_empty() {
            None
        } else {
            Some(def.agent.clone())
        },
        pipeline: def.pipeline.clone(),
        enabled: def.enabled,
        on_busy,
        source_type,
        source_config,
        input_template: if def.input_template.0.is_empty() {
            None
        } else {
            Some(def.input_template.0.clone())
        },
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    }
}
