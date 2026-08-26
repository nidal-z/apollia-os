//! MCP-server-scoped OAuth token persistence.
//!
//! Tokens issued by the MCP HTTP OAuth flow are stored in the existing
//! [`SecretStore`] backend under a dedicated namespace, parallel to the
//! `apollia-connector-<provider>` namespace used by [`AuthManager`] for
//! Google / Microsoft.
//!
//! Keying:
//! - `service = "apollia-mcp-oauth"`: stable, distinct from connector entries.
//! - `user = <server_name>`: the persisted MCP server name (one token per
//!   server in v0.1.0, multi-account deferred to a later release).
//!
//! Value: JSON-serialised [`StoredMcpToken`].
//!
//! The store does not own any concurrency primitives. Refresh single-flight
//! and rotation are the orchestrator's responsibility (Phase 2 + 3).

use serde::{Deserialize, Serialize};

use crate::error::AuthError;
use crate::secret_storage::SecretStore;

/// Keychain / age-file service slot reserved for MCP OAuth tokens.
///
/// Kept as a constant so callers cannot drift; tests pin its value.
pub const MCP_OAUTH_SERVICE: &str = "apollia-mcp-oauth";

/// Persisted state of a successful MCP OAuth handshake.
///
/// Carries every piece of information the orchestrator needs to:
/// 1. Inject `Authorization: Bearer <access_token>` into outgoing MCP requests.
/// 2. Refresh the token without re-discovering the Authorization Server.
/// 3. Re-negotiate from scratch on hard invalidation (revoked / scope mismatch).
///
/// `expires_at` is a Unix epoch in seconds. `None` means the token is presumed
/// non-expiring (rare; some self-hosted MCPs issue long-lived tokens).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredMcpToken {
    /// Bearer access token to inject in `Authorization` headers.
    pub access_token: String,
    /// Optional refresh token used to renew `access_token` without user
    /// interaction (OAuth 2.1 §4.2.4).
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Token type: always `"Bearer"` in MCP HTTP OAuth, kept for forward
    /// compatibility (e.g. future DPoP support).
    pub token_type: String,
    /// Unix epoch seconds at which `access_token` expires. `None` ⇒ presumed
    /// long-lived.
    #[serde(default)]
    pub expires_at: Option<i64>,
    /// Scopes effectively granted by the Authorization Server (which may be a
    /// subset of those requested).
    #[serde(default)]
    pub scope: Vec<String>,
    /// MCP server URL the token is bound to (RFC 8707 `resource=` parameter).
    /// Verified by the AS at issuance; never accept a token whose
    /// `resource_uri` no longer matches the configured server URL.
    pub resource_uri: String,
    /// Authorization Server issuer URL, cached so the orchestrator can refresh
    /// without re-running discovery on the hot path.
    pub as_url: String,
    /// `client_id` Apollia used at this AS. Either the CIMD URL when the AS
    /// supports CIMD, or the dynamically-registered identifier (RFC 7591).
    pub client_id: String,
    /// `sub` claim extracted from the access token when it is a parseable JWT.
    /// Best-effort, surfaced to the UI for the "connected as …" label.
    #[serde(default)]
    pub identity_sub: Option<String>,
    /// `email` claim, idem.
    #[serde(default)]
    pub identity_email: Option<String>,
    /// Unix epoch seconds of token issuance (i.e. when this record was created
    /// or last refreshed). Used for telemetry / staleness detection.
    pub created_at: i64,
}

impl StoredMcpToken {
    /// True when `expires_at` is set and already in the past relative to
    /// `now_epoch_secs`.
    pub fn is_expired(&self, now_epoch_secs: i64) -> bool {
        match self.expires_at {
            Some(exp) => now_epoch_secs >= exp,
            None => false,
        }
    }

    /// True when the token will expire within `leeway_secs` from
    /// `now_epoch_secs`. Used by the refresh path to renew proactively before
    /// an in-flight request observes a 401.
    pub fn needs_refresh(&self, now_epoch_secs: i64, leeway_secs: i64) -> bool {
        match self.expires_at {
            Some(exp) => now_epoch_secs + leeway_secs >= exp,
            None => false,
        }
    }
}

// ─── Persistence helpers ─────────────────────────────────────────────────────

/// Persist `token` under `(MCP_OAUTH_SERVICE, server_name)`.
///
/// Overwrites any prior token for the same server; the OAuth flow is
/// idempotent, callers should not need to delete-then-save.
pub fn save_mcp_token(
    store: &dyn SecretStore,
    server_name: &str,
    token: &StoredMcpToken,
) -> Result<(), AuthError> {
    validate_server_name(server_name)?;
    let payload =
        serde_json::to_string(token).map_err(|e| AuthError::Serialization(e.to_string()))?;
    store.set(MCP_OAUTH_SERVICE, server_name, &payload)
}

/// Load the persisted token for `server_name`, if any.
///
/// Returns `Ok(None)` when no token has been stored yet; distinguishes
/// "never connected" from "connection failed".
pub fn load_mcp_token(
    store: &dyn SecretStore,
    server_name: &str,
) -> Result<Option<StoredMcpToken>, AuthError> {
    validate_server_name(server_name)?;
    let Some(payload) = store.get(MCP_OAUTH_SERVICE, server_name)? else {
        return Ok(None);
    };
    let token: StoredMcpToken =
        serde_json::from_str(&payload).map_err(|e| AuthError::Serialization(e.to_string()))?;
    Ok(Some(token))
}

/// Remove the persisted token for `server_name`. Idempotent: never errors on
/// missing entries.
pub fn delete_mcp_token(store: &dyn SecretStore, server_name: &str) -> Result<(), AuthError> {
    validate_server_name(server_name)?;
    store.delete(MCP_OAUTH_SERVICE, server_name)
}

// ─── JWT introspection (best-effort, no signature verification) ─────────────

/// Identity claims extracted from a JWT access token. Used only for UI
/// display ("connected as …"); never as an authorisation signal
/// (the AS has already done its job at issuance).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JwtIdentityClaims {
    /// `sub` claim: stable subject identifier.
    pub sub: Option<String>,
    /// `email` claim: commonly present in OIDC ID tokens, sometimes mirrored
    /// onto access tokens.
    pub email: Option<String>,
}

/// Best-effort `sub` / `email` extraction from a JWT access token.
///
/// Returns `JwtIdentityClaims::default()` for opaque tokens (non-JWT shape) or
/// malformed payloads; we do not propagate parse errors because token
/// validity is owned by the AS, not by Apollia.
pub fn extract_identity_claims(access_token: &str) -> JwtIdentityClaims {
    // JWTs are `header.payload.signature`, base64url-encoded each part.
    let parts: Vec<&str> = access_token.split('.').collect();
    if parts.len() != 3 {
        return JwtIdentityClaims::default();
    }
    let Ok(payload_bytes) = base64_url_decode(parts[1]) else {
        return JwtIdentityClaims::default();
    };
    let Ok(payload_str) = std::str::from_utf8(&payload_bytes) else {
        return JwtIdentityClaims::default();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(payload_str) else {
        return JwtIdentityClaims::default();
    };
    JwtIdentityClaims {
        sub: value
            .get("sub")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        email: value
            .get("email")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    }
}

// ─── Internals ──────────────────────────────────────────────────────────────

/// Validate `server_name` against the same charset accepted by
/// `McpServerConfig::validate` (`[a-z0-9_-]+`). Catches typos / accidental
/// path traversal in callers; the SecretStore backends sanitise too, but
/// fail-fast here gives a clearer error.
fn validate_server_name(server_name: &str) -> Result<(), AuthError> {
    if server_name.is_empty() {
        return Err(AuthError::Serialization(
            "mcp server_name must not be empty".into(),
        ));
    }
    if !server_name
        .chars()
        .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_' | '-'))
    {
        return Err(AuthError::Serialization(format!(
            "mcp server_name '{server_name}' contains invalid characters (allowed: a-z, 0-9, _, -)"
        )));
    }
    Ok(())
}

/// Minimal base64url decoder (no padding, URL-safe alphabet); JWTs use this
/// flavour per RFC 7515 §2.
fn base64_url_decode(input: &str) -> Result<Vec<u8>, &'static str> {
    // Reconstitute standard-base64 padding so we can use the `base64` crate
    // re-export from a transitive dep. Apollia-auth already pulls in `base64`
    // via reqwest's TLS stack, but to stay decoupled we implement it locally,
    // ~30 lines, no new dep.
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut lookup = [0u8; 256];
    for (i, &b) in ALPHABET.iter().enumerate() {
        lookup[b as usize] = i as u8;
    }
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for &c in bytes {
        if !ALPHABET.contains(&c) {
            return Err("non-base64url character");
        }
        buffer = (buffer << 6) | (lookup[c as usize] as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1u32 << bits) - 1;
        }
    }
    Ok(out)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret_storage::AgeFileSecretStore;

    fn temp_store() -> (AgeFileSecretStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = AgeFileSecretStore::new(dir.path().to_path_buf(), "test-pass".into())
            .expect("AgeFileSecretStore::new");
        (store, dir)
    }

    fn sample_token() -> StoredMcpToken {
        StoredMcpToken {
            access_token: "at-abc123".into(),
            refresh_token: Some("rt-def456".into()),
            token_type: "Bearer".into(),
            expires_at: Some(1_800_000_000),
            scope: vec!["read".into(), "write".into()],
            resource_uri: "https://mcp.example.com/mcp".into(),
            as_url: "https://auth.example.com".into(),
            client_id: "https://apollia.fr/.well-known/mcp-client-metadata".into(),
            identity_sub: Some("user-42".into()),
            identity_email: Some("alice@example.com".into()),
            created_at: 1_700_000_000,
        }
    }

    #[test]
    fn test_round_trip_preserves_every_field() {
        // GIVEN a token persisted under an arbitrary server name
        let (store, _dir) = temp_store();
        let original = sample_token();
        save_mcp_token(&store, "linear", &original).expect("save");

        // WHEN we load it back
        let loaded = load_mcp_token(&store, "linear")
            .expect("load")
            .expect("present");

        // THEN every field is preserved verbatim
        assert_eq!(loaded, original);
    }

    #[test]
    fn test_load_returns_none_for_unknown_server() {
        // GIVEN an empty store
        let (store, _dir) = temp_store();

        // WHEN we look up a server with no token
        let loaded = load_mcp_token(&store, "notion").expect("load");

        // THEN we get Ok(None), distinct from an error
        assert!(loaded.is_none());
    }

    #[test]
    fn test_save_overwrites_existing_token() {
        // GIVEN a token already persisted
        let (store, _dir) = temp_store();
        let first = sample_token();
        save_mcp_token(&store, "linear", &first).expect("save first");

        // WHEN we save a new token with a different access_token under the same name
        let mut second = sample_token();
        second.access_token = "at-new".into();
        save_mcp_token(&store, "linear", &second).expect("save second");

        // THEN the loaded token is the second one
        let loaded = load_mcp_token(&store, "linear")
            .expect("load")
            .expect("present");
        assert_eq!(loaded.access_token, "at-new");
    }

    #[test]
    fn test_delete_is_idempotent_for_missing_entries() {
        // GIVEN an empty store
        let (store, _dir) = temp_store();

        // WHEN we delete a non-existent token
        let result = delete_mcp_token(&store, "ghost");

        // THEN it succeeds (matches SecretStore::delete contract)
        assert!(result.is_ok());
    }

    #[test]
    fn test_delete_then_load_returns_none() {
        // GIVEN a saved token
        let (store, _dir) = temp_store();
        save_mcp_token(&store, "stripe", &sample_token()).expect("save");

        // WHEN we delete it
        delete_mcp_token(&store, "stripe").expect("delete");

        // THEN load returns None
        let loaded = load_mcp_token(&store, "stripe").expect("load");
        assert!(loaded.is_none());
    }

    #[test]
    fn test_invalid_server_name_is_rejected() {
        // GIVEN an empty store
        let (store, _dir) = temp_store();

        // WHEN we try to save under an invalid name (mirrors McpServerConfig validation)
        let err = save_mcp_token(&store, "Has Spaces", &sample_token()).expect_err("invalid name");

        // THEN the error is a Serialization variant with explanatory text
        match err {
            AuthError::Serialization(msg) => assert!(msg.contains("invalid characters")),
            other => panic!("expected Serialization, got {other:?}"),
        }
    }

    #[test]
    fn test_empty_server_name_is_rejected() {
        // GIVEN an empty store
        let (store, _dir) = temp_store();

        // WHEN we try to save under an empty name
        let err = save_mcp_token(&store, "", &sample_token()).expect_err("empty name");

        // THEN we get a clear error rather than a silent SecretStore write
        match err {
            AuthError::Serialization(msg) => assert!(msg.contains("must not be empty")),
            other => panic!("expected Serialization, got {other:?}"),
        }
    }

    #[test]
    fn test_is_expired_handles_none_expiry() {
        // GIVEN a long-lived token (expires_at = None)
        let mut token = sample_token();
        token.expires_at = None;

        // WHEN / THEN it is never considered expired
        assert!(!token.is_expired(0));
        assert!(!token.is_expired(i64::MAX));
        assert!(!token.needs_refresh(i64::MAX, 60));
    }

    #[test]
    fn test_is_expired_when_past_expiry() {
        // GIVEN a token with expires_at = 1_000
        let mut token = sample_token();
        token.expires_at = Some(1_000);

        // WHEN expiry is evaluated one second after the deadline
        // THEN the token is expired
        assert!(token.is_expired(1_001));
        // WHEN it is evaluated exactly at the deadline
        // THEN it is expired too: the contract is `>=`, not `>`
        assert!(token.is_expired(1_000));
        // WHEN it is evaluated one second before
        // THEN it is not expired
        assert!(!token.is_expired(999));
    }

    #[test]
    fn test_needs_refresh_within_leeway_window() {
        // GIVEN a token expiring at 1000 and a 60s leeway
        let mut token = sample_token();
        token.expires_at = Some(1_000);

        // WHEN refresh is evaluated well before the leeway window
        // THEN no refresh is due
        assert!(!token.needs_refresh(900, 60));
        // WHEN it is evaluated on the leeway boundary
        // THEN a refresh is due
        assert!(token.needs_refresh(940, 60));
        // WHEN it is evaluated past expiry
        // THEN a refresh is still due
        assert!(token.needs_refresh(1_100, 60));
    }

    #[test]
    fn test_extract_identity_claims_from_jwt() {
        // GIVEN a 3-segment JWT-shaped token with a known payload.
        // Header `{"alg":"none"}` and payload `{"sub":"abc","email":"a@b"}`,
        // base64url encoded with no padding.
        let header = "eyJhbGciOiJub25lIn0";
        let payload = "eyJzdWIiOiJhYmMiLCJlbWFpbCI6ImFAYiJ9";
        let signature = "sig";
        let jwt = format!("{header}.{payload}.{signature}");

        // WHEN we extract identity claims
        let claims = extract_identity_claims(&jwt);

        // THEN sub and email are populated
        assert_eq!(claims.sub.as_deref(), Some("abc"));
        assert_eq!(claims.email.as_deref(), Some("a@b"));
    }

    #[test]
    fn test_extract_identity_claims_returns_empty_for_opaque_tokens() {
        // GIVEN an opaque bearer token, carrying no JWT segment at all
        // WHEN identity claims are extracted from it
        let claims = extract_identity_claims("opaque-random-string-with-no-dots");

        // THEN we get empty claims rather than a panic
        assert_eq!(claims, JwtIdentityClaims::default());
    }

    #[test]
    fn test_extract_identity_claims_handles_malformed_payload() {
        // GIVEN a three-segment token whose payload is not valid base64
        // WHEN identity claims are extracted from it
        let claims = extract_identity_claims("header.not_valid_base64!.sig");

        // THEN we get empty claims rather than a panic
        assert_eq!(claims, JwtIdentityClaims::default());
    }

    #[test]
    fn test_service_namespace_is_stable() {
        // GIVEN the keyring service name MCP OAuth tokens are stored under
        // WHEN the constant is read
        // THEN it is the published one: a rename strands every token already stored
        assert_eq!(MCP_OAUTH_SERVICE, "apollia-mcp-oauth");
    }
}
