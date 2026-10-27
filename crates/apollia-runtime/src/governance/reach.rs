//! The connections a call of a shared tool reaches.
//!
//! The file tools and `code_context` are natives, shared by every folder
//! connection: the call's tool names no connection, its arguments do. A file
//! tool given an absolute path reaches the folder that holds the path;
//! `code_context` reaches the code folder it names, or every code folder of
//! the agent when it names none. The decision refuses a call one of whose
//! folders is in pause, as it refuses a tool of a connection in pause.
//!
//! A relative path is anchored in a folder granted to the agent, and a folder
//! in pause is not one of its roots (`apollia_tools::granted_folders`), so it
//! reaches none in pause.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use apollia_core::paths::DataFile;
use apollia_core::FolderConnectionStore;
use apollia_tools::folder_access::{FOLDER_READ_TOOLS, FOLDER_WRITE_TOOLS};
use apollia_tools::CODE_CONTEXT_TOOL;
use serde_json::Value;

/// The keys a file tool names its path under.
const PATH_KEYS: [&str; 2] = ["path", "dir"];

/// The folder connections a call of `access_id` with `input` by `agent_id`
/// reaches, read from the `system.db` and `agents.db` of `data_dir`, for the
/// decision to refuse the call when one is in pause. Nothing is read while no
/// folder of `paused` is in pause.
#[must_use]
pub fn call_reaches(
    agent_id: &str,
    access_id: &str,
    input: &Value,
    data_dir: Option<&Path>,
    paused: &HashSet<String>,
) -> Vec<String> {
    let any_folder_paused = paused
        .iter()
        .any(|id| crate::connections::folder_id_of(id).is_some());
    let Some(data_dir) = data_dir.filter(|_| any_folder_paused) else {
        return Vec::new();
    };
    if FOLDER_READ_TOOLS.contains(&access_id) || FOLDER_WRITE_TOOLS.contains(&access_id) {
        return file_tool_reach(input, data_dir).into_iter().collect();
    }
    if access_id == CODE_CONTEXT_TOOL {
        return code_context_reach(agent_id, input, data_dir);
    }
    Vec::new()
}

/// The folder connection that holds the absolute path a file tool is given,
/// the deepest one when folders nest.
fn file_tool_reach(input: &Value, data_dir: &Path) -> Option<String> {
    let path = PATH_KEYS
        .iter()
        .find_map(|key| input.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(Path::new)
        .filter(|p| p.is_absolute())?;
    let system = DataFile::System.path(data_dir);
    if !system.exists() {
        return None;
    }
    let folders = match FolderConnectionStore::open(&system).and_then(|store| store.list()) {
        Ok(folders) => folders,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                "governance.reach.folders_unreadable"
            );
            return None;
        }
    };
    let path = canonical(path);
    folders
        .iter()
        .filter(|folder| path.starts_with(canonical(Path::new(&folder.path))))
        .max_by_key(|folder| folder.path.len())
        .map(|folder| crate::connections::folder_connection_id(&folder.id))
}

/// The code folders a `code_context` call reaches: the one it names, by its
/// label or its path, else every one of the agent.
fn code_context_reach(agent_id: &str, input: &Value, data_dir: &Path) -> Vec<String> {
    let folders = match crate::agents::sources::code_folders(data_dir, agent_id) {
        Ok(folders) => folders,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                "governance.reach.code_folders_unreadable"
            );
            return Vec::new();
        }
    };
    let named = input
        .get("folder")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|f| !f.is_empty());
    folders
        .into_iter()
        .filter(|(folder, _)| {
            named.is_none_or(|wanted| folder.label == wanted || folder.path == Path::new(wanted))
        })
        .map(|(_, connection)| connection)
        .collect()
}

/// `path` with its longest existing ancestor resolved and its `.` and `..`
/// taken out, so that a path written through a link or a parent step lands
/// where the folder's own canonical path does.
fn canonical(path: &Path) -> PathBuf {
    let mut lexical = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                lexical.pop();
            }
            other => lexical.push(other),
        }
    }
    let mut existing = lexical.as_path();
    let mut rest = Vec::new();
    loop {
        if let Ok(resolved) = existing.canonicalize() {
            return rest.iter().rev().fold(resolved, |acc, part| acc.join(part));
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_owned());
                existing = parent;
            }
            _ => return lexical,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
