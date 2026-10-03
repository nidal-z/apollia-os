//! Node.js through the engine store.
//!
//! Node.js is not shipped inside Apollia. A pinned LTS release is published in
//! the engine manifest of the channel (engine `nodejs`, variant `cpu`), and it
//! is installed the first time a tool server launched by `node` or `npx` needs
//! it: the manifest signature is verified before any archive is requested, the
//! archive digest is compared, and the archive is extracted under
//! `<data_dir>/engines/nodejs/<tag>/cpu/`. A machine without network imports
//! it from a disk with [`crate::engine_store::import_from_dir`], through the
//! same verification. The `stdio` transport of `apollia-mcp` then launches the
//! server on that Node.js by its absolute path.
//!
//! [`ensure`] starts the download at most once per store and answers at once;
//! [`state`] reads where it stands. The download runs on its own task, which
//! is the only writer of its progress, published through a `watch` channel.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use tokio::sync::watch;

use crate::engine_store::fetch::install_package_with_progress;
use crate::engine_store::installed::{newest_installed, InstalledPackage};
use crate::engine_store::manifest::{Engine, Variant};
use crate::engine_store::settings::EngineStoreSettings;
use crate::engine_store::EngineStore;

/// The variant every Node.js package is published under.
pub const NODE_VARIANT: Variant = Variant::Cpu;

/// Where Node.js stands on this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
#[schema(as = NodeRuntimeState)]
pub enum NodeState {
    /// Not installed, and no download running.
    Absent,
    /// Being downloaded.
    Downloading {
        /// Bytes of the archive received so far.
        received: u64,
        /// Size of the archive the verified manifest states; 0 until the
        /// manifest is verified.
        total: u64,
    },
    /// Installed and verified.
    Ready {
        /// Release tag, `v22.11.0`.
        tag: String,
        /// Directory it was extracted into.
        path: String,
    },
    /// The last download failed; asking for Node.js again retries it.
    Failed {
        /// Why, readable as is.
        error: String,
    },
}

/// The download of each store, by store root. An entry lives from [`ensure`]
/// until its outcome is read; the lock is held for a lookup or an insertion,
/// never across an await.
static JOBS: OnceLock<Mutex<HashMap<PathBuf, watch::Receiver<NodeState>>>> = OnceLock::new();

fn jobs() -> &'static Mutex<HashMap<PathBuf, watch::Receiver<NodeState>>> {
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The newest Node.js installed under `root`, the engine store root.
#[must_use]
pub fn installed_node(root: &Path) -> Option<InstalledPackage> {
    newest_installed(root, Engine::NodeJs, NODE_VARIANT)
}

fn ready(package: &InstalledPackage) -> NodeState {
    NodeState::Ready {
        tag: package.tag.clone(),
        path: package.path.display().to_string(),
    }
}

/// Where Node.js stands under `root`, the engine store root.
#[must_use]
pub fn state(root: &Path) -> NodeState {
    let running = jobs()
        .lock()
        .ok()
        .and_then(|map| map.get(root).map(|rx| rx.borrow().clone()));
    match running {
        Some(s @ (NodeState::Downloading { .. } | NodeState::Failed { .. })) => s,
        _ => installed_node(root).map_or(NodeState::Absent, |p| ready(&p)),
    }
}

/// Start installing Node.js into `store` unless it is installed or already
/// downloading, and answer where it stands. A failed download is tried again.
#[must_use]
pub fn ensure(store: &EngineStore, settings: &EngineStoreSettings) -> NodeState {
    if let Some(package) = installed_node(&store.root) {
        return ready(&package);
    }
    let Ok(mut map) = jobs().lock() else {
        return NodeState::Failed {
            error: "the Node.js download registry is unavailable".to_owned(),
        };
    };
    if let Some(rx) = map.get(&store.root) {
        let current = rx.borrow().clone();
        if matches!(current, NodeState::Downloading { .. }) {
            return current;
        }
    }
    let start = NodeState::Downloading {
        received: 0,
        total: 0,
    };
    let (tx, rx) = watch::channel(start.clone());
    map.insert(store.root.clone(), rx);
    drop(map);
    let store = store.clone();
    let settings = settings.clone();
    tokio::spawn(async move {
        tracing::event!(
            tracing::Level::INFO,
            base_url = %settings.base_url,
            "engine_store.node.download.started"
        );
        let progress = |received, total| {
            tx.send_replace(NodeState::Downloading { received, total });
        };
        let outcome = install_package_with_progress(
            &store,
            &settings,
            Engine::NodeJs,
            NODE_VARIANT,
            progress,
        )
        .await;
        let end = match outcome {
            Ok(package) => {
                tracing::event!(
                    tracing::Level::INFO,
                    tag = %package.tag,
                    "engine_store.node.download.ready"
                );
                ready(&package)
            }
            Err(error) => {
                tracing::event!(
                    tracing::Level::WARN,
                    error = %error,
                    "engine_store.node.download.failed"
                );
                NodeState::Failed {
                    error: error.to_string(),
                }
            }
        };
        tx.send_replace(end);
    });
    start
}

#[cfg(test)]
mod tests;
