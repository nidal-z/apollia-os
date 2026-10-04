//! The folders the former settings named, added once as folder connections.
//!
//! Before folders were connections, `apollia.toml` named the folders an agent
//! worked in: `[filesystem] trusted_paths` and `[chat] default_workspace`. At
//! the first start that knows folder connections, each folder they name
//! becomes one, through the same admission and journal as a folder chosen in
//! the picker. The home folder and the data directory are left out: neither is
//! a folder a user would grant whole. A folder imported is not granted to any
//! agent; the user grants it on the agent's sheet.
//!
//! The import runs once per `system.db`: a folder removed afterwards stays
//! removed. Both keys are then withdrawn, and a file that still carries them
//! loads with a `config.key.retired` warning.

use std::path::{Path, PathBuf};

use apollia_core::FolderConnectionStore;

use crate::api::routes_connections::folders::{add_as, AddFolderBody};
use crate::governance::ConnectionGovernance;

/// Name of the import in `system.db`.
const IMPORT_KEY: &str = "legacy_folders";

/// Who the connection journal names for a folder imported here.
const IMPORT_ACTOR: &str = "settings_import";

/// The folders `content` (an `apollia.toml`) names in its former keys, `~`
/// resolved, home folder left out, in the order written.
#[must_use]
pub fn legacy_folder_paths(content: &str) -> Vec<PathBuf> {
    folder_paths_under(content, apollia_core::paths::home_dir().as_deref())
}

/// [`legacy_folder_paths`] with `home` as the home folder.
fn folder_paths_under(content: &str, home: Option<&Path>) -> Vec<PathBuf> {
    let Ok(doc) = content.parse::<toml::Table>() else {
        return Vec::new();
    };
    let trusted = doc
        .get("filesystem")
        .and_then(|f| f.get("trusted_paths"))
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str);
    let workspace = doc
        .get("chat")
        .and_then(|c| c.get("default_workspace"))
        .and_then(toml::Value::as_str);
    let mut out: Vec<PathBuf> = Vec::new();
    for raw in trusted.chain(workspace) {
        let Some(path) = expand_home(raw.trim(), home) else {
            continue;
        };
        if home == Some(path.as_path()) || out.contains(&path) {
            continue;
        }
        out.push(path);
    }
    out
}

/// `raw` with a leading `~` resolved against `home`; `None` for an empty
/// entry, the home folder itself, or a `~` that cannot be resolved.
fn expand_home(raw: &str, home: Option<&Path>) -> Option<PathBuf> {
    if raw.is_empty() || raw == "~" || raw == "~/" {
        return None;
    }
    match raw.strip_prefix("~/") {
        Some(rest) => home.map(|h| h.join(rest)),
        None => Some(PathBuf::from(raw)),
    }
}

/// Add each folder the former keys of the file at `config_path` name as a
/// folder connection of `data_dir`, once. Answers how many were added.
///
/// A folder that no longer exists, is already a connection, or is refused by
/// the admission is skipped with a warning. Nothing here stops the start.
pub fn import_legacy_folders(
    config_path: Option<&Path>,
    data_dir: &Path,
    governance: &ConnectionGovernance,
) -> usize {
    let store = match crate::api::routes_connections::folders::store(data_dir) {
        Ok(store) => store,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                "connections.folders.import.unavailable"
            );
            return 0;
        }
    };
    if store.imported(IMPORT_KEY).unwrap_or(true) {
        return 0;
    }
    let content = config_path
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    let added = import_paths(&store, &legacy_folder_paths(&content), data_dir, governance);
    if let Err(error) = store.mark_imported(IMPORT_KEY) {
        tracing::event!(
            tracing::Level::WARN,
            error = %error,
            "connections.folders.import.unrecorded"
        );
    }
    tracing::event!(
        tracing::Level::INFO,
        added = added,
        "connections.folders.imported"
    );
    added
}

fn import_paths(
    store: &FolderConnectionStore,
    paths: &[PathBuf],
    data_dir: &Path,
    governance: &ConnectionGovernance,
) -> usize {
    let data_dir = data_dir.canonicalize().unwrap_or_else(|_| data_dir.into());
    let mut added = 0;
    for path in paths {
        if path.canonicalize().is_ok_and(|p| p == data_dir) {
            continue;
        }
        let body = AddFolderBody {
            path: path.to_string_lossy().into_owned(),
            label: None,
        };
        match add_as(governance, store, &body, IMPORT_ACTOR) {
            Ok(_) => added += 1,
            Err((_, error)) => tracing::event!(
                tracing::Level::WARN,
                path = %path.display(),
                code = %error.code,
                "connections.folders.import.skipped"
            ),
        }
    }
    added
}

#[cfg(test)]
mod tests;
