//! The folders an agent reaches with the file tools: the folders granted to it.
//!
//! A folder of this machine is a connection (`folder:<id>` in `system.db`).
//! Its tools are granted to an agent one by one, named
//! `conn:folder:<id>/<tool>` (`conn:folder:4f2c/file_read`), the shape of the
//! tool of any other connection. The file tools themselves stay the natives
//! (`file_read`, `file_write`...), rooted at the union of the granted folders:
//!
//! - a folder with any of its tools granted is a read root;
//! - a folder with a writing tool granted (`file_write`, `file_edit`,
//!   `notebook_edit`) is also a write root.
//!
//! The roots keep the order in which the folders were added, so the first one
//! anchors a relative path. A folder whose connection is in pause is no root,
//! and is taken out of every root that holds it: nothing of it is read or
//! written until it is resumed, whatever path or walk leads there, and the
//! decision on a call whose path lands in it refuses the call for the pause.
//! An agent with no folder granted has no root, and its file tools refuse
//! with a message that says what to do.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use apollia_core::paths::DataFile;
use apollia_core::{FolderConnection, FolderConnectionStore};

/// Prefix of the access to a tool of a folder connection.
pub const FOLDER_ACCESS_PREFIX: &str = "conn:folder:";

/// The file tools that read under the granted folders.
pub const FOLDER_READ_TOOLS: [&str; 5] = [
    "file_read",
    "file_list",
    "file_glob",
    "file_grep",
    "notebook_read",
];

/// The file tools that write under the folders granted for writing.
pub const FOLDER_WRITE_TOOLS: [&str; 3] = ["file_write", "file_edit", "notebook_edit"];

/// The access to `tool` of the folder connection `folder_id`:
/// `conn:folder:<id>/<tool>`.
#[must_use]
pub fn folder_access_name(folder_id: &str, tool: &str) -> String {
    format!("{FOLDER_ACCESS_PREFIX}{folder_id}/{tool}")
}

/// `name` read as the access to a tool of a folder connection: the folder
/// identifier and the file tool. `None` unless the identifier is not empty and
/// the tool is a file tool.
#[must_use]
pub fn parse_folder_access(name: &str) -> Option<(&str, &str)> {
    let (folder, tool) = name.strip_prefix(FOLDER_ACCESS_PREFIX)?.split_once('/')?;
    let known = FOLDER_READ_TOOLS.contains(&tool) || FOLDER_WRITE_TOOLS.contains(&tool);
    (!folder.is_empty() && known).then_some((folder, tool))
}

/// What the file tools of one agent may reach.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderRoots {
    /// Every granted folder, in the order they were added.
    pub read: Vec<PathBuf>,
    /// The folders granted for writing, in the same order.
    pub write: Vec<PathBuf>,
    /// Every stored folder whose connection is in pause, granted or not: a
    /// subtree no file tool reads or writes, even under a root that holds it
    /// ([`crate::SandboxSpec::with_paused`]).
    pub paused: Vec<PathBuf>,
    /// Every stored folder, for the decision on a call to name the folder
    /// the path of the call lands in.
    pub stored: Vec<StoredFolder>,
}

/// A stored folder, as the pauses name its connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFolder {
    /// The identifier of its connection, `folder:<id>`.
    pub connection: String,
    /// Its path, as stored.
    pub path: PathBuf,
}

impl FolderRoots {
    /// The folder a relative path lands in: the first granted one.
    #[must_use]
    pub fn anchor(&self) -> Option<&Path> {
        self.read.first().map(PathBuf::as_path)
    }

    /// The connection of the deepest stored folder that holds where the path
    /// `input` of a file tool lands, read as the tool reads it
    /// ([`crate::sandbox_path::landing`], [`crate::sandbox_path::real_path`]).
    /// No path means the anchor, where a tool given none searches. `None`
    /// when the path lands in no stored folder, or is relative and no folder
    /// is granted.
    #[must_use]
    pub fn folder_of(&self, input: Option<&str>) -> Option<&str> {
        let anchor = self.anchor().unwrap_or(Path::new(""));
        let landed = crate::sandbox_path::landing(input.unwrap_or("."), anchor)?;
        self.folder_landed(&landed)
    }

    /// The connection of the deepest stored folder that holds `landed`, a
    /// path as [`crate::sandbox_path::landing`] reads it.
    fn folder_landed(&self, landed: &Path) -> Option<&str> {
        if !landed.is_absolute() {
            return None;
        }
        let real = crate::sandbox_path::real_path(landed)?;
        self.stored
            .iter()
            .filter_map(|folder| {
                let path = crate::sandbox_path::real_path(&folder.path)?;
                real.starts_with(&path)
                    .then(|| (path.components().count(), folder))
            })
            .max_by_key(|(depth, _)| *depth)
            .map(|(_, folder)| folder.connection.as_str())
    }

    /// The sandbox of a file tool reaching `roots`, this agent's folders in
    /// pause taken out of them.
    #[must_use]
    pub fn sandbox(&self, roots: Vec<PathBuf>) -> crate::SandboxSpec {
        crate::SandboxSpec::from(roots).with_paused(self.paused.clone())
    }
}

/// Prefix of the identifier of the connection of a stored folder, as the
/// connection list and the pauses name it (`folder:<id>`).
const FOLDER_CONNECTION_PREFIX: &str = "folder:";

/// The access list an executor checks, and the roots of the file tools, from
/// `access` and the stored `folders` (oldest first), leaving out of the roots
/// every folder whose connection is in `paused` (`folder:<id>`).
///
/// Each granted folder tool becomes the native it names, so the model calls
/// `file_read` and the call is checked against one list; the folder access
/// itself, which no executor answers, is taken out. An access to a folder no
/// longer stored grants nothing. A folder in pause keeps its tools, so that a
/// call naming it is refused for the pause rather than for a missing tool.
#[must_use]
pub fn resolve_folder_access(
    access: Vec<String>,
    folders: &[FolderConnection],
    paused: &HashSet<String>,
) -> (Vec<String>, FolderRoots) {
    let granted: Vec<(&str, &str)> = access
        .iter()
        .filter_map(|name| parse_folder_access(name))
        .collect();
    let mut roots = FolderRoots::default();
    for folder in folders {
        let connection = format!("{FOLDER_CONNECTION_PREFIX}{}", folder.id);
        let in_pause = paused.contains(&connection);
        if in_pause {
            roots.paused.push(PathBuf::from(&folder.path));
        }
        roots.stored.push(StoredFolder {
            connection,
            path: PathBuf::from(&folder.path),
        });
        let tools: Vec<&str> = granted
            .iter()
            .filter(|(id, _)| *id == folder.id)
            .map(|(_, tool)| *tool)
            .collect();
        if tools.is_empty() || in_pause {
            continue;
        }
        roots.read.push(PathBuf::from(&folder.path));
        if tools.iter().any(|t| FOLDER_WRITE_TOOLS.contains(t)) {
            roots.write.push(PathBuf::from(&folder.path));
        }
    }
    let known = |id: &str| folders.iter().any(|f| f.id == id);
    let mut tools: Vec<String> = Vec::with_capacity(access.len());
    for name in &access {
        let resolved = match parse_folder_access(name) {
            Some((id, tool)) if known(id) => tool.to_owned(),
            Some(_) => continue,
            None if name.starts_with(FOLDER_ACCESS_PREFIX) => continue,
            None => name.clone(),
        };
        if !tools.contains(&resolved) {
            tools.push(resolved);
        }
    }
    (tools, roots)
}

/// [`resolve_folder_access`] over the folders stored in the `system.db` of
/// `data_dir`, those in `paused` left out of the roots. A store that cannot be
/// read grants no folder.
#[must_use]
pub fn granted_folders(
    access: Vec<String>,
    data_dir: &Path,
    paused: &HashSet<String>,
) -> (Vec<String>, FolderRoots) {
    let path = DataFile::System.path(data_dir);
    let folders = if path.exists() {
        match FolderConnectionStore::open(&path).and_then(|store| store.list()) {
            Ok(folders) => folders,
            Err(error) => {
                tracing::event!(
                    tracing::Level::WARN,
                    error = %error,
                    detail = "no folder is reachable",
                    "tools.folders.unreadable"
                );
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    resolve_folder_access(access, &folders, paused)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod paused_tests;

#[cfg(test)]
mod walk_tests;
