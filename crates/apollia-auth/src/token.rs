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

/// Token endpoint response (RFC 6749 §5.1 success + §5.2 error). All fields
/// are optional so the same struct deserialises both success and error
/// payloads. Google / Microsoft return `{"error":"…","error_description":"…"}`
/// with HTTP 400 when something goes wrong, which previously failed to parse
/// because `access_token` was required and absent. `into_stored` then enforces
/// the success-vs-error distinction with a clear, surfaced message.
#[derive(Debug, Deserialize)]
struct TokenResponse {
    #[serde(default)]
    access_token: Option<String>,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    scope: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
    error_uri: Option<String>,
}

impl TokenResponse {
    fn into_stored(self, previous_refresh: Option<&str>) -> Result<StoredToken, AuthError> {
        if let Some(err) = self.error {
            let desc = self.error_description.unwrap_or_default();
            let uri = self
                .error_uri
                .map(|u| format!(" - see {u}"))
                .unwrap_or_default();
            return Err(AuthError::TokenExchangeFailed(format!(
                "{err}: {desc}{uri}"
            )));
        }

        let access_token = self.access_token.ok_or_else(|| {
            AuthError::TokenExchangeFailed(
                "token endpoint returned neither `access_token` nor `error` - \
                 unrecognised provider response"
                    .into(),
            )
        })?;

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
            access_token,
            refresh_token,
            expires_at,
            scopes,
        })
    }
}

/// Parse a `TokenResponse` from `response`, surfacing the raw body when
/// deserialisation fails so misconfigured providers (wrong client type, HTML
/// error pages from reverse proxies, etc.) are diagnosable. Without this, the
/// caller used to see only the generic `error decoding response body` from
/// `reqwest`, hiding the actual error.
async fn parse_token_response(response: reqwest::Response) -> Result<TokenResponse, AuthError> {
    let status = response.status();
    let body = apollia_core::net::read_capped_text(response, apollia_core::net::MAX_METADATA_BYTES)
        .await
        .map_err(|e| AuthError::HttpError(e.to_string()))?;

    serde_json::from_str::<TokenResponse>(&body).map_err(|e| {
        let preview = body.chars().take(500).collect::<String>();
        AuthError::TokenExchangeFailed(format!(
            "HTTP {status}: failed to parse token endpoint response ({e}). \
             Body preview: {preview}"
        ))
    })
}

// ─── Exchange ─────────────────────────────────────────────────────────────────

/// Exchange an authorization code for a [`StoredToken`] using PKCE.
pub async fn exchange_code(
    provider: &ProviderConfig,
    flow: &OAuth2PkceFlow,
    code: &str,
) -> Result<StoredToken, AuthError> {
    let client =
        apollia_core::net::safe_client().map_err(|e| AuthError::HttpError(e.to_string()))?;
    let mut params: Vec<(&str, &str)> = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("code_verifier", flow.code_verifier.as_str()),
        ("redirect_uri", flow.redirect_uri.as_str()),
        ("client_id", provider.client_id.as_str()),
    ];
    // Append the client_secret when the provider config carries one. Google's
    // Installed App type requires it at the token endpoint (non-standard);
    // Microsoft and other spec-compliant public clients leave this `None`.
    if let Some(secret) = provider.client_secret.as_deref() {
        params.push(("client_secret", secret));
    }

    let response = client
        .post(provider.token_url)
        .form(&params)
        .send()
        .await
        .map_err(|e| AuthError::HttpError(e.to_string()))?;

    let token_resp = parse_token_response(response).await?;

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

    let client =
        apollia_core::net::safe_client().map_err(|e| AuthError::HttpError(e.to_string()))?;
    let mut params: Vec<(&str, &str)> = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh),
        ("client_id", provider.client_id.as_str()),
    ];
    if let Some(secret) = provider.client_secret.as_deref() {
        params.push(("client_secret", secret));
    }

    let response = client
        .post(provider.token_url)
        .form(&params)
        .send()
        .await
        .map_err(|e| AuthError::HttpError(e.to_string()))?;

    let token_resp = parse_token_response(response).await?;

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
    fn test_token_response_surfaces_provider_error() {
        // GIVEN a Google-style error payload (no access_token, error + description set)
        let body = r#"{"error":"invalid_grant","error_description":"Bad Request","error_uri":"https://tools.ietf.org/html/rfc6749#section-5.2"}"#;
        let parsed: TokenResponse = serde_json::from_str(body).expect("parse error payload");

        // WHEN we convert to StoredToken
        let result = parsed.into_stored(None);

        // THEN we get a TokenExchangeFailed with the provider's exact message
        match result {
            Err(AuthError::TokenExchangeFailed(msg)) => {
                assert!(msg.contains("invalid_grant"), "got: {msg}");
                assert!(msg.contains("Bad Request"), "got: {msg}");
            }
            other => panic!("expected TokenExchangeFailed, got {other:?}"),
        }
    }

    #[test]
    fn test_token_response_rejects_missing_access_token_and_error() {
        // GIVEN a malformed payload (neither access_token nor error)
        let body = r#"{"some_unknown_field":"oops"}"#;
        let parsed: TokenResponse = serde_json::from_str(body).expect("parse blob");

        // WHEN into_stored is called
        let result = parsed.into_stored(None);

        // THEN we surface a clear failure rather than panicking on the missing field
        assert!(matches!(result, Err(AuthError::TokenExchangeFailed(_))));
    }

    #[test]
    fn test_token_response_success_path_still_works() {
        // GIVEN a valid Google success payload
        let body = r#"{"access_token":"ya29.xyz","expires_in":3599,"refresh_token":"1//rt","scope":"https://www.googleapis.com/auth/gmail.send","token_type":"Bearer"}"#;
        // WHEN it is parsed and turned into a stored token
        let parsed: TokenResponse = serde_json::from_str(body).expect("parse success");

        let stored = parsed.into_stored(None).expect("into_stored");
        // THEN the access token, the refresh token, the scopes and the expiry all survive
        assert_eq!(stored.access_token, "ya29.xyz");
        assert_eq!(stored.refresh_token.as_deref(), Some("1//rt"));
        assert_eq!(
            stored.scopes,
            vec!["https://www.googleapis.com/auth/gmail.send"]
        );
        assert!(stored.expires_at.is_some());
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
        // WHEN expiry is evaluated
        // THEN treated as valid
        assert!(!token.is_expired());
    }
}
