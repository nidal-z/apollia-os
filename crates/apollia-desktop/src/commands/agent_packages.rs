//! Tauri IPC commands pour la gestion des agent packages.
//!
//! Commandes : list, detail, preview (dry-run), install, uninstall.
//! Les opérations lourdes (copie de fichiers, duck-typing Python) sont exécutées
//! via `spawn_blocking` pour ne pas bloquer le thread async de Tauri.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use apollia_aip::package_loader::{load_manifest_only, validate_manifest, AgentPackage};
use apollia_core::events::RuntimeEvent;
use apollia_runtime::api::routes_agents::AgentLoader;
use apollia_runtime::embedded::RuntimeHandle;
use apollia_runtime::eventbus::EventBusSender;
use apollia_tools::{AgentRepository, InstalledAgent, InstalledPackage, PackageRepository};
use apollia_triggers::{
    definition_repository::TriggerDefinitionRepository, OnBusy, TriggerDefinitionRow,
};
use serde::{Deserialize, Serialize};
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

/// Preview d'un trigger (dry-run, sans validation stricte).
#[derive(Debug, Serialize)]
pub struct TriggerPreview {
    pub id: String,
    pub source_type: String,
    pub agent: String,
    /// Expression cron (cron triggers).
    pub schedule: Option<String>,
    /// Intervalle (interval triggers).
    pub every: Option<String>,
    /// Chemin surveillé (file_watch triggers).
    pub path: Option<String>,
    /// Ce trigger nécessite une configuration supplémentaire (ex : secret webhook).
    pub needs_config: bool,
    pub enabled: bool,
}

/// Override de configuration à appliquer lors de l'installation (ex : secret webhook).
#[derive(Debug, Serialize, Deserialize)]
pub struct TriggerConfigOverride {
    pub id: String,
    /// Secret HMAC pour les webhooks.
    pub secret: Option<String>,
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
    pub triggers: Vec<TriggerPreview>,
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
    pub trigger_errors: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Raw TOML helpers (lenient, no semantic validation — for preview)
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

/// Dry-run : parse `agent.toml` + valide les manifestes sans écrire en DB.
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

/// Installe un package depuis un chemin local.
///
/// Flow (ordre strict) :
/// 1. Parse `agent.toml` (sync, sans PyO3)
/// 2. Si packages pip déclarés ET `deps_confirmed = false` → renvoie l'erreur
///    `DEPS_CONFIRMATION_REQUIRED:<n>:<list>` que le frontend parse pour
///    afficher un dialog de confirmation explicite.
/// 3. Copie le dossier vers `~/.apollia/agents/packages/<name>/`
/// 4. Pour chaque agent : crée son venv (`~/.apollia/venvs/<agent>/venv/`),
///    `pip install` ses packages, puis duck-type le `.py` avec le
///    `site-packages` du venv injecté dans `sys.path`.
/// 5. Sur la moindre erreur en étape 3-4 : **rollback** complet (suppression
///    `install_root` + tous les venvs créés).
/// 6. Sauvegarde DB + injection des triggers (inchangé).
#[tauri::command]
pub async fn install_agent_package(
    path: String,
    trigger_configs: Vec<TriggerConfigOverride>,
    deps_confirmed: bool,
    pkg_repo_state: State<'_, Arc<Mutex<PackageRepository>>>,
    agent_repo_state: State<'_, Arc<Mutex<AgentRepository>>>,
    _agent_loader: State<'_, Arc<dyn AgentLoader>>,
    runtime: State<'_, RuntimeHandle>,
) -> Result<InstallPackageResponse, String> {
    let root = PathBuf::from(&path);
    let data_dir = apollia_data_dir();

    // ── Step 1: parse manifest only (no PyO3, no venv) ─────────────────────
    let pkg: AgentPackage = {
        let root_clone = root.clone();
        tokio::task::spawn_blocking(move || load_manifest_only(&root_clone))
            .await
            .map_err(|e| format!("spawn error: {e}"))?
            .map_err(|e| format!("manifest validation failed: {e}"))?
    };

    let aggregate_pkgs = pkg.manifest.all_pip_packages();

    // ── Step 2: confirmation requise pour les deps pip ─────────────────────
    if !aggregate_pkgs.is_empty() && !deps_confirmed {
        return Err(format!(
            "DEPS_CONFIRMATION_REQUIRED:{}:{}",
            aggregate_pkgs.len(),
            aggregate_pkgs.join(",")
        ));
    }

    let pkg_name = pkg.manifest.package.name.clone();
    let pkg_version = pkg.manifest.package.version.clone();
    let install_root = data_dir.join("agents").join("packages").join(&pkg_name);
    let venvs_root = data_dir.join("venvs");

    // ── Step 3: copy directory ─────────────────────────────────────────────
    copy_dir_all(&root, &install_root).map_err(|e| format!("failed to copy package: {e}"))?;

    // Track everything we created so we can roll back on failure.
    let mut created_venvs: Vec<PathBuf> = Vec::new();

    // Build the per-agent install plan (name, installed .py path, pip pkgs).
    let install_plan: Vec<(String, PathBuf, Vec<String>)> = pkg
        .agents
        .iter()
        .map(|entry| {
            let rel = entry.entry.strip_prefix(&root).unwrap_or(&entry.entry);
            let installed_py = install_root.join(rel);
            let pkgs = pkg.manifest.agent_pip_packages(&entry.name);
            (entry.name.clone(), installed_py, pkgs)
        })
        .collect();

    // ── Step 4: venv + duck-type per agent. On failure, rollback. ──────────
    let parsed_result: Result<Vec<(String, PathBuf, apollia_core::AgentManifest)>, String> = async {
        let mut parsed: Vec<(String, PathBuf, apollia_core::AgentManifest)> =
            Vec::with_capacity(install_plan.len());

        for (agent_name, installed_py, agent_pkgs) in install_plan {
            // 4a: pip install in the agent's venv (if any packages declared).
            if !agent_pkgs.is_empty() {
                let executor =
                    apollia_tools::tools::python_executor::PythonExecutor::new(&agent_name, &venvs_root)
                        .map_err(|e| {
                            format!("VENV_CREATE_FAILED for '{}': {}", agent_name, e)
                        })?;
                executor
                    .setup_venv(&agent_pkgs)
                    .await
                    .map_err(|e| format!("PIP_INSTALL_FAILED for '{}': {}", agent_name, e))?;
                created_venvs.push(venvs_root.join(&agent_name));
            }

            // 4b: compute venv site-packages directories for sys.path injection.
            let venv_path = venvs_root.join(&agent_name).join("venv");
            let venv_site_packages = find_venv_site_packages(&venv_path);

            // 4c: duck-type the agent + extract its Python manifest in one PyO3 call.
            let py_path_for_blk = installed_py.clone();
            let name_for_blk = agent_name.clone();
            let manifest_res = tokio::task::spawn_blocking(move || {
                load_agent_manifest_with_sys_paths(&py_path_for_blk, &venv_site_packages)
            })
            .await
            .map_err(|e| format!("spawn error: {e}"))?;

            let manifest = match manifest_res {
                Ok(m) => m,
                Err(e) => {
                    return Err(format!(
                        "duck-typing failed for '{}': {}",
                        name_for_blk, e
                    ));
                }
            };

            parsed.push((agent_name, installed_py, manifest));
        }

        Ok(parsed)
    }
    .await;

    // ── Step 5: rollback on failure ────────────────────────────────────────
    let parsed_manifests = match parsed_result {
        Ok(p) => p,
        Err(err) => {
            let _ = std::fs::remove_dir_all(&install_root);
            for venv in &created_venvs {
                let _ = std::fs::remove_dir_all(venv);
            }
            return Err(err);
        }
    };

    let now = chrono::Utc::now().to_rfc3339();
    let mut agent_count = 0;

    {
        let agent_repo = agent_repo_state.lock().map_err(|_| "repo lock poisoned")?;
        for (name, installed_entry, mut manifest) in parsed_manifests {
            // Ensure name/version are consistent with the package declaration
            // (the Python manifest's `name` should already match, but we trust
            // the package as the source of truth for these fields).
            manifest.name = name.clone();
            if manifest.version.is_empty() {
                manifest.version = pkg_version.clone();
            }
            let agent = InstalledAgent {
                name,
                version: manifest.version.clone(),
                install_path: installed_entry.clone(),
                source_path: installed_entry,
                manifest,
                enabled: true,
                installed_at: now.clone(),
                updated_at: now.clone(),
            };
            agent_repo
                .save(&agent)
                .map_err(|e| format!("failed to save agent '{}': {e}", agent.name))?;
            agent_count += 1;
        }
    }

    {
        let pkg_repo = pkg_repo_state
            .lock()
            .map_err(|_| "pkg repo lock poisoned")?;
        let installed_pkg = InstalledPackage {
            name: pkg_name.clone(),
            version: pkg_version.clone(),
            root_path: install_root.clone(),
            manifest_json: pkg.manifest_json.clone(),
            installed_at: now.clone(),
            updated_at: now.clone(),
        };
        pkg_repo
            .save(&installed_pkg)
            .map_err(|e| format!("failed to save package: {e}"))?;
        for entry in &pkg.agents {
            pkg_repo
                .link_agent(&pkg_name, &entry.name)
                .map_err(|e| format!("failed to link agent: {e}"))?;
        }
    }

    // Inject triggers with user-provided overrides.
    let toml_str = std::fs::read_to_string(install_root.join("agent.toml"))
        .map_err(|e| format!("cannot read installed agent.toml: {e}"))?;

    let (trigger_count, trigger_errors) =
        inject_package_triggers(&data_dir, &toml_str, &trigger_configs);

    // Hot-reload the TriggerEngine from DB so new triggers activate immediately.
    if trigger_count > 0 {
        let _ = crate::commands::http_post_json(
            runtime.api_port,
            "/api/v1/triggers/reload",
            &serde_json::json!({}),
        )
        .await;
    }

    Ok(InstallPackageResponse {
        name: pkg_name,
        version: pkg_version,
        agent_count,
        trigger_count,
        trigger_errors,
    })
}

/// Liste tous les packages installés.
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

/// Désinstalle un package, ses agents et ses triggers.
///
/// Best-effort par design : si une étape échoue (DB désynchro, filesystem
/// déjà supprimé, registry runtime sans entrée), on continue à purger le
/// reste plutôt que d'avorter. Un uninstall qui échoue partiellement laisse
/// l'utilisateur dans un état non-réinstallable. Le rapport d'erreurs est
/// agrégé en fin de fonction pour journalisation.
///
/// Cleanup ordre :
/// 1. Snapshot des metadata DB (lookup tolérant : best-effort).
/// 2. Suppression `installed_agents` (cascade).
/// 3. Suppression `installed_packages` (cascade sur `package_agents`).
/// 4. Désenregistrement runtime in-memory + émission `AgentUninstalled`.
/// 5. Suppression filesystem : `install_root` + venv worker par agent.
/// 6. Si aucune trace trouvée nulle part → retourne erreur pour informer le
///    frontend qu'il n'y avait rien à désinstaller.
#[tauri::command]
pub async fn uninstall_agent_package(
    name: String,
    pkg_repo_state: State<'_, Arc<Mutex<PackageRepository>>>,
    agent_repo_state: State<'_, Arc<Mutex<AgentRepository>>>,
    runtime: State<'_, RuntimeHandle>,
    event_bus: State<'_, EventBusSender>,
) -> Result<(), String> {
    let data_dir = apollia_data_dir();
    let venvs_root = data_dir.join("venvs");
    let install_root_default = data_dir.join("agents").join("packages").join(&name);

    // ── Step 1: best-effort snapshot of DB metadata ──────────────────────────
    let (root_path, agent_names) = {
        let pkg_repo = pkg_repo_state.lock().map_err(|_| "repo lock poisoned")?;
        match pkg_repo.get(&name) {
            Ok(Some(pkg)) => {
                let agents = pkg_repo.list_agents_for_package(&name).unwrap_or_default();
                (pkg.root_path, agents)
            }
            _ => (install_root_default.clone(), Vec::new()),
        }
    };

    // Filesystem fallback : si la DB n'a pas d'agents listés mais le
    // dossier d'install existe, on peut quand même y aller (à minima
    // purger l'install_root et tout venv qui pourrait y correspondre).
    let mut errors: Vec<String> = Vec::new();

    // ── Step 2: delete installed_agents rows (idempotent) ────────────────────
    {
        let agent_repo = agent_repo_state.lock().map_err(|_| "repo lock poisoned")?;
        for agent_name in &agent_names {
            if let Err(e) = agent_repo.delete(agent_name) {
                errors.push(format!("agent_repo.delete({agent_name}): {e}"));
            }
        }
        // Tente aussi par le nom du package — couvre le cas où un seul
        // agent porte le nom du package (workers standalone).
        let _ = agent_repo.delete(&name);
    }

    // ── Step 3: delete installed_packages (cascade) ──────────────────────────
    {
        let pkg_repo = pkg_repo_state
            .lock()
            .map_err(|_| "pkg repo lock poisoned")?;
        if let Err(e) = pkg_repo.delete(&name) {
            errors.push(format!("pkg_repo.delete({name}): {e}"));
        }
    }

    // ── Step 4: unregister runtime + emit events ─────────────────────────────
    let names_to_unregister: Vec<String> = if agent_names.is_empty() {
        vec![name.clone()]
    } else {
        agent_names.clone()
    };
    for agent_name in &names_to_unregister {
        if let Ok(Some(agent_id)) = runtime.registry_handle.find_by_name(agent_name).await {
            let _ = runtime
                .router_handle
                .unregister_coordinator(&agent_id)
                .await;
            let _ = runtime.registry_handle.unregister(agent_id.as_str()).await;
        }
        let _ = event_bus.send(RuntimeEvent::AgentUninstalled {
            name: agent_name.clone(),
        });
    }

    // ── Step 5: filesystem purge ─────────────────────────────────────────────
    // 5a — install_root du package (resolved depuis la DB, fallback chemin par défaut).
    if root_path.exists() {
        if let Err(e) = std::fs::remove_dir_all(&root_path) {
            errors.push(format!("remove_dir_all({}): {e}", root_path.display()));
        }
    }
    if install_root_default != root_path && install_root_default.exists() {
        let _ = std::fs::remove_dir_all(&install_root_default);
    }

    // 5b — venvs par agent : ~/.apollia/venvs/<agent_name>/
    // Critique : sans ça, un pip install partiellement réussi laissait des
    // packages installés qui pouvaient shadow la nouvelle install.
    for agent_name in &names_to_unregister {
        let agent_venv = venvs_root.join(agent_name);
        if agent_venv.exists() {
            if let Err(e) = std::fs::remove_dir_all(&agent_venv) {
                errors.push(format!("remove_dir_all({}): {e}", agent_venv.display()));
            }
        }
    }

    // 5c — venv du package lui-même (cas worker standalone où agent name = pkg name)
    let pkg_venv = venvs_root.join(&name);
    if pkg_venv.exists() && !names_to_unregister.contains(&name) {
        let _ = std::fs::remove_dir_all(&pkg_venv);
    }

    // ── Step 6: report ───────────────────────────────────────────────────────
    if !errors.is_empty() {
        tracing::warn!(
            package = %name,
            errors = ?errors,
            "uninstall completed with non-fatal errors"
        );
    }

    // Si on n'a rien purgé du tout (ni DB, ni filesystem, ni venv), prévenir
    // le frontend. Sinon, l'opération est considérée comme un succès même
    // partiel — l'utilisateur doit pouvoir re-installer ensuite.
    let anything_purged = !agent_names.is_empty()
        || root_path.exists() == false  // un remove_dir_all a réussi
        || install_root_default.exists() == false;
    if !anything_purged && errors.iter().any(|e| e.contains("pkg_repo.delete")) {
        return Err(format!("Package '{name}' not found in any registry"));
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn apollia_data_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".apollia")
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
    let validated =
        apollia_aip::validator::validate_agent(&module).map_err(|e| e.to_string())?;
    Ok(validated.manifest)
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

/// Injecte les triggers du package dans `triggers_def.db` en appliquant les overrides utilisateur.
/// Retourne `(nombre de triggers créés, liste d'erreurs par trigger)`.
fn inject_package_triggers(
    data_dir: &Path,
    toml_str: &str,
    overrides: &[TriggerConfigOverride],
) -> (usize, Vec<String>) {
    // Parse raw triggers leniently (input_template defaults to "").
    let raw: RawPreviewRoot = match toml::from_str(toml_str) {
        Ok(r) => r,
        Err(e) => return (0, vec![format!("TOML parse error: {e}")]),
    };

    if raw.triggers.is_empty() {
        return (0, vec![]);
    }

    // The TriggerEngine reads definitions from `triggers_def.db` (not `triggers.db`,
    // which stores fire history). Writing to the wrong file is silently ignored by reload.
    let triggers_db = data_dir.join("triggers_def.db");
    let repo = match TriggerDefinitionRepository::open(&triggers_db) {
        Ok(r) => r,
        Err(e) => return (0, vec![format!("cannot open triggers_def.db: {e}")]),
    };

    let mut count = 0;
    let mut errors = Vec::new();

    for t in &raw.triggers {
        // Apply user-provided override for this trigger.
        let secret = overrides
            .iter()
            .find(|o| o.id == t.id)
            .and_then(|o| o.secret.clone())
            .or_else(|| t.source.secret.clone());

        // Validate webhook secret.
        if t.source.kind == "webhook" {
            if secret.as_deref().unwrap_or("").is_empty() {
                errors.push(format!(
                    "trigger '{}': webhook secret is required but not provided",
                    t.id
                ));
                continue;
            }
        }

        let row = build_trigger_row(t, secret);
        let _ = repo.delete(&t.id);
        if let Err(e) = repo.insert(&row) {
            errors.push(format!("trigger '{}': {e}", t.id));
        } else {
            count += 1;
        }
    }

    (count, errors)
}

fn build_trigger_row(
    t: &RawPreviewTrigger,
    webhook_secret_override: Option<String>,
) -> TriggerDefinitionRow {
    let (source_type, source_config) = match t.source.kind.as_str() {
        "interval" => (
            "interval".to_string(),
            serde_json::json!({"every": t.source.every.as_deref().unwrap_or("1h")}),
        ),
        "oneshot" => (
            "oneshot".to_string(),
            serde_json::json!({"fire_at": t.source.schedule.as_deref().unwrap_or("")}),
        ),
        "file_watch" => (
            "file_watch".to_string(),
            serde_json::json!({
                "path": t.source.path.as_deref().unwrap_or(""),
                "events": ["create", "modify"],
                "follow_symlinks": false,
                "exclude_patterns": [],
            }),
        ),
        "webhook" => (
            "webhook".to_string(),
            serde_json::json!({"secret": webhook_secret_override.as_deref().unwrap_or("")}),
        ),
        _ => (
            "cron".to_string(),
            serde_json::json!({"schedule": t.source.schedule.as_deref().unwrap_or("0 * * * *")}),
        ),
    };

    TriggerDefinitionRow {
        id: t.id.clone(),
        agent: if t.agent.is_empty() {
            None
        } else {
            Some(t.agent.clone())
        },
        enabled: t.enabled,
        on_busy: OnBusy::Queue,
        source_type,
        source_config,
        input_template: None,
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    }
}
