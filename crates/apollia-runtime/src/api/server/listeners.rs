//! The two accept loops behind the server, TCP (optionally TLS) and Unix.
//!
//! axum 0.7 serves a bare `TcpListener` natively, which can terminate neither
//! TLS nor a Unix socket, so both loops are manual `hyper-util` accepts driving
//! the same router.

use std::path::Path;

use axum::Router;
use tokio::net::TcpListener;
#[cfg(unix)]
use tokio::net::UnixListener;
use tokio::sync::watch;

use crate::api::server::APIServerError;

/// Return `true` when `bind_addr` denotes a loopback interface.
///
/// `localhost` and any address parsing as a loopback IP (`127.0.0.0/8`, `::1`)
/// are loopback. Anything else, including an unparseable host, is treated as
/// non-loopback so a token is required (fail-fast).
pub(super) fn is_loopback_addr(bind_addr: &str) -> bool {
    if bind_addr.eq_ignore_ascii_case("localhost") {
        return true;
    }
    bind_addr
        .parse::<std::net::IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

/// Build a [`tokio_rustls::TlsAcceptor`] from PEM certificate and key paths.
///
/// Uses the ring crypto provider (already vendored) and the PEM helpers from
/// `rustls-pki-types`, so no additional PEM crate is required. Any IO or parse
/// failure is a fail-fast [`APIServerError::TlsConfigLoad`].
pub(super) fn build_tls_acceptor(
    cert_path: &Path,
    key_path: &Path,
) -> Result<tokio_rustls::TlsAcceptor, APIServerError> {
    use tokio_rustls::rustls::pki_types::pem::PemObject;
    use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
    use tokio_rustls::rustls::ServerConfig;

    let cert_err = |e: &dyn std::fmt::Display| APIServerError::TlsConfigLoad {
        path: cert_path.display().to_string(),
        reason: e.to_string(),
    };

    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert_path)
        .map_err(|e| cert_err(&e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| cert_err(&e))?;

    let key =
        PrivateKeyDer::from_pem_file(key_path).map_err(|e| APIServerError::TlsConfigLoad {
            path: key_path.display().to_string(),
            reason: e.to_string(),
        })?;

    let provider = std::sync::Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
    let server_config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| cert_err(&e))?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| cert_err(&e))?;

    Ok(tokio_rustls::TlsAcceptor::from(std::sync::Arc::new(
        server_config,
    )))
}

/// Log a per-connection serving error, downgrading benign client-close noise.
pub(super) fn log_tcp_conn_error<E: std::fmt::Display>(e: &E) {
    let msg = e.to_string();
    if msg.contains("shut") || msg.contains("broken pipe") || msg.contains("connection reset") {
        tracing::debug!(error = %e, "api.tcp.connection.closed");
    } else {
        tracing::error!(error = %e, "api.tcp.connection.failed");
    }
}

/// Serve HTTP requests over a TCP listener using hyper-util, optionally
/// terminating TLS.
///
/// Mirrors [`serve_unix`]: an accept loop hands each connection to hyper via
/// `TokioIo` + `TowerToHyperService`. When `tls_acceptor` is `Some`, each
/// accepted stream completes a TLS handshake before being served, and a failed
/// handshake drops that connection only. `None` serves cleartext, identical to
/// the prior `axum::serve` behavior.
pub(super) async fn serve_tcp(
    listener: TcpListener,
    router: Router,
    tls_acceptor: Option<tokio_rustls::TlsAcceptor>,
    shutdown_rx: &mut watch::Receiver<bool>,
) {
    use hyper_util::rt::TokioIo;
    use hyper_util::server::conn::auto::Builder as ServerBuilder;
    use hyper_util::service::TowerToHyperService;

    let builder = ServerBuilder::new(hyper_util::rt::TokioExecutor::new());

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, _addr)) => {
                        let svc = TowerToHyperService::new(router.clone());
                        let conn_builder = builder.clone();
                        let acceptor = tls_acceptor.clone();
                        tokio::spawn(async move {
                            match acceptor {
                                Some(acceptor) => {
                                    let tls_stream = match acceptor.accept(stream).await {
                                        Ok(s) => s,
                                        Err(e) => {
                                            tracing::debug!(
                                                error = %e,
                                                "api.tcp.tls.handshake.failed"
                                            );
                                            return;
                                        }
                                    };
                                    let io = TokioIo::new(tls_stream);
                                    if let Err(e) = conn_builder.serve_connection(io, svc).await {
                                        log_tcp_conn_error(&e);
                                    }
                                }
                                None => {
                                    let io = TokioIo::new(stream);
                                    if let Err(e) = conn_builder.serve_connection(io, svc).await {
                                        log_tcp_conn_error(&e);
                                    }
                                }
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "api.tcp.accept.failed");
                    }
                }
            }
            _ = shutdown_rx.wait_for(|v| *v) => break,
        }
    }
}

/// Serve HTTP requests over a Unix domain socket using hyper-util.
///
/// Runs an accept loop that converts each incoming `UnixStream` into
/// a hyper connection via `TokioIo`, then dispatches to the axum router
/// through `TowerToHyperService`.
#[cfg(unix)]
pub(super) async fn serve_unix(
    listener: UnixListener,
    router: Router,
    shutdown_rx: &mut watch::Receiver<bool>,
) {
    use hyper_util::rt::TokioIo;
    use hyper_util::server::conn::auto::Builder as ServerBuilder;
    use hyper_util::service::TowerToHyperService;

    let builder = ServerBuilder::new(hyper_util::rt::TokioExecutor::new());

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, _addr)) => {
                        let io = TokioIo::new(stream);
                        let svc = TowerToHyperService::new(router.clone());
                        let conn_builder = builder.clone();
                        tokio::spawn(async move {
                            if let Err(e) = conn_builder.serve_connection(io, svc).await {
                                // "error shutting down connection" is benign, the CLI client
                                // closed its end before hyper completed the graceful shutdown.
                                // Note: hyper emits "shutting down" (not "shutdown") in this message,
                                // so we match on "shut" to cover both variants.
                                let msg = e.to_string();
                                if msg.contains("shut") || msg.contains("broken pipe") || msg.contains("connection reset") {
                                    tracing::debug!(error = %e, "api.unix.connection.closed");
                                } else {
                                    tracing::error!(error = %e, "api.unix.connection.failed");
                                }
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "api.unix.accept.failed");
                    }
                }
            }
            _ = shutdown_rx.wait_for(|v| *v) => break,
        }
    }
}
