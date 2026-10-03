//! The local inference engines, and which one serves a model.
//!
//! The user never picks an engine: the format of the model does. A GGUF file
//! runs on the embedded `llama-server`, an MLX folder runs on `vllm-mlx`, and
//! the format is the one the model index records for it. Both engines sit
//! behind [`LocalEngine`], so the backend that talks to them, the list of
//! resident models and the unload are written once.
//!
//! - [`LocalEngine`] is what a supervisor offers: make a model servable, list
//!   and stop the resident ones, and say how a request must be shaped for it.
//! - [`engine_for_format`] is the whole routing rule.
//! - [`local_engine_override`] is the router factory that applies it to every
//!   local backend row.
//!
//! An MLX model asked on a machine that is not a Mac with Apple Silicon, or on
//! a build that carries no MLX engine, is not handed to `llama-server`: it gets
//! a backend that refuses every call with the reason, so the chat says why
//! instead of failing on a folder `llama-server` cannot open.

mod backend;
mod capabilities;
mod rows;

pub use backend::LocalModelBackend;
pub use capabilities::{model_capabilities, ModelCapabilities};
pub use rows::{local_row_for, row_name};

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use apollia_core::{LlmBackendConfig, LlmProvider};
use apollia_llm::model_index::{ModelFormat, ModelIndex, INDEX_FILE_NAME};
use apollia_llm::types::{CompletionModel, CompletionRequest, CompletionResponse, StreamChunk};
use apollia_llm::LlmError;
use futures::Stream;
use thiserror::Error;

use crate::engine_store::Engine;
use crate::llama_server::{LlamaServerSupervisor, LoadedModel, ModelLease, UnloadError};
use crate::mlx_engine::MlxEngineSupervisor;

/// Why a local engine could not serve a model.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LocalEngineError {
    /// The engine process could not be started or never became ready.
    #[error("{engine}: {reason}")]
    Launch {
        /// Wire name of the engine.
        engine: &'static str,
        /// What went wrong, as the supervisor reported it.
        reason: String,
    },
    /// An MLX model on a machine where MLX does not run.
    #[error(
        "MLX models only run on a Mac with Apple Silicon; this machine is out of scope \
         for MLX and runs GGUF models"
    )]
    MlxNeedsAppleSilicon,
    /// An MLX model on a Mac whose build carries no MLX engine.
    #[error("the MLX engine is not installed on this machine; download a GGUF model instead")]
    MlxEngineMissing,
}

/// What a local engine supervisor offers to the rest of the runtime.
///
/// Implemented by [`LlamaServerSupervisor`] and [`MlxEngineSupervisor`]. A
/// process serves exactly one model, and the engine owns its processes:
/// launching on demand, respawning after a crash, stopping on shutdown.
pub trait LocalEngine: Send + Sync + 'static {
    /// Which engine this is.
    fn engine(&self) -> Engine;

    /// Context window, in tokens, for a backend that asks for none.
    fn default_n_ctx(&self) -> u32;

    /// The window a backend asking for `requested` would be served with,
    /// before any process exists, for the router to size compaction.
    fn launch_window(&self, backend: &str, requested: Option<u32>) -> u32;

    /// Make `model_path` servable and return a lease on the process serving it.
    fn ensure_model(
        &self,
        backend: &str,
        model_path: String,
        n_ctx: Option<u32>,
    ) -> impl Future<Output = Result<ModelLease, LocalEngineError>> + Send;

    /// Every resident model, most recently used first.
    fn loaded_models(&self) -> impl Future<Output = Vec<LoadedModel>> + Send;

    /// Stop every process serving `model_path`.
    fn unload_model(
        &self,
        model_path: &str,
    ) -> impl Future<Output = Result<usize, UnloadError>> + Send;

    /// How many models may stay resident at once.
    fn max_loaded(&self) -> usize;

    /// Whether the endpoint reads the llama.cpp `grammar` field. When it does
    /// not, a structured-output call carries `response_format` instead.
    fn reads_llama_cpp_grammar(&self) -> bool;

    /// The `model` field a request to the process must carry. `llama-server`
    /// ignores it; `vllm-mlx` answers 404 unless it names the folder it
    /// serves.
    fn request_model(&self, model_id: &str, model_path: &str) -> String;
}

impl LocalEngine for LlamaServerSupervisor {
    fn engine(&self) -> Engine {
        Engine::LlamaServer
    }

    fn default_n_ctx(&self) -> u32 {
        self.n_ctx()
    }

    fn launch_window(&self, backend: &str, requested: Option<u32>) -> u32 {
        self.window_for(&self.default_load_blocking(backend), requested)
    }

    async fn ensure_model(
        &self,
        backend: &str,
        model_path: String,
        n_ctx: Option<u32>,
    ) -> Result<ModelLease, LocalEngineError> {
        LlamaServerSupervisor::ensure_model(self, backend, model_path, n_ctx)
            .await
            .map_err(|e| LocalEngineError::Launch {
                engine: Engine::LlamaServer.as_str(),
                reason: e.to_string(),
            })
    }

    async fn loaded_models(&self) -> Vec<LoadedModel> {
        LlamaServerSupervisor::loaded_models(self).await
    }

    async fn unload_model(&self, model_path: &str) -> Result<usize, UnloadError> {
        LlamaServerSupervisor::unload_model(self, model_path).await
    }

    fn max_loaded(&self) -> usize {
        LlamaServerSupervisor::max_loaded(self)
    }

    fn reads_llama_cpp_grammar(&self) -> bool {
        true
    }

    fn request_model(&self, model_id: &str, _model_path: &str) -> String {
        model_id.to_owned()
    }
}

/// The engine that runs a model of `format`. The whole routing rule.
#[must_use]
pub const fn engine_for_format(format: ModelFormat) -> Engine {
    match format {
        ModelFormat::Gguf => Engine::LlamaServer,
        ModelFormat::Mlx => Engine::VllmMlx,
    }
}

/// Whether this build runs on a Mac with Apple Silicon, where MLX runs.
#[must_use]
pub const fn is_apple_silicon() -> bool {
    cfg!(all(target_os = "macos", target_arch = "aarch64"))
}

/// The format of the model a local backend row designates.
///
/// The model index decides: the entry whose identifier is the row's name, or
/// whose file or folder is the row's model path. A path the index does not
/// know (a model placed by hand and not yet indexed) is read from the disk: a
/// folder is MLX, anything else GGUF.
#[must_use]
pub fn model_format(
    models_dir: Option<&Path>,
    backend_name: &str,
    model_path: &str,
) -> ModelFormat {
    let path = Path::new(model_path);
    if let Some(dir) = models_dir {
        if let Ok(index) = ModelIndex::load(&dir.join(INDEX_FILE_NAME)) {
            let found = index
                .get(backend_name)
                .or_else(|| index.models.iter().find(|m| m.owns(path)));
            if let Some(model) = found {
                return model.format;
            }
        }
    }
    if path.is_dir() {
        ModelFormat::Mlx
    } else {
        ModelFormat::Gguf
    }
}

/// Build the backend for a local row whose model has `format`.
///
/// `apple_silicon` is a parameter so the refusal off Apple Silicon is tested
/// on the machine the tests run on, whatever it is.
pub fn route_local<L: LocalEngine, M: LocalEngine>(
    cfg: &LlmBackendConfig,
    format: ModelFormat,
    llama: Option<&Arc<L>>,
    mlx: Option<&Arc<M>>,
    apple_silicon: bool,
) -> Option<Arc<dyn CompletionModel>> {
    let n_ctx = apollia_llm::context_window::configured(&cfg.config_json);
    match engine_for_format(format) {
        Engine::LlamaServer => {
            let engine = Arc::clone(llama?);
            Some(LocalModelBackend::new(
                engine,
                cfg.name.clone(),
                cfg.name.clone(),
                cfg.model.clone(),
                n_ctx,
            ) as Arc<dyn CompletionModel>)
        }
        Engine::VllmMlx => {
            let refusal = if !apple_silicon {
                LocalEngineError::MlxNeedsAppleSilicon
            } else if let Some(engine) = mlx {
                return Some(LocalModelBackend::new(
                    Arc::clone(engine),
                    cfg.name.clone(),
                    cfg.name.clone(),
                    cfg.model.clone(),
                    n_ctx,
                ) as Arc<dyn CompletionModel>);
            } else {
                LocalEngineError::MlxEngineMissing
            };
            tracing::event!(
                tracing::Level::WARN,
                backend = %cfg.name,
                model = %cfg.model,
                reason = %refusal,
                "llm.local_engine.refused"
            );
            Some(Arc::new(RefusedLocalModel {
                backend_name: cfg.name.clone(),
                model_id: cfg.name.clone(),
                reason: refusal.to_string(),
            }) as Arc<dyn CompletionModel>)
        }
        // The store also holds Node.js for the tool servers; it runs no
        // model, and `engine_for_format` never names it.
        Engine::NodeJs => None,
    }
}

/// Router factory for the local backend rows (`provider = llama-cpp`).
///
/// Each row is routed by the format of its model (see [`model_format`]) to
/// the engine that runs it. Other providers return `None`, so the router
/// builds them normally. A GGUF row returns `None` when no `llama-server` was
/// found, which leaves local inference unconfigured rather than crashing, as
/// it always did.
pub fn local_engine_override(
    llama: Option<Arc<LlamaServerSupervisor>>,
    mlx: Option<Arc<MlxEngineSupervisor>>,
    models_dir: Option<PathBuf>,
) -> impl Fn(&LlmBackendConfig) -> Option<Arc<dyn CompletionModel>> {
    move |cfg: &LlmBackendConfig| {
        if !matches!(cfg.provider, LlmProvider::LlamaCpp) {
            return None;
        }
        let path = backend::expand_home(&cfg.model);
        let format = model_format(models_dir.as_deref(), &cfg.name, &path);
        route_local(
            cfg,
            format,
            llama.as_ref(),
            mlx.as_ref(),
            is_apple_silicon(),
        )
    }
}

/// A local model no engine on this machine can run. Every call answers the
/// reason, so the caller shows it rather than a generic failure.
struct RefusedLocalModel {
    backend_name: String,
    model_id: String,
    reason: String,
}

impl RefusedLocalModel {
    fn error(&self) -> LlmError {
        LlmError::BackendUnavailable {
            backend: self.backend_name.clone(),
            reason: self.reason.clone(),
        }
    }
}

#[async_trait::async_trait]
impl CompletionModel for RefusedLocalModel {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Err(self.error())
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        Err(self.error())
    }

    fn is_available(&self) -> bool {
        false
    }

    fn backend_name(&self) -> &str {
        &self.backend_name
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }
}

#[cfg(test)]
mod tests;
