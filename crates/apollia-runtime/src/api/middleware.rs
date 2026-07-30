//! Token-based authentication middleware for the TCP REST API.
//!
//! Implements [`TokenAuthLayer`], a Tower [`Layer`] that enforces Bearer token
//! authentication on incoming requests. Applied exclusively to the TCP listener;
//! the Unix socket listener is intentionally left unauthenticated (filesystem
//! permissions provide equivalent isolation).
//!
//! Token lifecycle:
//! - [`load_or_generate_token`] reads `<data_dir>/api-token` on startup.
//! - If the file is absent, 32 random bytes are generated, hex-encoded, and
//!   written with `0600` permissions before the server accepts connections.
//! - The token is compared in constant time via [`constant_time_eq`] to prevent
//!   timing-based token recovery.

use std::path::Path;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::http::header::AUTHORIZATION;
use axum::http::{Request, Response, StatusCode};
use axum::response::IntoResponse;
use futures::future::BoxFuture;
use tower::{Layer, Service};

// ─────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────

/// Errors returned by the token authentication middleware.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// The `Authorization` header is absent from the request.
    #[error("missing Authorization header")]
    MissingHeader,
    /// The `Authorization` header is present but does not match the expected token.
    #[error("invalid token")]
    InvalidToken,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response<Body> {
        let body = serde_json::json!({ "error": self.to_string() });
        (StatusCode::UNAUTHORIZED, axum::Json(body)).into_response()
    }
}

// ─────────────────────────────────────────────
// Token file management
// ─────────────────────────────────────────────

/// Errors that can occur when loading or generating the API token file.
#[derive(Debug, thiserror::Error)]
pub enum TokenFileError {
    /// I/O error creating the parent data directory before the first write.
    #[error("failed to create data directory: {0}")]
    CreateDir(#[source] std::io::Error),
    /// I/O error reading the existing token file.
    #[error("failed to read api-token file: {0}")]
    Read(#[source] std::io::Error),
    /// I/O error writing the newly generated token file.
    #[error("failed to write api-token file: {0}")]
    Write(#[source] std::io::Error),
    /// The file was read successfully but its contents are empty.
    #[error("api-token file is empty")]
    EmptyToken,
}

/// Loads the API token from `<data_dir>/api-token`, generating it if absent.
///
/// On first call (file absent):
/// - `<data_dir>` is created if it does not exist yet.
/// - 32 cryptographically random bytes are generated.
/// - They are hex-encoded into a 64-character ASCII string.
/// - The string is written to `<data_dir>/api-token` with permissions `0600`.
///
/// On subsequent calls:
/// - The file is read and its contents are trimmed of whitespace.
///
/// # Errors
///
/// Returns [`TokenFileError`] if the file cannot be read or written.
pub fn load_or_generate_token(data_dir: &Path) -> Result<String, TokenFileError> {
    let token_path = data_dir.join("api-token");
    if token_path.exists() {
        let raw = std::fs::read_to_string(&token_path).map_err(TokenFileError::Read)?;
        let token = raw.trim().to_owned();
        if token.is_empty() {
            return Err(TokenFileError::EmptyToken);
        }
        return Ok(token);
    }

    // On a clean machine the data directory does not exist yet: create it
    // before the first write so the very first `start` cannot fail on a
    // missing parent. create_dir_all is a no-op when the directory exists.
    std::fs::create_dir_all(data_dir).map_err(TokenFileError::CreateDir)?;

    let token = generate_hex_token();
    write_token_file(&token_path, &token)?;
    tracing::info!(path = %token_path.display(), "API token generated and written");
    Ok(token)
}

/// Generates a 64-character hex string from 32 random bytes.
fn generate_hex_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Writes the token to `path` with permissions `0600` on Unix systems.
fn write_token_file(path: &Path, token: &str) -> Result<(), TokenFileError> {
    std::fs::write(path, token).map_err(TokenFileError::Write)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(TokenFileError::Write)?;
    }

    Ok(())
}

// ─────────────────────────────────────────────
// Tower Layer + Service
// ─────────────────────────────────────────────

/// Tower [`Layer`] that wraps a service with Bearer token authentication.
///
/// Applied exclusively to the TCP listener. Requests arriving via the Unix socket
/// bypass this layer entirely.
#[derive(Clone)]
pub struct TokenAuthLayer {
    expected_token: String,
}

impl TokenAuthLayer {
    /// Creates a new `TokenAuthLayer` that accepts only requests bearing `token`.
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            expected_token: token.into(),
        }
    }
}

impl<S> Layer<S> for TokenAuthLayer {
    type Service = TokenAuthMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TokenAuthMiddleware {
            inner,
            expected_token: self.expected_token.clone(),
        }
    }
}

/// Tower [`Service`] that enforces Bearer token authentication on each request.
///
/// Produced by [`TokenAuthLayer`]. Cloneable so it can be shared across Tokio tasks.
#[derive(Clone)]
pub struct TokenAuthMiddleware<S> {
    inner: S,
    expected_token: String,
}

impl<S, B> Service<Request<B>> for TokenAuthMiddleware<S>
where
    S: Service<Request<B>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let expected = self.expected_token.clone();
        // Extract the token from the header *before* moving `req` into the future.
        let auth_result = extract_bearer_token(req.headers());
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let token = match auth_result {
                Err(e) => {
                    tracing::warn!(reason = %e, "TCP API auth rejected");
                    return Ok(e.into_response());
                }
                Ok(t) => t,
            };

            if !constant_time_eq::constant_time_eq(token.as_bytes(), expected.as_bytes()) {
                tracing::warn!("TCP API auth rejected: invalid token");
                return Ok(AuthError::InvalidToken.into_response());
            }

            inner.call(req).await
        })
    }
}

/// Extracts the Bearer token from the `Authorization` header.
///
/// Expects the exact format `Authorization: Bearer <token>`.
fn extract_bearer_token(headers: &axum::http::HeaderMap) -> Result<String, AuthError> {
    let header_value = headers.get(AUTHORIZATION).ok_or(AuthError::MissingHeader)?;

    let header_str = header_value.to_str().map_err(|_| AuthError::InvalidToken)?;

    header_str
        .strip_prefix("Bearer ")
        .map(str::to_owned)
        .ok_or(AuthError::InvalidToken)
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    const TOKEN: &str = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
    const WRONG_TOKEN: &str = "cafecafecafecafecafecafecafecafecafecafecafecafecafecafecafecafe";

    fn build_protected_router() -> Router {
        Router::new()
            .route("/ping", get(|| async { "pong" }))
            .layer(TokenAuthLayer::new(TOKEN))
    }

    fn build_open_router() -> Router {
        Router::new().route("/ping", get(|| async { "pong" }))
    }

    #[tokio::test]
    async fn test_tcp_request_without_auth_header_returns_401() {
        // GIVEN a router protected by TokenAuthLayer
        let app = build_protected_router();

        // WHEN a request is sent without an Authorization header
        let req = Request::builder().uri("/ping").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();

        // THEN 401 with body {"error": "missing Authorization header"}
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let body = axum::body::to_bytes(resp.into_body(), 256).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"], "missing Authorization header");
    }

    #[tokio::test]
    async fn test_tcp_request_with_wrong_token_returns_401() {
        // GIVEN a router protected by TokenAuthLayer
        let app = build_protected_router();

        // WHEN a request is sent with an incorrect token
        let req = Request::builder()
            .uri("/ping")
            .header(AUTHORIZATION, format!("Bearer {WRONG_TOKEN}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        // THEN 401 with body {"error": "invalid token"}
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let body = axum::body::to_bytes(resp.into_body(), 256).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"], "invalid token");
    }

    #[tokio::test]
    async fn test_tcp_request_with_correct_token_returns_200() {
        // GIVEN a router protected by TokenAuthLayer
        let app = build_protected_router();

        // WHEN a request is sent with the correct token
        let req = Request::builder()
            .uri("/ping")
            .header(AUTHORIZATION, format!("Bearer {TOKEN}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        // THEN 200 OK
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_unix_socket_request_without_token_succeeds() {
        // GIVEN a router without TokenAuthLayer (Unix socket path)
        let app = build_open_router();

        // WHEN a request is sent without an Authorization header
        let req = Request::builder().uri("/ping").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();

        // THEN 200 OK, no auth enforced on the Unix socket router
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_require_token_false_bypasses_auth() {
        // GIVEN a router built without TokenAuthLayer (require_token = false)
        let app = build_open_router();

        // WHEN a request is sent without any Authorization header
        let req = Request::builder().uri("/ping").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();

        // THEN 200 OK, no auth check performed
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_token_file_auto_generated_on_first_boot() {
        // GIVEN no api-token file exists in a fresh temp directory
        let dir = tempfile::tempdir().unwrap();

        // WHEN load_or_generate_token is called
        let token = load_or_generate_token(dir.path()).unwrap();

        // THEN a 64-character hex token is returned and the file exists
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(dir.path().join("api-token").exists());
    }

    #[tokio::test]
    async fn test_token_generated_when_data_dir_absent() {
        // GIVEN a data dir path whose parent does not exist yet (clean machine)
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join("missing-parent").join(".apollia");
        assert!(!data_dir.exists());

        // WHEN load_or_generate_token is called on the absent directory
        let token = load_or_generate_token(&data_dir).unwrap();

        // THEN the directory is created and the token file now exists
        assert_eq!(token.len(), 64);
        assert!(data_dir.exists());
        let token_path = data_dir.join("api-token");
        assert!(token_path.exists());

        // AND the token file keeps its 0600 permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&token_path).unwrap().permissions().mode() & 0o777;
            assert_eq!(
                mode, 0o600,
                "api-token file must have mode 0600, got {mode:o}"
            );
        }
    }

    #[tokio::test]
    async fn test_token_file_permissions_are_0600() {
        // GIVEN no api-token file exists
        let dir = tempfile::tempdir().unwrap();

        // WHEN a token is generated
        load_or_generate_token(dir.path()).unwrap();

        // THEN the file has mode 0600 (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::metadata(dir.path().join("api-token")).unwrap();
            let mode = meta.permissions().mode() & 0o777;
            assert_eq!(
                mode, 0o600,
                "api-token file must have mode 0600, got {mode:o}"
            );
        }
    }

    #[test]
    fn test_constant_time_comparison() {
        // GIVEN two equal and two unequal tokens
        let token_a = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let token_b = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let token_c = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

        // WHEN compared with constant_time_eq
        // THEN equal tokens match, unequal tokens do not
        assert!(constant_time_eq::constant_time_eq(
            token_a.as_bytes(),
            token_b.as_bytes()
        ));
        assert!(!constant_time_eq::constant_time_eq(
            token_a.as_bytes(),
            token_c.as_bytes()
        ));
    }
}
