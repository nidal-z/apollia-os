//! PKCE helpers: code verifier, code challenge, and flow state (RFC 7636).

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::providers::ProviderConfig;

// ─── Primitives ───────────────────────────────────────────────────────────────

/// Generate a cryptographically secure PKCE code verifier.
///
/// Returns a 43-character base64url string without padding derived from 32
/// random bytes, as specified in RFC 7636 section 4.1.
pub fn generate_code_verifier() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Derive the PKCE code challenge from a verifier.
///
/// Computes `BASE64URL(SHA256(ASCII(verifier)))` as specified in RFC 7636 section 4.2.
/// The result is a 43-character base64url string without padding.
pub fn generate_code_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

// ─── Flow ─────────────────────────────────────────────────────────────────────

/// All OAuth2 PKCE parameters required to execute a single authorization request.
#[derive(Debug, Clone)]
pub struct OAuth2PkceFlow {
    /// High-entropy random code verifier (kept secret until token exchange).
    pub code_verifier: String,
    /// SHA-256 challenge derived from `code_verifier`.
    pub code_challenge: String,
    /// Opaque random state used for CSRF protection.
    pub state: String,
    /// Local redirect URI including the ephemeral port (e.g. `http://127.0.0.1:PORT/callback`).
    pub redirect_uri: String,
}

impl OAuth2PkceFlow {
    /// Create a new PKCE flow for a callback server bound to `redirect_port`.
    pub fn new(redirect_port: u16) -> Self {
        let code_verifier = generate_code_verifier();
        let code_challenge = generate_code_challenge(&code_verifier);
        let state = generate_code_verifier();
        let redirect_uri = format!("http://127.0.0.1:{redirect_port}/callback");
        Self {
            code_verifier,
            code_challenge,
            state,
            redirect_uri,
        }
    }
}

// ─── URL builder ──────────────────────────────────────────────────────────────

/// Build the full OAuth2 authorization URL for `provider` and `flow`.
///
/// Includes `response_type=code`, `code_challenge_method=S256`, the PKCE
/// challenge, and the CSRF state parameter.
pub fn build_auth_url(provider: &ProviderConfig, flow: &OAuth2PkceFlow) -> String {
    let scopes = provider.scopes.join(" ");
    let mut url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
        provider.auth_url,
        urlencoding::encode(&provider.client_id),
        urlencoding::encode(&flow.redirect_uri),
        urlencoding::encode(&scopes),
        urlencoding::encode(&flow.state),
        urlencoding::encode(&flow.code_challenge),
    );
    // Google needs `access_type=offline` to return a refresh token, and
    // `prompt=consent` so that reconnecting an already-authorized account
    // actually re-grants newly requested scopes (without it, Google replays the
    // existing grant and silently omits the new scopes). `include_granted_scopes`
    // keeps previously granted scopes during this incremental re-consent.
    if provider.name == "google" {
        url.push_str("&access_type=offline&prompt=consent&include_granted_scopes=true");
    }
    url
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_verifier_length() {
        // GIVEN nothing
        // WHEN a code verifier is generated
        let verifier = generate_code_verifier();
        // THEN 43 chars base64url without padding
        assert_eq!(verifier.len(), 43);
        assert!(!verifier.contains('='));
        assert!(!verifier.contains('+'));
        assert!(!verifier.contains('/'));
    }

    #[test]
    fn build_auth_url_adds_google_consent_params() {
        // GIVEN a Google provider config and a flow
        let provider = ProviderConfig {
            name: "google",
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth",
            token_url: "https://oauth2.googleapis.com/token",
            client_id: "cid".to_string(),
            client_secret: Some("sec".to_string()),
            scopes: vec!["openid", "https://www.googleapis.com/auth/tasks"],
        };
        let flow = OAuth2PkceFlow::new(8080);

        // WHEN the auth URL is built
        let url = build_auth_url(&provider, &flow);

        // THEN it forces re-consent and offline access so reconnecting re-grants new scopes
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("prompt=consent"));
        assert!(url.contains("include_granted_scopes=true"));
    }

    #[test]
    fn build_auth_url_omits_google_params_for_microsoft() {
        // GIVEN a Microsoft provider config
        let provider = ProviderConfig {
            name: "microsoft",
            auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
            token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token",
            client_id: "cid".to_string(),
            client_secret: None,
            scopes: vec!["User.Read", "offline_access"],
        };
        let flow = OAuth2PkceFlow::new(8080);

        // WHEN the auth URL is built
        let url = build_auth_url(&provider, &flow);

        // THEN the Google-specific params are not appended
        assert!(!url.contains("access_type=offline"));
        assert!(!url.contains("prompt=consent"));
    }

    #[test]
    fn test_code_challenge_rfc7636_appendix_b() {
        // GIVEN the RFC 7636 Appendix B test vector
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        // WHEN
        let challenge = generate_code_challenge(verifier);
        // THEN
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn test_pkce_flow_unique_state_and_verifier() {
        // GIVEN two flows created on the same port
        let f1 = OAuth2PkceFlow::new(8080);
        let f2 = OAuth2PkceFlow::new(8080);
        // WHEN comparing their fields
        // THEN both state and code_verifier differ
        assert_ne!(f1.state, f2.state);
        assert_ne!(f1.code_verifier, f2.code_verifier);
    }
}
