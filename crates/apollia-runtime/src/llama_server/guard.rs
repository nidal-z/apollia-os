//! The configuration a model would be launched with, and the load guardrail
//! held against it before anything resident is stopped to make room.

use std::path::Path;

use crate::local_engine::guardrails::{self, GuardrailPolicy, LoadEstimate};
use crate::local_engine::{GuardrailRefusal, LoadConsent};

use super::config::env_getter;
use super::load_settings::{self, LoadSettings};
use super::{ngl, parallel, projector, LlamaServerConfig, LlamaServerSupervisor};

impl LlamaServerSupervisor {
    /// The configuration `model_path` would be launched with at `n_ctx` under
    /// `load`: the preset, then the environment, then the plans.
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
        base.n_ctx = n_ctx;
        // The preset, then the environment, then the plans: the slot count
        // first, since the offload plan sizes the cache those slots hold (see
        // `parallel` and `ngl` for why the literals were wrong). The window is
        // already resolved by `effective_n_ctx`: the environment, then the
        // preset, then the backend's own window, then the default.
        load_settings::compose(
            &base,
            load,
            env_getter,
            parallel::apply_parallel_plan,
            ngl::apply_offload_plan,
        )
    }

    /// Hold the launch of `config` to the guardrail in force now.
    ///
    /// The policy is read from `apollia.toml` at every call, and the footprint
    /// is sized with the window and slot count `config` launches with.
    pub(super) async fn admit(
        &self,
        config: &LlamaServerConfig,
        consent: LoadConsent,
    ) -> Result<(), GuardrailRefusal> {
        let config_file = self.config_file.clone();
        let imposed = self.imposed.clone();
        let model_path = config.model_path.clone();
        let shape = ngl::runtime_shape(config);
        let judged = tokio::task::spawn_blocking(move || {
            let policy = GuardrailPolicy::read_under(config_file.as_deref(), &imposed);
            if policy.level == guardrails::GuardrailLevel::Disabled {
                return Ok(());
            }
            let path = Path::new(&model_path);
            let estimate = std::fs::metadata(path).ok().map(|meta| {
                LoadEstimate::gguf(path, meta.len(), &shape, &apollia_llm::hardware::detect())
            });
            guardrails::enforce(&policy, estimate.as_ref(), consent, &model_path)
        })
        .await;
        // A panicked estimate is no estimate: the launch goes ahead, as for an
        // unreadable file.
        judged.unwrap_or(Ok(()))
    }
}
