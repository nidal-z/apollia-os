//! The `llama-server` instance that serves embeddings, beside the chat models.
//!
//! An instance launched with `--embeddings` serves `/v1/embeddings` and nothing
//! else, so search by meaning needs a process of its own. It is held apart
//! from the resident chat models: it does not count against
//! `APOLLIA_LLAMA_MAX_LOADED` and never evicts one, so asking for a vector
//! never unloads the model a conversation is using. It is started on the first
//! request and stopped after [`EMBEDDING_IDLE`] without one; the supervision
//! loop checks that every few seconds.
//!
//! One instance at most: asking for another embeddings model, or the same one
//! with another pooling, stops the first.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;

use apollia_llm::recommend::Pooling;

use super::config::HOST;
use super::{
    pick_free_port, Instance, LlamaServerConfig, LlamaServerError, LlamaServerSupervisor,
    LoadSettings, ModelLease, UnloadError,
};

/// How long the instance stays loaded after its last request.
pub const EMBEDDING_IDLE: Duration = Duration::from_secs(10 * 60);

/// Context window of the instance, in tokens, at most. Texts to embed are
/// passages of a document or a query, far below it; a larger window would
/// reserve memory no request uses.
pub const EMBEDDING_MAX_CTX: u32 = 2048;

/// The resident embeddings process.
pub(super) struct EmbeddingInstance {
    pub(super) instance: Instance,
    pub(super) pooling: Pooling,
    /// When a request last asked for it, on the monotonic clock that decides
    /// the idle stop.
    pub(super) last_call: Instant,
}

/// The `--pooling` value of a pooling.
fn pooling_arg(pooling: Pooling) -> &'static str {
    match pooling {
        Pooling::Mean => "mean",
        Pooling::Cls => "cls",
        Pooling::Last => "last",
    }
}

/// The window the instance is launched with for a model reading
/// `context_length` tokens at most.
#[must_use]
pub fn embedding_window(context_length: u32) -> u32 {
    context_length.clamp(1, EMBEDDING_MAX_CTX)
}

/// The argument vector of the embeddings instance.
///
/// No chat template, no reasoning parser: the process serves vectors only.
/// The batch and micro-batch equal the window, since an embeddings model reads
/// a whole text in one pass and refuses one longer than the micro-batch.
pub(super) fn build_embedding_args(
    model_path: &str,
    pooling: Pooling,
    n_ctx: u32,
    port: u16,
) -> Vec<String> {
    [
        "-m",
        model_path,
        "--embeddings",
        "--pooling",
        pooling_arg(pooling),
        "-c",
        &n_ctx.to_string(),
        "-b",
        &n_ctx.to_string(),
        "-ub",
        &n_ctx.to_string(),
        "-ngl",
        "999",
        "-np",
        "1",
        "-lv",
        "4",
        "--host",
        HOST,
        "--port",
        &port.to_string(),
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect()
}

impl LlamaServerSupervisor {
    /// A lease on the embeddings instance serving `model_path` with `pooling`,
    /// launching it when it is not resident.
    ///
    /// The chat models are left alone: the instance sits outside the
    /// residency ceiling and nothing is evicted to make room for it.
    ///
    /// # Errors
    /// [`LlamaServerError`] when the process cannot be spawned or never becomes
    /// healthy.
    pub async fn ensure_embedding_model(
        &self,
        model_path: &str,
        pooling: Pooling,
        context_length: u32,
    ) -> Result<ModelLease, LlamaServerError> {
        let n_ctx = embedding_window(context_length);
        let _guard = self.respawn_lock.lock().await;
        if let Some(lease) = self.touch_embedding(model_path, pooling).await {
            return Ok(lease);
        }
        self.stop_embedding("replaced").await;

        let port = pick_free_port()?;
        let args = build_embedding_args(model_path, pooling, n_ctx, port);
        tracing::event!(
            tracing::Level::INFO,
            model = %model_path,
            pooling = pooling_arg(pooling),
            n_ctx,
            port,
            args = %args.join(" "),
            "llama.server.embeddings.spawn"
        );
        let mut child = self.spawn_child(&args)?;
        self.wait_until_healthy(&mut child, port).await?;

        let config = LlamaServerConfig {
            model_path: model_path.to_owned(),
            n_ctx,
            n_gpu_layers: 999,
            n_parallel: Some(1),
            ..LlamaServerConfig::default()
        };
        let instance = Instance::new(
            config,
            LoadSettings::default(),
            port,
            child,
            self.next_tick(),
        );
        let lease = ModelLease::new(
            instance.base_url(),
            instance.n_ctx,
            Arc::clone(&instance.in_flight),
        );
        tracing::event!(
            tracing::Level::INFO,
            model = %model_path,
            port,
            "llama.server.embeddings.healthy"
        );
        *self.embedding.lock().await = Some(EmbeddingInstance {
            instance,
            pooling,
            last_call: Instant::now(),
        });
        Ok(lease)
    }

    /// A lease on the resident embeddings instance when it serves this model
    /// with this pooling and is alive; a dead one is forgotten.
    async fn touch_embedding(&self, model_path: &str, pooling: Pooling) -> Option<ModelLease> {
        let mut slot = self.embedding.lock().await;
        let resident = slot.as_mut()?;
        if resident.instance.model_path != model_path || resident.pooling != pooling {
            return None;
        }
        if !resident.instance.is_running() {
            tracing::event!(
                tracing::Level::WARN,
                model = %model_path,
                "llama.server.embeddings.dead"
            );
            *slot = None;
            return None;
        }
        resident.last_call = Instant::now();
        resident.instance.last_used = self.next_tick();
        resident.instance.last_used_at = Utc::now();
        Some(ModelLease::new(
            resident.instance.base_url(),
            resident.instance.n_ctx,
            Arc::clone(&resident.instance.in_flight),
        ))
    }

    /// Stop the embeddings instance, if there is one.
    async fn stop_embedding(&self, reason: &'static str) {
        let taken = self.embedding.lock().await.take();
        if let Some(mut resident) = taken {
            tracing::event!(
                tracing::Level::INFO,
                model = %resident.instance.model_path,
                reason,
                "llama.server.embeddings.stopped"
            );
            let _ = resident.instance.child.kill().await;
            let _ = resident.instance.child.wait().await;
        }
    }

    /// Stop the embeddings instance when, at `now`, its last request is
    /// [`EMBEDDING_IDLE`] old or more and none is in progress. Returns whether
    /// it was stopped.
    ///
    /// `now` is a parameter so the stop can be checked against a clock moved by
    /// hand; the supervision loop passes the real one.
    pub(super) async fn stop_idle_embedding(&self, now: Instant) -> bool {
        let idle = {
            let slot = self.embedding.lock().await;
            slot.as_ref().is_some_and(|resident| {
                resident.instance.in_flight.load(Ordering::Acquire) == 0
                    && now.saturating_duration_since(resident.last_call) >= EMBEDDING_IDLE
            })
        };
        if idle {
            self.stop_embedding("idle").await;
        }
        idle
    }

    /// Unload the embeddings instance when it serves `model_path`; `None` when
    /// it does not, so the caller looks among the chat models.
    pub(super) async fn unload_embedding(
        &self,
        model_path: &str,
    ) -> Option<Result<usize, UnloadError>> {
        let active_requests = {
            let slot = self.embedding.lock().await;
            let resident = slot.as_ref()?;
            if resident.instance.model_path != model_path {
                return None;
            }
            resident.instance.in_flight.load(Ordering::Acquire)
        };
        if active_requests > 0 {
            return Some(Err(UnloadError::InUse {
                model_path: model_path.to_owned(),
                active_requests,
            }));
        }
        self.stop_embedding("unloaded").await;
        Some(Ok(1))
    }
}

#[cfg(test)]
pub(crate) mod tests;
