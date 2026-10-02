//! Errors of the mailbox connection.

use apollia_auth::AuthError;
use thiserror::Error;

/// Why a mailbox connection could not be described, verified or stored.
///
/// The verification failures (unreachable host, refused credentials, TLS) are
/// distinct variants because each one tells the operator something different
/// to fix: the address, the password, or the server. [`ImapError::code`]
/// gives each a stable identifier for the HTTP and command-line surfaces.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ImapError {
    /// The address has no `@`, or nothing on one side of it.
    #[error("`{email}` is not an email address")]
    InvalidEmail {
        /// The address as given.
        email: String,
    },

    /// No host was given and the address's domain is not a known provider.
    #[error("no IMAP host is known for `{domain}`; give the server address")]
    UnknownProvider {
        /// Domain of the address.
        domain: String,
    },

    /// The host is empty or carries whitespace or a control character.
    #[error("`{host}` is not a server address")]
    InvalidHost {
        /// The host as given.
        host: String,
    },

    /// Port 0 cannot be connected to.
    #[error("port 0 is not a server port")]
    InvalidPort,

    /// The server could not be reached: name resolution, refused or timed out
    /// connection.
    #[error("{host}:{port} cannot be reached: {detail}")]
    Unreachable {
        /// Server address.
        host: String,
        /// Server port.
        port: u16,
        /// What the network layer answered.
        detail: String,
    },

    /// The TLS handshake failed: untrusted certificate, wrong name, or a
    /// server that does not speak TLS on this port.
    #[error("the secure connection to {host} failed: {detail}")]
    Tls {
        /// Server address.
        host: String,
        /// What the TLS layer answered.
        detail: String,
    },

    /// The server refused the identifier and password.
    #[error("the server refused the identifier or the password")]
    CredentialsRejected,

    /// The server answered something the protocol does not allow.
    #[error("the server answered outside the protocol: {detail}")]
    Protocol {
        /// What was unexpected.
        detail: String,
    },

    /// The server refused to open a folder: it does not exist, or it only
    /// holds other folders.
    #[error("the folder `{folder}` cannot be opened: {detail}")]
    FolderRefused {
        /// Folder asked for.
        folder: String,
        /// What the server answered.
        detail: String,
    },

    /// A search asked for something IMAP cannot express.
    #[error("the search cannot be sent: {detail}")]
    InvalidQuery {
        /// What is wrong with it.
        detail: String,
    },

    /// The message asked for is not in the folder.
    #[error("no message {uid} in `{folder}`")]
    MessageNotFound {
        /// Folder searched.
        folder: String,
        /// Identifier asked for.
        uid: u32,
    },

    /// No mailbox connection is stored for this address.
    #[error("no mailbox connection for `{email}`")]
    NotFound {
        /// The address asked for.
        email: String,
    },

    /// The secret store refused to read or write the password.
    #[error("the password could not be kept in the secret store")]
    Secret(#[source] AuthError),

    /// The connection list on disk could not be read or written.
    #[error("the mailbox connection list could not be {action}: {detail}")]
    Index {
        /// `read` or `written`.
        action: &'static str,
        /// What failed.
        detail: String,
    },
}

impl ImapError {
    /// Stable identifier of the failure, for the HTTP and command-line
    /// surfaces.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidEmail { .. } => "invalid_email",
            Self::UnknownProvider { .. } => "unknown_provider",
            Self::InvalidHost { .. } => "invalid_host",
            Self::InvalidPort => "invalid_port",
            Self::Unreachable { .. } => "unreachable",
            Self::Tls { .. } => "tls",
            Self::CredentialsRejected => "credentials_rejected",
            Self::Protocol { .. } => "protocol",
            Self::FolderRefused { .. } => "folder_refused",
            Self::InvalidQuery { .. } => "invalid_query",
            Self::MessageNotFound { .. } => "message_not_found",
            Self::NotFound { .. } => "not_found",
            Self::Secret(_) => "secret_store",
            Self::Index { .. } => "index",
        }
    }
}
