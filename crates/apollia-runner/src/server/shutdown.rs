//! `POST /shutdown` : arrêt propre du runner.
//!
//! Appelé par le daemon avant de tuer le runner. Le runner s'engage à exit
//! dans le délai annoncé (`exit_in_ms`). Au-delà, le daemon envoie SIGTERM
//! puis SIGKILL.

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
    // Délai annoncé : 200 ms est suffisant pour fermer les fichiers de
    // modèles mappés et libérer la VRAM.
    let exit_in_ms = 200;

    // Trigger le shutdown du serveur axum (graceful_shutdown).
    // On utilise oneshot car le receiver est passé à axum::serve.
    let mut guard = state.shutdown_tx.lock().await;
    if let Some(tx) = guard.take() {
        let _ = tx.send(());
        tracing::info!("shutdown requested via POST /shutdown");
    }

    Json(Response::success_no_id(ShutdownData { exit_in_ms }))
}
