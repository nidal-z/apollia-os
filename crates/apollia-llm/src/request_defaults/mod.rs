//! The request settings a call starts from, resolved in three levels.
//!
//! 1. **The model**: the defaults its publisher recommends, read by
//!    [`crate::model_defaults::resolve`] from the file the downloader fills and
//!    from the table the binary embeds.
//! 2. **The agent's package**: the `request` of the model defaults its
//!    manifest declares ([`PackageRequestSettings`]).
//! 3. **The user**: the request settings of the preset chosen for the agent,
//!    else of the default preset of the agent's model ([`RequestSettings`]).
//!
//! A level wins over the ones below it field by field: a field it leaves
//! unset keeps the value of the level below. A value the call itself sets is
//! never replaced ([`RequestDefaults::apply_to`]): the levels are defaults.

use std::path::Path;

use apollia_core::manifest::PackageRequestSettings;
use apollia_core::model_preset::RequestSettings;

use crate::gguf_probe::{probe_file, SCREEN_BYTES};
use crate::model_defaults::{resolve, ModelDefaults, ModelHints, UserOverrides};
use crate::model_index::{ModelIndex, INDEX_FILE_NAME};
use crate::types::CompletionRequest;

#[cfg(test)]
mod tests;

/// Name of the publisher defaults file inside the models directory.
pub const SAMPLING_DEFAULTS_FILE_NAME: &str = "sampling-defaults.json";

/// Request settings resolved for one call. Every field is optional: an unset
/// one is left to the engine.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RequestDefaults {
    /// Sampling temperature.
    pub temperature: Option<f32>,
    /// Upper bound on the length of an answer, in tokens.
    pub max_tokens: Option<u32>,
    /// Sampling seed.
    pub seed: Option<u64>,
    /// Top-p (nucleus) sampling.
    pub top_p: Option<f32>,
    /// Top-k sampling.
    pub top_k: Option<u32>,
    /// Min-p sampling.
    pub min_p: Option<f32>,
    /// Presence penalty.
    pub presence_penalty: Option<f32>,
    /// Repetition penalty.
    pub repeat_penalty: Option<f32>,
    /// Strings that end the answer.
    pub stop: Option<Vec<String>>,
}

impl RequestDefaults {
    /// Level 1: the defaults of a model.
    pub fn from_model(defaults: &ModelDefaults) -> Self {
        Self {
            temperature: defaults.temperature,
            top_p: defaults.top_p,
            top_k: defaults.top_k,
            repeat_penalty: defaults.repetition_penalty,
            ..Self::default()
        }
    }

    /// Level 2: the request settings an agent's package declares.
    pub fn from_package(settings: &PackageRequestSettings) -> Self {
        Self {
            temperature: settings.temperature,
            max_tokens: settings.max_tokens,
            seed: settings.seed,
            top_p: settings.top_p,
            top_k: settings.top_k,
            repeat_penalty: settings.repetition_penalty,
            ..Self::default()
        }
    }

    /// Level 3: the request settings of a preset. Its JSON Schema is not a
    /// default: a structured call states its schema and validates against it.
    pub fn from_preset(settings: &RequestSettings) -> Self {
        Self {
            temperature: settings.temperature,
            max_tokens: settings.max_tokens,
            seed: None,
            top_p: settings.top_p,
            top_k: settings.top_k,
            min_p: settings.min_p,
            presence_penalty: settings.presence_penalty,
            repeat_penalty: settings.repeat_penalty,
            stop: (!settings.stop.is_empty()).then(|| settings.stop.clone()),
        }
    }

    /// Resolve the three levels: `preset` over `package` over `model`, field
    /// by field.
    pub fn resolve(model: &Self, package: &Self, preset: &Self) -> Self {
        preset.clone().over(&package.clone().over(model))
    }

    /// `self` over `lower`: each field `self` leaves unset takes the value of
    /// `lower`.
    #[must_use]
    pub fn over(self, lower: &Self) -> Self {
        Self {
            temperature: self.temperature.or(lower.temperature),
            max_tokens: self.max_tokens.or(lower.max_tokens),
            seed: self.seed.or(lower.seed),
            top_p: self.top_p.or(lower.top_p),
            top_k: self.top_k.or(lower.top_k),
            min_p: self.min_p.or(lower.min_p),
            presence_penalty: self.presence_penalty.or(lower.presence_penalty),
            repeat_penalty: self.repeat_penalty.or(lower.repeat_penalty),
            stop: self.stop.or_else(|| lower.stop.clone()),
        }
    }

    /// Fill each field `req` leaves unset; a value the call sets stays.
    pub fn apply_to(&self, req: &mut CompletionRequest) {
        fill(&mut req.temperature, self.temperature);
        fill(&mut req.max_tokens, self.max_tokens);
        fill(&mut req.seed, self.seed);
        fill(&mut req.top_p, self.top_p);
        fill(&mut req.top_k, self.top_k);
        fill(&mut req.min_p, self.min_p);
        fill(&mut req.presence_penalty, self.presence_penalty);
        fill(&mut req.repeat_penalty, self.repeat_penalty);
        if req.stop.is_none() {
            req.stop.clone_from(&self.stop);
        }
    }
}

fn fill<T: Copy>(slot: &mut Option<T>, value: Option<T>) {
    if slot.is_none() {
        *slot = value;
    }
}

/// Level 1 for the model a backend serves: [`resolve`] over the publisher
/// defaults file of `models_dir` and the embedded table.
///
/// `model_id` and `backend` are the identifiers the router knows the model
/// by. When the models index of `models_dir` holds that model, its file name
/// joins the hints and, for a GGUF, the architecture its header declares,
/// which is what the embedded table matches on. `models_dir` is `None` when
/// no data directory is known: only the embedded table is read then, and
/// only through an architecture `model_id` names as a GGUF path.
pub fn model_level(model_id: &str, backend: &str, models_dir: Option<&Path>) -> ModelDefaults {
    let overrides = models_dir
        .map(|dir| load_overrides(&dir.join(SAMPLING_DEFAULTS_FILE_NAME)))
        .unwrap_or_default();
    let indexed = models_dir.and_then(|dir| indexed_path(dir, &[model_id, backend]));
    let file_path = indexed.or_else(|| {
        let path = Path::new(model_id);
        path.is_file().then(|| path.to_path_buf())
    });
    let file_name = file_path
        .as_deref()
        .and_then(Path::file_name)
        .and_then(|n| n.to_str())
        .map(str::to_owned);
    let arch = file_path.as_deref().and_then(gguf_architecture);
    let hints = ModelHints {
        arch: arch.as_deref(),
        name: Some(backend),
        file_name: file_name.as_deref(),
        repo_id: None,
        model_id: Some(model_id),
    };
    resolve(&hints, &overrides)
}

fn load_overrides(path: &Path) -> UserOverrides {
    match UserOverrides::load(path) {
        Ok(overrides) => overrides,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                path = %path.display(),
                error = %error,
                detail = "the publisher defaults on disk are skipped",
                "llm.request_defaults.overrides_unreadable"
            );
            UserOverrides::default()
        }
    }
}

/// The weights of the indexed model one of `keys` designates: by its index
/// identifier, else by its file name.
fn indexed_path(models_dir: &Path, keys: &[&str]) -> Option<std::path::PathBuf> {
    let index_path = models_dir.join(INDEX_FILE_NAME);
    if !index_path.is_file() {
        return None;
    }
    let index = ModelIndex::load(&index_path).ok()?;
    keys.iter().find_map(|key| {
        index
            .get(key)
            .or_else(|| {
                index
                    .models
                    .iter()
                    .find(|m| m.path.file_name().and_then(|n| n.to_str()) == Some(*key))
            })
            .map(|m| m.path.clone())
    })
}

/// `general.architecture` of a GGUF file, read from its header.
fn gguf_architecture(path: &Path) -> Option<String> {
    let is_gguf = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("gguf"));
    if !is_gguf {
        return None;
    }
    probe_file(path, SCREEN_BYTES).ok()?.architecture
}
