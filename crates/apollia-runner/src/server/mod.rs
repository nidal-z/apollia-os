//! axum HTTP server exposing the IPC endpoints.
//!
//! The runner serves speech-to-text (whisper) only. Local LLM inference runs
//! through the embedded llama-server managed by the daemon, so there are no
//! `/llm/*` endpoints here.

use std::sync::Arc;
use std::time::SystemTime;

use axum::routing::{get, post};
use axum::Router;
use tokio::sync::oneshot;

use crate::lifecycle::ModelCache;

pub mod error;
pub mod handshake;
pub mod health;
pub mod shutdown;
pub mod stt;

/// State shared across all axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub model_cache: Arc<ModelCache>,
    pub started_at: SystemTime,
    /// Sender used to trigger server shutdown from the handler.
    pub shutdown_tx: Arc<tokio::sync::Mutex<Option<oneshot::Sender<()>>>>,

    /// Shared whisper backend (present only with the `local-cpu` feature).
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
        .route("/stt/transcribe", post(stt::transcribe))
        .with_state(state)
}
