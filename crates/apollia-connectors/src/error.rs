//! Error types for `apollia-connectors`.

use thiserror::Error;

/// Errors that can occur during a connector operation.
///
/// Connector implementations should map upstream API errors onto these
/// variants. Apollia tools use the variant to decide whether to surface the
/// failure to the user (e.g. `NotConnected` becomes "Run `apollia auth
/// connect google` to connect Gmail") or retry transparently (`RateLimited`,
/// `Network`).
#[derive(Debug, Error)]
pub enum ConnectorError {
    /// No OAuth token is stored for the requested `(provider, account_id)`.
    ///
    /// The user must complete the OAuth flow before this operation can succeed.
    #[error("connector {provider} not connected for account {account}")]
    NotConnected {
        /// Provider identifier (e.g. `"google"`).
        provider: &'static str,
        /// Account identifier (e.g. the connected email address).
        account: String,
    },

    /// The required OAuth scope is missing from the stored token.
    ///
    /// The user must re-connect with the additional scope.
    #[error("scope missing for {operation}: requires {required}")]
    ScopeMissing {
        /// Operation identifier (e.g. `"gmail.send"`).
        operation: &'static str,
        /// The scope group the user must grant (catalog identifier).
        required: &'static str,
    },

    /// The upstream returned 401 even after a refresh attempt.
    ///
    /// Typically indicates the user revoked Apollia's access from the SaaS
    /// admin console — the stored token is no longer valid and must be
    /// re-acquired through the OAuth flow.
    #[error("unauthorized: token rejected by {provider}")]
    Unauthorized {
        /// Provider identifier.
        provider: &'static str,
    },

    /// The upstream rate-limited the request and retries have been exhausted.
    ///
    /// Tool callers may surface a friendly message to the user; the agent
    /// loop should back off rather than retry immediately.
    #[error("rate limited by {provider} after {retries} retries")]
    RateLimited {
        /// Provider identifier.
        provider: &'static str,
        /// Number of retries already attempted.
        retries: u32,
    },

    /// A network-level error (DNS, TCP, TLS, timeout).
    #[error("network: {0}")]
    Network(String),

    /// The upstream response body could not be parsed.
    #[error("decoding response: {0}")]
    Decoding(String),

    /// The upstream returned a non-2xx status and the operation cannot proceed.
    #[error("upstream {status} from {provider}: {body}")]
    Upstream {
        /// Provider identifier.
        provider: &'static str,
        /// HTTP status code.
        status: u16,
        /// Body excerpt (truncated to ~512 chars).
        body: String,
    },

    /// The operation argument or runtime context is invalid.
    ///
    /// Distinct from upstream errors — this captures programmer or caller
    /// mistakes that should never be retried.
    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    /// The active sovereignty profile forbids cloud connector calls.
    ///
    /// Surfaced when the user has selected `local_only` mode but a tool still
    /// attempts to reach a SaaS endpoint. The runtime should never let this
    /// fire in practice, but it is the safety net of last resort.
    #[error("sovereignty profile {profile} blocks cloud connectors")]
    SovereigntyBlocked {
        /// Profile identifier (`"local_only"`).
        profile: &'static str,
    },

    /// An auth crate error bubbled up unchanged (token refresh failed, etc.).
    #[error("auth: {0}")]
    Auth(#[from] apollia_auth::AuthError),
}
