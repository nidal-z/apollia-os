//! How long a local model stays loaded: conversation leases, pins, and the
//! unloads that follow from them.
//!
//! A local model holds gigabytes of memory for as long as its process lives.
//! The runtime gives that memory back on its own, along three rules:
//!
//! - a model no request has used for the delay of `[llm.residency]
//!   idle_unload_minutes` is unloaded by a periodic sweep (see [`engines`]);
//! - a model whose last open conversation closes is unloaded at once;
//! - a model the user pinned is never unloaded by either rule.
//!
//! A conversation is open while an interface shows it, and the interface says
//! so with a lease: it opens one naming the model the conversation designates,
//! and releases it when the conversation leaves the screen. Every lease belongs
//! to a client, and when the client's event stream ends, every lease it held is
//! released, so a closed window never keeps a model loaded.
//!
//! A model answering a request is never unloaded by any of these rules: the
//! engines refuse to stop a process that serves one (see
//! [`UnloadError::InUse`](crate::llama_server::UnloadError::InUse)).
//!
//! Leases live in memory: they describe the interfaces connected now, and a
//! restart closes every connection. Pins are persisted in `system.db`.

mod engines;
pub mod room;

pub use engines::{
    spawn_idle_sweep, start_residency, unload_idle, unload_if_released, ClientLeases, Engines,
    IDLE_SWEEP_EVERY,
};

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use apollia_core::config::{resolve_setting, LlmResidencyConfig, PolicySettings, IDLE_UNLOAD_KEY};
use apollia_core::{EventBusSender, ModelUnloadReason, RuntimeEvent};
use thiserror::Error;

/// Why a pin could not be changed.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ResidencyError {
    /// The model path is blank.
    #[error("modelPath must not be empty")]
    EmptyPath,
    /// `system.db` could not be written.
    #[error("the pin could not be saved: {0}")]
    Store(String),
}

/// One open conversation of one client.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct LeaseKey {
    client: String,
    conversation: String,
}

/// The leases and pins in force.
#[derive(Debug, Default)]
struct Book {
    /// The model each open conversation designates.
    leases: BTreeMap<LeaseKey, String>,
    /// Paths of the pinned models.
    pins: BTreeSet<String>,
}

impl Book {
    fn is_leased(&self, model_path: &str) -> bool {
        self.leases.values().any(|m| m == model_path)
    }
}

/// The leases and pins every local engine consults before unloading a model.
pub struct Residency {
    book: Mutex<Book>,
    /// `system.db`, where pins are kept. `None` keeps them in memory only.
    system_db: Option<PathBuf>,
    /// `apollia.toml`, whose idle delay is read at every sweep. `None` holds
    /// the default delay.
    config_file: Option<PathBuf>,
    /// Where an unload is announced.
    events: Option<EventBusSender>,
}

impl Residency {
    /// The residency of an installation, its pins read from `system_db`.
    ///
    /// A `system.db` that cannot be read starts with no pin, with a warning:
    /// a model is then unloaded too early, never kept against the user.
    #[must_use]
    pub fn new(
        system_db: Option<PathBuf>,
        config_file: Option<PathBuf>,
        events: Option<EventBusSender>,
    ) -> Arc<Self> {
        let pins = system_db
            .as_deref()
            .map(|path| {
                apollia_core::llm_backend::LlmBackendRepository::open(path)
                    .and_then(|repo| repo.pinned_models())
                    .unwrap_or_else(|error| {
                        tracing::event!(
                            tracing::Level::WARN,
                            error = %error,
                            "llm.residency.pins_unreadable"
                        );
                        Vec::new()
                    })
            })
            .unwrap_or_default();
        Arc::new(Self {
            book: Mutex::new(Book {
                leases: BTreeMap::new(),
                pins: pins.into_iter().collect(),
            }),
            system_db,
            config_file,
            events,
        })
    }

    /// The book, even after a panic elsewhere left the lock poisoned: every
    /// update below leaves it consistent.
    fn book(&self) -> std::sync::MutexGuard<'_, Book> {
        self.book.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Open the lease of `conversation`, a conversation `client` shows, on
    /// `model_path`.
    ///
    /// A conversation designates one model: a lease it held on another is
    /// moved, and that other model is returned when no lease is left on it,
    /// for the caller to unload.
    pub fn open_lease(&self, client: &str, conversation: &str, model_path: &str) -> Option<String> {
        let mut book = self.book();
        let key = LeaseKey {
            client: client.to_owned(),
            conversation: conversation.to_owned(),
        };
        let previous = book.leases.insert(key, model_path.to_owned());
        previous.filter(|p| p != model_path && !book.is_leased(p))
    }

    /// Release the lease of `conversation` held by `client`, returning its
    /// model when no lease is left on it.
    pub fn close_lease(&self, client: &str, conversation: &str) -> Option<String> {
        let mut book = self.book();
        let key = LeaseKey {
            client: client.to_owned(),
            conversation: conversation.to_owned(),
        };
        let model = book.leases.remove(&key)?;
        (!book.is_leased(&model)).then_some(model)
    }

    /// Release every lease `client` holds, returning the models no lease is
    /// left on.
    pub fn release_client(&self, client: &str) -> Vec<String> {
        let mut book = self.book();
        let mut released = BTreeSet::new();
        book.leases.retain(|key, model| {
            if key.client == client {
                released.insert(model.clone());
                false
            } else {
                true
            }
        });
        released
            .into_iter()
            .filter(|model| !book.is_leased(model))
            .collect()
    }

    /// How many open conversations designate `model_path`.
    #[must_use]
    pub fn leases_on(&self, model_path: &str) -> usize {
        self.book()
            .leases
            .values()
            .filter(|m| *m == model_path)
            .count()
    }

    /// Whether `model_path` must stay loaded: pinned, or designated by an open
    /// conversation.
    #[must_use]
    pub fn is_held(&self, model_path: &str) -> bool {
        let book = self.book();
        book.pins.contains(model_path) || book.is_leased(model_path)
    }

    /// The pinned models, in path order.
    #[must_use]
    pub fn pins(&self) -> Vec<String> {
        self.book().pins.iter().cloned().collect()
    }

    /// Pin `model_path`, or unpin it, and save the pin in `system.db`.
    ///
    /// # Errors
    ///
    /// [`ResidencyError::EmptyPath`] for a blank path, and
    /// [`ResidencyError::Store`] when `system.db` cannot be written, in which
    /// case the pin is left as it was.
    pub fn set_pinned(&self, model_path: &str, pinned: bool) -> Result<(), ResidencyError> {
        let model_path = model_path.trim();
        if model_path.is_empty() {
            return Err(ResidencyError::EmptyPath);
        }
        if let Some(path) = &self.system_db {
            apollia_core::llm_backend::LlmBackendRepository::open(path)
                .and_then(|repo| repo.set_model_pinned(model_path, pinned))
                .map_err(|e| ResidencyError::Store(e.to_string()))?;
        }
        let mut book = self.book();
        if pinned {
            book.pins.insert(model_path.to_owned());
        } else {
            book.pins.remove(model_path);
        }
        tracing::event!(
            tracing::Level::INFO,
            model = %model_path,
            pinned,
            "llm.model.pin_changed"
        );
        Ok(())
    }

    /// The delay after which an unused model is unloaded, read now from
    /// `apollia.toml`; `None` when the user chose never.
    ///
    /// An unreadable file or value holds the default delay, with a warning.
    #[must_use]
    pub fn idle_unload(&self) -> Option<Duration> {
        let default = LlmResidencyConfig::default();
        let Some(path) = &self.config_file else {
            return default.idle_unload();
        };
        let minutes = resolve_setting(path, IDLE_UNLOAD_KEY, &PolicySettings::none())
            .map(|resolved| resolved.value.as_u64());
        match minutes {
            Ok(Some(idle_unload_minutes)) => LlmResidencyConfig {
                idle_unload_minutes,
            }
            .idle_unload(),
            Ok(None) | Err(_) => {
                tracing::event!(
                    tracing::Level::WARN,
                    key = IDLE_UNLOAD_KEY,
                    detail = "the default delay applies",
                    "llm.residency.delay_unreadable"
                );
                default.idle_unload()
            }
        }
    }

    /// Announce that `engine` unloaded `model_path` for `reason`.
    pub fn announce_unloaded(&self, engine: &str, model_path: &str, reason: ModelUnloadReason) {
        tracing::event!(
            tracing::Level::INFO,
            engine = %engine,
            model = %model_path,
            reason = %reason,
            "llm.model.unloaded"
        );
        if let Some(events) = &self.events {
            let _ = events.send(RuntimeEvent::LlmModelUnloaded {
                engine: engine.to_owned(),
                model_path: model_path.to_owned(),
                reason,
            });
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
