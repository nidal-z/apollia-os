//! The actor that owns the registry of sources.
//!
//! One thread holds the connection to `system.db` and serves the commands of a
//! bounded channel in order; callers hold a clonable [`SourceRegistryHandle`].
//! SQLite calls block, so the actor runs on a thread of its own rather than on
//! the async workers. It stops when the last handle is dropped.

use std::path::Path;

use apollia_core::source_registry::{NewSource, Source, SourceRegistryError, SourceRegistryStore};
use tokio::sync::{mpsc, oneshot};

use crate::checked::{checked_sources, CheckedSource};

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
}

type Reply<T> = oneshot::Sender<Result<T, SourceRegistryError>>;

enum Command {
    List(Reply<Vec<Source>>),
    Create(NewSource, Reply<Source>),
    Delete(String, Reply<Source>),
    CheckEntries(Vec<String>, Reply<()>),
    Checked(String, Vec<String>, Reply<Vec<CheckedSource>>),
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
    /// returns: a registry that cannot be read is known at startup.
    ///
    /// # Errors
    /// [`SourcesError::Registry`] when the file cannot be opened or migrated,
    /// [`SourcesError::Spawn`] when the thread cannot be started.
    pub fn open(path: &Path) -> Result<Self, SourcesError> {
        let store = SourceRegistryStore::open(path)?;
        let (tx, rx) = mpsc::channel(COMMAND_CAPACITY);
        std::thread::Builder::new()
            .name("apollia-source-registry".to_owned())
            .spawn(move || run(store, rx))
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

    /// Removes the source `id` and returns it.
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
}

/// The actor loop: one command at a time, until every handle is gone.
fn run(store: SourceRegistryStore, mut rx: mpsc::Receiver<Command>) {
    tracing::event!(tracing::Level::DEBUG, "sources.registry.started");
    while let Some(command) = rx.blocking_recv() {
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
                let _ = reply.send(created);
            }
            Command::Delete(id, reply) => {
                let deleted = store.delete(&id);
                if deleted.is_ok() {
                    tracing::event!(
                        tracing::Level::INFO,
                        source = %id,
                        "sources.registry.deleted"
                    );
                }
                let _ = reply.send(deleted);
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
        }
    }
    tracing::event!(tracing::Level::DEBUG, "sources.registry.stopped");
}

#[cfg(test)]
mod tests;
