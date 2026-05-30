//! Permission engine errors.

/// Three-layer permission engine error.
#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    /// SQLite error during a rule database or audit log operation.
    #[error("SQLite error: {0}")]
    Database(#[from] rusqlite::Error),

    /// Explicit denial decision (AutoDenied*).
    #[error("permission denied: {reason}")]
    Denied {
        /// Human-readable reason for the denial.
        reason: String,
    },

    /// Invalid rule format (e.g. empty prefix, empty tool name).
    #[error("invalid rule format: {0}")]
    InvalidRule(String),
}
