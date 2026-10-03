//! The routes of the compatible server, and the accept loop that serves them.
//!
//! Every path mounted here with `.route(...)` is a public endpoint, and
//! `scripts/check_compat_routes.py` compares them with the table of
//! `docs/site/docs/reference/compatible-endpoints.md`. Any other path answers
//! an OpenAI-format 404.

use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::http::Uri;
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use super::errors::{error_response, ErrorKind, Family};
use super::relay::{self, RelayState};
use super::tokens::ServerTokensHandle;

/// Largest request body the server reads: a chat request carrying images
/// inline is the largest a client sends.
const MAX_REQUEST_BODY: usize = 32 * 1024 * 1024;

/// Builds the router of the compatible server over the local models `relay`
/// reaches.
///
/// With `tokens`, every request, the unknown paths included, must present a
/// token in use (see [`super::auth`]); without, the server answers anyone on
/// the loopback interface.
pub(crate) fn compat_router(tokens: Option<ServerTokensHandle>, relay: Arc<RelayState>) -> Router {
    let router = Router::new()
        .route("/v1/models", get(relay::list_models))
        .route("/v1/chat/completions", post(relay::chat_completions))
        .route("/v1/responses", post(relay::responses))
        .route("/v1/embeddings", post(relay::embeddings))
        .route("/v1/messages", post(relay::messages))
        .route("/v1/messages/count_tokens", post(relay::count_tokens))
        .fallback(unknown_endpoint)
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY))
        .with_state(relay);
    match tokens {
        Some(tokens) => router.layer(axum::middleware::from_fn_with_state(
            tokens,
            super::auth::require_token,
        )),
        None => router,
    }
}

/// Answers a path no route serves, in the OpenAI error format.
async fn unknown_endpoint(uri: Uri) -> Response {
    error_response(
        Family::OpenAi,
        ErrorKind::UnknownEndpoint,
        format!("Unknown endpoint: {}", uri.path()),
    )
}

/// Serves `router` on `listener` until `cancel` fires.
///
/// When it fires, the listener is dropped, so the port refuses any new
/// connection, and the connections still open are closed.
pub(super) async fn serve(listener: TcpListener, router: Router, cancel: CancellationToken) {
    use hyper_util::rt::{TokioExecutor, TokioIo};
    use hyper_util::server::conn::auto::Builder as ServerBuilder;
    use hyper_util::service::TowerToHyperService;

    let builder = ServerBuilder::new(TokioExecutor::new());
    loop {
        tokio::select! {
            biased;
            () = cancel.cancelled() => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, _addr)) => {
                    let service = TowerToHyperService::new(router.clone());
                    let builder = builder.clone();
                    let cancel = cancel.clone();
                    tokio::spawn(async move {
                        let connection = builder.serve_connection(TokioIo::new(stream), service);
                        tokio::select! {
                            result = connection => {
                                if let Err(error) = result {
                                    tracing::event!(
                                        tracing::Level::DEBUG,
                                        error = %error,
                                        "compat_server.connection.failed"
                                    );
                                }
                            }
                            () = cancel.cancelled() => {}
                        }
                    });
                }
                Err(error) => {
                    tracing::event!(
                        tracing::Level::WARN,
                        error = %error,
                        "compat_server.accept.failed"
                    );
                }
            },
        }
    }
}
