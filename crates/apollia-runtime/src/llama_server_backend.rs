//! `LlamaServerBackend`: routes `CompletionModel` calls to the embedded
//! [`LlamaServerSupervisor`](crate::llama_server::LlamaServerSupervisor).
//!
//! A local `LlamaCpp` backend is served by the managed `llama-server` process
//! rather than the `apollia-runner` sidecar. This backend wraps the existing
//! OpenAI-compatible client ([`OpenAICompatibleClient`]) and resolves the
//! server's base URL dynamically on every call, so it survives a respawn (which
//! changes the loopback port). Before each request it asks the supervisor to
//! serve this backend's model (idempotent: a no-op when already loaded, a
//! process restart otherwise).
//!
//! Tool calling stays native: `llama-server` runs with `--jinja`, so the model's
//! own chat template drives tool formatting, exactly like the OpenAI-compatible
//! path. `is_local` is therefore left `false` (no GBNF grammar is attached).

use std::pin::Pin;
use std::sync::Arc;

use apollia_core::{LlmBackendConfig, LlmProvider};
use apollia_llm::backends::openai::{ApiBackendConfig, OpenAICompatibleClient};
use apollia_llm::types::{CompletionModel, CompletionRequest, CompletionResponse, StreamChunk};
use apollia_llm::LlmError;
use futures::Stream;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::llama_server::LlamaServerSupervisor;

/// `CompletionModel` backed by the embedded `llama-server`.
pub struct LlamaServerBackend {
    supervisor: Arc<LlamaServerSupervisor>,
    backend_name: String,
    model_id: String,
    model_path: String,
    /// Own cancellation token: the router builds its factory-provided backends
    /// outside its own token's scope, and local inference cancellation is
    /// best-effort (the supervisor can kill the process).
    cancel: CancellationToken,
    /// Cached OpenAI client, keyed by the base URL it was built for, so a stable
    /// server is not rebuilt on every call but a respawn (new port) is picked up.
    client: Mutex<Option<(String, Arc<OpenAICompatibleClient>)>>,
}

impl LlamaServerBackend {
    /// Build a backend bound to `supervisor` for the model at `model_path`.
    pub fn new(
        supervisor: Arc<LlamaServerSupervisor>,
        backend_name: String,
        model_id: String,
        model_path: String,
    ) -> Arc<Self> {
        Arc::new(Self {
            supervisor,
            backend_name,
            model_id,
            model_path: expand_home(&model_path),
            cancel: CancellationToken::new(),
            client: Mutex::new(None),
        })
    }

    /// Ensure the server is serving this backend's model, then return an OpenAI
    /// client pointed at its current base URL (rebuilt only when the URL changed).
    async fn ready_client(&self) -> Result<Arc<OpenAICompatibleClient>, LlmError> {
        self.supervisor
            .switch_model(self.model_path.clone())
            .await
            .map_err(|e| LlmError::BackendUnavailable {
                backend: self.backend_name.clone(),
                reason: format!("llama-server: {e}"),
            })?;

        let base =
            self.supervisor
                .base_url()
                .await
                .ok_or_else(|| LlmError::BackendUnavailable {
                    backend: self.backend_name.clone(),
                    reason: "llama-server is not running".to_owned(),
                })?;

        let mut guard = self.client.lock().await;
        if let Some((url, client)) = guard.as_ref() {
            if url == &base {
                return Ok(Arc::clone(client));
            }
        }
        let cfg = ApiBackendConfig {
            name: self.backend_name.clone(),
            api_url: base.clone(),
            // llama-server ignores the key; async-openai requires a non-empty one.
            api_key_env: String::new(),
            model: self.model_id.clone(),
        };
        let client = Arc::new(OpenAICompatibleClient::new(
            &cfg,
            "sk-no-key".to_owned(),
            self.cancel.clone(),
        ));
        *guard = Some((base, Arc::clone(&client)));
        Ok(client)
    }
}

#[async_trait::async_trait]
impl CompletionModel for LlamaServerBackend {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.ready_client().await?.complete(req).await
    }

    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        self.ready_client().await?.stream(req).await
    }

    fn is_available(&self) -> bool {
        true
    }

    fn backend_name(&self) -> &str {
        &self.backend_name
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn context_window(&self) -> Option<usize> {
        // The server is launched with `-c n_ctx`, so this is the usable window.
        // Reporting it lets the router size context compaction and avoid sending
        // a prompt that overflows the server (a hard 400 from llama-server).
        Some(self.supervisor.n_ctx() as usize)
    }
}

/// `LlamaCpp -> managed llama-server` override factory for the `LlmRouter`.
///
/// Each `LlamaCpp` provider backend is routed to a [`LlamaServerBackend`] wired
/// to the shared `supervisor`. Other providers return `None` so the router
/// instantiates them normally; all backends return `None` when no supervisor is
/// available (the binary was not found), leaving local inference unconfigured
/// rather than crashing.
pub fn llama_server_override(
    supervisor: Option<Arc<LlamaServerSupervisor>>,
) -> impl Fn(&LlmBackendConfig) -> Option<Arc<dyn CompletionModel>> {
    move |cfg: &LlmBackendConfig| {
        if !matches!(cfg.provider, LlmProvider::LlamaCpp) {
            return None;
        }
        let supervisor = supervisor.clone()?;
        Some(LlamaServerBackend::new(
            supervisor,
            cfg.name.clone(),
            cfg.name.clone(),
            cfg.model.clone(),
        ) as Arc<dyn CompletionModel>)
    }
}

/// Expand a leading `~/` to `$HOME/` so a model path stored with a tilde becomes
/// the absolute path `llama-server` requires. Idempotent for other paths.
fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    path.to_string()
}
