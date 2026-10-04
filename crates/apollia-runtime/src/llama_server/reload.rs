//! Launching a model under the load settings of its default preset, and
//! relaunching it when those settings change.
//!
//! The preset is read from `system.db` at every request, so a change made in
//! the model sheet reaches the next request without a restart. A process is
//! launched with fixed flags, so a model whose settings changed while it was
//! resident is stopped and launched again, and the bus hears
//! [`RuntimeEvent::LlmModelReloaded`].

use std::path::PathBuf;

use apollia_core::RuntimeEvent;

use crate::local_engine::LoadConsent;

use super::load_settings::{self, LoadSettings};
use super::{LlamaServerError, LlamaServerSupervisor, ModelLease};

/// Directory, beside `system.db`, holding the chat templates presets carry.
const TEMPLATE_DIR: &str = "llama-templates";

impl LlamaServerSupervisor {
    /// The load settings of the default preset of the local model `backend`,
    /// read now. No database, no preset or an unreadable one reads as none.
    pub async fn default_load(&self, backend: &str) -> LoadSettings {
        let Some(path) = self.system_db.clone() else {
            return LoadSettings::default();
        };
        let backend = backend.to_owned();
        tokio::task::spawn_blocking(move || load_settings::read_default(&path, &backend))
            .await
            .unwrap_or_default()
    }

    /// [`Self::default_load`] for a synchronous caller, such as a backend
    /// being built.
    #[must_use]
    pub(crate) fn default_load_blocking(&self, backend: &str) -> LoadSettings {
        match &self.system_db {
            Some(path) => load_settings::read_default(path, backend),
            None => LoadSettings::default(),
        }
    }

    /// Make `model_path` servable and return a lease on the process serving it.
    ///
    /// `backend` names the local model whose default preset is read now from
    /// `system.db`; its load settings are laid over the base configuration
    /// (see `load_settings`). A resident process launched under other
    /// settings is stopped and launched again.
    ///
    /// Returns immediately when that model already has a live process, which is
    /// what makes a second resident model worth having: the hand-off between two
    /// agents on two models costs a lookup rather than a reload. Otherwise a
    /// process is started, evicting the least recently used one first when the
    /// residency ceiling is reached. A model the user loaded by hand is served
    /// by that process whatever `n_ctx` asks, under the settings it was loaded
    /// with (see `explicit`).
    ///
    /// The URL is carried by the lease rather than read back through a separate
    /// accessor: with several instances alive, "the current one" is not a
    /// well-defined question, and a caller that asked for a specific model must
    /// not receive another one's port. Holding the lease for the length of a
    /// request is what lets [`unload_model`](Self::unload_model) refuse to stop
    /// a model in the middle of an answer.
    ///
    /// A launch is first held to the load guardrail, before any resident
    /// model is stopped to make room: a refused model leaves the others as
    /// they were. `consent` says whether the user confirmed a launch the
    /// guardrail would refuse.
    ///
    /// # Errors
    ///
    /// [`LlamaServerError::GuardrailRefused`] when the guardrail refuses the
    /// model and the user did not confirm; another [`LlamaServerError`] when
    /// the process cannot be spawned or never becomes healthy.
    pub async fn ensure_model(
        &self,
        backend: &str,
        model_path: String,
        n_ctx: Option<u32>,
        consent: LoadConsent,
    ) -> Result<ModelLease, LlamaServerError> {
        // The window is the environment's, else the preset's, else `n_ctx`,
        // else the default. A resident process for the same model launched
        // under other settings is stopped first, and the relaunch is announced
        // once the new process is healthy.
        let load = self.default_load(backend).await;
        let n_ctx = self.window_for(&load, n_ctx);
        let _guard = self.respawn_lock.lock().await;
        // A model the user loaded by hand serves every request for it, under
        // the settings it was loaded with, whatever window the request asks.
        if let Some(lease) = self.touch_explicit_instance(&model_path).await {
            return Ok(lease);
        }
        if let Some(lease) = self.touch_live_instance(&model_path, n_ctx, &load).await {
            return Ok(lease);
        }
        let config = self.launch_config(model_path.clone(), n_ctx, &load).await;
        let footprint_gb = self.admit(&config, consent).await?;
        let stale = self.stop_stale_instances(&model_path, &load).await;
        self.evict_until_below_ceiling().await;
        let lease = self
            .spawn_configured(config, load, false, footprint_gb)
            .await?;
        if stale {
            tracing::event!(
                tracing::Level::INFO,
                backend = %backend,
                model = %model_path,
                "llm.model.reloaded"
            );
            if let Some(events) = &self.events {
                let _ = events.send(RuntimeEvent::LlmModelReloaded {
                    backend: backend.to_owned(),
                    model_path,
                });
            }
        }
        Ok(lease)
    }

    /// The window a model is launched with: `APOLLIA_LLAMA_N_CTX`, else the
    /// preset's, else `requested` (the backend's own), else the default.
    #[must_use]
    pub fn window_for(&self, load: &LoadSettings, requested: Option<u32>) -> u32 {
        self.effective_n_ctx(load.n_ctx.or(requested))
    }

    /// Stop every resident process of `model_path` launched under settings
    /// other than `load`. Returns whether one was stopped.
    async fn stop_stale_instances(&self, model_path: &str, load: &LoadSettings) -> bool {
        let stale: Vec<_> = {
            let mut instances = self.instances.lock().await;
            let (stale, kept) = std::mem::take(&mut *instances)
                .into_iter()
                .partition(|i| i.model_path == model_path && &i.load != load);
            *instances = kept;
            stale
        };
        let stopped = !stale.is_empty();
        for mut instance in stale {
            tracing::event!(
                tracing::Level::INFO,
                model = %instance.model_path,
                port = instance.port,
                "llama.server.instance.settings_changed"
            );
            let _ = instance.child.kill().await;
            let _ = instance.child.wait().await;
        }
        stopped
    }

    /// The file holding the preset's chat template, written now, or `None`
    /// when the preset carries none. A template that cannot be written is
    /// dropped with a warning, and the model loads with its own.
    pub(super) async fn template_file(&self, load: &LoadSettings) -> Option<PathBuf> {
        let template = load.chat_template.clone()?;
        let dir = match &self.system_db {
            Some(db) => db.parent()?.join(TEMPLATE_DIR),
            None => std::env::temp_dir().join("apollia").join(TEMPLATE_DIR),
        };
        let written =
            tokio::task::spawn_blocking(move || load_settings::write_template(&dir, &template))
                .await;
        match written {
            Ok(Ok(path)) => Some(path),
            Ok(Err(error)) => {
                tracing::event!(
                    tracing::Level::WARN,
                    error = %error,
                    detail = "the model loads with its own template",
                    "llama.server.preset.template_unwritable"
                );
                None
            }
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests;
