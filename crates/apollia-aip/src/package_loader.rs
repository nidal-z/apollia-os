//! Loading and validation of an Agent Package from its directory.
//!
//! A package is described by an `agent.toml` at the directory root. The
//! manifest itself, its invariants and the entry paths it declares live in
//! [`apollia_tools::agent_package`], which the runtime reads without Python;
//! this module adds the AIP duck-typing of each declared `.py`.

use std::path::{Path, PathBuf};

pub use apollia_tools::agent_package::{
    load_manifest_only, validate_manifest, AgentEntry, AgentPackage, AgentRole, LoadedAgentEntry,
    PackageLoaderError, PackageManifest, PackageMeta, PipConfig, ToolsConfig, WebToolConfig,
};

use crate::loader::{load_agent_module, AIPLoaderError};

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
