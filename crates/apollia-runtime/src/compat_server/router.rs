//! The routes of the compatible server, and the accept loop that serves them.
//!
//! Every path mounted here with `.route(...)` is a public endpoint, and
//! `scripts/check_compat_routes.py` compares them with the table of
//! `docs/site/docs/reference/compatible-endpoints.md`. No endpoint is served
//! yet: every request falls through to an OpenAI-format 404.

use axum::extract::connect_info::ConnectInfo;
use axum::http::{StatusCode, Uri};
use axum::{Json, Router};
use serde::Serialize;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use super::origin::CallOrigin;
use super::tokens::ServerTokensHandle;

/// The error envelope of the OpenAI API, which the compatible server answers
/// with whatever the client called.
#[derive(Debug, Serialize)]
pub(crate) struct OpenAiErrorBody {
    pub(crate) error: OpenAiError,
}

/// The inner object of [`OpenAiErrorBody`].
#[derive(Debug, Serialize)]
pub(crate) struct OpenAiError {
    /// What went wrong, for a person.
    pub(crate) message: String,
    /// The class of the error, as OpenAI names it (`invalid_request_error`).
    #[serde(rename = "type")]
    pub(crate) kind: &'static str,
    /// A stable identifier of the error.
    pub(crate) code: &'static str,
}

/// Builds the router of the compatible server.
///
/// With `tokens`, every request, the unknown paths included, must present a
/// token in use (see [`super::auth`]); without, the server answers anyone on
/// the loopback interface.
pub fn compat_router(tokens: Option<ServerTokensHandle>) -> Router {
    let router = Router::new().fallback(unknown_endpoint);
    match tokens {
        Some(tokens) => router.layer(axum::middleware::from_fn_with_state(
            tokens,
            super::auth::require_token,
        )),
        None => router,
    }
}

/// Answers a path no route serves, in the OpenAI error format.
async fn unknown_endpoint(uri: Uri) -> (StatusCode, Json<OpenAiErrorBody>) {
    (
        StatusCode::NOT_FOUND,
        Json(OpenAiErrorBody {
            error: OpenAiError {
                message: format!("Unknown endpoint: {}", uri.path()),
                kind: "invalid_request_error",
                code: "unknown_url",
            },
        }),
    )
}

/// Serves `router` on `listener` until `cancel` fires.
///
/// Every request carries the address of its peer (`ConnectInfo`) and the
/// [`CallOrigin`] read from it once, when the connection is accepted.
///
/// When it fires, the listener is dropped, so the port refuses any new
/// connection, and the connections still open are closed.
pub(super) async fn serve(listener: TcpListener, router: Router, cancel: CancellationToken) {
    use hyper_util::rt::{TokioExecutor, TokioIo};
    use hyper_util::server::conn::auto::Builder as ServerBuilder;
    use hyper_util::service::TowerToHyperService;
    use tower::ServiceExt;

    let builder = ServerBuilder::new(TokioExecutor::new());
    loop {
        tokio::select! {
            biased;
            () = cancel.cancelled() => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, peer)) => {
                    let origin = CallOrigin::of_peer(peer);
                    let service = TowerToHyperService::new(router.clone().map_request(
                        move |mut request: axum::http::Request<_>| {
                            request.extensions_mut().insert(ConnectInfo(peer));
                            request.extensions_mut().insert(origin);
                            request
                        },
                    ));
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
