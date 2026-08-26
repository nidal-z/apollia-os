//! Production execution backend for the Apollia Desktop app.
//!
//! Mirrors the production setup used by `apollia-os start` (apollia-cli).
//! See `apollia-cli/src/commands/start.rs` for the CLI counterpart.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use apollia_core::AgentManifest;
use apollia_memory::user_memory::UserMemoryRepository;
use apollia_runtime::api::routes_agents::AgentLoader;
use apollia_tools::ToolCredentialStore;

/// The execution backend proper lives in `runner`, what builds one per agent in
/// `factory`, and the chat-side runner in `chat_runner`.
pub mod chat_runner;
pub mod factory;
pub mod runner;

// ─── Agent loader ─────────────────────────────────────────────────────────────

/// Compute the per-agent venv's site-packages directories, given the agent's
/// installed `.py` path. By convention the agent name matches the `.py` file
/// stem (e.g. `chart-worker.py` -> agent `chart-worker` -> venv at
/// `~/.apollia/venvs/chart-worker/venv/`).
fn venv_site_packages_for_agent_path(agent_py_path: &Path) -> Vec<PathBuf> {
    let agent_name = match agent_py_path.file_stem().and_then(|s| s.to_str()) {
        Some(s) => s,
        None => return Vec::new(),
    };
    venv_site_packages_for_agent_name(agent_name)
}

/// Same as [`venv_site_packages_for_agent_path`] but takes the agent name
/// directly. Use this when the manifest is already available.
fn venv_site_packages_for_agent_name(agent_name: &str) -> Vec<PathBuf> {
    let home = apollia_core::paths::home_dir_or_temp()
        .display()
        .to_string();
    let base = apollia_core::paths::data_dir_under(home).join("venvs");
    apollia_tools::tools::python_executor::agent_venv_site_packages(&base, agent_name)
}

/// Real agent loader using AIPLoader + validate_agent.
///
/// Injects the agent's per-package venv into `sys.path` so top-level imports
/// of pip-installed packages (e.g. `matplotlib`, `openpyxl`) resolve at
/// runtime. The venv path is derived from the agent name (= `.py` file stem).
pub struct AIPAgentLoader;

impl AgentLoader for AIPAgentLoader {
    fn load_and_validate(&self, path: &Path) -> Result<AgentManifest, String> {
        let extras = venv_site_packages_for_agent_path(path);
        let module = apollia_aip::loader::load_agent_module_with_sys_paths(path, &extras)
            .map_err(|e| e.to_string())?;
        let validated =
            apollia_aip::validator::validate_agent(&module).map_err(|e| e.to_string())?;
        Ok(validated.manifest)
    }
}

// ─── Shared resources ─────────────────────────────────────────────────────────

fn default_memory_dir() -> PathBuf {
    let home = apollia_core::paths::home_dir_or_temp()
        .display()
        .to_string();
    apollia_core::paths::data_dir_under(home).join("memory")
}

/// Apollia data directory (`~/.apollia/`), used to open the shared
/// [`ToolCredentialStore`] backing `ctx.secrets`.
fn default_data_dir() -> PathBuf {
    let home = apollia_core::paths::home_dir_or_temp()
        .display()
        .to_string();
    apollia_core::paths::data_dir_under(home)
}

/// Opens the shared [`ToolCredentialStore`] backing `ctx.secrets`.
///
/// Returns `None` if the governance database does not exist yet or if opening
/// fails. The agent then gets `None` on every `ctx.secrets.get(key)`, which is
/// a non-fatal semantic.
fn open_secret_store(data_dir: &Path) -> Option<Arc<std::sync::Mutex<ToolCredentialStore>>> {
    let db_path = data_dir.join(apollia_tools::GOVERNANCE_DB_FILENAME);
    if !db_path.exists() {
        return None;
    }
    let keyfile_path = data_dir.join(".keyfile");
    match ToolCredentialStore::new(&db_path, &keyfile_path) {
        Ok(store) => Some(Arc::new(std::sync::Mutex::new(store))),
        Err(e) => {
            tracing::warn!(
                target: "apollia.aip.secrets",
                error = %e,
                detail = "the agent sees no key",
                "secrets.store.open.failed"
            );
            None
        }
    }
}

// ─── Chat Agent Runner ───────────────────────────────────────────────────────

/// from `governance.db`. Either source disables a tool; duplicates removed.
fn merge_disabled(static_disabled: &[String], mut runtime_disabled: Vec<String>) -> Vec<String> {
    for name in static_disabled {
        if !runtime_disabled.iter().any(|n| n == name) {
            runtime_disabled.push(name.clone());
        }
    }
    runtime_disabled
}

/// Flattens a [`UserMemoryRepository`] into the `{"profile": [(k, v), ...]}`
/// shape consumed by `RuntimeContext.user_context`. Returns `None` for an empty
/// repo so chat-mode agents see no profile rather than an empty object.
fn build_user_context_from_repo(
    repo: &UserMemoryRepository,
) -> Option<std::collections::HashMap<String, Vec<(String, String)>>> {
    let entries = repo.list_all().unwrap_or_default();
    if entries.is_empty() {
        return None;
    }
    let mut map = std::collections::HashMap::new();
    map.insert(
        "profile".to_string(),
        entries.into_iter().map(|e| (e.key, e.value)).collect(),
    );
    Some(map)
}
