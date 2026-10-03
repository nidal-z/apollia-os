//! The token check of the compatible server, when `[server] require_auth`
//! asks for one.
//!
//! A client presents its token the way its own API expects: OpenAI clients in
//! `Authorization: Bearer <token>`, Anthropic clients in `x-api-key: <token>`.
//! Either header is read. A request without a token in use answers 401 in the
//! error format of the endpoint it called, Anthropic's for `/v1/messages`,
//! OpenAI's for every other path, so the client shows its usual message.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;

use super::errors::{error_response, ErrorKind, Family};
use super::tokens::ServerTokensHandle;

const REFUSAL: &str = "Missing or invalid token. Create one in Apollia and send it in the \
                       Authorization header as a Bearer token, or in the x-api-key header.";

/// The token a request presents: the `Authorization: Bearer` value first,
/// then `x-api-key`.
fn presented_token(headers: &HeaderMap) -> Option<String> {
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let api_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty());
    bearer.or(api_key).map(str::to_owned)
}

/// The 401 answer, in the format of the endpoint `path` names.
fn unauthorized(path: &str) -> Response {
    error_response(
        Family::of_path(path),
        ErrorKind::Unauthorized,
        REFUSAL.to_owned(),
    )
}

/// Lets through a request that presents a token in use, and answers 401 to
/// any other.
pub(super) async fn require_token(
    State(tokens): State<ServerTokensHandle>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let path = request.uri().path().to_owned();
    let accepted = match presented_token(request.headers()) {
        Some(token) => tokens.verify(token).await,
        None => false,
    };
    if !accepted {
        tracing::event!(
            tracing::Level::WARN,
            path = %path,
            "compat_server.auth.rejected"
        );
        return unauthorized(&path);
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_bearer_header_wins_and_the_api_key_header_is_read_alone() {
        // GIVEN headers carrying both, and headers carrying x-api-key alone
        let mut both = HeaderMap::new();
        both.insert("authorization", "Bearer apo_first".parse().unwrap());
        both.insert("x-api-key", "apo_second".parse().unwrap());
        let mut key_only = HeaderMap::new();
        key_only.insert("x-api-key", "apo_second".parse().unwrap());

        // WHEN the presented token is read from each
        let (from_both, from_key) = (presented_token(&both), presented_token(&key_only));

        // THEN the bearer is taken first, and x-api-key on its own
        assert_eq!(from_both.as_deref(), Some("apo_first"));
        assert_eq!(from_key.as_deref(), Some("apo_second"));
    }
}
