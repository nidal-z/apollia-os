//! The draft models a model of the index can be loaded with.
//!
//! A draft model proposes tokens that a larger model checks, which only pays
//! when both number their tokens alike. A draft is therefore proposed for a
//! model when it is filed as one and the two GGUF headers declare the same
//! vocabulary, as [`same_vocabulary`] reads it.

use std::path::Path;

use super::{IndexedModel, ModelCategory, ModelFormat, ModelIndex};
use crate::gguf_probe::vocabulary::same_vocabulary;
use crate::gguf_probe::{probe_file, CONFIRM_BYTES};

impl ModelIndex {
    /// The draft models that can be proposed for the model `id`: the other
    /// GGUF models filed as drafts whose header declares the same vocabulary
    /// as its own. Empty for an unknown model and for an MLX one.
    ///
    /// The headers are read from disk at every call.
    #[must_use]
    pub fn compatible_drafts(&self, id: &str) -> Vec<&IndexedModel> {
        let Some(main) = self.get(id).filter(|m| m.format == ModelFormat::Gguf) else {
            return Vec::new();
        };
        let Ok(main_header) = probe_file(main.weights_path(), CONFIRM_BYTES) else {
            return Vec::new();
        };
        self.models
            .iter()
            .filter(|m| m.id != main.id && m.format == ModelFormat::Gguf)
            .filter(|m| m.effective_category() == ModelCategory::Draft)
            .filter(|m| {
                probe_file(m.weights_path(), CONFIRM_BYTES)
                    .is_ok_and(|header| same_vocabulary(&main_header, &header))
            })
            .collect()
    }

    /// The draft model `draft_id` when it can draft for the model that owns
    /// the weights at `main_path`, as [`Self::compatible_drafts`] decides.
    #[must_use]
    pub fn draft_for(&self, main_path: &Path, draft_id: &str) -> Option<&IndexedModel> {
        let main = self
            .models
            .iter()
            .find(|m| m.owns(main_path) || m.source_file_override.as_deref() == Some(main_path))?;
        self.compatible_drafts(&main.id)
            .into_iter()
            .find(|m| m.id == draft_id)
    }
}
