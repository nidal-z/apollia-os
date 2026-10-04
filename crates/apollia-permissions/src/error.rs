//! Errors of the permission-rule crate.

/// Error raised by the permission-rule store, the consent store and the
/// audit-log reader.
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

    /// The "execute code" family never takes a standing consent: every
    /// execution asks.
    #[error("the execute_code family cannot be granted for always (agent '{agent_id}')")]
    CodeFamilyRefused {
        /// Agent the consent was requested for.
        agent_id: String,
    },

    /// Invalid standing consent (empty agent, unknown family or scope).
    #[error("invalid consent: {0}")]
    InvalidConsent(String),

    /// A governance event could not be written to the journal outbox.
    #[error("failed to serialize a journal event: {0}")]
    Event(#[source] serde_json::Error),
}
