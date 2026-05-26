//! `GET /handshake` : annonce du protocole + GPU info.

use axum::extract::State;
use axum::Json;

use crate::ipc::{Backend, HandshakeData, Response};

use super::AppState;

pub async fn handle(State(_state): State<AppState>) -> Json<Response<HandshakeData>> {
    let data = HandshakeData {
        protocol_version: "1.0".into(),
        runner_version: env!("CARGO_PKG_VERSION").into(),
        backend: Backend::from_features(),
        // GPU info détaillée sera ajoutée en STORY-004 quand on aura les
        // backends réels capables de l'introspecter (compute capability,
        // driver_version, etc.). Phase 1 : pas de GPU détectée côté runner
        // (la détection vit côté daemon dans gpu_detection.rs).
        gpu: None,
        supported_endpoints: supported_endpoints(),
        max_concurrent_requests: 1,
    };
    Json(Response::success_no_id(data))
}

fn supported_endpoints() -> Vec<String> {
    #[allow(unused_mut)]
    let mut endpoints = vec![
        "/handshake".to_string(),
        "/health".to_string(),
        "/shutdown".to_string(),
    ];

    // Endpoints LLM/STT exposés sur tous les builds avec backend local.
    #[cfg(feature = "local-cpu")]
    {
        endpoints.extend([
            "/llm/load_model".to_string(),
            "/llm/unload_model".to_string(),
            "/llm/complete".to_string(),
            "/llm/stream".to_string(),
            "/llm/embed".to_string(),
            "/stt/transcribe".to_string(),
        ]);
    }

    endpoints
}
