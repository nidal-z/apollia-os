//! Error types for the append-only hash-chained audit journal.

use thiserror::Error;

/// Errors raised while opening, writing, or querying the audit journal.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AuditJournalError {
    /// Failed to open the SQLite file backing the journal.
    #[error("failed to open audit journal database: {0}")]
    Open(String),
    /// Failed to create the schema (table, index, or triggers).
    #[error("failed to initialize audit journal schema: {0}")]
    SchemaInit(String),
    /// A SQLite operation failed at runtime (insert or query).
    #[error("audit journal sqlite error: {0}")]
    Sqlite(String),
    /// The background actor is no longer reachable (channel closed).
    #[error("audit journal actor unavailable")]
    ActorUnavailable,
    /// The signer could not be built and the policy is fail-hard.
    #[error("audit journal signer unavailable: {0}")]
    SignerUnavailable(String),
    /// Failed to serialize an entry payload to its canonical form.
    #[error("failed to serialize audit journal payload: {0}")]
    Serialize(String),
}
