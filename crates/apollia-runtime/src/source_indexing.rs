//! The indexing of folder and file sources, started at boot.
//!
//! The indexer of `apollia-sources` is handed one thing that reaches beyond
//! the disk: a [`PassageEmbedder`]. Here it is [`LocalEmbedder`], which asks
//! the local engine for vectors through the loopback client of `apollia-llm`,
//! and refuses any other address. Without an embeddings model the indexes are
//! built for keywords alone.

use std::path::Path;
use std::sync::Arc;

use apollia_llm::embeddings_client::{EmbedPurpose, EmbeddingError};
use apollia_sources::index::ChunkVectors;
use apollia_sources::{PassageEmbedder, SourceRegistryHandle};

use crate::embeddings::LocalEmbedder;
use crate::llama_server::LlamaServerSupervisor;

impl PassageEmbedder for LocalEmbedder {
    async fn model(&self) -> Result<String, EmbeddingError> {
        self.resolve().await.map(|(entry, _)| entry.id)
    }

    async fn embed_passages(&self, texts: &[String]) -> Result<ChunkVectors, EmbeddingError> {
        let (entry, _) = self.resolve().await?;
        let vectors = self.embed(texts, EmbedPurpose::Passage).await?;
        Ok(ChunkVectors {
            model: entry.id,
            vectors,
        })
    }
}

/// Starts the indexer over `registry`, its vectors computed by the default
/// embeddings model of `data_dir`, served by `supervisor`. A failure is
/// traced and leaves the sources without a live index; nothing else depends
/// on it.
pub async fn start(
    registry: &SourceRegistryHandle,
    data_dir: &Path,
    supervisor: Option<Arc<LlamaServerSupervisor>>,
) {
    let embedder = LocalEmbedder::new(data_dir.to_path_buf(), supervisor);
    match registry.start_indexing(embedder).await {
        Ok(()) => tracing::event!(tracing::Level::INFO, "supervisor.source_indexer.ready"),
        Err(e) => tracing::event!(
            tracing::Level::WARN,
            error = %e,
            "supervisor.source_indexer.failed"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compiles only for an embedder the indexer accepts.
    fn accepted_by_the_indexer<E: PassageEmbedder>(_: &E) {}

    #[tokio::test]
    async fn test_the_local_embedder_is_what_the_indexer_is_given_and_without_a_model_says_keywords(
    ) {
        // GIVEN the local embedder of an empty data directory, no engine
        let dir = tempfile::tempdir().unwrap();
        let embedder = LocalEmbedder::new(dir.path().to_path_buf(), None);

        // WHEN it is handed to the indexer's bound and asked for its model
        accepted_by_the_indexer(&embedder);
        let model = PassageEmbedder::model(&embedder).await;

        // THEN there is no model, which the indexer reads as keywords alone
        let error = model.unwrap_err();
        assert_eq!(error, EmbeddingError::NoModel);
        assert!(error.falls_back_to_keywords());
    }
}
