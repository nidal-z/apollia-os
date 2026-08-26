//! `POST /shutdown`: graceful runner shutdown.
//!
//! Called by the daemon before killing the runner. The runner commits to exit
//! within the announced delay (`exit_in_ms`). Past that, the daemon sends
//! SIGTERM then SIGKILL.

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::ipc::Response;

use super::AppState;

#[derive(Debug, Serialize)]
pub struct ShutdownData {
    pub exit_in_ms: u64,
}

pub async fn handle(State(state): State<AppState>) -> Json<Response<ShutdownData>> {
    // Announced delay: 200 ms is enough to close mapped model files and
    // release the VRAM.
    let exit_in_ms = 200;

    // Trigger the axum server shutdown (graceful_shutdown).
    // A oneshot is used because the receiver is handed to axum::serve.
    let mut guard = state.shutdown_tx.lock().await;
    if let Some(tx) = guard.take() {
        let _ = tx.send(());
        tracing::info!(
            reason = "the /shutdown endpoint",
            "runner.shutdown.requested"
        );
    }

    Json(Response::success_no_id(ShutdownData { exit_in_ms }))
}
