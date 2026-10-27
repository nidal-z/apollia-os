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
//! - **A folder chosen again.** A source whose folder connection now names
//!   another folder forgets its old index and the watch over the old folder,
//!   and is indexed in the new one: at once when the registry is told
//!   (`SourceRegistryHandle::folder_repointed`), else at the next sweep,
//!   which reads the folder connections once for every source.
//! - **After a restart.** The first pass compares the disk with what the
//!   index holds, so changes made while Apollia was stopped are caught.
//! - **With one model.** When the default embeddings model changes, every
//!   index is emptied and built again with the new one.
//! - **On this machine.** The indexes live under the data directory, never
//!   beside the documents, and the text recognised on a scanned page or a
//!   picture lives there only. The actor is given one thing that reaches
//!   beyond the disk, its [`PassageEmbedder`], whose implementation in the
//!   runtime calls the local engine over the loopback only.
//! - **Scanned pages and pictures.** They are read by optical character
//!   recognition ([`crate::Ocr`]). The first time a pass leaves pages waiting
//!   for its language data, the actor fetches the files the registry pins,
//!   checked against their digests ([`apollia_llm::ocr_data::fetch_missing`]),
//!   then reads those pages. That fetch carries nothing of the documents; it
//!   is tried once per run, and a machine without the network takes the same
//!   files from a disk.
//!
//! The actor stops when the registry that started it stops.
//!
//! [`SourceRegistryHandle::start_indexing`]: crate::SourceRegistryHandle::start_indexing

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use apollia_core::folder_connection::FolderConnectionStore;
use apollia_core::source_registry::{IndexState, Source, SourceKind};
use apollia_llm::embeddings_client::EmbeddingError;
use notify::RecommendedWatcher;
use tokio::sync::{mpsc, oneshot};

use crate::actor::RegistryLink;
use crate::index::{ChunkVectors, IndexError, SourceIndex};
use crate::ocr::Ocr;

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
    /// The folder connection of these sources now names another folder:
    /// read where they are again and index them there at once.
    Repointed(Vec<String>),
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
    /// Where the language data of recognition is fetched from; `None` in a
    /// build without the engine, which asks for nothing.
    ocr_data: Option<Arc<OcrDataFetch>>,
    /// Set once the language data of recognition has been asked of the
    /// registry, so that it is asked once per run.
    ocr_data_asked: AtomicBool,
}

/// Where the language data of recognition comes from when a pass leaves
/// pages waiting: the files the registry pins, the address that serves them,
/// and the downloader that checks each one against its digest.
pub(crate) struct OcrDataFetch {
    pub(crate) catalogue: apollia_llm::recommend::OcrCatalogue,
    pub(crate) base: String,
    pub(crate) manager: apollia_llm::DownloadManager,
}

impl OcrDataFetch {
    /// The registry shipped in the binary, over the network. `None` in a
    /// build without the engine, and when the shipped entries do not load.
    pub(crate) fn shipped() -> Option<Self> {
        if !Ocr::compiled() {
            return None;
        }
        match apollia_llm::recommend::OcrCatalogue::embedded() {
            Ok(catalogue) => Some(Self {
                catalogue,
                base: apollia_llm::ocr_data::REGISTRY_BASE.to_owned(),
                manager: apollia_llm::DownloadManager::new(),
            }),
            Err(e) => {
                tracing::event!(
                    tracing::Level::WARN,
                    error = %e,
                    "sources.indexer.ocr_catalogue_unreadable"
                );
                None
            }
        }
    }
}

/// Where the language data of recognition lives under `data_dir`.
fn tessdata_dir(data_dir: &Path) -> PathBuf {
    apollia_llm::ocr_data::tessdata_dir(&data_dir.join("models"))
}

/// What an indexer is started over, beside its registry and its embedder.
pub(crate) struct Start {
    /// The `system.db` the registry lives in.
    pub(crate) system_db: PathBuf,
    /// The directory the indexes are written under.
    pub(crate) data_dir: PathBuf,
    /// Time between two sweeps.
    pub(crate) interval: Duration,
    /// Where the language data of recognition is fetched from; `None` never
    /// fetches it.
    pub(crate) ocr_data: Option<OcrDataFetch>,
}

/// Starts the indexer over the registry `registry` and returns the handle the
/// registry keeps. A sweep runs at once, then every `start.interval`.
pub(crate) fn spawn<E: PassageEmbedder>(
    registry: RegistryLink,
    embedder: E,
    start: Start,
) -> IndexerHandle {
    let Start {
        system_db,
        data_dir,
        interval,
        ocr_data,
    } = start;
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
        ocr_data: ocr_data.map(Arc::new),
        ocr_data_asked: AtomicBool::new(false),
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
                    Some(IndexerCommand::Changed { source, paths }) => {
                        self.changed(source, paths).await;
                    }
                    Some(command) => self.serve(command).await,
                },
                Some(()) = self.rescan_rx.recv() => self.sweep(true).await,
                _ = ticker.tick() => self.sweep(false).await,
            }
        }
        tracing::event!(tracing::Level::INFO, "sources.indexer.stopped");
    }

    /// Serves a command other than a change of files.
    async fn serve(&mut self, command: IndexerCommand) {
        match command {
            IndexerCommand::Sync => self.sweep(false).await,
            IndexerCommand::Repointed(ids) => self.repointed(ids).await,
            IndexerCommand::Refresh(reply) => {
                self.sweep(true).await;
                let _ = reply.send(());
            }
            IndexerCommand::Changed { .. } => {}
        }
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
        let roots = self.resolve(&wanted).await;
        for source in wanted {
            let root = roots.get(&source.id).cloned();
            self.follow(&source, root, all || model_changed, model.clone())
                .await;
        }
    }

    /// Follows the sources `ids` to the folder their connection names now,
    /// without waiting for the sweep: a source that moved forgets its old
    /// index and the watch over its old folder, and each one is indexed
    /// again against the disk.
    async fn repointed(&mut self, ids: Vec<String>) {
        let Some(sources) = self.registry.list().await else {
            return;
        };
        let wanted: Vec<Source> = sources
            .into_iter()
            .filter(|s| matches!(s.kind, SourceKind::Folder | SourceKind::File))
            .filter(|s| ids.contains(&s.id))
            .collect();
        let model = match &self.model {
            Some(model) => model.clone(),
            None => self.current_model().await,
        };
        let roots = self.resolve(&wanted).await;
        for source in wanted {
            let root = roots.get(&source.id).cloned();
            self.follow(&source, root, true, model.clone()).await;
        }
    }

    /// Keeps `source` in step with `root`, where its files are now: a root
    /// changed since it was first read drops its old index and watch, a
    /// source not kept yet starts being kept, and a full pass runs over a
    /// new one, or over any one when `again` is set.
    async fn follow(
        &mut self,
        source: &Source,
        root: Option<PathBuf>,
        again: bool,
        model: Option<String>,
    ) {
        let moved = match (self.tracked.get(&source.id), &root) {
            (Some(kept), Some(root)) => kept.root != *root,
            _ => false,
        };
        if moved {
            self.untrack(&source.id);
        }
        let new = !self.tracked.contains_key(&source.id);
        if new && !self.track(source, root).await {
            return;
        }
        if new || again {
            self.full_pass(&source.id, model).await;
        }
    }

    /// Where the files of each of `sources` are now, read in one go under
    /// `spawn_blocking`: the folder connections are read once, whatever
    /// the number of sources.
    async fn resolve(&self, sources: &[Source]) -> HashMap<String, PathBuf> {
        let system_db = self.system_db.clone();
        let sources = sources.to_vec();
        tokio::task::spawn_blocking(move || roots_of(&system_db, &sources))
            .await
            .unwrap_or_default()
    }

    /// Starts keeping `source`, whose files are at `root`, and watches
    /// them. `false`, its state set to `error`, when its files could not be
    /// found.
    async fn track(&mut self, source: &Source, root: Option<PathBuf>) -> bool {
        let Some(root) = root else {
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
        let outcome = self.pass(id, root.clone(), scopes, model.clone()).await;
        self.registry
            .set_state(id, state_after(outcome.as_ref(), model.as_deref()))
            .await;
        self.registry.indexed(id, root).await;
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
            self.serve(command).await;
        }
    }

    /// Asks the registry, once per run, for the language data of recognition
    /// that is missing, then has every source read again. A build without
    /// the engine asks for nothing. A failure is traced: the pages stay
    /// marked as waiting, and the data can be imported from a disk.
    fn fetch_ocr_data(&self) {
        let Some(fetch) = self.ocr_data.clone() else {
            return;
        };
        if self.ocr_data_asked.swap(true, Ordering::AcqRel) {
            return;
        }
        let dir = tessdata_dir(&self.data_dir);
        let rescan = self.rescan_tx.clone();
        tokio::spawn(async move {
            let fetched = apollia_llm::ocr_data::fetch_missing(
                &dir,
                &fetch.catalogue,
                &fetch.manager,
                &fetch.base,
            )
            .await;
            match fetched {
                Ok(languages) => {
                    tracing::event!(
                        tracing::Level::INFO,
                        fetched = languages.len(),
                        "sources.indexer.ocr_data_fetched"
                    );
                    if !languages.is_empty() {
                        let _ = rescan.try_send(());
                    }
                }
                Err(error) => tracing::event!(
                    tracing::Level::WARN,
                    error = %error,
                    "sources.indexer.ocr_data_unavailable"
                ),
            }
        });
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
            let ocr = Ocr::open(&tessdata_dir(&data_dir));
            let means = pass::Means {
                model: model.as_deref(),
                embed: &embed,
                ocr: &ocr,
            };
            pass::run(&mut index, &root, &scopes, &means)
        })
        .await;
        match outcome {
            Ok(Ok(outcome)) => {
                if outcome.awaiting_ocr > 0 {
                    self.fetch_ocr_data();
                }
                tracing::event!(
                    tracing::Level::INFO,
                    source = %id,
                    indexed = outcome.indexed,
                    removed = outcome.removed,
                    skipped = outcome.skipped,
                    failed = outcome.failed,
                    rebuilt = outcome.rebuilt,
                    awaiting_ocr = outcome.awaiting_ocr,
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

/// Where the files of each of `sources` are: the folder of its connection,
/// or its file. The folder connections are read once, and only when a folder
/// source is among them. A source whose connection is gone, or whose store
/// cannot be read, is left out.
pub(crate) fn roots_of(system_db: &Path, sources: &[Source]) -> HashMap<String, PathBuf> {
    let folders: HashMap<String, String> = if sources.iter().any(|s| s.kind == SourceKind::Folder) {
        FolderConnectionStore::open(system_db)
            .and_then(|store| store.list())
            .map(|all| all.into_iter().map(|c| (c.id, c.path)).collect())
            .unwrap_or_default()
    } else {
        HashMap::new()
    };
    let mut roots = HashMap::new();
    for source in sources {
        let Some(reference) = source.reference.as_deref() else {
            continue;
        };
        let path = match source.kind {
            SourceKind::Folder => {
                let id = reference.strip_prefix("folder:").unwrap_or(reference);
                let Some(path) = folders.get(id) else {
                    continue;
                };
                PathBuf::from(path)
            }
            SourceKind::File => PathBuf::from(reference),
            _ => continue,
        };
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        roots.insert(source.id.clone(), path);
    }
    roots
}

#[cfg(test)]
mod tests;
