//! MCP HTTP OAuth orchestrator.
//!
//! Wires the primitives from [`mcp_oauth`](crate::mcp_oauth) +
//! [`pkce`](crate::pkce) + [`callback`](crate::callback) +
//! [`mcp_token_store`](crate::mcp_token_store) into a complete OAuth 2.1
//! flow, **without any provider-specific code**. The exact same sequence
//! runs for Notion, Linear, Sentry, Stripe, or any future MCP server that
//! conforms to MCP HTTP OAuth spec 2025-11-25.
//!
//! Two entry points:
//!
//! - [`negotiate_token`]: interactive, opens a browser, drives the full
//!   discovery + PKCE flow, persists the resulting [`StoredMcpToken`].
//! - [`ensure_fresh_token`]: non-interactive, returns the current access
//!   token, refreshing it via OAuth `refresh_token` grant when within the
//!   expiry leeway. Singleflight by `server_name` so concurrent tool calls
//!   from a hot agent don't burst N parallel refreshes.
//!
//! The orchestrator does not depend on `tauri`, `webbrowser`, or any UI
//! crate; the caller injects an `open_browser` closure. This keeps the
//! module testable in isolation and reusable from the CLI (e.g. print the
//! URL instead of opening a browser).

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use dashmap::DashMap;
use serde::Deserialize;
use thiserror::Error;
use tokio::sync::Mutex;

use crate::callback::{bind_ephemeral_port, wait_for_callback};
use crate::error::AuthError;
use crate::mcp_oauth::{
    canonical_resource_uri, parse_www_authenticate, read_json_capped, read_text_capped,
    AuthorizationServerMetadata, McpDiscoveryClient, ProtectedResourceMetadata, APOLLIA_CIMD_URL,
    MAX_OAUTH_RESPONSE_BYTES,
};
use crate::mcp_token_store::{
    delete_mcp_token, extract_identity_claims, load_mcp_token, save_mcp_token, StoredMcpToken,
};
use crate::pkce::{generate_code_challenge, generate_code_verifier};
use crate::secret_storage::SecretStore;

// ─── Public errors ──────────────────────────────────────────────────────────

/// Errors specific to the MCP OAuth orchestrator.
///
/// Wraps the lower-level [`AuthError`] for transport / parsing failures and
/// adds orchestration-specific variants (no AS available, PKCE unsupported,
/// no client registration method, reauthentication required).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum McpOAuthError {
    /// Lower-level transport, parsing, or storage error.
    #[error(transparent)]
    Auth(#[from] AuthError),

    /// The PRM document had no `authorization_servers` entries.
    #[error("MCP server '{server}' protected resource metadata declares no authorization servers")]
    NoAuthorizationServer { server: String },

    /// The AS does not advertise PKCE S256 support; Apollia refuses to start
    /// the flow rather than silently downgrade (spec MUST per RFC 9700).
    #[error(
        "authorization server at {as_url} does not advertise PKCE S256 \
         (code_challenge_methods_supported); refusing to start a downgraded flow"
    )]
    PkceS256NotSupported { as_url: String },

    /// The AS supports neither CIMD nor RFC 7591 DCR; Apollia cannot identify
    /// itself.
    #[error(
        "authorization server at {as_url} supports neither CIMD nor RFC 7591 \
         Dynamic Client Registration; manual pre-registration is required"
    )]
    NoClientRegistrationMethod { as_url: String },

    /// No persisted token exists for the requested server; caller must run
    /// `negotiate_token` interactively first.
    #[error("no MCP OAuth token persisted for server '{0}'; run negotiate_token first")]
    NoStoredToken(String),

    /// Refresh failed and the user must re-authenticate from scratch.
    /// Distinct from transient transport errors so the wizard can offer the
    /// right CTA ("Sign in again") instead of "Retry".
    #[error("MCP server '{server}' rejected refresh - re-authentication required ({reason})")]
    ReauthRequired { server: String, reason: String },

    /// Configuration is malformed (e.g. URL parse failure).
    #[error("invalid OAuth orchestration config: {0}")]
    InvalidConfig(String),
}

// ─── Public input shape ─────────────────────────────────────────────────────

/// Inputs to [`negotiate_token`]. Borrowed so the caller doesn't need to clone
/// the server URL just to drive a flow.
#[derive(Debug)]
pub struct NegotiateRequest<'a> {
    /// Persisted MCP server name, used as the token store key.
    pub server_name: &'a str,
    /// MCP server URL (e.g. `https://mcp.linear.app/sse`).
    pub server_url: &'a str,
    /// Raw `WWW-Authenticate` header value captured from a prior 401, if any.
    /// Used to short-circuit PRM discovery onto the URL the server points to.
    pub www_authenticate: Option<&'a str>,
    /// Scopes the caller wants. `None` defers to the AS's `scopes_supported`
    /// list. Empty `Vec` means "no scope"; explicit zero rather than default.
    pub scopes: Option<Vec<String>>,
    /// Pre-registered OAuth client identifier issued by the AS dev portal.
    /// When provided, bypasses CIMD/DCR resolution entirely. Required for
    /// AS that don't support CIMD or anonymous DCR (Figma as of 2026-05).
    /// Sourced from the catalog enrichment's `oauth_pre_registered_client_id_env`
    /// when set.
    pub client_id_override: Option<&'a str>,
}

// ─── Singleflight registry ──────────────────────────────────────────────────

/// Per-server mutex map that serialises refresh attempts.
///
/// A hot agent can issue N tool calls in parallel against the same MCP server;
/// when the token expires they would each independently try to refresh. With
/// this lock only the first refresh hits the network, the others see the
/// freshly-rotated token after acquiring the mutex.
fn refresh_locks() -> &'static DashMap<String, Arc<Mutex<()>>> {
    static LOCKS: OnceLock<DashMap<String, Arc<Mutex<()>>>> = OnceLock::new();
    LOCKS.get_or_init(DashMap::new)
}

fn lock_for(server_name: &str) -> Arc<Mutex<()>> {
    refresh_locks()
        .entry(server_name.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

// ─── Token endpoint response ────────────────────────────────────────────────

/// RFC 6749 §5.1 token endpoint response. Tolerant `scope` parsing: some AS
/// return a string, others an array; we accept both.
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default = "default_bearer")]
    token_type: String,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    scope: Option<String>,
}

fn default_bearer() -> String {
    "Bearer".into()
}

// ─── negotiate_token ────────────────────────────────────────────────────────

/// Drive a full MCP HTTP OAuth flow and persist the resulting token.
///
/// Sequence:
/// 1. Resolve PRM URL (prefer `WWW-Authenticate: resource_metadata=…`, else
///    fall back to `<server_url>/.well-known/oauth-protected-resource`).
/// 2. Fetch PRM → pick first `authorization_servers[]` entry.
/// 3. Fetch AS metadata (RFC 8414 + OIDC variants).
/// 4. Refuse if PKCE S256 not advertised (spec MUST).
/// 5. Resolve `client_id`: CIMD when supported, RFC 7591 DCR otherwise.
/// 6. Bind loopback callback listener, generate PKCE pair + state.
/// 7. Open browser at authorize URL with `resource=` (RFC 8707 MUST).
/// 8. Await callback, exchange code for tokens with `resource=` repeated.
/// 9. Persist via [`save_mcp_token`].
///
/// The `open_browser` callback receives the authorize URL, typically wired
/// to the `open` crate by Tauri, or to a CLI prompt in headless contexts.
pub async fn negotiate_token(
    req: NegotiateRequest<'_>,
    store: &dyn SecretStore,
    open_browser: impl FnOnce(&str) -> Result<(), AuthError> + Send,
) -> Result<StoredMcpToken, McpOAuthError> {
    let discovery = McpDiscoveryClient::new()?;

    // 1-3. Discovery.
    let prm = fetch_prm(&discovery, req.server_url, req.www_authenticate).await?;
    let as_url =
        prm.authorization_servers
            .first()
            .cloned()
            .ok_or(McpOAuthError::NoAuthorizationServer {
                server: req.server_name.to_string(),
            })?;
    let as_metadata = discovery.fetch_as_metadata(&as_url).await?;

    // 4. PKCE S256 enforcement.
    if !as_metadata.supports_pkce_s256() {
        return Err(McpOAuthError::PkceS256NotSupported { as_url });
    }

    // 5. Client identification: CIMD if the AS supports it, otherwise DCR.
    //
    // We bind the loopback listener *before* DCR so the registered
    // `redirect_uri` matches the actual port we'll listen on. Order matters
    // for AS that strict-match (some do, even though loopback ports SHOULD be
    // wildcardable per RFC 8252 §7.3).
    let (listener, port) = bind_ephemeral_port().await.map_err(McpOAuthError::Auth)?;
    let redirect_uri = format!("http://127.0.0.1:{port}/oauth/callback");

    let client_id = resolve_client_id(
        &discovery,
        &as_metadata,
        &redirect_uri,
        req.client_id_override,
    )
    .await?;

    // 6. PKCE + state.
    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);
    let state = generate_code_verifier(); // 43-char random suffices for state too

    // 7. Compose authorize URL; RFC 8707 `resource=` is MUST.
    let resource = canonical_resource_uri(req.server_url);
    let scopes = pick_scopes(
        &req.scopes,
        &prm,
        &as_metadata,
        req.client_id_override.is_some(),
    );
    let authorize_url = build_authorize_url(AuthorizeUrlParts {
        authorization_endpoint: &as_metadata.authorization_endpoint,
        client_id: &client_id,
        redirect_uri: &redirect_uri,
        scope: &scopes,
        state: &state,
        code_challenge: &code_challenge,
        resource: &resource,
    });

    // 8. Drive the browser → callback → exchange.
    tracing::info!(
        server = %req.server_name,
        client_id = %client_id,
        scopes = ?scopes,
        authorize_url = %authorize_url,
        "mcp.oauth.browser.opening"
    );
    open_browser(&authorize_url)?;
    let code = wait_for_callback(listener, &state)
        .await
        .map_err(McpOAuthError::Auth)?;

    let token_response = exchange_code(
        &discovery,
        ExchangeCodeParts {
            token_endpoint: &as_metadata.token_endpoint,
            client_id: &client_id,
            code: &code,
            code_verifier: &code_verifier,
            redirect_uri: &redirect_uri,
            resource: &resource,
        },
    )
    .await?;

    // 9. Materialise + persist.
    let now = chrono::Utc::now().timestamp();
    let identity = extract_identity_claims(&token_response.access_token);
    let stored = StoredMcpToken {
        access_token: token_response.access_token,
        refresh_token: token_response.refresh_token,
        token_type: token_response.token_type,
        expires_at: token_response.expires_in.map(|secs| now + secs),
        scope: parse_scope_string(&token_response.scope, &scopes),
        resource_uri: resource,
        as_url: as_metadata.issuer.clone().unwrap_or_else(|| as_url.clone()),
        client_id,
        identity_sub: identity.sub,
        identity_email: identity.email,
        created_at: now,
    };
    save_mcp_token(store, req.server_name, &stored)?;
    Ok(stored)
}

// ─── ensure_fresh_token ─────────────────────────────────────────────────────

/// Return a fresh access token for `server_name`, refreshing it if necessary.
///
/// Refresh leeway: 60 seconds, we proactively renew the token if it will
/// expire within the next minute, so in-flight tool calls don't observe a
/// 401 mid-flight.
///
/// Concurrent calls for the same `server_name` are serialised through a
/// per-server mutex (singleflight): the second-and-later callers wait, then
/// observe the freshly-rotated token without issuing their own refresh.
///
/// Returns [`McpOAuthError::NoStoredToken`] when no token has been persisted
/// (caller must run [`negotiate_token`] interactively). Returns
/// [`McpOAuthError::ReauthRequired`] when the AS rejects the refresh; the
/// stored token is **deleted** in this case so the next install attempt
/// starts from a clean slate.
pub async fn ensure_fresh_token(
    server_name: &str,
    store: &dyn SecretStore,
) -> Result<String, McpOAuthError> {
    const REFRESH_LEEWAY_SECS: i64 = 60;

    // Fast path: load + return without locking when the token has plenty of
    // life left. The singleflight lock is only needed to coordinate refresh.
    //
    // load_mcp_token may fail when the OS keychain refuses access, e.g.
    // the user clicked "Deny" on the macOS keychain prompt or the dev-mode
    // binary signature changed since the entry's ACL was set up. We map
    // these errors to ReauthRequired so the UI surfaces the right CTA
    // ("Reconnect") instead of a generic spawn-failed message.
    let initial = match load_mcp_token(store, server_name) {
        Ok(Some(token)) => token,
        Ok(None) => return Err(McpOAuthError::NoStoredToken(server_name.to_string())),
        Err(e) => {
            return Err(McpOAuthError::ReauthRequired {
                server: server_name.to_string(),
                reason: format!("keychain access failed ({e}) - re-allow Apollia in Keychain Access or sign in again"),
            });
        }
    };
    let now = chrono::Utc::now().timestamp();
    if !initial.needs_refresh(now, REFRESH_LEEWAY_SECS) {
        return Ok(initial.access_token);
    }

    // Slow path: acquire the per-server lock and re-check inside the
    // critical section. A concurrent refresh may already have rotated the
    // token by the time we acquire the lock.
    let lock = lock_for(server_name);
    let _guard = lock.lock().await;

    let now = chrono::Utc::now().timestamp();
    let current = load_mcp_token(store, server_name)?
        .ok_or_else(|| McpOAuthError::NoStoredToken(server_name.to_string()))?;
    if !current.needs_refresh(now, REFRESH_LEEWAY_SECS) {
        return Ok(current.access_token);
    }

    let Some(refresh_token) = current.refresh_token.as_deref() else {
        // Token has no refresh token (some AS issue access-only tokens) and is
        // about to expire, so re-auth required.
        return Err(McpOAuthError::ReauthRequired {
            server: server_name.to_string(),
            reason: "no refresh_token stored".into(),
        });
    };

    match refresh_grant(&current, refresh_token).await {
        Ok(rotated) => {
            save_mcp_token(store, server_name, &rotated)?;
            Ok(rotated.access_token)
        }
        Err(e) => {
            // Hard refresh failure (`invalid_grant`, revoked token, etc.):
            // wipe the stale token so the wizard surfaces re-auth instead of
            // a confusing "token expired" loop.
            let _ = delete_mcp_token(store, server_name);
            Err(McpOAuthError::ReauthRequired {
                server: server_name.to_string(),
                reason: format!("{e}"),
            })
        }
    }
}

// ─── Helpers (private) ──────────────────────────────────────────────────────

/// Fetch the Protected Resource Metadata, preferring the URL advertised by a
/// captured `WWW-Authenticate: resource_metadata=…` header when available.
async fn fetch_prm(
    discovery: &McpDiscoveryClient,
    server_url: &str,
    www_authenticate: Option<&str>,
) -> Result<ProtectedResourceMetadata, AuthError> {
    if let Some(value) = www_authenticate {
        if let Some(parsed) = parse_www_authenticate(value) {
            if let Some(prm_url) = parsed.resource_metadata {
                return discovery.fetch_prm_at(&prm_url).await;
            }
        }
    }
    discovery.fetch_prm(server_url).await
}

/// Resolve the `client_id`, augmented with a manual pre-registration override.
///
/// Priority:
/// 1. **Caller override** (`client_id_override`): used verbatim. Bypasses
///    discovery. The wizard injects this when the catalog enrichment declares
///    `oauth_pre_registered_client_id_env` and the corresponding env var is
///    populated. Required for AS that support neither CIMD nor anonymous DCR
///    (Figma today: dev-portal app registration only).
/// 2. **CIMD** when the AS advertises `client_id_metadata_document_supported`
///    (modern MCP-compliant servers: Linear, Sentry, Notion).
/// 3. **DCR (RFC 7591)** when a registration endpoint is published.
/// 4. **CIMD as last resort** when no registration endpoint is available and
///    no CIMD flag: Apollia tries it on the off chance the AS accepts it.
/// 5. Otherwise, a guidance-bearing error pointing at the AS dev portal.
///
/// An earlier version of this function did an implicit DCR-then-CIMD fallback
/// on 401/403. That made debugging harder because
/// CIMD failures surface in the **browser** (post-redirect) while DCR errors
/// surface in Apollia directly. The current version fails fast on DCR errors
/// so the operator sees the right diagnostic.
async fn resolve_client_id(
    discovery: &McpDiscoveryClient,
    as_metadata: &AuthorizationServerMetadata,
    redirect_uri: &str,
    client_id_override: Option<&str>,
) -> Result<String, McpOAuthError> {
    if let Some(id) = client_id_override {
        if !id.trim().is_empty() {
            return Ok(id.to_string());
        }
    }
    if as_metadata.client_id_metadata_document_supported == Some(true) {
        return Ok(APOLLIA_CIMD_URL.to_string());
    }
    let Some(dcr_endpoint) = as_metadata.registration_endpoint.as_deref() else {
        // No DCR endpoint, no explicit CIMD flag: try CIMD anyway (some AS
        // accept it without advertising). If the AS rejects, the user will
        // see a clear error from the AS itself.
        return Ok(APOLLIA_CIMD_URL.to_string());
    };
    discovery
        .register_client(dcr_endpoint, redirect_uri)
        .await
        .map(|reg| reg.client_id)
        .map_err(|e| {
            // DCR failed; do NOT auto-fallback to CIMD (we'd hide the real
            // error behind a worse one). Surface the original DCR failure so
            // the operator can choose: register manually at the AS dev portal,
            // or contact the AS support to enable DCR for their workspace.
            tracing::warn!(
                error = %e,
                detail = "pre-registration at the authorization server portal is likely required",
                "mcp.oauth.dcr.failed"
            );
            e.into()
        })
}

/// Choose the effective scope list: caller's selection > PRM `scopes_supported`.
/// AS metadata is consulted last because its scopes are server-defined, while
/// PRM scopes are resource-defined and more accurate for MCP.
///
/// When `has_client_id_override` is true (operator pre-registered Apollia at
/// the provider's dev portal), we fall back to an empty list rather than the
/// PRM defaults. Pre-registered OAuth apps have their scopes baked at
/// registration time, and many AS reject the MCP-spec placeholder
/// `mcp:connect` advertised in PRMs (Figma, for instance). An empty list
/// makes `build_authorize_url` omit `scope=` entirely so the AS applies the
/// scopes configured at the dev portal.
fn pick_scopes(
    caller: &Option<Vec<String>>,
    prm: &ProtectedResourceMetadata,
    _as_metadata: &AuthorizationServerMetadata,
    has_client_id_override: bool,
) -> Vec<String> {
    if let Some(scopes) = caller {
        return scopes.clone();
    }
    if has_client_id_override {
        return Vec::new();
    }
    prm.scopes_supported.clone()
}

/// Parse the AS-reported `scope` string (space-separated per RFC 6749 §3.3)
/// into a Vec. Falls back to the requested scopes when absent.
fn parse_scope_string(reported: &Option<String>, requested: &[String]) -> Vec<String> {
    match reported {
        Some(s) => s.split_whitespace().map(String::from).collect::<Vec<_>>(),
        None => requested.to_vec(),
    }
}

/// Fields needed to compose the authorize URL, grouped to keep the signature
/// of [`build_authorize_url`] readable.
struct AuthorizeUrlParts<'a> {
    authorization_endpoint: &'a str,
    client_id: &'a str,
    redirect_uri: &'a str,
    scope: &'a [String],
    state: &'a str,
    code_challenge: &'a str,
    resource: &'a str,
}

fn build_authorize_url(p: AuthorizeUrlParts<'_>) -> String {
    let scope_param = p.scope.join(" ");
    let mut url = format!(
        "{base}?response_type=code&client_id={cid}&redirect_uri={redir}\
         &state={state}&code_challenge={cc}&code_challenge_method=S256&resource={res}",
        base = p.authorization_endpoint,
        cid = urlencoding::encode(p.client_id),
        redir = urlencoding::encode(p.redirect_uri),
        state = urlencoding::encode(p.state),
        cc = urlencoding::encode(p.code_challenge),
        res = urlencoding::encode(p.resource),
    );
    if !scope_param.is_empty() {
        url.push_str(&format!("&scope={}", urlencoding::encode(&scope_param)));
    }
    url
}

/// Fields needed to exchange an authorization code for tokens, grouped to keep
/// the signature of [`exchange_code`] readable.
struct ExchangeCodeParts<'a> {
    token_endpoint: &'a str,
    client_id: &'a str,
    code: &'a str,
    code_verifier: &'a str,
    redirect_uri: &'a str,
    resource: &'a str,
}

/// POST the authorization code to the token endpoint, including `resource=`
/// per RFC 8707 (MUST be repeated at the token endpoint).
async fn exchange_code(
    _discovery: &McpDiscoveryClient,
    p: ExchangeCodeParts<'_>,
) -> Result<TokenResponse, AuthError> {
    let http = apollia_core::net::configured_endpoint_client_builder()
        .build()
        .map_err(|e| AuthError::HttpError(e.to_string()))?;
    let mut form: HashMap<&str, &str> = HashMap::new();
    form.insert("grant_type", "authorization_code");
    form.insert("code", p.code);
    form.insert("code_verifier", p.code_verifier);
    form.insert("redirect_uri", p.redirect_uri);
    form.insert("client_id", p.client_id);
    form.insert("resource", p.resource);

    let response = http
        .post(p.token_endpoint)
        .form(&form)
        .send()
        .await
        .map_err(|e| AuthError::HttpError(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = read_text_capped(response, MAX_OAUTH_RESPONSE_BYTES)
            .await
            .unwrap_or_else(|_| "<unreadable>".into());
        return Err(AuthError::TokenExchangeFailed(format!("{status}: {body}")));
    }
    read_json_capped::<TokenResponse>(response, MAX_OAUTH_RESPONSE_BYTES).await
}

/// Refresh an access token using the OAuth `refresh_token` grant.
///
/// `resource=` is repeated per RFC 8707; the refresh response is rebound to
/// the same MCP server URI.
async fn refresh_grant(
    current: &StoredMcpToken,
    refresh_token: &str,
) -> Result<StoredMcpToken, AuthError> {
    let discovery = McpDiscoveryClient::new()?;
    let as_metadata = discovery.fetch_as_metadata(&current.as_url).await?;

    let http = apollia_core::net::configured_endpoint_client_builder()
        .build()
        .map_err(|e| AuthError::HttpError(e.to_string()))?;
    let mut form: HashMap<&str, &str> = HashMap::new();
    form.insert("grant_type", "refresh_token");
    form.insert("refresh_token", refresh_token);
    form.insert("client_id", &current.client_id);
    form.insert("resource", &current.resource_uri);
    // Include scope to keep the refreshed token bound to the same grant;
    // optional per RFC 6749 §6 but helps when the AS narrows scopes by default.
    let scope_param = current.scope.join(" ");
    if !scope_param.is_empty() {
        form.insert("scope", &scope_param);
    }

    let response = http
        .post(&as_metadata.token_endpoint)
        .form(&form)
        .send()
        .await
        .map_err(|e| AuthError::HttpError(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = read_text_capped(response, MAX_OAUTH_RESPONSE_BYTES)
            .await
            .unwrap_or_else(|_| "<unreadable>".into());
        return Err(AuthError::TokenExchangeFailed(format!("{status}: {body}")));
    }
    let token: TokenResponse = read_json_capped(response, MAX_OAUTH_RESPONSE_BYTES).await?;

    let now = chrono::Utc::now().timestamp();
    let identity = extract_identity_claims(&token.access_token);
    Ok(StoredMcpToken {
        access_token: token.access_token,
        // Some AS issue a new refresh token, others reuse the existing one.
        refresh_token: token.refresh_token.or_else(|| Some(refresh_token.into())),
        token_type: token.token_type,
        expires_at: token.expires_in.map(|secs| now + secs),
        scope: parse_scope_string(&token.scope, &current.scope),
        resource_uri: current.resource_uri.clone(),
        as_url: current.as_url.clone(),
        client_id: current.client_id.clone(),
        identity_sub: identity.sub.or_else(|| current.identity_sub.clone()),
        identity_email: identity.email.or_else(|| current.identity_email.clone()),
        created_at: now,
    })
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret_storage::AgeFileSecretStore;
    use axum::{routing::post, Json, Router};
    use std::sync::Arc;
    use tokio::sync::Mutex as AsyncMutex;

    fn temp_store() -> (AgeFileSecretStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = AgeFileSecretStore::new(dir.path().to_path_buf(), "pass".into())
            .expect("AgeFileSecretStore::new");
        (store, dir)
    }

    fn fresh_token() -> StoredMcpToken {
        let now = chrono::Utc::now().timestamp();
        StoredMcpToken {
            access_token: "at-fresh".into(),
            refresh_token: Some("rt-fresh".into()),
            token_type: "Bearer".into(),
            expires_at: Some(now + 3600),
            scope: vec!["read".into()],
            resource_uri: "https://mcp.example.com".into(),
            as_url: "https://auth.example.com".into(),
            client_id: APOLLIA_CIMD_URL.into(),
            identity_sub: None,
            identity_email: None,
            created_at: now,
        }
    }

    // ── ensure_fresh_token unit tests (no network) ───────────────────────────

    #[tokio::test]
    async fn test_ensure_fresh_returns_existing_when_not_expired() {
        // GIVEN a stored token with 1h remaining
        let (store, _dir) = temp_store();
        save_mcp_token(&store, "happy", &fresh_token()).unwrap();

        // WHEN ensure_fresh_token is called
        let bearer = ensure_fresh_token("happy", &store).await.unwrap();

        // THEN it returns the existing access_token (no refresh)
        assert_eq!(bearer, "at-fresh");
    }

    #[tokio::test]
    async fn test_ensure_fresh_returns_no_stored_token_error() {
        // GIVEN an empty store
        let (store, _dir) = temp_store();

        // WHEN we ask for a server that was never persisted
        let err = ensure_fresh_token("never-seen", &store).await.unwrap_err();

        // THEN we get NoStoredToken (caller must run negotiate_token)
        assert!(matches!(err, McpOAuthError::NoStoredToken(_)));
    }

    #[tokio::test]
    async fn test_ensure_fresh_returns_reauth_when_no_refresh_token() {
        // GIVEN a token about to expire (10s remaining) with no refresh_token
        let (store, _dir) = temp_store();
        let mut token = fresh_token();
        token.refresh_token = None;
        token.expires_at = Some(chrono::Utc::now().timestamp() + 10);
        save_mcp_token(&store, "no-refresh", &token).unwrap();

        // WHEN ensure_fresh_token is called
        let err = ensure_fresh_token("no-refresh", &store).await.unwrap_err();

        // THEN we get ReauthRequired with a clear reason
        match err {
            McpOAuthError::ReauthRequired { server, reason } => {
                assert_eq!(server, "no-refresh");
                assert!(reason.contains("no refresh_token"));
            }
            other => panic!("expected ReauthRequired, got {other:?}"),
        }
    }

    // ── build_authorize_url unit tests ───────────────────────────────────────

    #[test]
    fn test_authorize_url_includes_all_required_params() {
        // GIVEN a typical parts set
        let parts = AuthorizeUrlParts {
            authorization_endpoint: "https://auth.example.com/authorize",
            client_id: APOLLIA_CIMD_URL,
            redirect_uri: "http://127.0.0.1:54321/oauth/callback",
            scope: &["read".into(), "write".into()],
            state: "state-abc",
            code_challenge: "challenge-xyz",
            resource: "https://mcp.example.com",
        };
        // WHEN the authorize URL is built
        let url = build_authorize_url(parts);

        // THEN it carries every mandatory OAuth 2.1 / MCP parameter
        assert!(url.starts_with("https://auth.example.com/authorize?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("code_challenge=challenge-xyz"));
        assert!(url.contains("state=state-abc"));
        assert!(url.contains("resource=https%3A%2F%2Fmcp.example.com"));
        assert!(url.contains("scope=read%20write"));
        // PKCE S256 + RFC 8707 are both present; this is the MCP spec MUST.
    }

    #[test]
    fn test_authorize_url_omits_scope_when_empty() {
        // GIVEN parts with no scopes selected
        let parts = AuthorizeUrlParts {
            authorization_endpoint: "https://a/x",
            client_id: "cid",
            redirect_uri: "http://127.0.0.1:8/oauth/callback",
            scope: &[],
            state: "s",
            code_challenge: "c",
            resource: "https://m",
        };
        // WHEN the authorize URL is built
        let url = build_authorize_url(parts);

        // THEN no `scope=` parameter is included (rather than `scope=`)
        assert!(!url.contains("scope="));
    }

    // ── parse_scope_string ───────────────────────────────────────────────────

    // ── pick_scopes (override defensive guard, Figma fix) ────────

    fn dummy_prm_with_scopes(scopes: &[&str]) -> ProtectedResourceMetadata {
        ProtectedResourceMetadata {
            resource: "https://mcp.example.com".into(),
            authorization_servers: vec!["https://auth.example.com".into()],
            bearer_methods_supported: vec![],
            scopes_supported: scopes.iter().map(|s| s.to_string()).collect(),
            resource_name: None,
        }
    }

    #[test]
    fn test_pick_scopes_returns_empty_when_override_and_no_caller() {
        // GIVEN a PRM publishing the MCP placeholder scope (Figma case) AND a
        // pre-registered client_id override in effect AND no caller selection.
        let prm = dummy_prm_with_scopes(&["mcp:connect"]);
        let meta = dummy_as_metadata();

        // WHEN pick_scopes resolves the effective scope list
        let scopes = pick_scopes(&None, &prm, &meta, true);

        // THEN the PRM scope is dropped; empty list means build_authorize_url
        // omits `scope=`, so the AS applies the scopes from the dev portal.
        assert!(scopes.is_empty(), "expected [], got {scopes:?}");
    }

    #[test]
    fn test_pick_scopes_caller_selection_wins_even_with_override() {
        // GIVEN a pre-registered client_id override but a caller-supplied list
        // (operator explicitly chose scopes; we honour their intent).
        let prm = dummy_prm_with_scopes(&["mcp:connect"]);
        let meta = dummy_as_metadata();
        let caller = Some(vec!["files:read".to_string()]);

        // WHEN pick_scopes resolves
        let scopes = pick_scopes(&caller, &prm, &meta, true);

        // THEN caller wins over the override defensive guard.
        assert_eq!(scopes, vec!["files:read".to_string()]);
    }

    #[test]
    fn test_pick_scopes_falls_back_to_prm_without_override() {
        // GIVEN no caller selection AND no override (the CIMD/DCR path).
        let prm = dummy_prm_with_scopes(&["read", "write"]);
        let meta = dummy_as_metadata();

        // WHEN pick_scopes resolves
        let scopes = pick_scopes(&None, &prm, &meta, false);

        // THEN PRM scopes are used as advertised (the normal MCP-spec path).
        assert_eq!(scopes, vec!["read".to_string(), "write".to_string()]);
    }

    #[test]
    fn test_parse_scope_string_splits_on_whitespace() {
        // GIVEN a scope string holding three whitespace-separated scopes
        // WHEN it is parsed, with no requested set to fall back on
        let scopes = parse_scope_string(&Some("read write admin".into()), &[]);
        // THEN the three scopes come back separately
        assert_eq!(scopes, vec!["read", "write", "admin"]);
    }

    #[test]
    fn test_parse_scope_string_falls_back_to_requested() {
        // GIVEN a server that returned no scope string, and two requested scopes
        let requested = vec!["a".to_string(), "b".to_string()];
        // WHEN the scopes are parsed
        let scopes = parse_scope_string(&None, &requested);
        // THEN the requested set is kept as the granted one
        assert_eq!(scopes, requested);
    }

    // ── resolve_client_id (CIMD vs DCR priority) ────────

    fn dummy_as_metadata() -> AuthorizationServerMetadata {
        AuthorizationServerMetadata {
            authorization_endpoint: "https://a/authorize".into(),
            token_endpoint: "https://a/token".into(),
            registration_endpoint: None,
            code_challenge_methods_supported: vec!["S256".into()],
            revocation_endpoint: None,
            issuer: None,
            client_id_metadata_document_supported: None,
        }
    }

    #[tokio::test]
    async fn test_resolve_client_id_prefers_cimd_when_explicitly_supported() {
        // GIVEN an AS metadata that advertises CIMD support AND a DCR endpoint.
        let mut meta = dummy_as_metadata();
        meta.client_id_metadata_document_supported = Some(true);
        meta.registration_endpoint = Some("https://a/register".into());
        let discovery = McpDiscoveryClient::new().unwrap();

        // WHEN we resolve the client_id
        let cid = resolve_client_id(&discovery, &meta, "http://127.0.0.1:0/oauth/callback", None)
            .await
            .unwrap();

        // THEN CIMD wins; DCR is not called even though the endpoint exists.
        assert_eq!(cid, APOLLIA_CIMD_URL);
    }

    #[tokio::test]
    async fn test_resolve_client_id_honours_override_above_everything() {
        // GIVEN an AS that supports CIMD AND has a DCR endpoint, but the
        // caller supplies a pre-registered client_id (typical of Figma where
        // neither CIMD nor anonymous DCR are accepted; the operator
        // pre-registered Apollia at figma.com/developers).
        let mut meta = dummy_as_metadata();
        meta.client_id_metadata_document_supported = Some(true);
        meta.registration_endpoint = Some("https://a/register".into());
        let discovery = McpDiscoveryClient::new().unwrap();

        // WHEN we resolve with an explicit override
        let cid = resolve_client_id(
            &discovery,
            &meta,
            "http://127.0.0.1:0/oauth/callback",
            Some("figma-issued-client-id-xyz"),
        )
        .await
        .unwrap();

        // THEN the override wins; CIMD/DCR are bypassed entirely.
        assert_eq!(cid, "figma-issued-client-id-xyz");
    }

    #[tokio::test]
    async fn test_resolve_client_id_uses_cimd_when_no_registration_endpoint() {
        // GIVEN an AS with no DCR endpoint and no explicit CIMD flag.
        let meta = dummy_as_metadata();
        let discovery = McpDiscoveryClient::new().unwrap();

        // WHEN we resolve
        let cid = resolve_client_id(&discovery, &meta, "http://127.0.0.1:0/oauth/callback", None)
            .await
            .unwrap();

        // THEN we default to CIMD (the only option left).
        assert_eq!(cid, APOLLIA_CIMD_URL);
    }

    /// Spawn an in-process axum mock that simulates the AS endpoints we hit.
    async fn spawn_mock_as() -> (String, Arc<AsyncMutex<MockState>>) {
        let state = Arc::new(AsyncMutex::new(MockState::default()));

        let app = Router::new()
            .route(
                "/.well-known/oauth-protected-resource",
                axum::routing::get({
                    let state = state.clone();
                    move || {
                        let state = state.clone();
                        async move {
                            let s = state.lock().await;
                            Json(serde_json::json!({
                                "resource": s.resource_uri,
                                "authorization_servers": [s.issuer.clone()],
                                "scopes_supported": ["read", "write"],
                            }))
                        }
                    }
                }),
            )
            .route(
                "/.well-known/oauth-authorization-server",
                axum::routing::get({
                    let state = state.clone();
                    move || {
                        let state = state.clone();
                        async move {
                            let s = state.lock().await;
                            Json(serde_json::json!({
                                "issuer": s.issuer,
                                "authorization_endpoint": format!("{}/authorize", s.issuer),
                                "token_endpoint": format!("{}/token", s.issuer),
                                "registration_endpoint": format!("{}/register", s.issuer),
                                "code_challenge_methods_supported": ["S256"],
                            }))
                        }
                    }
                }),
            )
            .route(
                "/register",
                post({
                    let state = state.clone();
                    move |body: Json<serde_json::Value>| {
                        let state = state.clone();
                        async move {
                            let mut s = state.lock().await;
                            s.last_dcr_body = Some(body.0);
                            Json(serde_json::json!({
                                "client_id": "registered-client-id",
                            }))
                        }
                    }
                }),
            )
            .route(
                "/token",
                post({
                    let state = state.clone();
                    move |body: axum::extract::Form<HashMap<String, String>>| {
                        let state = state.clone();
                        async move {
                            let mut s = state.lock().await;
                            s.last_token_form = Some(body.0.clone());
                            Json(serde_json::json!({
                                "access_token": "at-from-mock",
                                "refresh_token": "rt-from-mock",
                                "token_type": "Bearer",
                                "expires_in": 3600,
                                "scope": "read write",
                            }))
                        }
                    }
                }),
            );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let base = format!("http://127.0.0.1:{port}");
        {
            let mut s = state.lock().await;
            s.issuer = base.clone();
            s.resource_uri = base.clone();
        }
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (base, state)
    }

    #[derive(Default)]
    struct MockState {
        issuer: String,
        resource_uri: String,
        last_dcr_body: Option<serde_json::Value>,
        last_token_form: Option<HashMap<String, String>>,
    }

    #[tokio::test]
    async fn test_negotiate_token_full_flow_persists_token() {
        // GIVEN a mock AS that accepts DCR + returns a token, an empty token
        // store, and an `open_browser` closure that drives the callback by
        // visiting the redirect URI with the right `state`.
        let (_mock_base, mock_state) = spawn_mock_as().await;
        let (store, _dir) = temp_store();
        let mock_state_clone = mock_state.clone();

        let open_browser = move |url: &str| {
            // Parse `state` and `redirect_uri` from the authorize URL and
            // simulate the user consenting by hitting the redirect with a
            // canned authorization code.
            let url = url.to_string();
            let mock_state = mock_state_clone.clone();
            tokio::spawn(async move {
                // Best-effort URL parsing, sufficient for tests.
                let state = parse_param(&url, "state").unwrap_or_default();
                let redirect = parse_param(&url, "redirect_uri").unwrap_or_default();
                let redirect_decoded = urlencoding::decode(&redirect).unwrap().to_string();
                let callback_url =
                    format!("{}?code=auth-code-123&state={}", redirect_decoded, state);
                // Tiny delay so wait_for_callback has time to start the server.
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                let _ = reqwest::get(&callback_url).await;
                drop(mock_state); // keep the mock alive until callback fires
            });
            Ok(())
        };

        // WHEN we run the full negotiation
        let req = NegotiateRequest {
            server_name: "mocky",
            server_url: &mock_state.lock().await.resource_uri.clone(),
            www_authenticate: None,
            scopes: None, // defer to PRM scopes_supported ⇒ ["read","write"]
            client_id_override: None,
        };
        let result = negotiate_token(req, &store, open_browser).await.unwrap();

        // THEN the token returned is the one from the mock AS, with the
        // scopes echoed back and persisted under the requested server_name.
        assert_eq!(result.access_token, "at-from-mock");
        assert_eq!(result.refresh_token.as_deref(), Some("rt-from-mock"));
        assert_eq!(result.scope, vec!["read", "write"]);
        let stored = load_mcp_token(&store, "mocky").unwrap().unwrap();
        assert_eq!(stored.access_token, "at-from-mock");

        // AND the token-endpoint POST carried `resource=` (RFC 8707 MUST).
        let form = mock_state.lock().await.last_token_form.clone().unwrap();
        assert_eq!(form.get("grant_type").unwrap(), "authorization_code");
        assert_eq!(form.get("code_verifier").unwrap().len(), 43);
        assert!(
            form.get("resource")
                .unwrap()
                .starts_with("http://127.0.0.1:"),
            "expected resource= to match the canonical MCP server URI, got {:?}",
            form.get("resource")
        );
    }

    /// Best-effort `?key=value` extraction from a URL, tests only.
    fn parse_param(url: &str, key: &str) -> Option<String> {
        let qs_start = url.find('?')?;
        for pair in url[qs_start + 1..].split('&') {
            let (k, v) = pair.split_once('=')?;

            if k == key {
                return Some(v.to_string());
            }
        }
        None
    }
}
