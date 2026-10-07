//! One installation at a time per store, followed from outside.
//!
//! Installing an engine takes minutes, and the screen that asks for it has to
//! show where it stands: the signature being verified, then the archive
//! arriving, then the outcome. [`start_download`] and [`start_import`] start
//! the work on its own task and answer at once; [`state`] reads where it
//! stands; [`dismiss`] cancels a download or forgets an outcome that was read.
//!
//! The task is the only writer of its progress, published through a `watch`
//! channel. A refusal keeps its [`RefusalCode`] and whether an archive had
//! been requested when it happened, so a reader can say that nothing was
//! downloaded without guessing it from a sentence.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::engine_store::fetch::{import_from_dir_with_progress, install_package_with_progress};
use crate::engine_store::installed::newest_installed;
use crate::engine_store::manifest::{Engine, Variant};
use crate::engine_store::settings::EngineStoreSettings;
use crate::engine_store::{EngineStore, EngineStoreError, RefusalCode};

/// What an installation is, for the one who asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = EngineInstallationKind)]
pub enum InstallKind {
    /// The first download of a variant.
    Install,
    /// The download of a newer build of a variant already installed.
    Update,
    /// An import from a directory on a disk.
    Import,
}

/// Where the installation of a store stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
#[schema(as = EngineInstallationState)]
pub enum InstallationState {
    /// Nothing running, and no outcome waiting to be read.
    Idle,
    /// The manifest signature is being verified. No archive was requested.
    Verifying {
        /// What is being done.
        kind: InstallKind,
    },
    /// The signature held and the archive is arriving.
    Downloading {
        /// What is being done.
        kind: InstallKind,
        /// Bytes of the archive received so far.
        received: u64,
        /// Size of the archive the verified manifest states.
        total: u64,
    },
    /// The signature held and the archives of the directory are being
    /// compared to their digests and extracted.
    Installing {
        /// What is being done.
        kind: InstallKind,
    },
    /// Verified and installed. The next model load takes it.
    Installed {
        /// What was done.
        kind: InstallKind,
    },
    /// Refused or failed. Nothing was installed.
    Failed {
        /// What was being done.
        kind: InstallKind,
        /// Why, as a stable word.
        code: RefusalCode,
        /// Why, readable as is.
        error: String,
        /// True when an archive had been requested from the mirror: false
        /// means the refusal came before any download.
        archive_requested: bool,
    },
}

impl InstallationState {
    /// True while a task is working.
    #[must_use]
    pub fn is_running(&self) -> bool {
        matches!(
            self,
            Self::Verifying { .. } | Self::Downloading { .. } | Self::Installing { .. }
        )
    }
}

struct Job {
    state: watch::Receiver<InstallationState>,
    task: Option<JoinHandle<()>>,
}

/// The installation of each store, by store root. The lock is held for a
/// lookup, an insertion or a removal, never across an await.
static JOBS: OnceLock<Mutex<HashMap<PathBuf, Job>>> = OnceLock::new();

fn jobs() -> &'static Mutex<HashMap<PathBuf, Job>> {
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn failed(
    kind: InstallKind,
    error: &EngineStoreError,
    archive_requested: bool,
) -> InstallationState {
    tracing::event!(
        tracing::Level::WARN,
        kind = ?kind,
        code = ?error.code(),
        error = %error,
        "engine_store.installation.refused"
    );
    InstallationState::Failed {
        kind,
        code: error.code(),
        error: error.to_string(),
        archive_requested,
    }
}

/// Where the installation of the store under `root` stands.
#[must_use]
pub fn state(root: &Path) -> InstallationState {
    jobs()
        .lock()
        .ok()
        .and_then(|map| map.get(root).map(|job| job.state.borrow().clone()))
        .unwrap_or(InstallationState::Idle)
}

/// The progress of the installation of `root`, to wait on a state.
#[cfg(test)]
pub(crate) fn subscribe(root: &Path) -> Option<watch::Receiver<InstallationState>> {
    jobs()
        .lock()
        .ok()
        .and_then(|map| map.get(root).map(|job| job.state.clone()))
}

/// Record, as the outcome of the store under `root`, a refusal that happened
/// before any task could start. A running installation is left alone.
pub fn refuse(root: &Path, kind: InstallKind, error: &EngineStoreError) -> InstallationState {
    let Ok(mut map) = jobs().lock() else {
        return failed(kind, error, false);
    };
    if let Some(job) = map.get(root) {
        let current = job.state.borrow().clone();
        if current.is_running() {
            return current;
        }
    }
    let outcome = failed(kind, error, false);
    let (_tx, rx) = watch::channel(outcome.clone());
    map.insert(
        root.to_path_buf(),
        Job {
            state: rx,
            task: None,
        },
    );
    outcome
}

/// Run `work` as the installation of `root` unless one is running, and
/// answer where it stands. `work` receives the sender it publishes through.
fn start<F, Fut>(root: &Path, first: InstallationState, work: F) -> InstallationState
where
    F: FnOnce(watch::Sender<InstallationState>) -> Fut,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let Ok(mut map) = jobs().lock() else {
        return state(root);
    };
    if let Some(job) = map.get(root) {
        let current = job.state.borrow().clone();
        if current.is_running() {
            return current;
        }
    }
    let (tx, rx) = watch::channel(first.clone());
    let task = tokio::spawn(work(tx));
    map.insert(
        root.to_path_buf(),
        Job {
            state: rx,
            task: Some(task),
        },
    );
    first
}

/// Start downloading the newest `engine` `variant` build of the followed
/// channel into `store`, unless an installation is already running there,
/// and answer where it stands.
#[must_use]
pub fn start_download(
    store: &EngineStore,
    settings: &EngineStoreSettings,
    engine: Engine,
    variant: Variant,
) -> InstallationState {
    let kind = if newest_installed(&store.root, engine, variant).is_some() {
        InstallKind::Update
    } else {
        InstallKind::Install
    };
    let (store, settings) = (store.clone(), settings.clone());
    let root = store.root.clone();
    start(
        &root,
        InstallationState::Verifying { kind },
        move |tx| async move {
            tracing::event!(
                tracing::Level::INFO,
                engine = engine.as_str(),
                variant = variant.as_str(),
                kind = ?kind,
                "engine_store.installation.started"
            );
            let progress = |received, total| {
                tx.send_replace(InstallationState::Downloading {
                    kind,
                    received,
                    total,
                });
            };
            let outcome =
                install_package_with_progress(&store, &settings, engine, variant, progress).await;
            let archive_requested = matches!(*tx.borrow(), InstallationState::Downloading { .. });
            tx.send_replace(match outcome {
                Ok(_) => InstallationState::Installed { kind },
                Err(error) => failed(kind, &error, archive_requested),
            });
        },
    )
}

/// Start importing the engines of `dir` into `store`, unless an installation
/// is already running there, and answer where it stands.
#[must_use]
pub fn start_import(store: &EngineStore, dir: &Path) -> InstallationState {
    let kind = InstallKind::Import;
    let (store, dir) = (store.clone(), dir.to_path_buf());
    let root = store.root.clone();
    start(
        &root,
        InstallationState::Verifying { kind },
        move |tx| async move {
            let progress = tx.clone();
            let imported = tokio::task::spawn_blocking(move || {
                import_from_dir_with_progress(&store, &dir, || {
                    progress.send_replace(InstallationState::Installing { kind });
                })
            })
            .await;
            tx.send_replace(match imported {
                Ok(Ok(_)) => InstallationState::Installed { kind },
                Ok(Err(error)) => failed(kind, &error, false),
                Err(join) => InstallationState::Failed {
                    kind,
                    code: RefusalCode::Storage,
                    error: join.to_string(),
                    archive_requested: false,
                },
            });
        },
    )
}

/// Cancel the download of the store under `root`, or forget its outcome, and
/// answer where it then stands.
///
/// A download is stopped and the archive it was receiving is deleted. A
/// signature being verified and an import being extracted are left to end:
/// they are short, and stopping an extraction would leave half an engine.
pub async fn dismiss(root: &Path) -> InstallationState {
    let task = {
        let Ok(mut map) = jobs().lock() else {
            return state(root);
        };
        let Some(job) = map.get(root) else {
            return InstallationState::Idle;
        };
        let current = job.state.borrow().clone();
        if matches!(
            current,
            InstallationState::Verifying { .. } | InstallationState::Installing { .. }
        ) {
            return current;
        }
        map.remove(root).and_then(|job| job.task)
    };
    if let Some(task) = task {
        task.abort();
        // The archive is deleted when the task is dropped: wait for it.
        let _ = task.await;
    }
    InstallationState::Idle
}
