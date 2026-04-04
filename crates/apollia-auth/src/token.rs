//! OAuth2 token types and exchange / refresh operations.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::{error::AuthError, pkce::OAuth2PkceFlow, providers::ProviderConfig};

// ─── Token ────────────────────────────────────────────────────────────────────

/// An OAuth2 token stored in the OS keyring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredToken {
    /// The bearer access token used in API requests.
    pub access_token: String,
    /// An optional refresh token for obtaining new access tokens without re-login.
    pub refresh_token: Option<String>,
    /// Expiry instant of the access token, if known.
    pub expires_at: Option<DateTime<Utc>>,
    /// Scopes that were granted by the authorization server.
    pub scopes: Vec<String>,
}

impl StoredToken {
    /// Return `true` if the access token is expired.
    ///
    /// A token without an `expires_at` is treated as permanently valid.
    pub fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|exp| exp <= Utc::now())
    }
}

// ─── Internal response shape ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    scope: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

impl TokenResponse {
    fn into_stored(self, previous_refresh: Option<&str>) -> Result<StoredToken, AuthError> {
        if let Some(err) = self.error {
            let desc = self.error_description.unwrap_or_default();
            return Err(AuthError::TokenExchangeFailed(format!("{err}: {desc}")));
        }

        let expires_at = self
            .expires_in
            .map(|secs| Utc::now() + Duration::seconds(secs as i64));

        let scopes: Vec<String> = self
            .scope
            .map(|s| s.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_default();

        // Preserve the existing refresh token if the server does not rotate it.
        let refresh_token = self
            .refresh_token
            .or_else(|| previous_refresh.map(str::to_owned));

        Ok(StoredToken {
            access_token: self.access_token,
            refresh_token,
            expires_at,
            scopes,
        })
    }
}

// ─── Exchange ─────────────────────────────────────────────────────────────────

/// Exchange an authorization code for a [`StoredToken`] using PKCE.
pub async fn exchange_code(
    provider: &ProviderConfig,
    flow: &OAuth2PkceFlow,
    code: &str,
) -> Result<StoredToken, AuthError> {
    let client = reqwest::Client::new();
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("code_verifier", flow.code_verifier.as_str()),
        ("redirect_uri", flow.redirect_uri.as_str()),
        ("client_id", provider.client_id.as_str()),
    ];

    let response = client
        .post(provider.token_url)
        .form(&params)
        .send()
        .await
        .map_err(|e| AuthError::HttpError(e.to_string()))?;

    let token_resp: TokenResponse = response
        .json()
        .await
        .map_err(|e| AuthError::TokenExchangeFailed(e.to_string()))?;

    token_resp.into_stored(None)
}

// ─── Refresh ──────────────────────────────────────────────────────────────────

/// Obtain a new access token using the refresh token stored in `stored`.
///
/// Returns [`AuthError::NoRefreshToken`] if `stored` has no refresh token.
pub async fn refresh_token(
    provider: &ProviderConfig,
    stored: &StoredToken,
) -> Result<StoredToken, AuthError> {
    let refresh = stored
        .refresh_token
        .as_deref()
        .ok_or(AuthError::NoRefreshToken)?;

    let client = reqwest::Client::new();
    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh),
        ("client_id", provider.client_id.as_str()),
    ];

    let response = client
        .post(provider.token_url)
        .form(&params)
        .send()
        .await
        .map_err(|e| AuthError::HttpError(e.to_string()))?;

    let token_resp: TokenResponse = response
        .json()
        .await
        .map_err(|e| AuthError::TokenExchangeFailed(e.to_string()))?;

    token_resp.into_stored(stored.refresh_token.as_deref())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stored_token_expired_when_past() {
        // GIVEN a token whose expiry is 10 minutes in the past
        let token = StoredToken {
            access_token: "tok".into(),
            refresh_token: None,
            expires_at: Some(Utc::now() - Duration::minutes(10)),
            scopes: vec![],
        };
        // WHEN
        // THEN is_expired returns true
        assert!(token.is_expired());
    }

    #[test]
    fn test_stored_token_valid_when_future() {
        // GIVEN a token whose expiry is 1 hour in the future
        let token = StoredToken {
            access_token: "tok".into(),
            refresh_token: None,
            expires_at: Some(Utc::now() + Duration::hours(1)),
            scopes: vec![],
        };
        // WHEN
        // THEN is_expired returns false
        assert!(!token.is_expired());
    }

    #[test]
    fn test_stored_token_no_expiry_is_not_expired() {
        // GIVEN a token with no expiry
        let token = StoredToken {
            access_token: "tok".into(),
            refresh_token: None,
            expires_at: None,
            scopes: vec![],
        };
        // THEN treated as valid
        assert!(!token.is_expired());
    }
}
