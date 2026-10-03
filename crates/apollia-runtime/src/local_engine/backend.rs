//! `LocalModelBackend`: routes `CompletionModel` calls to a local engine
//! supervisor, `llama-server` for a GGUF model or `vllm-mlx` for an MLX one.
//!
//! A local backend row is served by a process the engine supervises rather
//! than the `apollia-runner` sidecar. This backend wraps the existing
//! OpenAI-compatible client ([`OpenAICompatibleClient`]) and resolves the
//! server's base URL dynamically on every call, so it survives a respawn (which
//! changes the loopback port). Before each request it asks the engine to serve
//! this backend's model (idempotent: a no-op when already loaded, a process
//! restart otherwise).
//!
//! Tool calling stays native: both engines run the model's own chat template,
//! exactly like the OpenAI-compatible path. `is_local` is therefore left
//! `false`. Structured output follows the engine: the llama.cpp `grammar`
//! field for `llama-server`, `response_format` with the schema for `vllm-mlx`.

use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use apollia_llm::backends::openai::{ApiBackendConfig, OpenAICompatibleClient};
use apollia_llm::types::{CompletionModel, CompletionRequest, CompletionResponse, StreamChunk};
use apollia_llm::LlmError;
use futures::{Stream, StreamExt};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::LocalEngine;
use crate::llama_server::ModelLease;

/// `CompletionModel` backed by a local engine.
pub struct LocalModelBackend<E: LocalEngine> {
    supervisor: Arc<E>,
    backend_name: String,
    model_id: String,
    model_path: String,
    /// The window this backend asks the supervisor for: its `context_window`
    /// setting, or the supervisor's default, capped at the model's training
    /// length. `None` only when neither the setting nor the header is known.
    n_ctx: Option<u32>,
    /// The window the process serving this model is launched with, after the
    /// environment and the model's default preset. Read from `system.db` when
    /// the backend is built, then taken from the lease of every request.
    launched_n_ctx: AtomicU32,
    /// Own cancellation token: the router builds its factory-provided backends
    /// outside its own token's scope, and local inference cancellation is
    /// best-effort (the supervisor can kill the process).
    cancel: CancellationToken,
    /// Cached OpenAI client, keyed by the base URL it was built for, so a stable
    /// server is not rebuilt on every call but a respawn (new port) is picked up.
    client: Mutex<Option<(String, Arc<OpenAICompatibleClient>)>>,
}

impl<E: LocalEngine> LocalModelBackend<E> {
    /// Build a backend bound to `supervisor` for the model at `model_path`.
    pub fn new(
        supervisor: Arc<E>,
        backend_name: String,
        model_id: String,
        model_path: String,
        n_ctx: Option<u32>,
    ) -> Arc<Self> {
        let model_path = expand_home(&model_path);
        let n_ctx = match trained_context(&model_path) {
            Some(trained) => Some(apollia_llm::context_window::cap_at_trained(
                n_ctx.unwrap_or_else(|| supervisor.default_n_ctx()),
                Some(trained),
            )),
            None => n_ctx,
        };
        let launched_n_ctx = supervisor.launch_window(&backend_name, n_ctx);
        Arc::new(Self {
            supervisor,
            backend_name,
            model_id,
            model_path,
            n_ctx,
            launched_n_ctx: AtomicU32::new(launched_n_ctx),
            cancel: CancellationToken::new(),
            client: Mutex::new(None),
        })
    }

    /// Ensure the server is serving this backend's model, then return an OpenAI
    /// client pointed at its current base URL (rebuilt only when the URL changed),
    /// with the lease that keeps the model from being unloaded mid-request. The
    /// caller holds the lease until the answer is complete.
    async fn ready_client(&self) -> Result<(Arc<OpenAICompatibleClient>, ModelLease), LlmError> {
        let lease = self
            .supervisor
            .ensure_model(&self.backend_name, self.model_path.clone(), self.n_ctx)
            .await
            .map_err(|e| LlmError::BackendUnavailable {
                backend: self.backend_name.clone(),
                reason: e.to_string(),
            })?;
        self.launched_n_ctx.store(lease.n_ctx(), Ordering::Relaxed);
        let base = lease.base_url().to_owned();

        let mut guard = self.client.lock().await;
        if let Some((url, client)) = guard.as_ref() {
            if url == &base {
                return Ok((Arc::clone(client), lease));
            }
        }
        let cfg = ApiBackendConfig {
            name: self.backend_name.clone(),
            api_url: base.clone(),
            // Neither engine reads the key; async-openai requires a non-empty one.
            api_key_env: String::new(),
            model: self
                .supervisor
                .request_model(&self.model_id, &self.model_path),
            // Not set here: this backend answers `context_window` from the
            // supervisor, which owns the `-c` the process was launched with.
            context_window: None,
            // A llama.cpp server reads the `grammar` field its protocol adds,
            // so a structured-output call there is constrained by the GBNF
            // Apollia builds rather than by the server's own schema converter
            // (measured on llama-server 10092, 2026-09-16: both forms are
            // accepted on `/v1/chat/completions`). `vllm-mlx` reads
            // `response_format` with the schema and constrains natively.
            llama_cpp_extensions: self.supervisor.reads_llama_cpp_grammar(),
        };
        let client = Arc::new(OpenAICompatibleClient::new(
            &cfg,
            "sk-no-key".to_owned(),
            self.cancel.clone(),
        ));
        *guard = Some((base, Arc::clone(&client)));
        Ok((client, lease))
    }
}

#[async_trait::async_trait]
impl<E: LocalEngine> CompletionModel for LocalModelBackend<E> {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let (client, _lease) = self.ready_client().await?;
        let response = client.complete(req).await?;
        if let Some(timings) = response.engine_timings.as_ref() {
            crate::llm_timings::observe_timings(&self.backend_name, &self.model_id, timings);
        }
        Ok(response)
    }

    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        let (client, lease) = self.ready_client().await?;
        let inner = client.stream(req).await?;
        // Observe in passing rather than consuming: the chunk continues
        // downstream so the turn decomposition can attribute it to an iteration.
        let backend = self.backend_name.clone();
        let model = self.model_id.clone();
        // The lease travels inside the stream: the model stays in use until the
        // consumer has drained or dropped the answer, not merely until the
        // first byte arrived.
        let observed = inner.inspect(move |item| {
            let _held = &lease;
            if let Ok(StreamChunk::Timings(timings)) = item {
                crate::llm_timings::observe_timings(&backend, &model, timings);
            }
        });
        Ok(Box::pin(observed))
    }

    fn is_available(&self) -> bool {
        true
    }

    /// Return `true`: the request is built by the OpenAI-compatible client this
    /// backend wraps, which writes `seed` into the body, and both engines read
    /// it.
    fn carries_seed(&self) -> bool {
        true
    }

    fn backend_name(&self) -> &str {
        &self.backend_name
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn context_window(&self) -> Option<usize> {
        // The process is launched for exactly this window, so this is the
        // usable window. Reporting it lets the router size context compaction
        // and the context gauge, and avoid sending a prompt that overflows the
        // server (a hard 400 from llama-server).
        Some(self.launched_n_ctx.load(Ordering::Relaxed) as usize)
    }
}

/// The training length the model declares: the GGUF header's, read from its
/// first mebibyte, where the architecture keys sit ahead of the tokenizer
/// arrays, or the `max_position_embeddings` of an MLX folder's `config.json`.
///
/// `None` for a model that cannot be read or does not say; the window is then
/// left as configured, and the engine reports an overflow in its own log.
fn trained_context(model_path: &str) -> Option<u64> {
    let path = std::path::Path::new(model_path);
    if path.is_dir() {
        return crate::mlx_engine::trained_context(path);
    }
    let facts = apollia_llm::gguf_probe::probe_file(
        std::path::Path::new(model_path),
        apollia_llm::gguf_probe::SCREEN_BYTES,
    )
    .ok()?;
    if let Some(trained) = facts.context_length {
        tracing::debug!(model = %model_path, trained, "llama.server.trained_context");
    }
    facts.context_length
}

/// Expand a leading `~/` to `$HOME/` so a model path stored with a tilde becomes
/// the absolute path `llama-server` requires. Idempotent for other paths.
pub(super) fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = apollia_core::paths::home_string() {
            return format!("{home}/{rest}");
        }
    }
    path.to_string()
}
