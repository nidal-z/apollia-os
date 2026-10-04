//! Watching the files of a source, so its index follows them.
//!
//! A folder is watched recursively; a file is watched through its parent
//! folder, since an editor often saves by writing a new file and renaming it
//! over the old one, which a watch on the file itself would lose. The
//! watcher's thread hands each change to the indexer's bounded channel, which
//! it holds weakly so that the indexer stops with its registry, and never
//! waits: when the channel is full, it asks once for a pass over every
//! source instead, through a channel of one place, and a pass already asked
//! for covers it.

use std::path::{Path, PathBuf};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::indexer::IndexerCommand;

/// Starts watching `root` for the source `source`: recursively for a folder,
/// through its parent for a file. `None`, with a trace, when the watch cannot
/// be set: the index is then kept by the passes alone.
pub(super) fn watch(
    source: &str,
    root: &Path,
    commands: mpsc::WeakSender<IndexerCommand>,
    rescan: mpsc::Sender<()>,
) -> Option<RecommendedWatcher> {
    let is_dir = root.is_dir();
    let (target, mode, only) = if is_dir {
        (root.to_path_buf(), RecursiveMode::Recursive, None)
    } else {
        let parent = root.parent()?.to_path_buf();
        (
            parent,
            RecursiveMode::NonRecursive,
            Some(root.to_path_buf()),
        )
    };
    let id = source.to_owned();
    let handler = move |event: notify::Result<notify::Event>| {
        let paths: Vec<PathBuf> = match event {
            Ok(event) if matches!(event.kind, EventKind::Access(_)) => return,
            Ok(event) => event
                .paths
                .into_iter()
                .filter(|p| only.as_ref().is_none_or(|only| p == only))
                .collect(),
            Err(_) => {
                // Changes were lost: one pass over everything settles them.
                let _ = rescan.try_send(());
                return;
            }
        };
        if paths.is_empty() {
            return;
        }
        // The indexer is gone once nothing upgrades: the watch is moot.
        let Some(commands) = commands.upgrade() else {
            return;
        };
        let command = IndexerCommand::Changed {
            source: id.clone(),
            paths,
        };
        if let Err(mpsc::error::TrySendError::Full(_)) = commands.try_send(command) {
            let _ = rescan.try_send(());
        }
    };
    let mut watcher = match notify::recommended_watcher(handler) {
        Ok(watcher) => watcher,
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                source = %source,
                error = %e,
                "sources.indexer.watch_failed"
            );
            return None;
        }
    };
    if let Err(e) = watcher.watch(&target, mode) {
        tracing::event!(
            tracing::Level::WARN,
            source = %source,
            error = %e,
            "sources.indexer.watch_failed"
        );
        return None;
    }
    Some(watcher)
}
