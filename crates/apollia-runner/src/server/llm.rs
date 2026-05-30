//! Handlers axum pour `/llm/*`.
//!
//! Cf. [IPC-PROTOCOL §3.4-3.7](../../../../docs/internal/architecture/IPC-PROTOCOL.md) :
//! - `POST /llm/load_model` : charge un GGUF dans le cache du runner.
//! - `POST /llm/unload_model` : libère le modèle (VRAM + RAM).
//! - `POST /llm/complete` : inférence non-streaming.
//! - `POST /llm/stream` : SSE token-by-token.
//! - `POST /llm/embed` : non implémenté (UnsupportedOperation).

use axum::extract::State;
#[cfg_attr(not(feature = "local-cpu"), allow(unused_imports))]
use axum::response::{IntoResponse, Response as AxumResponse};
use axum::Json;

use crate::ipc::{
    CompleteParams, EmbedParams, ErrorBody, ErrorCode, LoadModelParams, Request,
    UnloadModelParams,
};
#[cfg(feature = "local-cpu")]
use crate::ipc::Response;

use super::error::ipc_error_response;
use super::AppState;

/// Borne supérieure protocolaire pour `max_tokens` (IPC-PROTOCOL §7.2).
#[cfg(feature = "local-cpu")]
const MAX_TOKENS_CEILING: u32 = 32_768;
/// Plage admissible pour `temperature` (IPC-PROTOCOL §7.2).
#[cfg(feature = "local-cpu")]
const TEMPERATURE_RANGE: std::ops::RangeInclusive<f32> = 0.0..=2.0;

#[cfg(feature = "local-cpu")]
fn bad(message: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorCode::BadRequest, message)
}

#[cfg(feature = "local-cpu")]
fn validate_complete_params(p: &CompleteParams) -> Result<(), ErrorBody> {
    if p.model_id.trim().is_empty() {
        return Err(bad("model_id must not be empty"));
    }
    if p.messages.is_empty() {
        return Err(bad("messages must not be empty"));
    }
    if p.max_tokens == 0 || p.max_tokens > MAX_TOKENS_CEILING {
        return Err(bad(format!(
            "max_tokens must be in 1..={MAX_TOKENS_CEILING}, got {}",
            p.max_tokens
        )));
    }
    if !p.temperature.is_finite() || !TEMPERATURE_RANGE.contains(&p.temperature) {
        return Err(bad(format!(
            "temperature must be in [0.0, 2.0], got {}",
            p.temperature
        )));
    }
    Ok(())
}

#[cfg(not(feature = "local-cpu"))]
fn not_compiled(endpoint: &str) -> AxumResponse {
    ipc_error_response(ErrorBody::new(
        ErrorCode::UnsupportedOperation,
        format!("{endpoint} requires the local-cpu feature"),
    ))
}

#[cfg(feature = "local-cpu")]
pub async fn load_model(
    State(state): State<AppState>,
    Json(req): Json<Request<LoadModelParams>>,
) -> AxumResponse {
    use crate::backends::llama_cpp::validate_model_path;

    if let Err(e) = validate_model_path(&req.params.model_path) {
        return ipc_error_response(e);
    }

    match state.llama.load_model(req.params).await {
        Ok(data) => Json(Response::success(req.request_id, data)).into_response(),
        Err(e) => ipc_error_response(e),
    }
}

#[cfg(not(feature = "local-cpu"))]
pub async fn load_model(
    State(_state): State<AppState>,
    Json(_req): Json<Request<LoadModelParams>>,
) -> AxumResponse {
    not_compiled("/llm/load_model")
}

#[cfg(feature = "local-cpu")]
pub async fn unload_model(
    State(state): State<AppState>,
    Json(req): Json<Request<UnloadModelParams>>,
) -> AxumResponse {
    let removed = state.llama.unload(&req.params.model_id);
    // Keep the legacy ModelCache in sync (still consumed by /health).
    state.model_cache.unregister(&req.params.model_id);
    if removed {
        Json(Response::success(
            req.request_id,
            serde_json::json!({ "unloaded": true, "model_id": req.params.model_id }),
        ))
        .into_response()
    } else {
        ipc_error_response(ErrorBody::new(
            ErrorCode::ModelNotLoaded,
            format!("model '{}' not loaded", req.params.model_id),
        ))
    }
}

#[cfg(not(feature = "local-cpu"))]
pub async fn unload_model(
    State(_state): State<AppState>,
    Json(_req): Json<Request<UnloadModelParams>>,
) -> AxumResponse {
    not_compiled("/llm/unload_model")
}

#[cfg(feature = "local-cpu")]
pub async fn complete(
    State(state): State<AppState>,
    Json(req): Json<Request<CompleteParams>>,
) -> AxumResponse {
    if let Err(e) = validate_complete_params(&req.params) {
        return ipc_error_response(e);
    }
    match state.llama.complete(req.params).await {
        Ok(data) => Json(Response::success(req.request_id, data)).into_response(),
        Err(e) => ipc_error_response(e),
    }
}

#[cfg(not(feature = "local-cpu"))]
pub async fn complete(
    State(_state): State<AppState>,
    Json(_req): Json<Request<CompleteParams>>,
) -> AxumResponse {
    not_compiled("/llm/complete")
}

#[cfg(feature = "local-cpu")]
pub async fn stream(
    State(state): State<AppState>,
    Json(req): Json<Request<CompleteParams>>,
) -> AxumResponse {
    use axum::response::sse::{Event, KeepAlive, Sse};
    use futures::StreamExt;
    use std::convert::Infallible;

    if let Err(e) = validate_complete_params(&req.params) {
        return ipc_error_response(e);
    }

    let request_id = req.request_id;
    let token_stream = match state.llama.stream(req.params).await {
        Ok(s) => s,
        Err(e) => return ipc_error_response(e),
    };

    let event_stream = token_stream.map(move |item| {
        let event = match item {
            Ok(chunk) => {
                let finished = chunk.finish_reason.is_some();
                let envelope = crate::ipc::stream::SseChunk::new(request_id, chunk);
                let mut ev = Event::default()
                    .json_data(&envelope)
                    .unwrap_or_else(|_| Event::default().data("{}"));
                if finished {
                    ev = ev.event("done");
                }
                ev
            }
            Err(err) => {
                // Sérialise l'erreur en payload data ; le client lit le statut
                // d'erreur via le champ `ok` de l'enveloppe.
                let envelope = serde_json::json!({
                    "request_id": request_id,
                    "ok": false,
                    "error": err,
                });
                Event::default()
                    .event("error")
                    .json_data(&envelope)
                    .unwrap_or_else(|_| Event::default().data("{}"))
            }
        };
        Ok::<Event, Infallible>(event)
    });

    Sse::new(event_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

#[cfg(not(feature = "local-cpu"))]
pub async fn stream(
    State(_state): State<AppState>,
    Json(_req): Json<Request<CompleteParams>>,
) -> AxumResponse {
    not_compiled("/llm/stream")
}

pub async fn embed(
    State(_state): State<AppState>,
    Json(_req): Json<Request<EmbedParams>>,
) -> AxumResponse {
    // Embeddings locaux non câblés dans cette release : nécessite un backend
    // d'embeddings dédié (ex: BGE) qui n'est pas encore intégré au runner.
    ipc_error_response(ErrorBody::new(
        ErrorCode::UnsupportedOperation,
        "/llm/embed is not yet implemented in the runner",
    ))
}

// Les tests dépendent de `validate_complete_params` qui n'existe qu'avec la
// feature `local-cpu`. Gate identique.
#[cfg(all(test, feature = "local-cpu"))]
mod tests {
    use super::*;
    use crate::ipc::{ChatMessage, Role};

    fn base_complete() -> CompleteParams {
        CompleteParams {
            model_id: "test".into(),
            messages: vec![ChatMessage {
                role: Role::User,
                content: "hi".into(),
            }],
            max_tokens: 16,
            temperature: 0.7,
            top_p: 0.95,
            top_k: 40,
            repeat_penalty: 1.1,
            seed: None,
            stop: vec![],
        }
    }

    #[test]
    fn validate_rejects_temperature_too_high() {
        let mut p = base_complete();
        p.temperature = 5.0;
        let err = validate_complete_params(&p).expect_err("must reject");
        assert_eq!(err.code, ErrorCode::BadRequest);
    }

    #[test]
    fn validate_rejects_max_tokens_over_ceiling() {
        let mut p = base_complete();
        p.max_tokens = 100_000;
        let err = validate_complete_params(&p).expect_err("must reject");
        assert_eq!(err.code, ErrorCode::BadRequest);
    }

    #[test]
    fn validate_rejects_empty_messages() {
        let mut p = base_complete();
        p.messages.clear();
        let err = validate_complete_params(&p).expect_err("must reject");
        assert_eq!(err.code, ErrorCode::BadRequest);
    }

    #[test]
    fn validate_accepts_normal_params() {
        let p = base_complete();
        assert!(validate_complete_params(&p).is_ok());
    }
}
