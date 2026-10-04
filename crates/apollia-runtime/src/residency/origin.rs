//! Who loaded a resident model, and the idle delay that follows from it.
//!
//! A model loaded from the application and a model the compatible server
//! loaded for a request of another program are kept by the same rules, with
//! two differences the `[server]` section sets: the server's models have their
//! own idle delay, and loading one for a request can unload the others the
//! server loaded. The origin is fixed when the process is launched; a model the
//! application loaded and a request later used stays the application's.
//!
//! The origin is kept by load instant: a model the server loaded, unloaded,
//! then loaded again from the application is a new process with a new instant,
//! and counts as the application's.

use std::time::Duration;

use apollia_core::config::{
    resolve_setting, PolicySettings, ServerConfig, JIT_AUTO_UNLOAD_KEY, JIT_IDLE_TTL_KEY,
    JIT_KEEP_LAST_ONLY_KEY,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::llama_server::LoadedModel;

use super::Residency;

/// Who loaded a resident model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LoadOrigin {
    /// The application, a conversation or an agent of this runtime.
    App,
    /// The compatible server, for a request of another program.
    Server,
}

/// The idle delays the sweep applies, by origin; `None` keeps a model of that
/// origin loaded however long it stays unused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdleDelays {
    /// Delay of `[llm.residency]`, for the models the application loaded.
    pub general: Option<Duration>,
    /// Delay for the models the compatible server loaded.
    pub server: Option<Duration>,
}

impl IdleDelays {
    /// The same delay for every model, whoever loaded it.
    #[must_use]
    pub const fn uniform(delay: Option<Duration>) -> Self {
        Self {
            general: delay,
            server: delay,
        }
    }

    /// The delay of a model of `origin`.
    #[must_use]
    pub const fn of(&self, origin: LoadOrigin) -> Option<Duration> {
        match origin {
            LoadOrigin::App => self.general,
            LoadOrigin::Server => self.server,
        }
    }

    /// Whether no model of either origin is ever unloaded for idleness.
    #[must_use]
    pub const fn never(&self) -> bool {
        self.general.is_none() && self.server.is_none()
    }
}

/// `minutes` as a delay, `None` for zero.
fn minutes(minutes: u64) -> Option<Duration> {
    (minutes > 0).then(|| Duration::from_secs(minutes.saturating_mul(60)))
}

impl Residency {
    /// Record that the compatible server launched the process of `model_path`
    /// that finished loading at `loaded_at`.
    pub fn mark_loaded_by_server(&self, model_path: &str, loaded_at: DateTime<Utc>) {
        self.book()
            .server_loaded
            .insert(model_path.to_owned(), loaded_at);
        tracing::event!(
            tracing::Level::INFO,
            model = %model_path,
            "llm.model.loaded_by_server"
        );
    }

    /// Who loaded `model`.
    #[must_use]
    pub fn origin_of(&self, model: &LoadedModel) -> LoadOrigin {
        match self.book().server_loaded.get(&model.model_path) {
            Some(at) if *at == model.loaded_at => LoadOrigin::Server,
            _ => LoadOrigin::App,
        }
    }

    /// Forget the origin of `model_path` once its process is gone.
    pub(super) fn forget_origin(&self, model_path: &str) {
        self.book().server_loaded.remove(model_path);
    }

    /// The idle delays in force now: `[llm.residency]` for every model, and
    /// for the models the compatible server loaded, its own delay while
    /// `[server] jit_auto_unload` is on.
    #[must_use]
    pub fn idle_delays(&self) -> IdleDelays {
        let general = self.idle_unload();
        let defaults = ServerConfig::default();
        let auto_unload = self
            .server_setting(JIT_AUTO_UNLOAD_KEY)
            .and_then(|v| v.as_bool())
            .unwrap_or(defaults.jit_auto_unload);
        if !auto_unload {
            return IdleDelays::uniform(general);
        }
        let ttl = self
            .server_setting(JIT_IDLE_TTL_KEY)
            .and_then(|v| v.as_u64())
            .unwrap_or(defaults.jit_idle_ttl_minutes);
        IdleDelays {
            general,
            server: minutes(ttl),
        }
    }

    /// Whether loading a model for the compatible server unloads the other
    /// models it loaded, read now from `[server] jit_keep_last_only`.
    #[must_use]
    pub fn keep_last_server_model_only(&self) -> bool {
        self.server_setting(JIT_KEEP_LAST_ONLY_KEY)
            .and_then(|v| v.as_bool())
            .unwrap_or(ServerConfig::default().jit_keep_last_only)
    }

    /// The value of a `[server]` key read now, `None` without a file or when
    /// it cannot be read, in which case the default applies.
    fn server_setting(&self, key: &str) -> Option<Value> {
        let path = self.config_file.as_ref()?;
        match resolve_setting(path, key, &PolicySettings::none()) {
            Ok(resolved) => Some(resolved.value),
            Err(error) => {
                tracing::event!(
                    tracing::Level::WARN,
                    key = key,
                    error = %error,
                    "llm.residency.server_setting_unreadable"
                );
                None
            }
        }
    }
}
