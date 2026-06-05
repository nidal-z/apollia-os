//! axum HTTP server exposing the IPC endpoints.

use std::sync::Arc;
use std::time::SystemTime;

use axum::routing::{get, post};
use axum::Router;
use tokio::sync::oneshot;

use crate::lifecycle::ModelCache;

pub mod error;
pub mod handshake;
pub mod health;
pub mod llm;
pub mod shutdown;
pub mod stt;

/// State shared across all axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub model_cache: Arc<ModelCache>,
    pub started_at: SystemTime,
    /// Sender used to trigger server shutdown from the handler.
    pub shutdown_tx: Arc<tokio::sync::Mutex<Option<oneshot::Sender<()>>>>,

    /// Shared llama.cpp backend (`None` when compiled without the `local-cpu` feature).
    #[cfg(feature = "local-cpu")]
    pub llama: Arc<crate::backends::llama_cpp::LlamaCppBackend>,

    /// Shared whisper backend (`None` when compiled without the `local-cpu` feature).
    #[cfg(feature = "local-cpu")]
    pub whisper: Arc<crate::backends::whisper::WhisperBackend>,
}

impl AppState {
    pub fn new(shutdown_tx: oneshot::Sender<()>) -> Self {
        Self {
            model_cache: Arc::new(ModelCache::new()),
            started_at: SystemTime::now(),
            shutdown_tx: Arc::new(tokio::sync::Mutex::new(Some(shutdown_tx))),
            #[cfg(feature = "local-cpu")]
            llama: Arc::new(crate::backends::llama_cpp::LlamaCppBackend::new()),
            #[cfg(feature = "local-cpu")]
            whisper: Arc::new(crate::backends::whisper::WhisperBackend::new()),
        }
    }
}

/// Builds the axum router with all endpoints.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/handshake", get(handshake::handle))
        .route("/health", get(health::handle))
        .route("/shutdown", post(shutdown::handle))
        .route("/llm/load_model", post(llm::load_model))
        .route("/llm/unload_model", post(llm::unload_model))
        .route("/llm/complete", post(llm::complete))
        .route("/llm/stream", post(llm::stream))
        .route("/llm/tokenize", post(llm::tokenize))
        .route("/llm/embed", post(llm::embed))
        .route("/stt/transcribe", post(stt::transcribe))
        .with_state(state)
}
