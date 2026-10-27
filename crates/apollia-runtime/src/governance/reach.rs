//! The connections a call of a shared tool reaches.
//!
//! The file tools and `code_context` are natives, shared by every folder
//! connection: the call's tool names no connection, its arguments do. A file
//! tool reaches the deepest folder that holds where its path lands, read as
//! the tool reads it (`apollia_tools::sandbox_path::landing`, relative to the
//! first granted folder, `~` expanded, links followed); `code_context`
//! reaches the code folder it names, or every code folder of the agent when
//! it names none. The decision refuses a call one of whose folders is in
//! pause, as it refuses a tool of a connection in pause.
//!
//! The decision is not what keeps a folder in pause unread: a walk over a
//! sound folder names no nested one. The sandbox of the file tools takes every
//! folder in pause out of the roots that hold it
//! (`apollia_tools::SandboxSpec::with_paused`); the decision is what refuses
//! for the pause, and journals it, a call whose path names one.

use std::collections::HashSet;
use std::path::Path;

use apollia_tools::folder_access::{FOLDER_READ_TOOLS, FOLDER_WRITE_TOOLS};
use apollia_tools::{FolderRoots, CODE_CONTEXT_TOOL};
use serde_json::Value;

/// The keys a file tool names its path under.
const PATH_KEYS: [&str; 2] = ["path", "dir"];

/// The agent a call is reached for, as its turn knows it.
#[derive(Debug, Clone, Copy)]
pub struct Caller<'a> {
    /// The agent whose code folders `code_context` reads.
    pub agent_id: &'a str,
    /// The data directory its `agents.db` and `system.db` are read from.
    pub data_dir: Option<&'a Path>,
    /// Its folders, read once for the turn.
    pub folders: &'a FolderRoots,
}

/// The folder connections a call of `access_id` with `input` by `caller`
/// reaches, for the decision to refuse the call when one is in pause. A file
/// tool's folder is named from the caller's folders, read once for the turn;
/// the code folders of `code_context` from the `agents.db` and `system.db` of
/// its data directory. Nothing is resolved while no folder of `paused` is in
/// pause.
#[must_use]
pub fn call_reaches(
    caller: &Caller<'_>,
    access_id: &str,
    input: &Value,
    paused: &HashSet<String>,
) -> Vec<String> {
    let any_folder_paused = paused
        .iter()
        .any(|id| crate::connections::folder_id_of(id).is_some());
    if !any_folder_paused {
        return Vec::new();
    }
    if FOLDER_READ_TOOLS.contains(&access_id) || FOLDER_WRITE_TOOLS.contains(&access_id) {
        let path = PATH_KEYS
            .iter()
            .find_map(|key| input.get(*key).and_then(Value::as_str))
            .map(str::trim)
            .filter(|p| !p.is_empty());
        return caller
            .folders
            .folder_of(path)
            .map(str::to_owned)
            .into_iter()
            .collect();
    }
    match caller.data_dir {
        Some(data_dir) if access_id == CODE_CONTEXT_TOOL => {
            code_context_reach(caller.agent_id, input, data_dir)
        }
        _ => Vec::new(),
    }
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

#[cfg(test)]
pub(crate) mod tests;
