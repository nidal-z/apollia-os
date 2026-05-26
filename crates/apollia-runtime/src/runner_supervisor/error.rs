//! Types d'erreur pour le supervisor et le proxy runner.

use thiserror::Error;

/// Erreurs possibles lors du spawn, health check ou exécution d'un runner.
#[derive(Debug, Error)]
pub enum RunnerError {
    /// Le binaire runner attendu n'a pas été trouvé sur le disque.
    #[error("runner binary not found: {0}")]
    BinaryNotFound(String),

    /// Erreur I/O au spawn ou pendant la communication.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Le runner n'a pas émis `READY <port>\n` dans le délai imparti (10 sec).
    #[error("runner handshake timeout after {timeout_secs}s")]
    HandshakeTimeout { timeout_secs: u64 },

    /// La ligne `READY <port>` est malformée (port absent, non numérique, etc.).
    #[error("malformed READY line: {0}")]
    MalformedReady(String),

    /// La requête HTTP au runner a échoué (timeout, refus de connexion, etc.).
    #[error("http error: {0}")]
    Http(String),

    /// Le runner a renvoyé une erreur IPC normalisée.
    #[error("runner ipc error ({code:?}): {message}")]
    Ipc { code: String, message: String },

    /// Le runner s'est arrêté inopinément (crash ou exit non-zéro).
    #[error("runner crashed (exit code: {0:?})")]
    Crashed(Option<i32>),

    /// Erreur de sérialisation/désérialisation JSON.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Le supervisor est déjà en train de shutdown, refuse de nouveaux appels.
    #[error("supervisor shutting down")]
    ShuttingDown,
}
