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
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiConfig {
    /// IP address to bind the TCP listener to.
    ///
    /// Default: `"127.0.0.1"`, loopback only, unreachable from the network.
    #[serde(default = "default_api_bind")]
    pub bind: String,

    /// TCP port of the REST server.
    ///
    /// Default: `7771`.
    #[serde(default = "default_api_port")]
    pub port: u16,

    /// Require a Bearer token on every inbound TCP connection.
    ///
    /// When `true` (default), each TCP request must carry a valid
    /// `Authorization: Bearer <token>` header. Requests without a header or with
    /// an invalid token get `401 Unauthorized`.
    /// The Unix socket is never subject to this check.
    #[serde(default = "default_require_token")]
    pub require_token: bool,

    /// Local Unix socket path.
    ///
    /// Used by the CLI and the desktop app to talk to the runtime without
    /// authentication (local access only).
    /// Default: `~/.apollia/runtime.sock`. The parent directory must exist.
    #[serde(default = "default_unix_socket")]
    pub unix_socket: PathBuf,

    /// PEM certificate chain for native TLS on the TCP listener.
    ///
    /// When both `tls_cert` and `tls_key` are set, the TCP listener terminates
    /// TLS itself. When both are absent (the default), the listener stays
    /// cleartext, unchanged from prior behavior. Setting exactly one of the pair
    /// is a startup configuration error. The Unix socket is never affected.
    #[serde(default)]
    pub tls_cert: Option<PathBuf>,

    /// PEM private key matching [`tls_cert`](Self::tls_cert).
    ///
    /// See [`tls_cert`](Self::tls_cert) for the both-or-neither rule.
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
