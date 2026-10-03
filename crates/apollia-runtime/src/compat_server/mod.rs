//! The compatible server: a second listener of the runtime, separate from the
//! native API.
//!
//! The native API (`/api/v1/...`, port 7771 and the Unix socket) is the
//! runtime's own surface; the desktop and the CLI drive it. The compatible
//! server is the surface other applications of the machine call as they would
//! call OpenAI or Anthropic: its paths and bodies follow those formats
//! (`/v1/...`, `snake_case`), and it answers on its own port, 7772 by default,
//! on the loopback interface. Starting or stopping it never touches the native
//! API, so the desktop keeps its runtime whatever happens here.
//!
//! The server is off until someone turns it on. Its switch and its port are
//! the `[server] enabled` and `[server] port` settings of `apollia.toml`,
//! resolved through [`apollia_core::config::resolve_setting`]; starting and
//! stopping it writes `enabled`, so a runtime that restarts opens the listener
//! again if it was open.
//!
//! When `[server] require_auth` is on, every request must present a token of
//! the server (`apo_...`), created, listed and revoked through the native API
//! and kept by [`ServerTokensHandle`] as a SHA-256 in `system.db`; the
//! internal token of the desktop never opens this server.
//!
//! [`CompatServerHandle`] is the actor that owns the listener. The routes it
//! serves are assembled by [`compat_router`]; every path that router mounts is
//! listed on the public page of compatible endpoints, which
//! `scripts/check_compat_routes.py` holds.

mod actor;
mod auth;
mod router;
mod tokens;

#[cfg(test)]
mod auth_tests;
#[cfg(test)]
pub(crate) mod tests;

pub use actor::{CompatServerHandle, CompatServerStatus};
pub use router::compat_router;
pub use tokens::{CreatedServerToken, ServerTokensError, ServerTokensHandle, SERVER_TOKEN_PREFIX};

use apollia_core::config::SettingError;

/// Why the compatible server could not be started, stopped or read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CompatServerError {
    /// Another program already listens on the port.
    #[error("port {port} is already in use by another program")]
    PortInUse {
        /// The port asked for.
        port: u16,
    },
    /// The listener could not be opened for another reason.
    #[error("failed to listen on port {port}")]
    Bind {
        /// The port asked for.
        port: u16,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The `[server]` settings could not be read or written.
    #[error("failed to read or write the server settings")]
    Settings {
        /// The underlying error.
        #[from]
        source: SettingError,
    },
    /// The tokens of the server could not be counted.
    #[error(transparent)]
    Tokens(#[from] ServerTokensError),
    /// The actor that owns the listener has stopped.
    #[error("the compatible server has stopped with the runtime")]
    ActorGone,
}
