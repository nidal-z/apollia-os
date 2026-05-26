//! Envelope de request IPC : `{request_id, params}`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Envelope générique pour toute request HTTP du daemon vers le runner.
///
/// Le daemon génère un `request_id` UUID v4 que le runner replique dans sa
/// réponse, permettant la corrélation côté logs structurés.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request<P> {
    /// Identifiant unique de la requête, généré par le daemon.
    pub request_id: Uuid,
    /// Paramètres typés spécifiques à l'endpoint.
    pub params: P,
}

impl<P> Request<P> {
    pub fn new(params: P) -> Self {
        Self {
            request_id: Uuid::new_v4(),
            params,
        }
    }
}
