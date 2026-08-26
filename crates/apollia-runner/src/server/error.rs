//! Maps domain errors to an ErrorCode plus an HTTP status.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::ipc::{ErrorBody, ErrorCode, Response as IpcResponse};

/// Serializes an IPC error into an HTTP response.
///
/// The HTTP status is derived from the `ErrorCode`:
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

/// Converts a validation failure into a `BadRequest` `ErrorBody`.
pub fn bad_request(message: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorCode::BadRequest, message)
}
