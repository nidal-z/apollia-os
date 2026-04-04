//! Error types for `apollia-auth`.

use thiserror::Error;

/// Errors that can occur during the OAuth2 PKCE authentication flow.
#[derive(Debug, Error)]
pub enum AuthError {
    /// The `state` parameter in the callback does not match the expected value.
    ///
    /// This indicates a possible CSRF attack and the flow must be aborted.
    #[error("state mismatch — possible CSRF")]
    StateMismatch,

    /// The callback URL did not contain an authorization code.
    #[error("code manquant dans le callback")]
    MissingCode,

    /// The authorization server returned an error in the callback or token response.
    #[error("provider error: {0}")]
    ProviderError(String),

    /// The token exchange endpoint returned an error or an unexpected response.
    #[error("token exchange failed: {0}")]
    TokenExchangeFailed(String),

    /// An HTTP transport error occurred.
    #[error("erreur HTTP: {0}")]
    HttpError(String),

    /// The local callback HTTP server encountered an error.
    #[error("serveur callback: {0}")]
    CallbackServer(String),

    /// An OS keyring operation failed.
    #[error("keyring: {0}")]
    Keyring(String),

    /// JSON serialization or deserialization failed.
    #[error("sérialisation: {0}")]
    Serialization(String),

    /// A refresh was requested but no refresh token is stored.
    #[error("pas de refresh token disponible")]
    NoRefreshToken,

    /// The requested provider name is not supported.
    #[error("provider inconnu: {0}")]
    UnknownProvider(String),
}
