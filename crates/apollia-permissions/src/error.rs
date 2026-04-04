//! Erreurs du moteur de permissions.

/// Erreur du moteur de permissions 3 couches.
#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    /// Erreur SQLite lors d'une opération sur la base de règles ou l'audit log.
    #[error("SQLite error: {0}")]
    Database(#[from] rusqlite::Error),

    /// Décision de refus explicite (AutoDenied*).
    #[error("permission denied: {reason}")]
    Denied {
        /// Raison lisible du refus.
        reason: String,
    },

    /// Format de règle invalide (ex : préfixe vide, nom d'outil vide).
    #[error("invalid rule format: {0}")]
    InvalidRule(String),
}
