//! The decision the desktop takes before anything else: start a runtime in
//! this process, attach to one already serving, or refuse with a reason.
//!
//! The runtime can run inside this process or as a separate daemon
//! (`apollia-os start`), and both claim the same TCP port. The decision is the
//! claim itself: binding the port is what establishes that this process
//! starts the runtime, and the bound listener is handed to the runtime as is.
//! A probe followed by a bind would leave a window between the two in which
//! another process takes the port, and the loser of that race used to be the
//! process that removed the winner's Unix socket on its way to failing.
//!
//! Only when the port is held does the desktop ask what holds it, over
//! `GET /api/v1/health`, whose body carries a product marker and a version.
//! It is taken before PyO3, before any database and before any listener of the
//! runtime, so a refusal leaves nothing behind.

mod refusal;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::time::Duration;

pub(crate) use refusal::refuse;

/// The TCP port the runtime serves, whichever process runs it.
pub(crate) const RUNTIME_PORT: u16 = 7771;

/// How long the holder of the port has to answer the health request.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Upper bound on the health answer read from an unknown listener.
const PROBE_MAX_BYTES: u64 = 64 * 1024;

/// What the desktop does at startup.
#[derive(Debug)]
pub(crate) enum StartupDecision {
    /// Nothing held the port. This process holds it now, and the runtime it
    /// starts serves this very listener.
    Launch(TcpListener),
    /// A compatible Apollia runtime already serves the port.
    Attach {
        /// The version the serving runtime reports.
        version: String,
    },
    /// Neither: the port cannot be used and the reason names why.
    Refuse(RefusalReason),
}

/// Why the desktop does not start.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub(crate) enum RefusalReason {
    /// An Apollia runtime serves the port in a version this application
    /// cannot drive.
    #[error("an Apollia runtime in version {version} serves port {port}, and this application in version {ours} cannot drive it")]
    IncompatibleRuntime {
        /// The port the runtime serves.
        port: u16,
        /// The version it reports.
        version: String,
        /// The version of this application.
        ours: &'static str,
    },
    /// The holder of the port refused the token of this installation.
    #[error("the listener on port {port} refused the token of this installation")]
    TokenRefused {
        /// The port held.
        port: u16,
    },
    /// The holder of the port did not answer as an Apollia runtime.
    #[error("port {port} is held by a listener that does not answer as an Apollia runtime")]
    ForeignListener {
        /// The port held.
        port: u16,
    },
    /// The port could not be bound for a reason other than being held.
    #[error("port {port} cannot be bound")]
    PortUnavailable {
        /// The port asked for.
        port: u16,
        /// What the operating system answered.
        #[source]
        source: std::io::Error,
    },
}

/// What answered on a held port.
#[derive(Debug, PartialEq, Eq)]
enum Holder {
    /// An Apollia runtime, in the version it reports.
    Apollia { version: String },
    /// A listener that asked for a token and refused the one sent.
    TokenRefused,
    /// Anything else, including a listener that did not answer in time.
    Foreign,
}

/// Takes the startup decision for `addr`.
///
/// `token` is the bearer token of this installation, sent with the health
/// request so a runtime serving the same data directory can answer it.
pub(crate) fn decide(addr: SocketAddr, token: Option<&str>) -> StartupDecision {
    let port = addr.port();
    let error = match TcpListener::bind(addr) {
        Ok(listener) => return StartupDecision::Launch(listener),
        Err(error) => error,
    };
    if error.kind() != std::io::ErrorKind::AddrInUse {
        return StartupDecision::Refuse(RefusalReason::PortUnavailable {
            port,
            source: error,
        });
    }
    match identify(addr, token) {
        Holder::Apollia { version } if compatible(&version, env!("CARGO_PKG_VERSION")) => {
            StartupDecision::Attach { version }
        }
        Holder::Apollia { version } => {
            StartupDecision::Refuse(RefusalReason::IncompatibleRuntime {
                port,
                version,
                ours: env!("CARGO_PKG_VERSION"),
            })
        }
        Holder::TokenRefused => StartupDecision::Refuse(RefusalReason::TokenRefused { port }),
        Holder::Foreign => StartupDecision::Refuse(RefusalReason::ForeignListener { port }),
    }
}

/// Reads `<data_dir>/api-token` without creating it.
///
/// A process that has not decided to start the runtime must not generate a
/// token: a token the serving runtime does not hold would only earn a 401.
pub(crate) fn read_token(data_dir: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(data_dir.join("api-token")).ok()?;
    let token = raw.trim();
    (!token.is_empty()).then(|| token.to_owned())
}

/// Two versions are compatible when they share their major and minor numbers.
fn compatible(theirs: &str, ours: &str) -> bool {
    fn major_minor(version: &str) -> Option<(&str, &str)> {
        let mut parts = version.split(['.', '-', '+']);
        Some((parts.next()?, parts.next()?))
    }
    match (major_minor(theirs), major_minor(ours)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Asks the holder of `addr` what it is, over `GET /api/v1/health`.
fn identify(addr: SocketAddr, token: Option<&str>) -> Holder {
    match fetch_health(addr, token) {
        Ok(answer) => classify(&answer),
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                port = addr.port(),
                error = %error,
                "desktop.startup.probe_failed"
            );
            Holder::Foreign
        }
    }
}

/// Sends the health request and returns the raw answer.
fn fetch_health(addr: SocketAddr, token: Option<&str>) -> std::io::Result<Vec<u8>> {
    let mut stream = TcpStream::connect_timeout(&addr, PROBE_TIMEOUT)?;
    stream.set_read_timeout(Some(PROBE_TIMEOUT))?;
    stream.set_write_timeout(Some(PROBE_TIMEOUT))?;
    let authorization = token
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "GET /api/v1/health HTTP/1.1\r\nHost: {addr}\r\n{authorization}Connection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes())?;
    let mut answer = Vec::new();
    stream.take(PROBE_MAX_BYTES).read_to_end(&mut answer)?;
    Ok(answer)
}

/// Reads an HTTP answer to the health request.
fn classify(answer: &[u8]) -> Holder {
    let text = String::from_utf8_lossy(answer);
    let Some((head, body)) = text.split_once("\r\n\r\n") else {
        return Holder::Foreign;
    };
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1));
    match status {
        Some("401") => Holder::TokenRefused,
        Some("200") => {
            let json = match (body.find('{'), body.rfind('}')) {
                (Some(start), Some(end)) if start < end => &body[start..=end],
                _ => return Holder::Foreign,
            };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
                return Holder::Foreign;
            };
            let product = value.get("product").and_then(serde_json::Value::as_str);
            let version = value.get("version").and_then(serde_json::Value::as_str);
            match (product, version) {
                (Some(apollia_runtime::api::PRODUCT_MARKER), Some(version)) => Holder::Apollia {
                    version: version.to_owned(),
                },
                _ => Holder::Foreign,
            }
        }
        _ => Holder::Foreign,
    }
}

#[cfg(test)]
mod tests;
