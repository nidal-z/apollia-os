//! Errors of the permission-rule crate.

/// Error raised by the permission-rule store and its audit-log reader.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PermissionError {
    /// SQLite error during a rule database or audit log operation.
    #[error("SQLite error: {0}")]
    Database(#[from] rusqlite::Error),

    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] apollia_core::schema::SchemaError),

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
