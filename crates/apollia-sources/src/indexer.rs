//! The actor that keeps the index of each folder and file source alive.
//!
//! It is started by [`SourceRegistryHandle::start_indexing`] and owns, for
//! every source of kind `folder` or `file`, the watch over its files. It runs
//! on the async runtime and serves a bounded channel; the reading, cutting and
//! writing of files run under `spawn_blocking`.
//!
//! - **In the background.** A source added to the registry is indexed at once,
//!   its state going from `indexing` to `ready` (or `keywords_only` without an
//!   embeddings model, `error` when vectors could not be computed).
//! - **Following the files.** The watcher reports each change; the pass that
//!   follows reads again only the files whose date and then fingerprint
//!   changed, and drops the documents whose file is gone ([`pass`]).
//! - **After a restart.** The first pass compares the disk with what the
//!   index holds, so changes made while Apollia was stopped are caught.
//! - **With one model.** When the default embeddings model changes, every
//!   index is emptied and built again with the new one.
//! - **On this machine.** The indexes live under the data directory, never
//!   beside the documents. The only thing the actor is given that reaches
//!   beyond the disk is its [`PassageEmbedder`], whose implementation in the
//!   runtime calls the local engine over the loopback only.
//!
//! The actor stops when the registry that started it stops.
//!
//! [`SourceRegistryHandle::start_indexing`]: crate::SourceRegistryHandle::start_indexing

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use apollia_core::folder_connection::FolderConnectionStore;
use apollia_core::source_registry::{IndexState, Source, SourceKind};
use apollia_llm::embeddings_client::EmbeddingError;
use notify::RecommendedWatcher;
use tokio::sync::{mpsc, oneshot};

use crate::actor::RegistryLink;
use crate::index::{ChunkVectors, IndexError, SourceIndex};

pub mod extract;
mod pass;
mod watch;

pub use pass::{PassOutcome, MAX_INDEXED_FILE_BYTES};

/// Capacity of the indexer's channel. A watcher that finds it full asks for
/// one pass over every source instead of waiting.
pub const INDEXER_CAPACITY: usize = 256;

/// Time between two sweeps: the registry and the default embeddings model
/// are read again, and a change of model rebuilds every index.
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(60);

/// Time the indexer waits after a change before reading it, so that the
/// burst of events one save makes is read as one change.
const SETTLE_DELAY: Duration = Duration::from_millis(250);

/// Computes the vectors of the passages an index stores.
///
/// The runtime implements it over the local engine, reached on the loopback
/// only. It is the one thing the indexer is handed that leaves the disk.
pub trait PassageEmbedder: Send + Sync + 'static {
    /// Identifier of the default embeddings model, or the error that says
    /// there is none to use ([`EmbeddingError::falls_back_to_keywords`]).
    fn model(&self) -> impl Future<Output = Result<String, EmbeddingError>> + Send;

    /// One vector per text, embedded as passages, with the model's
    /// identifier.
    fn embed_passages(
        &self,
        texts: &[String],
    ) -> impl Future<Output = Result<ChunkVectors, EmbeddingError>> + Send;
}

/// A request to the indexer.
pub(crate) enum IndexerCommand {
    /// The registry changed: read it again.
    Sync,
    /// Files of `source` changed on the disk.
    Changed {
        /// The source.
        source: String,
        /// The paths the watcher reported.
        paths: Vec<PathBuf>,
    },
    /// Index every source again against the disk, then answer.
    Refresh(oneshot::Sender<()>),
}

/// The registry's way to reach the indexer.
#[derive(Clone, Debug)]
pub(crate) struct IndexerHandle {
    tx: mpsc::Sender<IndexerCommand>,
}

impl IndexerHandle {
    /// Hands `command` over without waiting. A full channel drops it: the
    /// next sweep reads the registry and the disk again and covers it.
    pub(crate) fn send(&self, command: IndexerCommand) {
        if self.tx.try_send(command).is_err() {
            tracing::event!(tracing::Level::DEBUG, "sources.indexer.request_dropped");
        }
    }
}

/// A source the indexer keeps: where its files are, and the watch over them.
struct Tracked {
    root: PathBuf,
    _watcher: Option<RecommendedWatcher>,
}

struct Indexer<E> {
    registry: RegistryLink,
    system_db: PathBuf,
    data_dir: PathBuf,
    embedder: Arc<E>,
    rx: mpsc::Receiver<IndexerCommand>,
    weak: mpsc::WeakSender<IndexerCommand>,
    rescan_tx: mpsc::Sender<()>,
    rescan_rx: mpsc::Receiver<()>,
    tracked: HashMap<String, Tracked>,
    model: Option<Option<String>>,
}

/// Starts the indexer over the registry `registry` of the `system.db` at
/// `system_db`, its indexes under `data_dir`, and returns the handle the
/// registry keeps. A sweep runs at once, then every `interval`.
pub(crate) fn spawn<E: PassageEmbedder>(
    registry: RegistryLink,
    system_db: PathBuf,
    data_dir: PathBuf,
    embedder: E,
    interval: Duration,
) -> IndexerHandle {
    let (tx, rx) = mpsc::channel(INDEXER_CAPACITY);
    let (rescan_tx, rescan_rx) = mpsc::channel(1);
    let indexer = Indexer {
        registry,
        system_db,
        data_dir,
        embedder: Arc::new(embedder),
        rx,
        weak: tx.downgrade(),
        rescan_tx,
        rescan_rx,
        tracked: HashMap::new(),
        model: None,
    };
    tokio::spawn(indexer.run(interval));
    IndexerHandle { tx }
}

impl<E: PassageEmbedder> Indexer<E> {
    async fn run(mut self, interval: Duration) {
        tracing::event!(tracing::Level::INFO, "sources.indexer.started");
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                command = self.rx.recv() => match command {
                    None => break,
                    Some(IndexerCommand::Sync) => self.sweep(false).await,
                    Some(IndexerCommand::Refresh(reply)) => {
                        self.sweep(true).await;
                        let _ = reply.send(());
                    }
                    Some(IndexerCommand::Changed { source, paths }) => {
                        self.changed(source, paths).await;
                    }
                },
                Some(()) = self.rescan_rx.recv() => self.sweep(true).await,
                _ = ticker.tick() => self.sweep(false).await,
            }
        }
        tracing::event!(tracing::Level::INFO, "sources.indexer.stopped");
    }

    /// The default model, `None` when there is none to use.
    async fn current_model(&self) -> Option<String> {
        match self.embedder.model().await {
            Ok(model) => Some(model),
            Err(e) => {
                if !e.falls_back_to_keywords() {
                    tracing::event!(
                        tracing::Level::WARN,
                        error = %e,
                        "sources.indexer.model_unreadable"
                    );
                }
                None
            }
        }
    }

    /// Reads the registry and the model again: stops keeping the sources
    /// removed, starts keeping the new ones, and runs a full pass over the
    /// new ones, or over every one when `all` is set or the model changed.
    async fn sweep(&mut self, all: bool) {
        let Some(sources) = self.registry.list().await else {
            return;
        };
        let model = self.current_model().await;
        let model_changed = self.model.as_ref().is_some_and(|m| *m != model);
        self.model = Some(model.clone());
        let wanted: Vec<Source> = sources
            .into_iter()
            .filter(|s| matches!(s.kind, SourceKind::Folder | SourceKind::File))
            .collect();
        let ids: HashSet<&str> = wanted.iter().map(|s| s.id.as_str()).collect();
        let gone: Vec<String> = self
            .tracked
            .keys()
            .filter(|id| !ids.contains(id.as_str()))
            .cloned()
            .collect();
        for id in gone {
            self.untrack(&id);
        }
        for source in wanted {
            let new = !self.tracked.contains_key(&source.id);
            if new && !self.track(&source).await {
                continue;
            }
            if new || all || model_changed {
                self.full_pass(&source.id, model.clone()).await;
            }
        }
    }

    /// Starts keeping `source`: resolves where its files are and watches
    /// them. `false`, its state set to `error`, when its files cannot be
    /// found.
    async fn track(&mut self, source: &Source) -> bool {
        let system_db = self.system_db.clone();
        let resolved = {
            let source = source.clone();
            tokio::task::spawn_blocking(move || root_of(&system_db, &source))
                .await
                .ok()
                .flatten()
        };
        let Some(root) = resolved else {
            tracing::event!(
                tracing::Level::WARN,
                source = %source.id,
                "sources.indexer.root_unresolved"
            );
            self.registry.set_state(&source.id, IndexState::Error).await;
            return false;
        };
        let watcher = watch::watch(&source.id, &root, self.weak.clone(), self.rescan_tx.clone());
        self.tracked.insert(
            source.id.clone(),
            Tracked {
                root,
                _watcher: watcher,
            },
        );
        true
    }

    /// Stops keeping the source `id` and removes its index, which a pass
    /// running when it was deleted may have written again.
    fn untrack(&mut self, id: &str) {
        self.tracked.remove(id);
        if let Err(e) = crate::index::remove_index(&self.data_dir, id) {
            tracing::event!(
                tracing::Level::WARN,
                source = %id,
                error = %e,
                "sources.index.remove_failed"
            );
        }
    }

    /// Indexes the whole of the source `id` against the disk.
    async fn full_pass(&mut self, id: &str, model: Option<String>) {
        let Some(root) = self.tracked.get(id).map(|t| t.root.clone()) else {
            return;
        };
        self.registry.set_state(id, IndexState::Indexing).await;
        let scopes = vec![root.clone()];
        let outcome = self.pass(id, root, scopes, model.clone()).await;
        self.registry
            .set_state(id, state_after(outcome.as_ref(), model.as_deref()))
            .await;
    }

    /// Reads again the files the watcher reported for `source`, with the
    /// changes that arrive while it settles.
    async fn changed(&mut self, source: String, paths: Vec<PathBuf>) {
        tokio::time::sleep(SETTLE_DELAY).await;
        let mut by_source: HashMap<String, Vec<PathBuf>> = HashMap::new();
        by_source.entry(source).or_default().extend(paths);
        let mut later = Vec::new();
        while let Ok(command) = self.rx.try_recv() {
            match command {
                IndexerCommand::Changed { source, paths } => {
                    by_source.entry(source).or_default().extend(paths);
                }
                other => later.push(other),
            }
        }
        let model = match &self.model {
            Some(model) => model.clone(),
            None => self.current_model().await,
        };
        for (id, paths) in by_source {
            let Some(root) = self.tracked.get(&id).map(|t| t.root.clone()) else {
                continue;
            };
            let scopes = scopes_of(&root, paths);
            if scopes.is_empty() {
                continue;
            }
            let outcome = self.pass(&id, root, scopes, model.clone()).await;
            self.registry
                .set_state(&id, state_after(outcome.as_ref(), model.as_deref()))
                .await;
        }
        for command in later {
            match command {
                IndexerCommand::Sync => self.sweep(false).await,
                IndexerCommand::Refresh(reply) => {
                    self.sweep(true).await;
                    let _ = reply.send(());
                }
                IndexerCommand::Changed { .. } => {}
            }
        }
    }

    /// One pass over `scopes` of the source `id`, under `spawn_blocking`.
    async fn pass(
        &self,
        id: &str,
        root: PathBuf,
        scopes: Vec<PathBuf>,
        model: Option<String>,
    ) -> Result<PassOutcome, String> {
        let data_dir = self.data_dir.clone();
        let source = id.to_owned();
        let embedder = Arc::clone(&self.embedder);
        let runtime = tokio::runtime::Handle::current();
        let outcome = tokio::task::spawn_blocking(move || -> Result<PassOutcome, IndexError> {
            let mut index = SourceIndex::open(&data_dir, &source)?;
            let embed = |texts: &[String]| runtime.block_on(embedder.embed_passages(texts));
            pass::run(&mut index, &root, &scopes, model.as_deref(), &embed)
        })
        .await;
        match outcome {
            Ok(Ok(outcome)) => {
                tracing::event!(
                    tracing::Level::INFO,
                    source = %id,
                    indexed = outcome.indexed,
                    removed = outcome.removed,
                    skipped = outcome.skipped,
                    failed = outcome.failed,
                    rebuilt = outcome.rebuilt,
                    "sources.indexer.pass"
                );
                Ok(outcome)
            }
            Ok(Err(e)) => {
                tracing::event!(
                    tracing::Level::WARN,
                    source = %id,
                    error = %e,
                    "sources.indexer.pass_failed"
                );
                Err(e.to_string())
            }
            Err(e) => {
                tracing::event!(
                    tracing::Level::WARN,
                    source = %id,
                    error = %e,
                    "sources.indexer.pass_failed"
                );
                Err(e.to_string())
            }
        }
    }
}

/// The state of an index after a pass: `error` when the pass failed or a
/// file could not be given its vectors, `ready` with a model, else
/// `keywords_only`.
fn state_after(outcome: Result<&PassOutcome, &String>, model: Option<&str>) -> IndexState {
    match outcome {
        Err(_) => IndexState::Error,
        Ok(outcome) if outcome.failed > 0 => IndexState::Error,
        Ok(_) if model.is_some() => IndexState::Ready,
        Ok(_) => IndexState::KeywordsOnly,
    }
}

/// The scopes a set of reported paths asks for: the paths under `root`,
/// hidden ones left out, and `root` alone when it is among them.
fn scopes_of(root: &Path, paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut scopes: Vec<PathBuf> = Vec::new();
    for path in paths {
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let hidden = relative
            .components()
            .any(|c| c.as_os_str().to_str().is_some_and(|n| n.starts_with('.')));
        if hidden {
            continue;
        }
        if relative.as_os_str().is_empty() {
            return vec![root.to_path_buf()];
        }
        if !scopes.contains(&path) {
            scopes.push(path);
        }
    }
    scopes
}

/// Where the files of `source` are: the folder of its connection, or its
/// file. `None` when the connection is gone or the store cannot be read.
fn root_of(system_db: &Path, source: &Source) -> Option<PathBuf> {
    let reference = source.reference.as_deref()?;
    let path = match source.kind {
        SourceKind::Folder => {
            let id = reference.strip_prefix("folder:").unwrap_or(reference);
            let store = FolderConnectionStore::open(system_db).ok()?;
            PathBuf::from(store.get(id).ok()??.path)
        }
        SourceKind::File => PathBuf::from(reference),
        _ => return None,
    };
    Some(std::fs::canonicalize(&path).unwrap_or(path))
}

#[cfg(test)]
mod tests;
