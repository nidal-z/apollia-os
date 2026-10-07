//! The draft model a GGUF model is launched with, for speculative decoding.
//!
//! A preset names its draft model by the identifier of the model index, the
//! name the user reads, never by a path. The launch resolves it here, against
//! the index kept beside the models: the draft has to be filed as one, and its
//! header has to declare the vocabulary of the model it drafts for. A draft
//! that is gone, was never one, or reads another vocabulary is dropped with a
//! warning and the model loads alone: a stale preset must not keep a model
//! from loading, and a draft of another vocabulary would only slow it down.
//!
//! Choosing a draft model fixes the speculative mode to `draft-simple`, the
//! one mode of the engine that reads `--model-draft`.

use std::path::Path;

use apollia_llm::model_index::INDEX_FILE_NAME;
use apollia_llm::ModelIndex;

use super::config::SpecType;
use super::load_settings::LoadSettings;

/// The path of the draft model `load` names for the model at `model_path`,
/// and `load` with its speculative mode brought in line, from the index of
/// the user's models directory.
pub(super) fn for_model(model_path: &str, load: &LoadSettings) -> (Option<String>, LoadSettings) {
    if load.draft_model.is_none() && load.spec_type != Some(SpecType::DraftSimple) {
        return (None, load.clone());
    }
    let home = apollia_core::paths::home_dir_or_temp();
    let models_dir = apollia_core::config::models_dir(&apollia_core::paths::data_dir_under(home));
    in_models_dir(&models_dir, Path::new(model_path), load)
}

/// [`for_model`] against the models of `models_dir`.
///
/// With a draft resolved, the mode is `draft-simple` whatever the settings
/// said. Without one, `draft-simple` is removed, since the engine has nothing
/// to draft with, and any other mode is left as it is.
pub(super) fn in_models_dir(
    models_dir: &Path,
    model: &Path,
    load: &LoadSettings,
) -> (Option<String>, LoadSettings) {
    let mut load = load.clone();
    let resolved = load
        .draft_model
        .as_deref()
        .and_then(|id| resolve(models_dir, model, id));
    match &resolved {
        Some(_) => load.spec_type = Some(SpecType::DraftSimple),
        None if load.spec_type == Some(SpecType::DraftSimple) => load.spec_type = None,
        None => {}
    }
    (resolved, load)
}

fn resolve(models_dir: &Path, model: &Path, draft_id: &str) -> Option<String> {
    let index = match ModelIndex::load(&models_dir.join(INDEX_FILE_NAME)) {
        Ok(index) => index,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                path = %models_dir.display(),
                error = %error,
                detail = "the model loads without its draft model",
                "llama.server.draft.index_unreadable"
            );
            return None;
        }
    };
    match index.draft_for(model, draft_id) {
        Some(draft) => Some(draft.weights_path().display().to_string()),
        None => {
            tracing::event!(
                tracing::Level::WARN,
                model = %model.display(),
                draft = %draft_id,
                detail = "not a draft model of the same vocabulary; the model loads alone",
                "llama.server.draft.refused"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests;
