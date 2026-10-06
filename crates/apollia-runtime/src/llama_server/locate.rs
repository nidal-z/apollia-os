//! Where the `llama-server` binary is found.

use std::path::PathBuf;

use apollia_core::config::EngineChoice;

use super::LlamaServerError;
use crate::engine_store::selection::{resolve, Resolved};
use crate::engine_store::Engine;

/// Locate the bundled `llama-server` binary, falling back to `PATH` in dev.
///
/// Resolution order: the `APOLLIA_LLAMA_SERVER_BIN` override, then the engine
/// `choice` of the settings names (`[engines] gguf`), then the bundle
/// layouts (next to the executable, a `runners/` sibling, the macOS
/// `Contents/Resources/runners/`, and the Linux product directory), then
/// the ambient `PATH`, then the common install dirs. The override lets a
/// developer whose `llama-server` lives in a non-standard directory (a llama.cpp
/// build tree) point the app at it, which a GUI launch cannot reach via `PATH`.
pub(super) fn locate_llama_server_binary(
    choice: EngineChoice,
) -> Result<PathBuf, LlamaServerError> {
    let ext = if cfg!(windows) { ".exe" } else { "" };
    let name = format!("llama-server{ext}");

    let store_root = apollia_core::paths::data_dir().map(|d| crate::engine_store::store_root(&d));
    if let Some(path) = env_or_store_binary(
        std::env::var_os("APOLLIA_LLAMA_SERVER_BIN").map(PathBuf::from),
        store_root.as_deref(),
        &name,
        choice,
        || {
            matches!(
                apollia_llm::hardware::detect().accelerator,
                apollia_llm::AcceleratorProfile::Cuda { .. }
            )
        },
    ) {
        return Ok(path);
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let mut candidates = vec![dir.join(&name)];
            candidates.extend(
                apollia_core::paths::bundled_resource_dirs(dir, "runners")
                    .into_iter()
                    .map(|d| d.join(&name)),
            );
            if let Some(found) = candidates.iter().find(|c| c.exists()) {
                return Ok(found.clone());
            }
        }
    }

    if let Ok(path) = which_on_path(&name) {
        tracing::warn!(
            path = %path.display(),
            detail = "bundled binary absent, dev fallback", "llama.server.binary.path_fallback"
        );
        return Ok(path);
    }

    // A GUI launch (Finder / .desktop) inherits a minimal PATH that omits the
    // usual install dirs, so a developer's system llama-server is invisible to
    // `which_on_path`. Probe the common locations explicitly as a last resort.
    let home = apollia_core::paths::home_string().unwrap_or_default();
    let common = [
        format!("/opt/homebrew/bin/{name}"),
        format!("/usr/local/bin/{name}"),
        format!("/usr/bin/{name}"),
        format!("{home}/.local/bin/{name}"),
        format!("{home}/.cargo/bin/{name}"),
    ];
    if let Some(found) = common.iter().map(PathBuf::from).find(|c| c.is_file()) {
        tracing::warn!(
            path = %found.display(),
            detail = "bundled binary absent, dev fallback", "llama.server.binary.system_fallback"
        );
        return Ok(found);
    }

    Err(LlamaServerError::BinaryNotFound(format!(
        "{name} not found next to the executable, in a bundled runners/ dir, on PATH, \
         or in the common install directories"
    )))
}

/// The binary a launcher takes under `choice`. A build of the engine store is
/// recorded as used now, which is what the automatic purge orders by.
pub(super) fn locate_for_launch(choice: EngineChoice) -> Result<PathBuf, LlamaServerError> {
    let path = locate_llama_server_binary(choice)?;
    crate::engine_store::purge::mark_used(&path);
    Ok(path)
}

/// The `llama-server` binary the runtime starts under `choice`, or `None`
/// when none is in reach: what the engine list marks as in service.
pub(crate) fn in_service_binary(choice: EngineChoice) -> Option<PathBuf> {
    locate_llama_server_binary(choice).ok()
}

/// The two sources that come before the embedded engine.
///
/// An explicit override wins: `APOLLIA_LLAMA_SERVER_BIN=/path/to/llama-server`.
/// Then the engine store, as `choice` says: the newest verified build of the
/// variant it names; nothing for the shipped engine; and for the automatic
/// choice, the newest verified CUDA build on a machine where an NVIDIA card is
/// detected. A named variant the store no longer holds falls back to the
/// automatic choice. `None` leaves the place to the embedded engine.
pub(super) fn env_or_store_binary(
    env_bin: Option<PathBuf>,
    store_root: Option<&std::path::Path>,
    name: &str,
    choice: EngineChoice,
    nvidia_detected: impl FnOnce() -> bool,
) -> Option<PathBuf> {
    if let Some(path) = env_bin {
        if path.is_file() {
            tracing::info!(path = %path.display(), "llama.server.binary.from_env");
            return Some(path);
        }
        tracing::warn!(
            path = %path.display(),
            detail = "falling back to auto-detection", "llama.server.binary.env_invalid"
        );
    }
    let path = match resolve(store_root, Engine::LlamaServer, choice).resolved {
        Resolved::Embedded => return None,
        Resolved::Stored(package) => crate::engine_store::find_binary(&package.path, name),
        Resolved::Automatic => None,
    }
    .or_else(|| crate::engine_store::preferred_llama_server(store_root?, name, nvidia_detected))?;
    tracing::info!(path = %path.display(), "llama.server.binary.from_engine_store");
    Some(path)
}

/// Minimal `PATH` lookup, avoiding a dependency on an external `which` crate.
fn which_on_path(name: &str) -> Result<PathBuf, LlamaServerError> {
    let path_var = std::env::var_os("PATH")
        .ok_or_else(|| LlamaServerError::BinaryNotFound("PATH is unset".to_owned()))?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(LlamaServerError::BinaryNotFound(format!(
        "{name} not on PATH"
    )))
}
