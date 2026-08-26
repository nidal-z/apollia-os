//! axum handlers for `/stt/*`.

use axum::extract::State;
#[cfg_attr(not(feature = "local-cpu"), allow(unused_imports))]
use axum::response::{IntoResponse, Response as AxumResponse};
use axum::Json;

#[cfg(feature = "local-cpu")]
use crate::ipc::Response;
use crate::ipc::{ErrorBody, ErrorCode, Request, TranscribeParams};

use super::error::ipc_error_response;
use super::AppState;

#[cfg(feature = "local-cpu")]
fn validate(p: &TranscribeParams) -> Result<(), ErrorBody> {
    if p.model_id.trim().is_empty() {
        return Err(ErrorBody::new(
            ErrorCode::BadRequest,
            "model_id must not be empty",
        ));
    }
    if !p.audio_path.is_absolute() {
        return Err(ErrorBody::new(
            ErrorCode::BadRequest,
            format!("audio_path must be absolute: {}", p.audio_path.display()),
        ));
    }
    match p.task.as_str() {
        "transcribe" | "translate" => Ok(()),
        other => Err(ErrorBody::new(
            ErrorCode::BadRequest,
            format!("task must be 'transcribe' or 'translate', got '{other}'"),
        )),
    }
}

#[cfg(feature = "local-cpu")]
pub async fn transcribe(
    State(state): State<AppState>,
    Json(req): Json<Request<TranscribeParams>>,
) -> AxumResponse {
    if let Err(e) = validate(&req.params) {
        return ipc_error_response(e);
    }
    match state.whisper.transcribe(req.params).await {
        Ok(data) => Json(Response::success(req.request_id, data)).into_response(),
        Err(e) => ipc_error_response(e),
    }
}

#[cfg(not(feature = "local-cpu"))]
pub async fn transcribe(
    State(_state): State<AppState>,
    Json(_req): Json<Request<TranscribeParams>>,
) -> AxumResponse {
    ipc_error_response(ErrorBody::new(
        ErrorCode::UnsupportedOperation,
        "/stt/transcribe requires the local-cpu feature",
    ))
}

// `validate` is gated by `feature = "local-cpu"`. Same for the tests.
#[cfg(all(test, feature = "local-cpu"))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn base() -> TranscribeParams {
        TranscribeParams {
            model_id: "whisper-base".into(),
            audio_path: PathBuf::from("/tmp/audio.wav"),
            language: None,
            task: "transcribe".into(),
        }
    }

    #[test]
    fn rejects_relative_audio_path() {
        // GIVEN a transcription request whose audio path is relative
        let mut p = base();
        p.audio_path = PathBuf::from("audio.wav");
        // WHEN the request is validated
        let err = validate(&p).expect_err("must reject relative path");
        // THEN it is refused as a bad request, the runner resolving no working directory
        assert_eq!(err.code, ErrorCode::BadRequest);
    }

    #[test]
    fn rejects_invalid_task() {
        // GIVEN a transcription request naming a task the backend does not offer
        let mut p = base();
        p.task = "summarize".into();
        // WHEN the request is validated
        let err = validate(&p).expect_err("must reject invalid task");
        // THEN it is refused as a bad request
        assert_eq!(err.code, ErrorCode::BadRequest);
    }

    #[test]
    fn accepts_translate() {
        // GIVEN a transcription request asking for the translate task
        let mut p = base();
        p.task = "translate".into();
        // WHEN the request is validated
        // THEN it is accepted: translate is one of the two supported tasks
        assert!(validate(&p).is_ok());
    }
}
