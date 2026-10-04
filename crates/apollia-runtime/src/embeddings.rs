//! Vectors for search by meaning, computed on this machine.
//!
//! The default embeddings model is a setting of `system.db`: the identifier of
//! an embeddings entry of the registry. [`LocalEmbedder`] reads it at every
//! call, finds the entry's file in the model index, has the `llama-server`
//! supervisor serve it on its dedicated instance (which never evicts the chat
//! model), and asks that instance for the vectors through the loopback client
//! of `apollia-llm`.
//!
//! Without a default the answer is [`EmbeddingError::NoModel`], which a caller
//! reads through [`EmbeddingError::falls_back_to_keywords`] as "search with
//! keywords alone".

use std::path::{Path, PathBuf};
use std::sync::Arc;

use apollia_core::paths::DataFile;
use apollia_core::{EmbeddingDefaultError, EmbeddingDefaultStore};
use apollia_llm::embeddings_client::{
    chosen_model, EmbedPurpose, EmbeddingError, EmbeddingsClient,
};
use apollia_llm::model_index::INDEX_FILE_NAME;
use apollia_llm::recommend::{EmbeddingCatalogue, EmbeddingModel};
use apollia_llm::ModelIndex;

use crate::llama_server::LlamaServerSupervisor;

/// Why the default embeddings model could not be set.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SetDefaultError {
    /// The identifier names no embeddings entry of the registry.
    #[error("{0} is not an embeddings model of the registry")]
    UnknownModel(String),
    /// The registry shipped in the binary does not parse.
    #[error("the shipped embeddings table is invalid: {0}")]
    Registry(String),
    /// `system.db` could not be read or written.
    #[error(transparent)]
    Store(#[from] EmbeddingDefaultError),
}

/// The default embeddings model of `data_dir`, `None` when unset.
///
/// # Errors
/// [`EmbeddingDefaultError`] when `system.db` cannot be opened or read.
pub fn default_model(data_dir: &Path) -> Result<Option<String>, EmbeddingDefaultError> {
    EmbeddingDefaultStore::open(&DataFile::System.path(data_dir))?.get()
}

/// Make `model_id` the default embeddings model of `data_dir`.
///
/// # Errors
/// [`SetDefaultError::UnknownModel`] when the registry has no such entry, and
/// [`SetDefaultError::Store`] when `system.db` cannot be written.
pub fn set_default_model(data_dir: &Path, model_id: &str) -> Result<(), SetDefaultError> {
    let catalogue =
        EmbeddingCatalogue::embedded().map_err(|e| SetDefaultError::Registry(e.to_string()))?;
    let model_id = model_id.trim();
    if catalogue.get(model_id).is_none() {
        return Err(SetDefaultError::UnknownModel(model_id.to_owned()));
    }
    EmbeddingDefaultStore::open(&DataFile::System.path(data_dir))?.set(model_id)?;
    tracing::event!(
        tracing::Level::INFO,
        model = %model_id,
        "llm.embeddings.default.set"
    );
    Ok(())
}

/// The file of `entry` in the model index of `models_dir`: the model whose
/// digest is the entry's, else the one carrying the entry's file name.
pub fn installed_path(models_dir: &Path, entry: &EmbeddingModel) -> Option<PathBuf> {
    let index = ModelIndex::load(&models_dir.join(INDEX_FILE_NAME)).ok()?;
    let by_digest = index.models.iter().find(|m| {
        m.sha256
            .as_deref()
            .is_some_and(|d| d.eq_ignore_ascii_case(&entry.sha256))
    });
    let by_name = || {
        index
            .models
            .iter()
            .find(|m| m.path.file_name().is_some_and(|n| n == entry.file.as_str()))
    };
    by_digest.or_else(by_name).map(|m| m.path.clone())
}

/// Computes vectors with the default embeddings model, on this machine.
#[derive(Clone)]
pub struct LocalEmbedder {
    data_dir: PathBuf,
    supervisor: Option<Arc<LlamaServerSupervisor>>,
}

impl LocalEmbedder {
    /// An embedder reading its setting and models under `data_dir`, served by
    /// `supervisor` (`None` when this machine has no local engine).
    #[must_use]
    pub fn new(data_dir: PathBuf, supervisor: Option<Arc<LlamaServerSupervisor>>) -> Self {
        Self {
            data_dir,
            supervisor,
        }
    }

    /// The registry entry of the default embeddings model, and its file.
    ///
    /// # Errors
    /// [`EmbeddingError::NoModel`] without a default,
    /// [`EmbeddingError::UnknownModel`] for an identifier the registry no
    /// longer has, [`EmbeddingError::NotInstalled`] when its file is absent,
    /// and [`EmbeddingError::Setting`] when `system.db` cannot be read.
    pub async fn resolve(&self) -> Result<(EmbeddingModel, PathBuf), EmbeddingError> {
        let data_dir = self.data_dir.clone();
        tokio::task::spawn_blocking(move || {
            let default =
                default_model(&data_dir).map_err(|e| EmbeddingError::Setting(e.to_string()))?;
            let catalogue = EmbeddingCatalogue::embedded()
                .map_err(|e| EmbeddingError::Setting(e.to_string()))?;
            let entry = chosen_model(default.as_deref(), &catalogue)?.clone();
            let models_dir = crate::installed_models::models_dir(&data_dir);
            let path = installed_path(&models_dir, &entry).ok_or_else(|| {
                EmbeddingError::NotInstalled {
                    model: entry.id.clone(),
                }
            })?;
            Ok((entry, path))
        })
        .await
        .map_err(|e| EmbeddingError::Setting(e.to_string()))?
    }

    /// One unit vector per text, in order, computed by the default embeddings
    /// model.
    ///
    /// # Errors
    /// The errors of [`Self::resolve`], [`EmbeddingError::EngineUnavailable`]
    /// without a local engine, [`EmbeddingError::Engine`] when the instance
    /// cannot start, and the errors of the request itself.
    pub async fn embed(
        &self,
        texts: &[String],
        purpose: EmbedPurpose,
    ) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let outcome = self.embed_with_default(texts, purpose).await;
        if let Err(e) = &outcome {
            if e.falls_back_to_keywords() {
                tracing::event!(
                    tracing::Level::INFO,
                    reason = %e,
                    "llm.embeddings.keywords_only"
                );
            } else {
                tracing::event!(
                    tracing::Level::WARN,
                    error = %e,
                    "llm.embeddings.failed"
                );
            }
        }
        outcome
    }

    async fn embed_with_default(
        &self,
        texts: &[String],
        purpose: EmbedPurpose,
    ) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let (entry, path) = self.resolve().await?;
        let supervisor = self
            .supervisor
            .as_ref()
            .ok_or(EmbeddingError::EngineUnavailable)?;
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let lease = supervisor
            .ensure_embedding_model(
                &path.display().to_string(),
                entry.pooling,
                entry.context_length,
            )
            .await
            .map_err(|e| EmbeddingError::Engine(e.to_string()))?;
        let client = EmbeddingsClient::new(lease.base_url(), entry)?;
        let vectors = client.embed(texts, purpose).await;
        drop(lease);
        vectors
    }
}

#[cfg(test)]
mod tests;
