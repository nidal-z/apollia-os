//! Codes d'erreur normalisés et body d'erreur sérialisable.

use serde::{Deserialize, Serialize};

/// Codes d'erreur normalisés.
///
/// La liste est extensible MINEUREMENT (nouveau variant) sans casser le
/// protocole. Le retrait d'un variant = changement MAJEUR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// LLM call avant `/llm/load_model`.
    ModelNotLoaded,
    /// Chargement GGUF impossible (chemin invalide, fichier corrompu, etc.).
    ModelLoadFailed,
    /// GPU memory dépassée pendant le chargement ou l'inférence.
    BackendOom,
    /// Inférence échouée (kernel crash, runtime error).
    InferenceFailed,
    /// JSON malformé ou params invalides.
    BadRequest,
    /// Endpoint pas implémenté par ce backend (ex: STT sur vulkan).
    UnsupportedOperation,
    /// Bug runner interne, ne devrait pas arriver.
    Internal,
}

/// Body sérialisable d'une erreur IPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    /// Code d'erreur normalisé.
    pub code: ErrorCode,
    /// Message lisible par humain, lowercase, pas de point final.
    pub message: String,
    /// Détails contextuels optionnels (ex: chemin, valeur reçue, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ErrorBody {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}
