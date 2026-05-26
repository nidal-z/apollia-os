//! Envelope de response IPC : `{ok, request_id, data | error}`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::error::ErrorBody;

/// Envelope générique de toute réponse HTTP du runner vers le daemon.
///
/// Sérialisé en JSON avec un champ `ok` discriminant pour permettre au daemon
/// de router vers `data` (success) ou `error` (failure) sans deviner.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Response<D> {
    /// Réponse de succès : `{ok: true, request_id, data}`.
    Ok {
        ok: bool,
        request_id: Option<Uuid>,
        data: D,
    },
    /// Réponse d'erreur : `{ok: false, request_id, error}`.
    Err {
        ok: bool,
        request_id: Option<Uuid>,
        error: ErrorBody,
    },
}

impl<D: Serialize> Response<D> {
    /// Construit une réponse de succès.
    pub fn success(request_id: Uuid, data: D) -> Self {
        Self::Ok {
            ok: true,
            request_id: Some(request_id),
            data,
        }
    }

    /// Construit une réponse de succès sans request_id (pour `/handshake`).
    pub fn success_no_id(data: D) -> Self {
        Self::Ok {
            ok: true,
            request_id: None,
            data,
        }
    }

    /// Construit une réponse d'erreur.
    pub fn error(request_id: Option<Uuid>, error: ErrorBody) -> Self {
        Self::Err {
            ok: false,
            request_id,
            error,
        }
    }
}
