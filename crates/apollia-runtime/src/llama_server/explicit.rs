//! Loading a model by hand, and estimating the load before it is made.
//!
//! A model loads on its own at the first request that needs it, under its
//! default preset (see `reload`). The user can also load one by hand, with a
//! window, an offload and advanced settings chosen for that load alone: they
//! are laid over the default preset for this instance and are not stored.
//!
//! A model loaded by hand serves every request for it, whatever window the
//! request asks for: a request carries no settings of its own, and relaunching
//! the model the user just loaded, under other settings, would undo the load.
//! The instance stays until it is unloaded or evicted.
//!
//! The estimate sizes the configuration the load would launch, through the
//! same assessment the Hub and the guardrail use, so the figures shown before
//! a load are those the guardrail then holds the load to.

use std::path::Path;

use apollia_llm::hardware::HardwareProfile;
use apollia_llm::recommend::{FitAssessment, MemoryArchitecture};
use serde_json::{Map, Value};

use crate::local_engine::guardrails::LoadEstimate;
use crate::local_engine::LoadConsent;

use super::load_settings::{self, LoadSettings};
use super::{ngl, LlamaServerConfig, LlamaServerError, LlamaServerSupervisor, ModelLease};

/// What loading a GGUF model under given settings would take.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExplicitEstimate {
    /// Context window the process would be launched with (`-c`).
    pub n_ctx: u32,
    /// Layers it would place on the accelerator (`-ngl`).
    pub n_gpu_layers: i32,
    /// Layers the model has, from its header; `0` when it could not be read.
    pub total_layers: u32,
    /// Window the model was trained with, from its header: the most a load
    /// can ask for. `None` when the header does not say.
    pub max_ctx: Option<u32>,
    /// Memory on the accelerator, in gigabytes.
    pub gpu_gb: f64,
    /// Weights, cache and engine overhead, in gigabytes.
    pub total_gb: f64,
    /// Whether the cache was sized from the header rather than by a
    /// proportional fallback.
    pub kv_cache_measured: bool,
    /// The estimate the load guardrail judges.
    pub guardrail: LoadEstimate,
}

impl LlamaServerSupervisor {
    /// The load settings of a load by hand of the local model `backend`: its
    /// default preset, with `overrides` laid over it key by key.
    pub async fn explicit_settings(
        &self,
        backend: &str,
        overrides: &Map<String, Value>,
    ) -> LoadSettings {
        let mut map = match self.system_db.clone() {
            Some(db) => {
                let backend = backend.to_owned();
                tokio::task::spawn_blocking(move || load_settings::read_default_map(&db, &backend))
                    .await
                    .unwrap_or_default()
            }
            None => Map::new(),
        };
        for (key, value) in overrides {
            map.insert(key.clone(), value.clone());
        }
        LoadSettings::from_map(&map)
    }

    /// What loading `model_path` under `load` would take, or `None` when the
    /// file cannot be read.
    pub async fn estimate_explicit(
        &self,
        model_path: &str,
        load: &LoadSettings,
    ) -> Option<ExplicitEstimate> {
        // The template file plays no part in the footprint; an estimate writes
        // nothing to disk.
        let load = LoadSettings {
            chat_template: None,
            ..load.clone()
        };
        let n_ctx = self.window_for(model_path, &load, None).await;
        let config = self
            .launch_config(model_path.to_owned(), n_ctx, &load)
            .await;
        tokio::task::spawn_blocking(move || {
            estimate_config(&config, &apollia_llm::hardware::detect())
        })
        .await
        .ok()
        .flatten()
    }

    /// Load `model_path` by hand under `load`, and return a lease on it.
    ///
    /// The launch is held to the load guardrail first, as a lazy load is. A
    /// resident process launched under the same settings is kept and marked
    /// as loaded by hand; any other process of the model is stopped.
    ///
    /// # Errors
    ///
    /// [`LlamaServerError::GuardrailRefused`] when the guardrail refuses the
    /// load and the user did not confirm; another [`LlamaServerError`] when
    /// the process cannot be spawned or never becomes healthy.
    pub async fn load_explicitly(
        &self,
        backend: &str,
        model_path: String,
        load: LoadSettings,
        consent: LoadConsent,
    ) -> Result<ModelLease, LlamaServerError> {
        let n_ctx = self.window_for(&model_path, &load, None).await;
        let _guard = self.respawn_lock.lock().await;
        if let Some(lease) = self.touch_live_instance(&model_path, n_ctx, &load).await {
            self.mark_explicit(&model_path, n_ctx, &load).await;
            return Ok(lease);
        }
        let config = self.launch_config(model_path.clone(), n_ctx, &load).await;
        let footprint_gb = self.admit(&config, consent).await?;
        self.stop_model(&model_path).await;
        self.evict_until_below_ceiling().await;
        let lease = self
            .spawn_configured(config, load, true, footprint_gb)
            .await?;
        tracing::event!(
            tracing::Level::INFO,
            backend = %backend,
            model = %model_path,
            n_ctx = lease.n_ctx(),
            "llm.model.loaded_by_hand"
        );
        Ok(lease)
    }

    /// A lease on the live process of `model_path` loaded by hand, marked as
    /// just used, or `None` when the model was not loaded by hand.
    pub(super) async fn touch_explicit_instance(&self, model_path: &str) -> Option<ModelLease> {
        let mut instances = self.instances.lock().await;
        let instance = instances
            .iter_mut()
            .filter(|i| i.explicit && i.model_path == model_path)
            .find_map(|i| i.is_running().then_some(i))?;
        instance.last_used = self.next_tick();
        instance.last_used_at = chrono::Utc::now();
        Some(ModelLease::new(
            instance.base_url(),
            instance.n_ctx,
            std::sync::Arc::clone(&instance.in_flight),
        ))
    }

    /// Mark the process of `model_path` launched at `n_ctx` under `load` as
    /// loaded by hand.
    async fn mark_explicit(&self, model_path: &str, n_ctx: u32, load: &LoadSettings) {
        let mut instances = self.instances.lock().await;
        for instance in instances
            .iter_mut()
            .filter(|i| i.model_path == model_path && i.n_ctx == n_ctx && &i.load == load)
        {
            instance.explicit = true;
        }
    }

    /// Stop every process serving `model_path`, whatever its settings.
    async fn stop_model(&self, model_path: &str) {
        let stopped: Vec<_> = {
            let mut instances = self.instances.lock().await;
            let (stopped, kept) = std::mem::take(&mut *instances)
                .into_iter()
                .partition(|i| i.model_path == model_path);
            *instances = kept;
            stopped
        };
        for mut instance in stopped {
            tracing::event!(
                tracing::Level::INFO,
                model = %instance.model_path,
                port = instance.port,
                "llama.server.instance.replaced_by_hand"
            );
            let _ = instance.child.kill().await;
            let _ = instance.child.wait().await;
        }
    }
}

/// Size the launch of `config` on the machine `profile` describes, or `None`
/// when the model file cannot be read.
#[must_use]
pub(crate) fn estimate_config(
    config: &LlamaServerConfig,
    profile: &HardwareProfile,
) -> Option<ExplicitEstimate> {
    let path = Path::new(&config.model_path);
    let size = std::fs::metadata(path).ok()?.len();
    let facts = apollia_llm::gguf_probe::probe_file(path, apollia_llm::gguf_probe::SCREEN_BYTES)
        .unwrap_or_default();
    let shape = ngl::runtime_shape(config);
    let fit = apollia_llm::recommend::assess_fit(size, &facts, profile, &shape);
    let architecture = MemoryArchitecture::of(profile);
    Some(ExplicitEstimate {
        n_ctx: config.n_ctx,
        n_gpu_layers: config.n_gpu_layers,
        total_layers: fit.plan.total_layers,
        max_ctx: facts.context_length.and_then(|n| u32::try_from(n).ok()),
        gpu_gb: gpu_gb(&fit, config.n_gpu_layers, architecture.uses_accelerator()),
        total_gb: fit.estimate.total_gb,
        kv_cache_measured: fit.estimate.kv_cache_measured,
        guardrail: LoadEstimate {
            total_gb: fit.estimate.total_gb,
            capacity_gb: architecture.total_capacity_gb(profile),
            badge: fit.badge,
        },
    })
}

/// Memory on the accelerator when `n_gpu_layers` layers are placed there: the
/// planned figure when the count is the plan's, else the share of the weights
/// those layers carry plus the cache, which sits wherever any layer does. A
/// negative count, or one past the model's layers, places them all.
fn gpu_gb(fit: &FitAssessment, n_gpu_layers: i32, accelerator: bool) -> f64 {
    if !accelerator || n_gpu_layers == 0 {
        return 0.0;
    }
    let total = fit.plan.total_layers.max(1);
    let on_device = u32::try_from(n_gpu_layers).map_or(total, |n| n.min(total));
    if on_device == fit.plan.n_gpu_layers {
        return fit.plan.device_gb;
    }
    let per_layer = fit.estimate.weights_gb / f64::from(total);
    per_layer * f64::from(on_device) + fit.estimate.kv_cache_gb
}

#[cfg(test)]
pub(crate) mod tests;
