//! The connections in pause, as a tool reads them at each of its calls.
//!
//! The tools of a turn are built once, when the turn starts, and a connection
//! can be paused while it runs. The decision on each call reads the pauses
//! governance publishes at that call; a file tool or the source search reads
//! the same published state at the start of its call, through a
//! [`PauseReader`], so a folder paused during the turn is served by neither.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use crate::folder_access::StoredFolder;

/// Reads the connections in pause (`folder:<id>`, `mcp:<name>`...) as
/// governance publishes them when it is asked.
pub trait PauseReader: Send + Sync {
    /// The connections in pause now.
    fn paused(&self) -> HashSet<String>;
}

/// The stored folders whose connection is in pause, read again at each call.
#[derive(Clone)]
pub struct LivePauses {
    reader: Arc<dyn PauseReader>,
    folders: Vec<StoredFolder>,
}

impl std::fmt::Debug for LivePauses {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LivePauses")
            .field("folders", &self.folders)
            .finish_non_exhaustive()
    }
}

impl LivePauses {
    /// The pauses `reader` reads, over the stored `folders`.
    #[must_use]
    pub fn new(reader: Arc<dyn PauseReader>, folders: Vec<StoredFolder>) -> Self {
        Self { reader, folders }
    }

    /// The paths of the stored folders whose connection is in pause now.
    #[must_use]
    pub fn folders(&self) -> Vec<PathBuf> {
        let paused = self.reader.paused();
        self.folders
            .iter()
            .filter(|folder| paused.contains(&folder.connection))
            .map(|folder| folder.path.clone())
            .collect()
    }
}

/// The sources held by a connection, whose pause is read again at each call
/// of the source search.
#[derive(Clone)]
pub struct LiveSourcePauses {
    reader: Arc<dyn PauseReader>,
    /// Each source the search may reach that a connection holds, with that
    /// connection: `(source id, connection id)`.
    holders: Vec<(String, String)>,
}

impl std::fmt::Debug for LiveSourcePauses {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveSourcePauses")
            .field("holders", &self.holders)
            .finish_non_exhaustive()
    }
}

impl LiveSourcePauses {
    /// The pauses `reader` reads, over the sources `holders` names with the
    /// connection that holds each.
    #[must_use]
    pub fn new(reader: Arc<dyn PauseReader>, holders: Vec<(String, String)>) -> Self {
        Self { reader, holders }
    }

    /// The sources whose connection is in pause now.
    #[must_use]
    pub fn sources(&self) -> Vec<String> {
        let paused = self.reader.paused();
        self.holders
            .iter()
            .filter(|(_, connection)| paused.contains(connection))
            .map(|(source, _)| source.clone())
            .collect()
    }
}
