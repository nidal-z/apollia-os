//! MCP-compliant OAuth 2.1 client (spec revision 2025-11-25).
//!
//! Implements the discovery + authorization flow required to connect a remote
//! MCP HTTP server without per-server configuration. Conforms to:
//!
//! - **RFC 9728** Protected Resource Metadata: well-known
//!   `/.well-known/oauth-protected-resource` exposed by the MCP server.
//! - **RFC 8414** Authorization Server Metadata: `/.well-known/oauth-authorization-server`.
//! - **OIDC Discovery 1.0**: `/.well-known/openid-configuration` fallback.
//! - **RFC 8707** Resource Indicators: `resource=` parameter MUST on every
//!   authorization + token request.
//! - **RFC 7636** PKCE S256 (always, clients are public in this model).
//! - **Client ID Metadata Documents (CIMD)**: recommended for clients without
//!   a pre-existing relationship with the AS. Apollia hosts its CIMD at
//!   `https://apollia.fr/.well-known/mcp-client-metadata`.
//! - **RFC 7591** Dynamic Client Registration: fallback for AS that does not
//!   accept CIMD.
//!
//! High-level flow:
//! 1. Client requests the MCP server endpoint without an `Authorization` header.
//! 2. Server responds with `401 WWW-Authenticate: Bearer resource_metadata="…"`.
//! 3. Client fetches the PRM document → extracts `authorization_servers[]`.
//! 4. Client fetches AS metadata (RFC 8414 path, then OIDC fallback).
//! 5. Client resolves its `client_id` (pre-registered → CIMD → DCR).
//! 6. Client launches the authorization code flow with PKCE S256 and `resource=`.
//! 7. Token endpoint exchange returns the access + refresh tokens.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::AuthError;

// ─── capped response reads (untrusted AS / resource metadata) ───────────────

/// Byte ceiling for a single OAuth / MCP-discovery HTTP response.
///
/// Authorization-server metadata, protected-resource metadata, DCR responses,
/// and token responses are all small, spec-bounded JSON documents. 1 MiB sits
/// far above any legitimate payload while bounding memory against a hostile or
/// broken peer. This is not per-server configurable: the OAuth flow runs before
/// any transport is built, so no `McpServerConfig` cap is in scope here.
pub(crate) const MAX_OAUTH_RESPONSE_BYTES: u64 = 1024 * 1024;

/// Read a response body into bytes, aborting once `limit` is exceeded, and map
/// the outcome onto this crate's error taxonomy.
///
/// The streaming read lives in [`apollia_core::net::read_capped_bytes`]; this
/// wrapper only translates its failures into an [`AuthError`].
pub(crate) async fn read_body_capped(
    response: reqwest::Response,
    limit: u64,
) -> Result<Vec<u8>, AuthError> {
    apollia_core::net::read_capped_bytes(response, limit)
        .await
        .map_err(|e| match e {
            apollia_core::net::ReadCappedError::TooLarge { limit, .. } => {
                AuthError::ResponseTooLarge(limit)
            }
            other => AuthError::HttpError(other.to_string()),
        })
}

/// Read and JSON-decode a response body, bounded by `limit`.
pub(crate) async fn read_json_capped<T: DeserializeOwned>(
    response: reqwest::Response,
    limit: u64,
) -> Result<T, AuthError> {
    let bytes = read_body_capped(response, limit).await?;
    serde_json::from_slice(&bytes).map_err(|e| AuthError::Serialization(e.to_string()))
}

/// Read a response body as a lossy UTF-8 string, bounded by `limit`.
pub(crate) async fn read_text_capped(
    response: reqwest::Response,
    limit: u64,
) -> Result<String, AuthError> {
    let bytes = read_body_capped(response, limit).await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

// ─── RFC 9728: Protected Resource Metadata ─────────────────────────────────

/// Protected Resource Metadata document (RFC 9728 §3).
///
/// Apollia consumes this to discover which authorization servers a given MCP
/// resource trusts.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProtectedResourceMetadata {
    /// Canonical URI of the resource (matches the MCP server's base URL).
    pub resource: String,
    /// One or more authorization server URLs that can issue tokens for this resource.
    pub authorization_servers: Vec<String>,
    /// Bearer presentation methods supported (`header`, `body`, `query`).
    #[serde(default)]
    pub bearer_methods_supported: Vec<String>,
    /// Scopes the resource supports.
    #[serde(default)]
    pub scopes_supported: Vec<String>,
    /// Optional human-readable name.
    #[serde(default)]
    pub resource_name: Option<String>,
}

/// Parse the `WWW-Authenticate: Bearer` header value, extracting the
/// `resource_metadata` URL and optional `scope`.
///
/// Returns `None` if the header is not a Bearer challenge or does not carry
/// the `resource_metadata` parameter. Tolerant to whitespace and quote variants.
pub fn parse_www_authenticate(value: &str) -> Option<WwwAuthenticate> {
    let trimmed = value.trim();
    if !trimmed.to_ascii_lowercase().starts_with("bearer") {
        return None;
    }
    let rest = trimmed["bearer".len()..].trim_start();
    let mut resource_metadata = None;
    let mut scope = None;
    for part in split_header_parameters(rest) {
        let (k, v) = part?;
        match k.to_ascii_lowercase().as_str() {
            "resource_metadata" => resource_metadata = Some(v),
            "scope" => scope = Some(v),
            _ => {}
        }
    }
    Some(WwwAuthenticate {
        resource_metadata,
        scope,
    })
}

/// Parsed view of an HTTP `WWW-Authenticate: Bearer …` header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WwwAuthenticate {
    /// URL to the Protected Resource Metadata document, when advertised.
    pub resource_metadata: Option<String>,
    /// Scope hint surfaced by the AS, when advertised.
    pub scope: Option<String>,
}

fn split_header_parameters(s: &str) -> impl Iterator<Item = Option<(String, String)>> + '_ {
    s.split(',').map(|chunk| {
        let chunk = chunk.trim();
        let eq = chunk.find('=')?;
        let key = chunk[..eq].trim().to_string();
        let raw_value = chunk[eq + 1..].trim();
        // Strip surrounding double quotes if present.
        let value =
            if raw_value.starts_with('"') && raw_value.ends_with('"') && raw_value.len() >= 2 {
                raw_value[1..raw_value.len() - 1].to_string()
            } else {
                raw_value.to_string()
            };
        Some((key, value))
    })
}

// ─── RFC 8414 / OIDC Discovery: Authorization Server Metadata ──────────────

/// Authorization Server metadata document subset (RFC 8414 + OIDC Discovery
/// overlap). Apollia only consumes the fields it needs for the code flow.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthorizationServerMetadata {
    /// Authorization endpoint URL.
    pub authorization_endpoint: String,
    /// Token endpoint URL.
    pub token_endpoint: String,
    /// Optional dynamic client registration endpoint (RFC 7591).
    #[serde(default)]
    pub registration_endpoint: Option<String>,
    /// Code challenge methods the AS supports; MUST include `S256`.
    #[serde(default)]
    pub code_challenge_methods_supported: Vec<String>,
    /// Optional revocation endpoint.
    #[serde(default)]
    pub revocation_endpoint: Option<String>,
    /// Issuer URL.
    #[serde(default)]
    pub issuer: Option<String>,
    /// Non-standard but widely-adopted (MCP spec) flag: the AS accepts CIMD
    /// URLs as `client_id`. When `true`, Apollia skips DCR entirely.
    #[serde(default)]
    pub client_id_metadata_document_supported: Option<bool>,
}

impl AuthorizationServerMetadata {
    /// True when the AS advertises PKCE S256 support.
    ///
    /// Apollia refuses to start a flow against an AS that does not support
    /// S256 (spec MUST). This check fails fast at discovery time rather than
    /// silently downgrading.
    pub fn supports_pkce_s256(&self) -> bool {
        self.code_challenge_methods_supported
            .iter()
            .any(|m| m.eq_ignore_ascii_case("S256"))
    }
}

/// Build the candidate well-known URLs to query for AS metadata, in priority
/// order.
///
/// Per MCP spec 2025-11-25 §authorization:
/// 1. `{issuer}/.well-known/oauth-authorization-server/<path>` (RFC 8414)
/// 2. `{issuer}/.well-known/openid-configuration/<path>` (OIDC Discovery)
/// 3. `<path>/.well-known/openid-configuration` (path-only OIDC fallback)
/// 4. `{issuer}/.well-known/oauth-authorization-server` (path-stripped)
/// 5. `{issuer}/.well-known/openid-configuration` (path-stripped)
pub fn as_metadata_candidates(issuer: &str) -> Vec<String> {
    let issuer = issuer.trim_end_matches('/');
    let (root, path) = split_issuer(issuer);
    let mut out = Vec::new();
    if !path.is_empty() {
        out.push(format!(
            "{root}/.well-known/oauth-authorization-server/{path}"
        ));
        out.push(format!("{root}/.well-known/openid-configuration/{path}"));
        out.push(format!("{root}/{path}/.well-known/openid-configuration"));
    }
    out.push(format!("{root}/.well-known/oauth-authorization-server"));
    out.push(format!("{root}/.well-known/openid-configuration"));
    out
}

fn split_issuer(issuer: &str) -> (String, String) {
    // Split scheme://host:port from any trailing path components.
    if let Some(idx) = issuer.find("://") {
        let after_scheme = &issuer[idx + 3..];
        if let Some(slash) = after_scheme.find('/') {
            let root = format!("{}{}", &issuer[..idx + 3], &after_scheme[..slash]);
            let path = after_scheme[slash + 1..].to_string();
            return (root, path);
        }
    }
    (issuer.to_string(), String::new())
}

// ─── Client ID Metadata Document (CIMD) ─────────────────────────────────────

/// Apollia's CIMD payload, hosted at
/// `https://apollia.fr/.well-known/mcp-client-metadata`.
///
/// The body is mostly static and shipped to Cloudflare Pages by
/// infrastructure; this struct lets us render it from build metadata if needed.
#[derive(Debug, Clone, Serialize)]
pub struct ClientIdMetadataDocument {
    /// Display name for the user consent screen.
    pub client_name: &'static str,
    /// Apollia marketing URL.
    pub client_uri: &'static str,
    /// Redirect URIs the AS may issue codes to. `127.0.0.1` host is mandatory
    /// for native clients per MCP spec; the port is replaced at runtime by
    /// the ephemeral callback listener (the wildcard form is allowed by RFC
    /// 8252 §7.3).
    pub redirect_uris: &'static [&'static str],
    /// Optional logo URL.
    pub logo_uri: &'static str,
    /// Optional ToS URL.
    pub tos_uri: &'static str,
    /// Optional privacy policy URL.
    pub policy_uri: &'static str,
    /// Stable software id.
    pub software_id: &'static str,
    /// Software version surfaced to the AS.
    pub software_version: &'static str,
    /// `token_endpoint_auth_method`: set to `"none"` for public clients.
    pub token_endpoint_auth_method: &'static str,
    /// Grant types the client supports.
    pub grant_types: &'static [&'static str],
    /// Response types the client supports.
    pub response_types: &'static [&'static str],
}

/// Canonical Apollia CIMD document.
///
/// Static; safe to share across goroutines / tasks.
pub const APOLLIA_CIMD: ClientIdMetadataDocument = ClientIdMetadataDocument {
    client_name: "Apollia OS",
    client_uri: "https://apollia.fr",
    redirect_uris: &["http://127.0.0.1/oauth/callback"],
    logo_uri: "https://apollia.fr/icon-512.png",
    tos_uri: "https://apollia.fr/mentions-legales",
    policy_uri: "https://apollia.fr/confidentialite",
    software_id: "io.apollia.os",
    software_version: env!("CARGO_PKG_VERSION"),
    token_endpoint_auth_method: "none",
    grant_types: &["authorization_code", "refresh_token"],
    response_types: &["code"],
};

/// Canonical hosted URL of Apollia's CIMD.
pub const APOLLIA_CIMD_URL: &str = "https://apollia.fr/.well-known/mcp-client-metadata";

// ─── RFC 8707: Resource Indicators helpers ─────────────────────────────────

/// Compute the canonical resource URI to send as `resource=` (RFC 8707 §2).
///
/// MCP servers identify themselves by their base URL; the canonical form
/// strips any trailing slash and percent-encodes nothing; we only normalise
/// trailing whitespace and slashes here.
pub fn canonical_resource_uri(server_url: &str) -> String {
    server_url.trim().trim_end_matches('/').to_string()
}

// ─── RFC 7591: Dynamic Client Registration request payload ─────────────────

/// Minimal DCR request body (RFC 7591 §2).
#[derive(Debug, Clone, Serialize)]
pub struct DcrRequest {
    /// Display name.
    pub client_name: &'static str,
    /// Redirect URIs (must match the URIs sent during the authorization flow).
    pub redirect_uris: Vec<String>,
    /// Public client: no client_secret.
    pub token_endpoint_auth_method: &'static str,
    /// Grant types we use.
    pub grant_types: &'static [&'static str],
    /// Response types we use.
    pub response_types: &'static [&'static str],
}

impl DcrRequest {
    /// Build a DCR request body with the runtime callback URI.
    pub fn for_callback(redirect_uri: &str) -> Self {
        Self {
            client_name: APOLLIA_CIMD.client_name,
            redirect_uris: vec![redirect_uri.to_string()],
            token_endpoint_auth_method: "none",
            grant_types: APOLLIA_CIMD.grant_types,
            response_types: APOLLIA_CIMD.response_types,
        }
    }
}

/// Response body returned by a successful DCR call (RFC 7591 §3.2.1).
#[derive(Debug, Clone, Deserialize)]
pub struct DcrResponse {
    /// Issued client identifier.
    pub client_id: String,
    /// Optional client_secret (always `None` for public clients).
    #[serde(default)]
    pub client_secret: Option<String>,
    /// Optional registration access token (RFC 7591 §3.2).
    #[serde(default)]
    pub registration_access_token: Option<String>,
}

// ─── Discovery client ────────────────────────────────────────────────────────

/// HTTP-bound discovery helper.
///
/// Used by callers to resolve the PRM and AS metadata documents for a given
/// MCP server URL. Stateless except for the underlying `reqwest::Client`.
#[derive(Clone)]
pub struct McpDiscoveryClient {
    http: reqwest::Client,
}

impl McpDiscoveryClient {
    /// Build a discovery client with the default reqwest configuration.
    pub fn new() -> Result<Self, AuthError> {
        // The authorisation server of an MCP endpoint follows the endpoint,
        // which the operator declares and which is often local.
        let http = apollia_core::net::configured_endpoint_client_builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| AuthError::HttpError(e.to_string()))?;
        Ok(Self { http })
    }

    /// Fetch the Protected Resource Metadata document for an MCP server.
    ///
    /// The well-known location is `<server>/.well-known/oauth-protected-resource[/<path>]`.
    /// Per RFC 9728 §3, when a `WWW-Authenticate` header points to a specific
    /// URL, callers should pass it via [`fetch_prm_at`](Self::fetch_prm_at)
    /// instead.
    pub async fn fetch_prm(
        &self,
        server_url: &str,
    ) -> Result<ProtectedResourceMetadata, AuthError> {
        let base = server_url.trim_end_matches('/');
        let url = format!("{base}/.well-known/oauth-protected-resource");
        self.fetch_prm_at(&url).await
    }

    /// Fetch the PRM document at a specific URL (typically the one carried by
    /// `WWW-Authenticate: resource_metadata=…`).
    pub async fn fetch_prm_at(&self, url: &str) -> Result<ProtectedResourceMetadata, AuthError> {
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| AuthError::HttpError(e.to_string()))?;
        if !response.status().is_success() {
            return Err(AuthError::ProviderError(format!(
                "PRM fetch returned {}",
                response.status()
            )));
        }
        read_json_capped::<ProtectedResourceMetadata>(response, MAX_OAUTH_RESPONSE_BYTES).await
    }

    /// Fetch AS metadata. Tries each candidate URL until one returns a 200.
    pub async fn fetch_as_metadata(
        &self,
        issuer: &str,
    ) -> Result<AuthorizationServerMetadata, AuthError> {
        let candidates = as_metadata_candidates(issuer);
        let mut last_err: Option<AuthError> = None;
        for url in &candidates {
            match self.http.get(url).send().await {
                Ok(r) if r.status().is_success() => {
                    match read_json_capped::<AuthorizationServerMetadata>(
                        r,
                        MAX_OAUTH_RESPONSE_BYTES,
                    )
                    .await
                    {
                        Ok(meta) => return Ok(meta),
                        Err(e) => last_err = Some(e),
                    }
                }
                Ok(_) => {} // Try the next candidate.
                Err(e) => last_err = Some(AuthError::HttpError(e.to_string())),
            }
        }
        Err(last_err.unwrap_or_else(|| {
            AuthError::ProviderError(format!(
                "no AS metadata found for issuer {issuer} (tried {} candidates)",
                candidates.len()
            ))
        }))
    }

    /// Register a new client with the AS via RFC 7591 DCR.
    pub async fn register_client(
        &self,
        registration_endpoint: &str,
        redirect_uri: &str,
    ) -> Result<DcrResponse, AuthError> {
        let body = DcrRequest::for_callback(redirect_uri);
        let response = self
            .http
            .post(registration_endpoint)
            .json(&body)
            .send()
            .await
            .map_err(|e| AuthError::HttpError(e.to_string()))?;
        if !response.status().is_success() {
            let status = response.status();
            let text = read_text_capped(response, MAX_OAUTH_RESPONSE_BYTES)
                .await
                .unwrap_or_else(|_| "<unreadable>".to_string());
            return Err(AuthError::ProviderError(format!(
                "DCR returned {status}: {text}"
            )));
        }
        read_json_capped::<DcrResponse>(response, MAX_OAUTH_RESPONSE_BYTES).await
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_www_authenticate_extracts_resource_metadata() {
        // GIVEN a Bearer challenge naming a resource metadata URL and a scope
        let header = r#"Bearer resource_metadata="https://example.com/.well-known/oauth-protected-resource", scope="read write""#;
        // WHEN the header is parsed
        let parsed = parse_www_authenticate(header).expect("parsed");
        // THEN both fields come back unquoted
        assert_eq!(
            parsed.resource_metadata.as_deref(),
            Some("https://example.com/.well-known/oauth-protected-resource")
        );
        assert_eq!(parsed.scope.as_deref(), Some("read write"));
    }

    #[test]
    fn test_parse_www_authenticate_returns_none_for_non_bearer() {
        // GIVEN a challenge announcing a scheme other than Bearer
        // WHEN the header is parsed
        // THEN nothing is parsed out of it
        assert!(parse_www_authenticate("Basic realm=foo").is_none());
    }

    #[test]
    fn test_parse_www_authenticate_handles_unquoted_values() {
        // GIVEN a Bearer challenge whose value carries no quotes
        let header = "Bearer resource_metadata=https://example.com/foo";
        // WHEN the header is parsed
        let parsed = parse_www_authenticate(header).expect("parsed");
        // THEN the value is read all the same
        assert_eq!(
            parsed.resource_metadata.as_deref(),
            Some("https://example.com/foo")
        );
    }

    #[test]
    fn test_parse_www_authenticate_case_insensitive_scheme() {
        // GIVEN a challenge whose scheme is spelled in lower case
        let header = "bearer resource_metadata=https://x";
        // WHEN the header is parsed
        // THEN it is recognised, the scheme being case insensitive
        assert!(parse_www_authenticate(header).is_some());
    }

    #[test]
    fn test_as_metadata_supports_pkce_s256_required() {
        // GIVEN server metadata announcing both plain and S256 challenges
        let meta = AuthorizationServerMetadata {
            authorization_endpoint: "x".into(),
            token_endpoint: "x".into(),
            registration_endpoint: None,
            code_challenge_methods_supported: vec!["plain".into(), "S256".into()],
            revocation_endpoint: None,
            issuer: None,
            client_id_metadata_document_supported: None,
        };
        // WHEN PKCE support is evaluated
        // THEN S256 is supported
        assert!(meta.supports_pkce_s256());
    }

    #[test]
    fn test_as_metadata_rejects_when_only_plain_supported() {
        // GIVEN server metadata announcing plain challenges only
        let meta = AuthorizationServerMetadata {
            authorization_endpoint: "x".into(),
            token_endpoint: "x".into(),
            registration_endpoint: None,
            code_challenge_methods_supported: vec!["plain".into()],
            revocation_endpoint: None,
            issuer: None,
            client_id_metadata_document_supported: None,
        };
        // WHEN PKCE support is evaluated
        // THEN S256 is not supported, so the flow must not start
        assert!(!meta.supports_pkce_s256());
    }

    #[test]
    fn test_as_metadata_candidates_includes_path_variants_when_path_present() {
        // GIVEN an issuer URL carrying a path
        // WHEN the metadata candidates are built
        let urls = as_metadata_candidates("https://example.com/foo/bar");
        // THEN the path-aware forms come first, the path-stripped ones after
        // Path-aware candidates first
        assert_eq!(
            urls[0],
            "https://example.com/.well-known/oauth-authorization-server/foo/bar"
        );
        assert_eq!(
            urls[1],
            "https://example.com/.well-known/openid-configuration/foo/bar"
        );
        assert_eq!(
            urls[2],
            "https://example.com/foo/bar/.well-known/openid-configuration"
        );
        // Then path-stripped fallbacks
        assert!(urls
            .contains(&"https://example.com/.well-known/oauth-authorization-server".to_string()));
        assert!(urls.contains(&"https://example.com/.well-known/openid-configuration".to_string()));
    }

    #[test]
    fn test_as_metadata_candidates_root_issuer_yields_two_urls() {
        // GIVEN an issuer URL with no path at all
        // WHEN the metadata candidates are built
        let urls = as_metadata_candidates("https://example.com");
        // THEN only the two well-known roots are tried
        assert_eq!(urls.len(), 2);
        assert!(urls
            .iter()
            .any(|u| u.ends_with("oauth-authorization-server")));
        assert!(urls.iter().any(|u| u.ends_with("openid-configuration")));
    }

    #[test]
    fn test_canonical_resource_uri_strips_trailing_slash() {
        // GIVEN resource URIs with a trailing slash, without one, and padded with spaces
        // WHEN each is canonicalised
        // THEN the trailing slash and the padding are gone, the rest untouched
        assert_eq!(
            canonical_resource_uri("https://x.com/api/"),
            "https://x.com/api"
        );
        assert_eq!(canonical_resource_uri("https://x.com"), "https://x.com");
        assert_eq!(
            canonical_resource_uri("  https://x.com/  "),
            "https://x.com"
        );
    }

    #[test]
    fn test_apollia_cimd_constants_are_stable() {
        // GIVEN the client identity document the runtime publishes
        // WHEN its fields are read
        // THEN the name, the auth method, the grant types and the response type are the declared ones
        assert_eq!(APOLLIA_CIMD.client_name, "Apollia OS");
        assert_eq!(APOLLIA_CIMD.token_endpoint_auth_method, "none");
        assert!(APOLLIA_CIMD.grant_types.contains(&"authorization_code"));
        assert!(APOLLIA_CIMD.grant_types.contains(&"refresh_token"));
        assert!(APOLLIA_CIMD.response_types.contains(&"code"));
    }

    #[test]
    fn test_dcr_request_for_callback_carries_runtime_redirect_uri() {
        // GIVEN the loopback callback URL the runtime listens on
        // WHEN a dynamic registration request is built for it
        let req = DcrRequest::for_callback("http://127.0.0.1:54321/oauth/callback");
        // THEN the request declares no client secret and carries that single redirect URI
        assert_eq!(req.token_endpoint_auth_method, "none");
        assert_eq!(req.redirect_uris.len(), 1);
        assert!(req.redirect_uris[0].contains("54321"));
    }

    // ── capped response reads ───────────────────────────────────────────────

    /// Bind a random local port, serve `app`, and return the bound address.
    async fn start_server(app: axum::Router) -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("TcpListener::bind must succeed in tests");
        let addr = listener.local_addr().expect("local_addr must be available");
        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("axum::serve must not fail");
        });
        addr
    }

    #[tokio::test]
    async fn test_read_body_capped_rejects_oversized() {
        // GIVEN a server returning a 4 KiB body, under a 1 KiB cap
        let app = axum::Router::new().route("/", axum::routing::get(|| async { "a".repeat(4096) }));
        let addr = start_server(app).await;
        let resp = reqwest::get(format!("http://{addr}/"))
            .await
            .expect("request must reach the test server");

        // WHEN the body is read with a 1 KiB cap
        let result = read_body_capped(resp, 1024).await;

        // THEN the read aborts with ResponseTooLarge instead of buffering it all
        assert!(
            matches!(result, Err(AuthError::ResponseTooLarge(1024))),
            "expected ResponseTooLarge, got {result:?}"
        );
    }

    #[tokio::test]
    async fn test_read_json_capped_ok_for_small_document() {
        // GIVEN a small PRM JSON document well under the cap
        let app = axum::Router::new().route(
            "/",
            axum::routing::get(|| async {
                (
                    [(reqwest::header::CONTENT_TYPE, "application/json")],
                    r#"{"resource":"https://x","authorization_servers":["https://as"]}"#,
                )
            }),
        );
        let addr = start_server(app).await;
        let resp = reqwest::get(format!("http://{addr}/"))
            .await
            .expect("request must reach the test server");

        // WHEN it is decoded with the production cap
        let prm: ProtectedResourceMetadata = read_json_capped(resp, MAX_OAUTH_RESPONSE_BYTES)
            .await
            .expect("small document must decode");

        // THEN the legitimate document deserializes unchanged
        assert_eq!(prm.authorization_servers, vec!["https://as"]);
    }
}
