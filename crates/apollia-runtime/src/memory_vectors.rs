//! The vectors of the agents' memory, computed in the background.
//!
//! The memory states what it needs from an embeddings model in
//! [`apollia_memory::vectors::MemoryEmbedder`]; [`LocalEmbedder`] answers it
//! with the default embeddings model of this machine. At boot the runtime
//! installs that embedder for the process (every memory search then ranks by
//! meaning when a model is present) and starts an actor which
//! gives a vector to every entry that has none, or one of another model or
//! dimension: a new entry, an entry written before a model was installed, and
//! every entry again after a change of model.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use apollia_llm::embeddings_client::{EmbedPurpose, EmbeddingError};
use apollia_memory::vectors::{
    install_embedder, vectorize_dir, EmbedError, EmbedFuture, EmbedRole, Embedded, EmbeddingsModel,
    MemoryEmbedder,
};
use tokio::sync::{mpsc, oneshot};

use crate::embeddings::LocalEmbedder;
use crate::llama_server::LlamaServerSupervisor;

/// Time between two background passes. A pass without a model costs one read
/// of the setting; with a model and nothing pending, one query per namespace.
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(30);

/// Requests the actor's mailbox holds before a new one is dropped: a pass
/// already queued covers it.
const MAILBOX: usize = 4;

fn embed_error(e: EmbeddingError) -> EmbedError {
    if e.falls_back_to_keywords() {
        EmbedError::Unavailable(e.to_string())
    } else {
        EmbedError::Failed(e.to_string())
    }
}

impl MemoryEmbedder for LocalEmbedder {
    fn model(&self) -> EmbedFuture<'_, EmbeddingsModel> {
        Box::pin(async move {
            let (entry, _) = self.resolve().await.map_err(embed_error)?;
            Ok(EmbeddingsModel {
                id: entry.id,
                dimensions: entry.dimensions,
            })
        })
    }

    fn embed<'a>(&'a self, texts: &'a [String], role: EmbedRole) -> EmbedFuture<'a, Embedded> {
        Box::pin(async move {
            let purpose = match role {
                EmbedRole::Query => EmbedPurpose::Query,
                EmbedRole::Passage => EmbedPurpose::Passage,
            };
            let (entry, _) = self.resolve().await.map_err(embed_error)?;
            let vectors = LocalEmbedder::embed(self, texts, purpose)
                .await
                .map_err(embed_error)?;
            Ok(Embedded {
                model: EmbeddingsModel {
                    id: entry.id,
                    dimensions: entry.dimensions,
                },
                vectors,
            })
        })
    }
}

/// A request for one pass; the reply, when asked for, carries how many
/// vectors were stored.
struct Pass(Option<oneshot::Sender<usize>>);

/// Gives a vector to the memory entries that lack one, one pass per request
/// of its mailbox. In the runtime the requests come from a ticker
/// ([`spawn`]); the actor ends when every sender is dropped.
struct MemoryVectorsActor {
    dir: PathBuf,
    embedder: Arc<dyn MemoryEmbedder>,
    rx: mpsc::Receiver<Pass>,
}

impl MemoryVectorsActor {
    /// Starts the actor over the namespace databases of `dir` and returns its
    /// mailbox.
    fn spawn(dir: PathBuf, embedder: Arc<dyn MemoryEmbedder>) -> mpsc::Sender<Pass> {
        let (tx, rx) = mpsc::channel(MAILBOX);
        tokio::spawn(Self { dir, embedder, rx }.run());
        tx
    }

    async fn run(mut self) {
        while let Some(Pass(reply)) = self.rx.recv().await {
            let stored = vectorize_dir(&self.dir, self.embedder.as_ref()).await;
            if let Some(reply) = reply {
                let _ = reply.send(stored);
            }
        }
    }
}

/// Starts the actor over `dir` and a ticker asking it for a pass at once, then
/// every `interval`. A tick that finds the mailbox full is skipped: a pass
/// already queued covers it.
fn spawn(dir: PathBuf, embedder: Arc<dyn MemoryEmbedder>, interval: Duration) {
    let mailbox = MemoryVectorsActor::spawn(dir, embedder);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            match mailbox.try_send(Pass(None)) {
                Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => {}
                Err(mpsc::error::TrySendError::Closed(_)) => break,
            }
        }
    });
}

/// Installs the embeddings model of `data_dir` as the embedder of this
/// process and starts the background passes over its memory directory.
pub fn start(data_dir: &Path, supervisor: Option<Arc<LlamaServerSupervisor>>) {
    let embedder: Arc<dyn MemoryEmbedder> =
        Arc::new(LocalEmbedder::new(data_dir.to_path_buf(), supervisor));
    if !install_embedder(Arc::clone(&embedder)) {
        tracing::event!(
            tracing::Level::DEBUG,
            detail = "an embedder was already installed in this process",
            "memory.vectors.embedder.kept"
        );
    }
    spawn(
        crate::memory_maintenance::memory_dir(data_dir),
        embedder,
        SWEEP_INTERVAL,
    );
    tracing::event!(tracing::Level::INFO, "memory.vectors.started");
}

#[cfg(test)]
mod tests;
