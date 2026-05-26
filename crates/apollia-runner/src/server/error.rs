//! Conversion des erreurs domaine vers ErrorCode + statut HTTP.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::ipc::{ErrorBody, ErrorCode, Response as IpcResponse};

/// Helper qui sérialise une erreur IPC en réponse HTTP.
///
/// Le statut HTTP est dérivé du `ErrorCode` :
///
/// | ErrorCode | HTTP |
/// |---|---|
/// | BadRequest | 400 |
/// | ModelNotLoaded | 404 |
/// | UnsupportedOperation | 501 |
/// | ModelLoadFailed, BackendOom, InferenceFailed, Internal | 500 |
pub fn ipc_error_response(error: ErrorBody) -> Response {
    let status = match error.code {
        ErrorCode::BadRequest => StatusCode::BAD_REQUEST,
        ErrorCode::ModelNotLoaded => StatusCode::NOT_FOUND,
        ErrorCode::UnsupportedOperation => StatusCode::NOT_IMPLEMENTED,
        ErrorCode::ModelLoadFailed
        | ErrorCode::BackendOom
        | ErrorCode::InferenceFailed
        | ErrorCode::Internal => StatusCode::INTERNAL_SERVER_ERROR,
    };

    let body: IpcResponse<()> = IpcResponse::error(None, error);
    (status, Json(body)).into_response()
}

/// Convertit une erreur générique en `ErrorBody` `Internal`.
pub fn internal_error(message: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorCode::Internal, message)
}

/// Convertit une violation de validation en `ErrorBody` `BadRequest`.
pub fn bad_request(message: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorCode::BadRequest, message)
}
