//! Installation of an agent package, and the filesystem and trigger work it
//! needs: copy the tree, build one virtualenv per agent, write the rows, then
//! inject the package's triggers.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use apollia_aip::package_loader::{load_manifest_only, AgentPackage};
use apollia_runtime::api::routes_agents::AgentLoader;
use apollia_runtime::embedded::RuntimeHandle;
use apollia_tools::{AgentRepository, InstalledAgent, InstalledPackage, PackageRepository};
use apollia_triggers::{
    definition_repository::TriggerDefinitionRepository, OnBusy, TriggerDefinitionRow,
};
use tauri::State;

use super::{
    apollia_data_dir, find_venv_site_packages, load_agent_manifest_with_sys_paths,
    InstallPackageResponse, RawPreviewRoot, RawPreviewTrigger, TriggerConfigOverride,
};

/// Installs a package from a local path.
///
/// Flow (strict order):
/// 1. Parse `agent.toml` (sync, without PyO3)
/// 2. If pip packages are declared AND `deps_confirmed = false`, return the
///    error `DEPS_CONFIRMATION_REQUIRED:<n>:<list>` that the frontend parses to
///    show an explicit confirmation dialog.
/// 3. Copy the folder to `~/.apollia/agents/packages/<name>/`
/// 4. For each agent: create its venv (`~/.apollia/venvs/<agent>/venv/`),
///    `pip install` its packages, then duck-type the `.py` with the venv's
///    `site-packages` injected into `sys.path`.
/// 5. On any error in steps 3-4: full **rollback** (remove `install_root` and
///    every created venv).
/// 6. Save to DB + inject triggers (unchanged).
#[tauri::command]
// Tauri command: the State<'_, _> injections plus the user-facing args push the
// count past 5 by design; grouping them into a struct would only obscure the IPC signature.
// REASON: Tauri command: each parameter is one invoke key or injected State; a struct would change the IPC contract.
#[allow(clippy::too_many_arguments)]
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

    // ── Step 2: confirmation required for the pip deps ─────────────────────
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
    let parsed_result: Result<Vec<(String, PathBuf, apollia_core::AgentManifest)>, String> =
        async {
            let mut parsed: Vec<(String, PathBuf, apollia_core::AgentManifest)> =
                Vec::with_capacity(install_plan.len());

            for (agent_name, installed_py, agent_pkgs) in install_plan {
                // 4a: pip install in the agent's venv (if any packages declared).
                if !agent_pkgs.is_empty() {
                    let executor = apollia_tools::tools::python_executor::PythonExecutor::new(
                        &agent_name,
                        &venvs_root,
                    )
                    .map_err(|e| format!("VENV_CREATE_FAILED for '{}': {}", agent_name, e))?;
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
                        return Err(format!("duck-typing failed for '{}': {}", name_for_blk, e));
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

/// Injects the package's triggers into `triggers_def.db`, applying user overrides.
/// Returns `(number of triggers created, list of per-trigger errors)`.
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
    let triggers_db = data_dir.join(apollia_core::paths::DataFile::TriggersDef.file_name());
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
        if t.source.kind == "webhook" && secret.as_deref().unwrap_or("").is_empty() {
            errors.push(format!(
                "trigger '{}': webhook secret is required but not provided",
                t.id
            ));
            continue;
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
