//! What the supervisor has resident, and how to stop one model on request.
//!
//! The residency ceiling evicts on its own, least recently used first, but an
//! operator who wants memory back had no way to say which model to drop short
//! of restarting the runtime. [`LlamaServerSupervisor::loaded_models`] answers
//! what is resident and [`LlamaServerSupervisor::unload_model`] stops one.
//!
//! An unload never cuts an answer short. Every request holds a [`ModelLease`]
//! on the process serving it for as long as it runs, and a process with a live
//! lease is refused with [`UnloadError::InUse`].

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use thiserror::Error;

use super::LlamaServerSupervisor;

/// A claim on a resident model for the length of one request.
///
/// Obtained from [`LlamaServerSupervisor::ensure_model`]. While at least one
/// lease on a process is alive, [`LlamaServerSupervisor::unload_model`] refuses
/// to stop it. Dropping the lease releases the claim, so a streamed answer
/// keeps its lease inside the stream and releases it when the stream ends.
#[derive(Debug)]
pub struct ModelLease {
    base_url: String,
    n_ctx: u32,
    in_flight: Arc<AtomicUsize>,
}

impl ModelLease {
    pub(crate) fn new(base_url: String, n_ctx: u32, in_flight: Arc<AtomicUsize>) -> Self {
        in_flight.fetch_add(1, Ordering::AcqRel);
        Self {
            base_url,
            n_ctx,
            in_flight,
        }
    }

    /// OpenAI-compatible base URL of the process serving the model.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Context window of one request on the process, in tokens: the window
    /// it was launched with, after the environment and the model's preset.
    #[must_use]
    pub fn n_ctx(&self) -> u32 {
        self.n_ctx
    }
}

impl Drop for ModelLease {
    fn drop(&mut self) {
        self.in_flight.fetch_sub(1, Ordering::AcqRel);
    }
}

/// One resident model, as the supervisor launched it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedModel {
    /// The engine serving it, chosen by the model's format.
    pub engine: crate::engine_store::Engine,
    /// Display name, the file name of the model without its extension.
    pub model: String,
    /// Path the process was launched with (`-m`), the key an unload takes.
    pub model_path: String,
    /// Loopback port the process listens on.
    pub port: u16,
    /// Context window in tokens (`-c`).
    pub n_ctx: u32,
    /// Layers placed on the accelerator (`-ngl`).
    pub n_gpu_layers: i32,
    /// Server slots (`-np`), `None` when the engine default applies.
    pub n_parallel: Option<u32>,
    /// When the process became healthy.
    pub loaded_at: DateTime<Utc>,
    /// When a request last asked for this model.
    pub last_used_at: DateTime<Utc>,
    /// Requests being served right now.
    pub active_requests: usize,
}

/// Why a model could not be unloaded.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnloadError {
    /// No live process serves this model.
    #[error("model {model_path} is not loaded")]
    NotLoaded {
        /// The path that was asked for.
        model_path: String,
    },
    /// The model is answering a request; stopping it would cut that answer.
    #[error(
        "model {model_path} is in use: {active_requests} response(s) in progress, \
         retry once they finish"
    )]
    InUse {
        /// The path that was asked for.
        model_path: String,
        /// Requests holding a lease on it when the unload was asked.
        active_requests: usize,
    },
}

/// Display name for a model path: the file name without its extension.
fn display_name(model_path: &str) -> String {
    Path::new(model_path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| model_path.to_owned())
}

impl LlamaServerSupervisor {
    /// Every model with a live process, most recently used first.
    ///
    /// A process that has exited is left out rather than reported: the
    /// supervision task owns its fate, and listing it would show a model that
    /// answers nothing.
    pub async fn loaded_models(&self) -> Vec<LoadedModel> {
        let mut instances = self.instances.lock().await;
        let mut live: Vec<(u64, LoadedModel)> = instances
            .iter_mut()
            .filter_map(|i| {
                if !i.is_running() {
                    return None;
                }
                Some((
                    i.last_used,
                    LoadedModel {
                        engine: crate::engine_store::Engine::LlamaServer,
                        model: display_name(&i.model_path),
                        model_path: i.model_path.clone(),
                        port: i.port,
                        n_ctx: i.n_ctx,
                        n_gpu_layers: i.n_gpu_layers,
                        n_parallel: i.n_parallel,
                        loaded_at: i.loaded_at,
                        last_used_at: i.last_used_at,
                        active_requests: i.in_flight.load(Ordering::Acquire),
                    },
                ))
            })
            .collect();
        live.sort_by_key(|(tick, _)| std::cmp::Reverse(*tick));
        live.into_iter().map(|(_, m)| m).collect()
    }

    /// Stop every process serving `model_path`, and forget it.
    ///
    /// Returns how many processes were stopped (more than one when the same
    /// model was launched with two context windows). The model is not
    /// respawned by supervision: it was stopped deliberately, like an eviction.
    ///
    /// # Errors
    ///
    /// [`UnloadError::NotLoaded`] when no process serves the path, and
    /// [`UnloadError::InUse`] when one of them holds a live [`ModelLease`], in
    /// which case nothing is stopped.
    pub async fn unload_model(&self, model_path: &str) -> Result<usize, UnloadError> {
        // Serialised with `ensure_model`, so no request can take a lease on a
        // process between the check below and its removal.
        let _guard = self.respawn_lock.lock().await;
        let mut instances = self.instances.lock().await;
        let active_requests: usize = instances
            .iter()
            .filter(|i| i.model_path == model_path)
            .map(|i| i.in_flight.load(Ordering::Acquire))
            .sum();
        if !instances.iter().any(|i| i.model_path == model_path) {
            return Err(UnloadError::NotLoaded {
                model_path: model_path.to_owned(),
            });
        }
        if active_requests > 0 {
            tracing::info!(
                model = %model_path,
                active_requests,
                "llama.server.unload.refused"
            );
            return Err(UnloadError::InUse {
                model_path: model_path.to_owned(),
                active_requests,
            });
        }
        let (stopping, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut *instances)
            .into_iter()
            .partition(|i| i.model_path == model_path);
        *instances = kept;
        let stopped = stopping.len();
        for mut instance in stopping {
            let _ = instance.child.kill().await;
            let _ = instance.child.wait().await;
            tracing::info!(
                model = %instance.model_path,
                port = instance.port,
                "llama.server.instance.unloaded"
            );
        }
        Ok(stopped)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{live_instance, supervisor_with_ceiling};
    use super::*;

    /// Whether a process with this id still exists, through `kill -0`.
    fn process_exists(pid: u32) -> bool {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[tokio::test]
    async fn test_unloading_the_first_of_two_models_stops_it_and_keeps_the_second() {
        // GIVEN two stand-in models resident under a ceiling of two
        let sup = supervisor_with_ceiling(2);
        let a = live_instance(&sup, "/models/a.gguf", 9001).await;
        let b = live_instance(&sup, "/models/b.gguf", 9002).await;
        let pid_a = a.child.id().expect("a has a pid");
        let pid_b = b.child.id().expect("b has a pid");
        sup.instances.lock().await.push(a);
        sup.instances.lock().await.push(b);
        assert_eq!(sup.loaded_models().await.len(), 2);

        // WHEN the first is unloaded
        let stopped = sup.unload_model("/models/a.gguf").await;

        // THEN its process is stopped and only the second is listed
        assert_eq!(stopped, Ok(1));
        assert!(!process_exists(pid_a), "the unloaded process must be gone");
        assert!(process_exists(pid_b), "the other process must keep running");
        let listed = sup.loaded_models().await;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].model_path, "/models/b.gguf");
        assert_eq!(listed[0].model, "b");
        assert_eq!(listed[0].port, 9002);
    }

    #[tokio::test]
    async fn test_unloading_a_model_answering_a_request_is_refused_as_in_use() {
        // GIVEN a resident model serving one request
        let sup = supervisor_with_ceiling(2);
        let a = live_instance(&sup, "/models/a.gguf", 9001).await;
        let pid_a = a.child.id().expect("a has a pid");
        sup.instances.lock().await.push(a);
        let lease = sup
            .touch_live_instance(
                "/models/a.gguf",
                sup.n_ctx(),
                &crate::llama_server::LoadSettings::default(),
            )
            .await
            .expect("a is live");

        // WHEN it is asked to unload
        let refused = sup.unload_model("/models/a.gguf").await;

        // THEN the unload is refused with a readable "in use" error, and the
        // process keeps serving
        let err = refused.expect_err("an in-use model must not unload");
        assert_eq!(
            err,
            UnloadError::InUse {
                model_path: "/models/a.gguf".to_owned(),
                active_requests: 1,
            }
        );
        assert!(err.to_string().contains("is in use"), "{err}");
        assert!(process_exists(pid_a));
        assert_eq!(sup.loaded_models().await[0].active_requests, 1);

        // AND once the request ends, the same unload goes through
        drop(lease);
        assert_eq!(sup.unload_model("/models/a.gguf").await, Ok(1));
        assert!(sup.loaded_models().await.is_empty());
    }

    #[tokio::test]
    async fn test_unloading_a_model_that_is_not_loaded_says_so() {
        // GIVEN a supervisor with nothing resident
        let sup = supervisor_with_ceiling(1);

        // WHEN a model is asked to unload
        let result = sup.unload_model("/models/absent.gguf").await;

        // THEN the answer names the model as not loaded
        assert_eq!(
            result,
            Err(UnloadError::NotLoaded {
                model_path: "/models/absent.gguf".to_owned(),
            })
        );
    }

    #[tokio::test]
    async fn test_the_list_carries_the_launch_facts_most_recent_first() {
        // GIVEN two resident models, the first requested again after the second
        let sup = supervisor_with_ceiling(2);
        let a = live_instance(&sup, "/models/a.gguf", 9001).await;
        let b = live_instance(&sup, "/models/b.gguf", 9002).await;
        sup.instances.lock().await.push(a);
        sup.instances.lock().await.push(b);
        drop(
            sup.touch_live_instance(
                "/models/a.gguf",
                sup.n_ctx(),
                &crate::llama_server::LoadSettings::default(),
            )
            .await,
        );

        // WHEN the resident models are listed
        let listed = sup.loaded_models().await;

        // THEN the most recently used comes first, with the facts it was
        // launched with
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].model_path, "/models/a.gguf");
        assert_eq!(listed[0].n_ctx, sup.n_ctx());
        assert_eq!(listed[0].n_gpu_layers, 99);
        assert_eq!(listed[0].n_parallel, Some(1));
        assert_eq!(listed[0].active_requests, 0);
        assert!(listed[0].last_used_at >= listed[0].loaded_at);
        assert_eq!(listed[1].model_path, "/models/b.gguf");
    }

    #[tokio::test]
    async fn test_a_dead_process_is_not_listed_as_loaded() {
        // GIVEN a resident model whose process has exited
        let sup = supervisor_with_ceiling(1);
        let mut a = live_instance(&sup, "/models/a.gguf", 9001).await;
        a.child.kill().await.expect("kill must succeed");
        a.child.wait().await.expect("wait must succeed");
        sup.instances.lock().await.push(a);

        // WHEN the resident models are listed
        let listed = sup.loaded_models().await;

        // THEN it is absent, since it answers nothing
        assert!(listed.is_empty());
    }
}
