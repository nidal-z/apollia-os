//! Installation of a package folder: copy the tree, build one virtualenv per
//! agent, import each agent, write the rows, then the package's triggers.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use apollia_tools::agent_package::{load_manifest_only, AgentPackage};
use apollia_tools::tools::python_executor::{agent_venv_site_packages, PythonExecutor};
use apollia_tools::{AgentRepository, InstalledAgent, InstalledPackage};
use apollia_triggers::definition_repository::TriggerDefinitionRepository;
use apollia_triggers::{OnBusy, TriggerDefinitionRow};

use super::{
    check_name, install_root, open_packages, AgentPackageError, AgentPackageInstallReport,
    AgentPackageTriggerConfig, RawTrigger, RawTriggers,
};
use crate::api::routes_agents::AgentLoader;

/// What the caller asks to install.
#[derive(Debug, Clone, Default)]
pub struct InstallRequest {
    /// The package folder on this machine.
    pub path: PathBuf,
    /// What the user gave each trigger.
    pub trigger_configs: Vec<AgentPackageTriggerConfig>,
    /// The user accepted the Python dependencies.
    pub deps_confirmed: bool,
}

/// A package installed: its report, and the agents the caller makes
/// available to tasks.
#[derive(Debug)]
pub struct Installed {
    /// What the installation did.
    pub report: AgentPackageInstallReport,
    /// Each agent recorded, with its installed entry point.
    pub agents: Vec<(String, PathBuf)>,
}

fn failed(message: impl std::fmt::Display) -> AgentPackageError {
    AgentPackageError::Install(message.to_string())
}

fn store(message: impl std::fmt::Display) -> AgentPackageError {
    AgentPackageError::Store(message.to_string())
}

/// Installs the package folder `request.path` under `data_dir`.
///
/// In order: parse `agent.toml`; refuse Python dependencies the caller has
/// not confirmed; copy the folder; for each agent, build its virtualenv and
/// import it with that virtualenv on `sys.path`, everything created removed
/// on the first failure; record the agents and the package; write the
/// triggers with the user's secrets.
///
/// # Errors
///
/// [`AgentPackageError::Manifest`] for an unreadable or invalid manifest,
/// [`AgentPackageError::InvalidName`] for a package or agent name that is a
/// path, [`AgentPackageError::DepsConfirmationRequired`],
/// [`AgentPackageError::Install`] when the copy, a virtualenv or an import
/// fails, [`AgentPackageError::Store`] when `agents.db` refuses a row.
pub async fn install(
    loader: Arc<dyn AgentLoader>,
    data_dir: &Path,
    request: InstallRequest,
) -> Result<Installed, AgentPackageError> {
    let root = request.path.clone();
    let pkg: AgentPackage = tokio::task::spawn_blocking(move || load_manifest_only(&root))
        .await
        .map_err(|e| failed(format!("spawn error: {e}")))?
        .map_err(|e| AgentPackageError::Manifest(format!("manifest validation failed: {e}")))?;

    check_name(&pkg.manifest.package.name)?;
    for agent in &pkg.agents {
        check_name(&agent.name)?;
    }
    let deps = pkg.manifest.all_pip_packages();
    if !deps.is_empty() && !request.deps_confirmed {
        return Err(AgentPackageError::DepsConfirmationRequired { packages: deps });
    }

    let name = pkg.manifest.package.name.clone();
    let version = pkg.manifest.package.version.clone();
    let target = install_root(data_dir, &name);
    copy_dir_all(&request.path, &target)
        .map_err(|e| failed(format!("failed to copy package: {e}")))?;

    let mut venvs = Venvs {
        root: data_dir.join("venvs"),
        created: Vec::new(),
    };
    let parsed = import_agents(&loader, &pkg, &request.path, &target, &mut venvs).await;
    let parsed = match parsed {
        Ok(parsed) => parsed,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&target);
            for venv in &venvs.created {
                let _ = std::fs::remove_dir_all(venv);
            }
            return Err(error);
        }
    };

    let now = chrono::Utc::now().to_rfc3339();
    let db = apollia_core::paths::DataFile::Agents.path(data_dir);
    let agent_repo = AgentRepository::open(&db).map_err(store)?;
    let mut agents = Vec::with_capacity(parsed.len());
    for (agent_name, entry, mut manifest) in parsed {
        manifest.name = agent_name.clone();
        if manifest.version.is_empty() {
            manifest.version = version.clone();
        }
        let agent = InstalledAgent {
            name: agent_name,
            version: manifest.version.clone(),
            install_path: entry.clone(),
            source_path: entry,
            manifest,
            enabled: true,
            installed_at: now.clone(),
            updated_at: now.clone(),
        };
        agent_repo
            .save(&agent)
            .map_err(|e| store(format!("failed to save agent '{}': {e}", agent.name)))?;
        agents.push((agent.name, agent.install_path));
    }

    let packages = open_packages(data_dir)?;
    packages
        .save(&InstalledPackage {
            name: name.clone(),
            version: version.clone(),
            root_path: target.clone(),
            manifest_json: pkg.manifest_json.clone(),
            installed_at: now.clone(),
            updated_at: now,
        })
        .map_err(|e| store(format!("failed to save package: {e}")))?;
    for entry in &pkg.agents {
        packages
            .link_agent(&name, &entry.name)
            .map_err(|e| store(format!("failed to link agent: {e}")))?;
    }

    let toml_str = std::fs::read_to_string(target.join("agent.toml"))
        .map_err(|e| failed(format!("cannot read installed agent.toml: {e}")))?;
    let (trigger_count, trigger_errors) =
        inject_triggers(data_dir, &toml_str, &request.trigger_configs);

    Ok(Installed {
        report: AgentPackageInstallReport {
            name,
            version,
            agent_count: agents.len(),
            trigger_count,
            trigger_errors,
        },
        agents,
    })
}

/// The folder of the virtualenvs, and those an installation created.
struct Venvs {
    /// `<data_dir>/venvs`.
    root: PathBuf,
    /// Each virtualenv created so far, removed if the installation fails.
    created: Vec<PathBuf>,
}

/// Builds the virtualenv of each agent that declares dependencies, then
/// imports it with that virtualenv on `sys.path`. Each virtualenv created is
/// pushed on `venvs.created` so the caller can remove it on failure.
async fn import_agents(
    loader: &Arc<dyn AgentLoader>,
    pkg: &AgentPackage,
    source: &Path,
    target: &Path,
    venvs: &mut Venvs,
) -> Result<Vec<(String, PathBuf, apollia_core::AgentManifest)>, AgentPackageError> {
    let mut parsed = Vec::with_capacity(pkg.agents.len());
    for entry in &pkg.agents {
        let relative = entry.entry.strip_prefix(source).unwrap_or(&entry.entry);
        let installed = target.join(relative);
        let deps = pkg.manifest.agent_pip_packages(&entry.name);
        if !deps.is_empty() {
            let executor = PythonExecutor::new(&entry.name, &venvs.root)
                .map_err(|e| failed(format!("VENV_CREATE_FAILED for '{}': {e}", entry.name)))?;
            venvs.created.push(venvs.root.join(&entry.name));
            executor
                .setup_venv(&deps)
                .await
                .map_err(|e| failed(format!("PIP_INSTALL_FAILED for '{}': {e}", entry.name)))?;
        }
        let site_packages = agent_venv_site_packages(&venvs.root, &entry.name);
        let loader = Arc::clone(loader);
        let path = installed.clone();
        let manifest = tokio::task::spawn_blocking(move || {
            loader.load_and_validate_with(&path, &site_packages)
        })
        .await
        .map_err(|e| failed(format!("spawn error: {e}")))?
        .map_err(|e| failed(format!("duck-typing failed for '{}': {e}", entry.name)))?;
        parsed.push((entry.name.clone(), installed, manifest));
    }
    Ok(parsed)
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Writes the package's triggers into `triggers_def.db`, the user's secret
/// first. Returns how many were written and why the others were not.
pub(crate) fn inject_triggers(
    data_dir: &Path,
    toml_str: &str,
    configs: &[AgentPackageTriggerConfig],
) -> (usize, Vec<String>) {
    let raw: RawTriggers = match toml::from_str(toml_str) {
        Ok(raw) => raw,
        Err(e) => return (0, vec![format!("TOML parse error: {e}")]),
    };
    if raw.triggers.is_empty() {
        return (0, Vec::new());
    }
    // The engine reads its definitions from `triggers_def.db`; `triggers.db`
    // holds the history of the fires, and a write there is never reloaded.
    let path = apollia_core::paths::DataFile::TriggersDef.path(data_dir);
    let repo = match TriggerDefinitionRepository::open(&path) {
        Ok(repo) => repo,
        Err(e) => return (0, vec![format!("cannot open triggers_def.db: {e}")]),
    };

    let mut count = 0;
    let mut errors = Vec::new();
    for trigger in &raw.triggers {
        let secret = configs
            .iter()
            .find(|c| c.id == trigger.id)
            .and_then(|c| c.secret.clone())
            .or_else(|| trigger.source.secret.clone());
        if trigger.source.kind == "webhook" && secret.as_deref().unwrap_or("").is_empty() {
            errors.push(format!(
                "trigger '{}': webhook secret is required but not provided",
                trigger.id
            ));
            continue;
        }
        let _ = repo.delete(&trigger.id);
        match repo.insert(&trigger_row(trigger, secret)) {
            Ok(_) => count += 1,
            Err(e) => errors.push(format!("trigger '{}': {e}", trigger.id)),
        }
    }
    (count, errors)
}

fn trigger_row(t: &RawTrigger, secret: Option<String>) -> TriggerDefinitionRow {
    let (source_type, source_config) = match t.source.kind.as_str() {
        "interval" => (
            "interval",
            serde_json::json!({"every": t.source.every.as_deref().unwrap_or("1h")}),
        ),
        "oneshot" => (
            "oneshot",
            serde_json::json!({"fire_at": t.source.schedule.as_deref().unwrap_or("")}),
        ),
        "file_watch" => (
            "file_watch",
            serde_json::json!({
                "path": t.source.path.as_deref().unwrap_or(""),
                "events": ["create", "modify"],
                "follow_symlinks": false,
                "exclude_patterns": [],
            }),
        ),
        "webhook" => (
            "webhook",
            serde_json::json!({"secret": secret.as_deref().unwrap_or("")}),
        ),
        _ => (
            "cron",
            serde_json::json!({"schedule": t.source.schedule.as_deref().unwrap_or("0 * * * *")}),
        ),
    };
    let now = chrono::Utc::now().to_rfc3339();
    TriggerDefinitionRow {
        id: t.id.clone(),
        agent: (!t.agent.is_empty()).then(|| t.agent.clone()),
        enabled: t.enabled,
        on_busy: OnBusy::Queue,
        source_type: source_type.to_owned(),
        source_config,
        input_template: None,
        created_at: now.clone(),
        updated_at: now,
    }
}
