//! The load settings a model preset carries, and how they reach the launch.
//!
//! A preset stores its load settings as a JSON object; this module reads the
//! keys the local engine knows into [`LoadSettings`] and lays them over the
//! base configuration. The order of precedence is fixed: an `APOLLIA_LLAMA_`
//! variable wins over the preset, and the preset wins over what the runtime
//! computes on its own (the window, the offload count, the slot count).
//!
//! A key the engine does not know is left alone, since a preset is shared with
//! the request settings and with engines that read other keys. A known key
//! whose value has the wrong shape is dropped with a warning: a typo in a
//! preset must not keep the model from loading.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use apollia_core::model_preset::ModelSource;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use super::config::{FlashAttn, LlamaServerConfig, SpecType};

/// Load settings of one preset. `None` leaves the field to the environment,
/// then to the automatic computation, then to the engine default.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoadSettings {
    /// Context window of one request, in tokens (`-c`).
    pub n_ctx: Option<u32>,
    /// Layers placed on the accelerator (`-ngl`).
    pub n_gpu_layers: Option<i32>,
    /// CPU threads (`-t`).
    pub n_threads: Option<i32>,
    /// Logical batch size (`-b`).
    pub n_batch: Option<u32>,
    /// Physical batch size (`-ub`).
    pub n_ubatch: Option<u32>,
    /// Requests served at once (`-np`).
    pub n_parallel: Option<u32>,
    /// One key/value buffer for every slot (`--kv-unified`).
    pub kv_unified: Option<bool>,
    /// Context checkpoints per slot (`--ctx-checkpoints`).
    pub ctx_checkpoints: Option<u32>,
    /// Reasoning token budget (`--reasoning-budget`).
    pub reasoning_budget: Option<i32>,
    /// Message injected when the reasoning budget is spent.
    pub reasoning_budget_message: Option<String>,
    /// RoPE base frequency; absent is the automatic mode.
    pub rope_freq_base: Option<f32>,
    /// RoPE frequency scale; absent is the automatic mode.
    pub rope_freq_scale: Option<f32>,
    /// Whether the key/value cache goes to the accelerator; `false` passes
    /// `--no-kv-offload`.
    pub kv_offload: Option<bool>,
    /// Keep the model in memory (`--mlock`).
    pub mlock: Option<bool>,
    /// Memory-map the model; `false` passes `--no-mmap`.
    pub mmap: Option<bool>,
    /// Engine seed (`--seed`).
    pub seed: Option<u32>,
    /// Draft model path for speculative decoding (`--model-draft`).
    pub draft_model: Option<String>,
    /// Speculative decoding mode (`--spec-type`).
    pub spec_type: Option<SpecType>,
    /// Jinja chat template text, written to a file passed by
    /// `--chat-template-file`.
    pub chat_template: Option<String>,
    /// Flash attention mode (`--flash-attn`).
    pub flash_attn: Option<FlashAttn>,
    /// Key cache type (`-ctk`).
    pub cache_type_k: Option<String>,
    /// Value cache type (`-ctv`).
    pub cache_type_v: Option<String>,
}

/// Read one key, dropping a value of the wrong shape with a warning.
fn key<T: DeserializeOwned>(map: &Map<String, Value>, name: &str) -> Option<T> {
    let raw = map.get(name)?;
    if raw.is_null() {
        return None;
    }
    match serde_json::from_value::<T>(raw.clone()) {
        Ok(value) => Some(value),
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                key = %name,
                error = %error,
                "llama.server.preset.load_key_invalid"
            );
            None
        }
    }
}

impl LoadSettings {
    /// The settings a preset's load object names, keyed by the field names of
    /// this struct.
    #[must_use]
    pub fn from_map(map: &Map<String, Value>) -> Self {
        let flash_attn = key::<String>(map, "flash_attn").and_then(|raw| {
            raw.parse::<FlashAttn>()
                .map_err(|error| {
                    tracing::event!(
                        tracing::Level::WARN,
                        key = "flash_attn",
                        error = %error,
                        "llama.server.preset.load_key_invalid"
                    );
                })
                .ok()
        });
        Self {
            n_ctx: key(map, "n_ctx"),
            n_gpu_layers: key(map, "n_gpu_layers"),
            n_threads: key(map, "n_threads"),
            n_batch: key(map, "n_batch"),
            n_ubatch: key(map, "n_ubatch"),
            n_parallel: key(map, "n_parallel"),
            kv_unified: key(map, "kv_unified"),
            ctx_checkpoints: key(map, "ctx_checkpoints"),
            reasoning_budget: key(map, "reasoning_budget"),
            reasoning_budget_message: key(map, "reasoning_budget_message"),
            rope_freq_base: key(map, "rope_freq_base"),
            rope_freq_scale: key(map, "rope_freq_scale"),
            kv_offload: key(map, "kv_offload"),
            mlock: key(map, "mlock"),
            mmap: key(map, "mmap"),
            seed: key(map, "seed"),
            draft_model: key(map, "draft_model"),
            spec_type: key(map, "spec_type"),
            chat_template: key(map, "chat_template"),
            flash_attn,
            cache_type_k: key(map, "cache_type_k"),
            cache_type_v: key(map, "cache_type_v"),
        }
    }

    /// Lay the settings over `config`. The chat template is not among them:
    /// the engine reads it from a file, which the caller writes and sets.
    pub fn apply(&self, config: &mut LlamaServerConfig) {
        fn set<T: Clone>(slot: &mut Option<T>, value: &Option<T>) {
            if value.is_some() {
                slot.clone_from(value);
            }
        }
        if let Some(v) = self.n_ctx {
            config.n_ctx = v;
        }
        if let Some(v) = self.n_gpu_layers {
            config.n_gpu_layers = v;
        }
        if let Some(v) = self.kv_unified {
            config.kv_unified = v;
        }
        if let Some(v) = self.kv_offload {
            config.no_kv_offload = !v;
        }
        if let Some(v) = self.mlock {
            config.mlock = v;
        }
        if let Some(v) = self.mmap {
            config.no_mmap = !v;
        }
        set(&mut config.n_threads, &self.n_threads);
        set(&mut config.n_batch, &self.n_batch);
        set(&mut config.n_ubatch, &self.n_ubatch);
        set(&mut config.n_parallel, &self.n_parallel);
        set(&mut config.ctx_checkpoints, &self.ctx_checkpoints);
        set(&mut config.reasoning_budget, &self.reasoning_budget);
        set(
            &mut config.reasoning_budget_message,
            &self.reasoning_budget_message,
        );
        set(&mut config.rope_freq_base, &self.rope_freq_base);
        set(&mut config.rope_freq_scale, &self.rope_freq_scale);
        set(&mut config.seed, &self.seed);
        set(&mut config.draft_model, &self.draft_model);
        set(&mut config.spec_type, &self.spec_type);
        set(&mut config.flash_attn, &self.flash_attn);
        set(&mut config.cache_type_k, &self.cache_type_k);
        set(&mut config.cache_type_v, &self.cache_type_v);
    }
}

/// The launch configuration of one model: the base, then the preset, then the
/// environment, then the automatic plans for whatever neither of the two
/// fixed. The chat template file, when the preset carries one, is already
/// set on `base`.
///
/// The window is not a plan: the supervisor resolves it before (the preset's,
/// else the backend's, else the default) and sets it on `base`, so it stands
/// whether or not the slot plan runs.
///
/// The plans are injected so the precedence is testable without reading a
/// model or the machine. Each plan already steps aside when its environment
/// variable is set; it is not called at all when the preset fixed its field.
pub(super) fn compose(
    base: &LlamaServerConfig,
    load: &LoadSettings,
    get_env: impl Fn(&str) -> Option<String>,
    plan_slots: impl FnOnce(&mut LlamaServerConfig),
    plan_offload: impl FnOnce(&mut LlamaServerConfig),
) -> LlamaServerConfig {
    let mut layered = base.clone();
    load.apply(&mut layered);
    let mut config = super::config::resolve_env_overrides(&layered, get_env);
    if load.n_parallel.is_none() {
        plan_slots(&mut config);
        if let Some(unified) = load.kv_unified {
            config.kv_unified = unified;
        }
    }
    if load.n_gpu_layers.is_none() {
        plan_offload(&mut config);
    }
    config
}

/// The load settings of the default preset of the local model `backend`, read
/// from `system_db` now. A missing file, no default preset or an unreadable
/// one reads as no setting, with a warning for the last.
pub(super) fn read_default(system_db: &Path, backend: &str) -> LoadSettings {
    if !system_db.exists() {
        return LoadSettings::default();
    }
    let read = apollia_core::LlmBackendRepository::open(system_db).and_then(|repo| {
        Ok(repo
            .list_presets(Some((ModelSource::Local, backend)))?
            .into_iter()
            .find(|p| p.is_model_default))
    });
    match read {
        Ok(Some(preset)) => LoadSettings::from_map(&preset.load),
        Ok(None) => LoadSettings::default(),
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                backend = %backend,
                error = %error,
                detail = "the model loads without its preset",
                "llama.server.preset.unreadable"
            );
            LoadSettings::default()
        }
    }
}

/// Write a chat template into `dir` and return the file. The name derives
/// from the text, so two presets with the same template share one file and a
/// changed template never overwrites the file a running process reads.
///
/// # Errors
/// The I/O error of creating the directory or writing the file.
pub(super) fn write_template(dir: &Path, template: &str) -> std::io::Result<PathBuf> {
    let mut hasher = DefaultHasher::new();
    template.hash(&mut hasher);
    let path = dir.join(format!("chat-template-{:016x}.jinja", hasher.finish()));
    std::fs::create_dir_all(dir)?;
    std::fs::write(&path, template)?;
    Ok(path)
}

#[cfg(test)]
mod tests;
