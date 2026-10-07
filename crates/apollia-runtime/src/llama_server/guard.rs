//! The configuration a model would be launched with, and the load guardrail
//! held against it, the resident models counted, before anything resident is
//! stopped to make room.

use std::path::Path;

use crate::local_engine::guardrails::{self, GuardrailPolicy, LoadEstimate};
use crate::local_engine::{GuardrailRefusal, LoadConsent};
use crate::residency::replace::unload_before_load;
use crate::residency::room::{plan_room, Room};

use super::config::env_getter;
use super::load_settings::{self, LoadSettings};
use super::{
    devices, draft, log_level, ngl, parallel, projector, LlamaServerConfig, LlamaServerSupervisor,
};

impl LlamaServerSupervisor {
    /// The configuration `model_path` would be launched with at `n_ctx` under
    /// `load`: the engine log level of the settings, the preset, then the
    /// environment, then the plans.
    pub(super) async fn launch_config(
        &self,
        model_path: String,
        n_ctx: u32,
        load: &LoadSettings,
    ) -> LlamaServerConfig {
        let template_file = self.template_file(load).await;
        let mut base = self.config.lock().await.clone();
        // The index is a file beside the models, and a model it does not know
        // sends the lookup to the disk: off the async workers either way.
        let lookup = model_path.clone();
        base.mmproj_path = tokio::task::spawn_blocking(move || projector::for_model(&lookup))
            .await
            .unwrap_or(None);
        base.model_path = model_path;
        base.chat_template_file = template_file.map(|path| path.display().to_string());
        // The draft model is named by its index identifier: resolved against
        // the index, on disk, and held to the vocabulary of this model.
        let (lookup, named) = (base.model_path.clone(), load.clone());
        let (draft_model, load) =
            tokio::task::spawn_blocking(move || draft::for_model(&lookup, &named))
                .await
                .unwrap_or_else(|_| (None, load.clone()));
        let load = &load;
        base.draft_model = draft_model;
        base.n_ctx = n_ctx;
        // The engine log level of the settings, read at every load so a change
        // applies to the next one; the environment still wins in `compose`.
        let config_file = self.config_file.clone();
        let imposed = self.imposed.clone();
        // The devices held back are read with it and laid on the hardware
        // probe first: the plans below size the model on what is left.
        let (level, listed) = tokio::task::spawn_blocking(move || {
            devices::sync_selection(config_file.as_deref(), &imposed);
            (
                log_level::read(config_file.as_deref(), &imposed),
                apollia_llm::hardware::detect().devices,
            )
        })
        .await
        .unwrap_or_default();
        log_level::apply(&mut base, level);
        // The preset, then the environment, then the plans: the slot count
        // first, since the offload plan sizes the cache those slots hold (see
        // `parallel` and `ngl` for why the literals were wrong). The window is
        // already resolved by `window_for`: the environment, then the preset,
        // then the backend's own window, then the default of the settings.
        let mut config = load_settings::compose(
            &base,
            load,
            env_getter,
            parallel::apply_parallel_plan,
            ngl::apply_offload_plan,
        );
        devices::apply(&mut config, &listed);
        config
    }

    /// Hold the launch of `config` to the guardrail in force now, and return
    /// the footprint it was judged at, in gigabytes.
    ///
    /// The policy is read from `apollia.toml` at every call, and the footprint
    /// is sized with the window and slot count `config` launches with. The
    /// models already resident count against the budget: when the sum is
    /// refused, the resident models nothing holds are stopped, least recently
    /// used first and only as many as needed, before the launch is refused.
    ///
    /// With `[llm.residency] unload_before_load` on, the resident models that
    /// are not pinned and answer no request are stopped first, whatever the
    /// room: see [`crate::residency::replace`].
    pub(super) async fn admit(
        &self,
        config: &LlamaServerConfig,
        consent: LoadConsent,
    ) -> Result<Option<f64>, GuardrailRefusal> {
        let config_file = self.config_file.clone();
        let imposed = self.imposed.clone();
        let model_path = config.model_path.clone();
        let shape = ngl::runtime_shape(config);
        let sized = tokio::task::spawn_blocking(move || {
            let policy = GuardrailPolicy::read_under(config_file.as_deref(), &imposed);
            let unload_first = unload_before_load(config_file.as_deref(), &imposed);
            if policy.level == guardrails::GuardrailLevel::Disabled {
                return (policy, None, unload_first);
            }
            let path = Path::new(&model_path);
            let estimate = std::fs::metadata(path).ok().map(|meta| {
                LoadEstimate::gguf(path, meta.len(), &shape, &apollia_llm::hardware::detect())
            });
            (policy, estimate, unload_first)
        })
        .await;
        // A panicked estimate is no estimate: the launch goes ahead, as for an
        // unreadable file.
        let Ok((policy, estimate, unload_first)) = sized else {
            return Ok(None);
        };
        if unload_first {
            return self
                .admit_alone(&policy, estimate, consent, &config.model_path)
                .await;
        }
        let Some(estimate) = estimate else {
            guardrails::enforce(&policy, None, consent, &config.model_path)?;
            return Ok(None);
        };
        let residents = self.residents_beside(&config.model_path).await;
        let (room, judged) = plan_room(&policy, &estimate, &residents);
        if let Room::Evict(models) = &room {
            self.make_room(models).await;
        }
        guardrails::enforce(&policy, Some(&judged), consent, &config.model_path)?;
        Ok(Some(estimate.total_gb))
    }
}
