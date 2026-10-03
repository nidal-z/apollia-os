//! Cross-origin requests on the compatible server.
//!
//! A web page served from another origin (a prototype on `localhost:3000`, a
//! file opened in the browser) may call the compatible server only if the
//! server says so: the browser first sends an `OPTIONS` preflight, and reads
//! the `Access-Control-*` headers of every answer. The `[server] cors` setting
//! turns those headers on; it is off by default, and the server then sends
//! none, so a browser refuses the call.
//!
//! The layer is written by hand on the pattern of
//! [`TokenAuthLayer`](crate::api::middleware::TokenAuthLayer) rather than taken
//! from a crate: what it does is a constant answer and one added header.
//!
//! It must sit above the token check of the compatible router, because a
//! browser never sends credentials on a preflight: an `OPTIONS` request is
//! answered here and never reaches what is beneath. The request that follows
//! the preflight is an ordinary request and passes through every layer. Only
//! the request log sits above it, so a preflight leaves its line like any other
//! request. [`with_cors`] is the one place that applies it.
//!
//! The native API never carries these headers: its router is assembled in
//! `src/api/` and this layer is not part of it.

use std::task::{Context, Poll};

use axum::body::Body;
use axum::http::header::{
    ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN,
    ACCESS_CONTROL_MAX_AGE,
};
use axum::http::{HeaderValue, Method, Request, Response, StatusCode};
use axum::Router;
use futures::future::BoxFuture;
use tower::{Layer, Service};

#[cfg(test)]
mod tests;

/// The origins allowed to call the server: any. The server listens on the
/// loopback interface, and every request still passes its authentication.
pub(crate) const ALLOWED_ORIGIN: &str = "*";

/// The methods the compatible endpoints answer to.
pub(crate) const ALLOWED_METHODS: &str = "GET, POST, OPTIONS";

/// The request headers a browser may send: the bearer of the OpenAI SDKs, the
/// key and version of the Anthropic SDKs, and the JSON body's type.
pub(crate) const ALLOWED_HEADERS: &str =
    "authorization, content-type, x-api-key, anthropic-version";

/// How long, in seconds, a browser may reuse a preflight answer.
const MAX_AGE_SECS: &str = "600";

/// Applies the cross-origin layer to `router` when `enabled`, outside every
/// layer the router already carries.
///
/// The compatible router is built first, the token check included, and handed
/// here; a layer added after this call sits outside it and sees the preflights,
/// which only the request log may do.
pub(crate) fn with_cors(router: Router, enabled: bool) -> Router {
    if enabled {
        router.layer(CorsLayer)
    } else {
        router
    }
}

/// Tower [`Layer`] that answers preflights and marks every answer with the
/// allowed origin.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CorsLayer;

impl<S> Layer<S> for CorsLayer {
    type Service = CorsMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        CorsMiddleware { inner }
    }
}

/// Tower [`Service`] produced by [`CorsLayer`].
#[derive(Debug, Clone)]
pub(crate) struct CorsMiddleware<S> {
    inner: S,
}

impl<S, B> Service<Request<B>> for CorsMiddleware<S>
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
        if req.method() == Method::OPTIONS {
            return Box::pin(async { Ok(preflight_answer()) });
        }
        let mut inner = self.inner.clone();
        Box::pin(async move {
            let mut response = inner.call(req).await?;
            response.headers_mut().insert(
                ACCESS_CONTROL_ALLOW_ORIGIN,
                HeaderValue::from_static(ALLOWED_ORIGIN),
            );
            Ok(response)
        })
    }
}

/// The answer to a preflight: no content, and what the browser may send next.
fn preflight_answer() -> Response<Body> {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::NO_CONTENT;
    let headers = response.headers_mut();
    headers.insert(
        ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static(ALLOWED_ORIGIN),
    );
    headers.insert(
        ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static(ALLOWED_METHODS),
    );
    headers.insert(
        ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static(ALLOWED_HEADERS),
    );
    headers.insert(
        ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static(MAX_AGE_SECS),
    );
    response
}
