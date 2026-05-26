//! Serveur HTTP axum exposant les endpoints IPC.

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

/// État partagé entre tous les handlers axum.
#[derive(Clone)]
pub struct AppState {
    pub model_cache: Arc<ModelCache>,
    pub started_at: SystemTime,
    /// Sender pour déclencher le shutdown du server depuis le handler.
    pub shutdown_tx: Arc<tokio::sync::Mutex<Option<oneshot::Sender<()>>>>,

    /// Backend llama.cpp partagé (`None` si compilé sans feature `local-cpu`).
    #[cfg(feature = "local-cpu")]
    pub llama: Arc<crate::backends::llama_cpp::LlamaCppBackend>,

    /// Backend whisper partagé (`None` si compilé sans feature `local-cpu`).
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

/// Construit le router axum avec tous les endpoints.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/handshake", get(handshake::handle))
        .route("/health", get(health::handle))
        .route("/shutdown", post(shutdown::handle))
        .route("/llm/load_model", post(llm::load_model))
        .route("/llm/unload_model", post(llm::unload_model))
        .route("/llm/complete", post(llm::complete))
        .route("/llm/stream", post(llm::stream))
        .route("/llm/embed", post(llm::embed))
        .route("/stt/transcribe", post(stt::transcribe))
        .with_state(state)
}
