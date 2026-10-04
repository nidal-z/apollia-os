//! The actor that owns the registry of sources.
//!
//! One thread holds the connection to `system.db` and serves the commands of a
//! bounded channel in order; callers hold a clonable [`SourceRegistryHandle`].
//! SQLite calls block, so the actor runs on a thread of its own rather than on
//! the async workers. It stops when the last handle is dropped.
//!
//! Once [`SourceRegistryHandle::start_indexing`] has started the indexer
//! ([`crate::indexer`]), the actor tells it of every source added or removed;
//! the indexer reaches back through a weak link, so it stops with the
//! registry.

use std::path::{Path, PathBuf};
use std::time::Duration;

use apollia_core::source_registry::{
    IndexState, NewSource, Source, SourceRegistryError, SourceRegistryStore,
};
use serde::Serialize;
use tokio::sync::{mpsc, oneshot};

use crate::checked::{checked_sources, CheckedSource};
use crate::index::{IndexError, SkippedFile, SourceIndex};
use crate::indexer::{IndexerCommand, IndexerHandle, PassageEmbedder, SWEEP_INTERVAL};
use crate::search::{search_sources, SourcesQuery, SourcesSearchOutcome};

/// Capacity of the command channel: a caller past it waits for room.
pub const COMMAND_CAPACITY: usize = 64;

/// Errors of a call to the registry actor.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SourcesError {
    /// The registry refused or failed the command.
    #[error(transparent)]
    Registry(#[from] SourceRegistryError),
    /// The actor thread could not be started.
    #[error("cannot start the source registry: {0}")]
    Spawn(#[source] std::io::Error),
    /// The actor has stopped and answers no more.
    #[error("the source registry has stopped")]
    Stopped,
    /// An index could not be opened or searched.
    #[error(transparent)]
    Index(#[from] IndexError),
    /// The registry file has no directory to hold the indexes in.
    #[error("the source registry has no data directory")]
    NoDataDirectory,
}

type Reply<T> = oneshot::Sender<Result<T, SourceRegistryError>>;

enum Command {
    List(Reply<Vec<Source>>),
    Create(NewSource, Reply<Source>),
    Delete(String, Reply<Source>),
    CheckEntries(Vec<String>, Reply<()>),
    Checked(String, Vec<String>, Reply<Vec<CheckedSource>>),
    Search(
        SourcesQuery,
        oneshot::Sender<Result<SourcesSearchOutcome, SourcesError>>,
    ),
    SetIndexState(String, IndexState, Reply<()>),
    Paths(Reply<(PathBuf, Option<PathBuf>)>),
    AttachIndexer(IndexerHandle),
    RefreshIndexes(oneshot::Sender<()>),
    IndexStatus(String, oneshot::Sender<Result<IndexStatus, SourcesError>>),
}

/// Where the index of a source stands, and what it left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IndexStatus {
    /// The source.
    pub source: String,
    /// The state the registry records.
    pub state: IndexState,
    /// Documents in the index.
    pub documents: u64,
    /// Chunks in the index.
    pub chunks: u64,
    /// Files left out: over the size cap, or unreadable.
    pub skipped: Vec<SkippedFile>,
}

/// The indexer's way back to the registry. Weak, so the registry stops when
/// its last handle is dropped, and the indexer after it.
#[derive(Debug)]
pub(crate) struct RegistryLink(mpsc::WeakSender<Command>);

impl RegistryLink {
    async fn ask<T>(&self, command: impl FnOnce(Reply<T>) -> Command) -> Option<T> {
        let tx = self.0.upgrade()?;
        let (reply, answer) = oneshot::channel();
        tx.send(command(reply)).await.ok()?;
        drop(tx);
        answer.await.ok()?.ok()
    }

    /// Every source, `None` when the registry is gone or cannot answer.
    pub(crate) async fn list(&self) -> Option<Vec<Source>> {
        self.ask(Command::List).await
    }

    /// Records the index state of the source `id`; a source removed in the
    /// meantime is not an error.
    pub(crate) async fn set_state(&self, id: &str, state: IndexState) {
        let id = id.to_owned();
        let _ = self
            .ask(|reply| Command::SetIndexState(id, state, reply))
            .await;
    }
}

/// A clonable handle to the registry actor.
#[derive(Clone, Debug)]
pub struct SourceRegistryHandle {
    tx: mpsc::Sender<Command>,
}

impl SourceRegistryHandle {
    /// Opens the registry of the `system.db` at `path` and starts its actor.
    ///
    /// The file is opened, and brought to the current schema, before this
    /// returns: a registry that cannot be read is known at startup. The
    /// directory holding it is the data directory, where the indexes of the
    /// sources live.
    ///
    /// # Errors
    /// [`SourcesError::Registry`] when the file cannot be opened or migrated,
    /// [`SourcesError::Spawn`] when the thread cannot be started.
    pub fn open(path: &Path) -> Result<Self, SourcesError> {
        let store = SourceRegistryStore::open(path)?;
        let data_dir = path.parent().map(Path::to_path_buf);
        let (tx, rx) = mpsc::channel(COMMAND_CAPACITY);
        let actor = Actor {
            store,
            system_db: path.to_path_buf(),
            data_dir,
            indexer: None,
        };
        std::thread::Builder::new()
            .name("apollia-source-registry".to_owned())
            .spawn(move || actor.run(rx))
            .map_err(SourcesError::Spawn)?;
        Ok(Self { tx })
    }

    async fn ask<T>(&self, command: impl FnOnce(Reply<T>) -> Command) -> Result<T, SourcesError> {
        let (reply, answer) = oneshot::channel();
        self.tx
            .send(command(reply))
            .await
            .map_err(|_| SourcesError::Stopped)?;
        Ok(answer.await.map_err(|_| SourcesError::Stopped)??)
    }

    /// Every source, oldest first.
    ///
    /// # Errors
    /// As [`SourceRegistryStore::list`], or [`SourcesError::Stopped`].
    pub async fn list(&self) -> Result<Vec<Source>, SourcesError> {
        self.ask(Command::List).await
    }

    /// Adds a source and returns it as stored.
    ///
    /// # Errors
    /// As [`SourceRegistryStore::create`], or [`SourcesError::Stopped`].
    pub async fn create(&self, source: NewSource) -> Result<Source, SourcesError> {
        self.ask(|reply| Command::Create(source, reply)).await
    }

    /// Removes the source `id` and its index, and returns it.
    ///
    /// # Errors
    /// As [`SourceRegistryStore::delete`], or [`SourcesError::Stopped`].
    pub async fn delete(&self, id: &str) -> Result<Source, SourcesError> {
        let id = id.to_owned();
        self.ask(|reply| Command::Delete(id, reply)).await
    }

    /// Checks an agent's source list before it is written.
    ///
    /// # Errors
    /// As [`SourceRegistryStore::check_entries`], or [`SourcesError::Stopped`].
    pub async fn check_entries(&self, entries: Vec<String>) -> Result<(), SourcesError> {
        self.ask(|reply| Command::CheckEntries(entries, reply))
            .await
    }

    /// The sources the resolved list `entries` of `agent` checks; see
    /// [`checked_sources`].
    ///
    /// # Errors
    /// As [`SourceRegistryStore::list`], or [`SourcesError::Stopped`].
    pub async fn checked(
        &self,
        agent: &str,
        entries: Vec<String>,
    ) -> Result<Vec<CheckedSource>, SourcesError> {
        let agent = agent.to_owned();
        self.ask(|reply| Command::Checked(agent, entries, reply))
            .await
    }

    /// Searches the indexes of the sources `query` names; see
    /// [`search_sources`]. The caller has already bounded the list to the
    /// sources the agent checked.
    ///
    /// # Errors
    /// [`SourcesError::Index`] when an index cannot be read,
    /// [`SourcesError::NoDataDirectory`] for a registry outside any
    /// directory, or [`SourcesError::Stopped`].
    pub async fn search(&self, query: SourcesQuery) -> Result<SourcesSearchOutcome, SourcesError> {
        let (reply, answer) = oneshot::channel();
        self.tx
            .send(Command::Search(query, reply))
            .await
            .map_err(|_| SourcesError::Stopped)?;
        answer.await.map_err(|_| SourcesError::Stopped)?
    }

    /// Starts the indexer over this registry: every folder and file source
    /// is indexed in the background and kept in step with its files, its
    /// vectors computed by `embedder`; see [`crate::indexer`]. Must be called
    /// within a Tokio runtime. A second call replaces the first indexer.
    ///
    /// # Errors
    /// [`SourcesError::NoDataDirectory`] for a registry outside any
    /// directory, and [`SourcesError::Stopped`].
    pub async fn start_indexing<E: PassageEmbedder>(
        &self,
        embedder: E,
    ) -> Result<(), SourcesError> {
        self.start_indexing_every(embedder, SWEEP_INTERVAL).await
    }

    /// [`Self::start_indexing`], sweeping every `interval`.
    ///
    /// # Errors
    /// As [`Self::start_indexing`].
    pub async fn start_indexing_every<E: PassageEmbedder>(
        &self,
        embedder: E,
        interval: Duration,
    ) -> Result<(), SourcesError> {
        let (system_db, data_dir) = self.ask(Command::Paths).await?;
        let data_dir = data_dir.ok_or(SourcesError::NoDataDirectory)?;
        let link = RegistryLink(self.tx.downgrade());
        let indexer = crate::indexer::spawn(link, system_db, data_dir, embedder, interval);
        self.tx
            .send(Command::AttachIndexer(indexer))
            .await
            .map_err(|_| SourcesError::Stopped)
    }

    /// Asks the indexer to index every source again against the disk. The
    /// receiver answers once the pass is over; it is closed at once when no
    /// indexer runs, or when the request could not be queued.
    ///
    /// # Errors
    /// [`SourcesError::Stopped`].
    pub async fn refresh_indexes(&self) -> Result<oneshot::Receiver<()>, SourcesError> {
        let (reply, done) = oneshot::channel();
        self.tx
            .send(Command::RefreshIndexes(reply))
            .await
            .map_err(|_| SourcesError::Stopped)?;
        Ok(done)
    }

    /// Where the index of the source `id` stands: its state, its counts and
    /// the files it left out.
    ///
    /// # Errors
    /// [`SourcesError::Registry`] with `NotFound` for an unknown source,
    /// [`SourcesError::Index`] when its index cannot be read, and
    /// [`SourcesError::Stopped`].
    pub async fn index_status(&self, id: &str) -> Result<IndexStatus, SourcesError> {
        let (reply, answer) = oneshot::channel();
        self.tx
            .send(Command::IndexStatus(id.to_owned(), reply))
            .await
            .map_err(|_| SourcesError::Stopped)?;
        answer.await.map_err(|_| SourcesError::Stopped)?
    }
}

/// The state of the actor thread.
struct Actor {
    store: SourceRegistryStore,
    system_db: PathBuf,
    data_dir: Option<PathBuf>,
    indexer: Option<IndexerHandle>,
}

impl Actor {
    /// The actor loop: one command at a time, until every handle is gone.
    fn run(mut self, mut rx: mpsc::Receiver<Command>) {
        tracing::event!(tracing::Level::DEBUG, "sources.registry.started");
        while let Some(command) = rx.blocking_recv() {
            self.serve(command);
        }
        tracing::event!(tracing::Level::DEBUG, "sources.registry.stopped");
    }

    /// Tells the indexer, when one runs, that the registry changed.
    fn tell_indexer(&self) {
        if let Some(indexer) = &self.indexer {
            indexer.send(IndexerCommand::Sync);
        }
    }

    fn serve(&mut self, command: Command) {
        let store = &self.store;
        let data_dir = self.data_dir.as_deref();
        // A caller that gave up has dropped its receiver; its answer is moot.
        match command {
            Command::List(reply) => {
                let _ = reply.send(store.list());
            }
            Command::Create(source, reply) => {
                let created = store.create(&source);
                if let Ok(source) = &created {
                    tracing::event!(
                        tracing::Level::INFO,
                        source = %source.id,
                        kind = %source.kind,
                        "sources.registry.created"
                    );
                }
                let ok = created.is_ok();
                let _ = reply.send(created);
                if ok {
                    self.tell_indexer();
                }
            }
            Command::Delete(id, reply) => {
                let deleted = store.delete(&id);
                let ok = deleted.is_ok();
                if ok {
                    tracing::event!(
                        tracing::Level::INFO,
                        source = %id,
                        "sources.registry.deleted"
                    );
                    remove_index_of(data_dir, &id);
                }
                let _ = reply.send(deleted);
                if ok {
                    self.tell_indexer();
                }
            }
            Command::CheckEntries(entries, reply) => {
                let _ = reply.send(store.check_entries(&entries));
            }
            Command::Checked(agent, entries, reply) => {
                let checked = store
                    .list()
                    .map(|registry| checked_sources(&agent, &entries, &registry));
                let _ = reply.send(checked);
            }
            Command::Search(query, reply) => {
                let outcome = match data_dir {
                    Some(dir) => search_sources(dir, &query).map_err(SourcesError::from),
                    None => Err(SourcesError::NoDataDirectory),
                };
                let _ = reply.send(outcome);
            }
            Command::SetIndexState(id, state, reply) => {
                let _ = reply.send(store.set_index_state(&id, state));
            }
            Command::Paths(reply) => {
                let _ = reply.send(Ok((self.system_db.clone(), self.data_dir.clone())));
            }
            Command::AttachIndexer(indexer) => {
                self.indexer = Some(indexer);
            }
            Command::RefreshIndexes(reply) => {
                if let Some(indexer) = &self.indexer {
                    indexer.send(IndexerCommand::Refresh(reply));
                }
            }
            Command::IndexStatus(id, reply) => {
                let _ = reply.send(index_status(store, data_dir, &id));
            }
        }
    }
}

/// Where the index of the source `id` stands. An index never written counts
/// nothing; reading it never creates it.
fn index_status(
    store: &SourceRegistryStore,
    data_dir: Option<&Path>,
    id: &str,
) -> Result<IndexStatus, SourcesError> {
    let source = store
        .get(id)?
        .ok_or_else(|| SourceRegistryError::NotFound(id.to_owned()))?;
    let data_dir = data_dir.ok_or(SourcesError::NoDataDirectory)?;
    let path = crate::index::index_path(data_dir, id)?;
    let (documents, chunks, skipped) = if path.exists() {
        let index = SourceIndex::open_at(&path, id)?;
        let (documents, chunks) = index.counts()?;
        (documents, chunks, index.skipped()?)
    } else {
        (0, 0, Vec::new())
    };
    Ok(IndexStatus {
        source: source.id,
        state: source.index_state,
        documents,
        chunks,
        skipped,
    })
}

/// Removes the index of a source just deleted. The source is gone whatever
/// happens here, so a file that cannot be removed is reported, not returned:
/// it is an orphan, never read again.
fn remove_index_of(data_dir: Option<&Path>, id: &str) {
    let Some(data_dir) = data_dir else {
        return;
    };
    if let Err(e) = crate::index::remove_index(data_dir, id) {
        tracing::event!(
            tracing::Level::WARN,
            source = %id,
            error = %e,
            "sources.index.remove_failed"
        );
    }
}

#[cfg(test)]
mod tests;
