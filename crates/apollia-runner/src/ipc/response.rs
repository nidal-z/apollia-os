//! IPC response envelope: `{ok, request_id, data | error}`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::error::ErrorBody;

/// Generic envelope for every HTTP response from the runner to the daemon.
///
/// Serialized to JSON with a discriminating `ok` field so the daemon can route
/// to `data` (success) or `error` (failure) without guessing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Response<D> {
    /// Success response: `{ok: true, request_id, data}`.
    Ok {
        ok: bool,
        request_id: Option<Uuid>,
        data: D,
    },
    /// Error response: `{ok: false, request_id, error}`.
    Err {
        ok: bool,
        request_id: Option<Uuid>,
        error: ErrorBody,
    },
}

impl<D: Serialize> Response<D> {
    /// Builds a success response.
    pub fn success(request_id: Uuid, data: D) -> Self {
        Self::Ok {
            ok: true,
            request_id: Some(request_id),
            data,
        }
    }

    /// Builds a success response without a request_id (for `/handshake`).
    pub fn success_no_id(data: D) -> Self {
        Self::Ok {
            ok: true,
            request_id: None,
            data,
        }
    }

    /// Builds an error response.
    pub fn error(request_id: Option<Uuid>, error: ErrorBody) -> Self {
        Self::Err {
            ok: false,
            request_id,
            error,
        }
    }
}
