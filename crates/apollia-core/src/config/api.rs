use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::ConfigError;

// ─────────────────────────────────────────────
// ApiConfig
// ─────────────────────────────────────────────

/// Local REST API configuration (`[api]` section in `apollia.toml`).
///
/// Controls TCP binding, static token authentication, and the local Unix
/// socket path. The Unix socket stays unauthenticated: only the owner of the
/// socket file can access it.
///
/// **Two loaders read this section, and they read different halves of it.** The
/// daemon started by `apollia-os start` builds its listener from `bind`,
/// `require_token`, `tls_cert` and `tls_key`. The runtime embedded in the
/// desktop application reads `unix_socket` and nothing else: it binds
/// `127.0.0.1:7771` with a token, always, whatever the file says. No key is read
/// by both, and `port` is read by neither. Every field below names its own
/// readers, because a type that states what a key means says nothing about
/// whether anything consults it.
///
/// A key a loader ignores is still parsed and validated, then dropped. Setting
/// one is silent, not an error.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiConfig {
    /// IP address to bind the TCP listener to.
    ///
    /// Default: `"127.0.0.1"`, loopback only, unreachable from the network.
    ///
    /// Read by the daemon only. The embedded runtime hardcodes `127.0.0.1`
    /// (`apollia-runtime/src/embedded.rs`, the `APIServerConfig` it builds), so
    /// a desktop installation never opens its listener beyond loopback.
    #[serde(default = "default_api_bind")]
    pub bind: String,

    /// TCP port of the REST server.
    ///
    /// Default: `7771`.
    ///
    /// Read by neither loader. Measured across the tree, the only readers of
    /// an [`ApiConfig`] are the daemon start path and the embedded loader, and
    /// neither touches this field. The daemon takes its port from
    /// `apollia-os start --port` and falls back to `7771` when the flag is
    /// absent, so a file that sets `port = 8080` still gets `7771`; the desktop
    /// application passes `7771` in code. The field is kept because an existing
    /// `apollia.toml` carrying the key must parse; wiring it would change the
    /// port a deployed file already resolves to, and removing it is a change to
    /// the public surface of `apollia-core`.
    #[serde(default = "default_api_port")]
    pub port: u16,

    /// Require a Bearer token on every inbound TCP connection.
    ///
    /// When `true` (default), each TCP request must carry a valid
    /// `Authorization: Bearer <token>` header. Requests without a header or with
    /// an invalid token get `401 Unauthorized`.
    /// The Unix socket is never subject to this check.
    ///
    /// Read by the daemon only. The embedded runtime always loads or generates
    /// the token and always installs the layer on its TCP listener, so
    /// `require_token = false` does not disarm a desktop installation.
    #[serde(default = "default_require_token")]
    pub require_token: bool,

    /// Local Unix socket path.
    ///
    /// Used by the CLI and the desktop app to talk to the runtime without
    /// authentication (local access only).
    /// Default: `~/.apollia/runtime.sock`. The parent directory must exist.
    ///
    /// Read by the embedded runtime only. The daemon takes its socket from
    /// `apollia-os start --socket` and falls back to the same default, so a
    /// file that moves the socket moves it for the desktop application alone.
    #[serde(default = "default_unix_socket")]
    pub unix_socket: PathBuf,

    /// PEM certificate chain for native TLS on the TCP listener.
    ///
    /// When both `tls_cert` and `tls_key` are set, the TCP listener terminates
    /// TLS itself. When both are absent (the default), the listener stays
    /// cleartext, unchanged from prior behavior. Setting exactly one of the pair
    /// is a startup configuration error. The Unix socket is never affected.
    ///
    /// Read by the daemon only. The embedded runtime is loopback-only and never
    /// terminates TLS, so the pair is inert in the desktop application.
    #[serde(default)]
    pub tls_cert: Option<PathBuf>,

    /// PEM private key matching [`tls_cert`](Self::tls_cert).
    ///
    /// See [`tls_cert`](Self::tls_cert) for the both-or-neither rule.
    ///
    /// Read by the daemon only, like [`tls_cert`](Self::tls_cert).
    #[serde(default)]
    pub tls_key: Option<PathBuf>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            bind: default_api_bind(),
            port: default_api_port(),
            require_token: default_require_token(),
            unix_socket: default_unix_socket(),
            tls_cert: None,
            tls_key: None,
        }
    }
}

impl ApiConfig {
    /// Validates the API configuration at startup (fail-fast).
    ///
    /// Checks that the parent directory of the Unix socket exists (a Unix socket
    /// whose parent directory is missing cannot be bound) and that the TLS
    /// certificate and key are set together or not at all.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let parent = self.unix_socket.parent().unwrap_or_else(|| {
            // Fallback to root, which is always accessible.
            std::path::Path::new("/")
        });
        if !parent.exists() {
            return Err(ConfigError::SocketParentMissing {
                path: self.unix_socket.display().to_string(),
            });
        }
        if self.tls_cert.is_some() != self.tls_key.is_some() {
            return Err(ConfigError::InvalidValue {
                field: "api.tls".to_owned(),
                reason: "tls_cert and tls_key must both be set or both be absent".to_owned(),
            });
        }
        Ok(())
    }
}

fn default_api_bind() -> String {
    "127.0.0.1".to_owned()
}

fn default_api_port() -> u16 {
    7771
}

fn default_require_token() -> bool {
    true
}

fn default_unix_socket() -> PathBuf {
    crate::paths::socket_path_or_temp()
}
