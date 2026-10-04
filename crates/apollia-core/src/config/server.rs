use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
// ServerConfig
// ─────────────────────────────────────────────

/// Port the compatible server listens on when `apollia.toml` names none.
///
/// Distinct from the native API port (7771), which the compatible server never
/// shares.
pub const DEFAULT_COMPAT_SERVER_PORT: u16 = 7772;

/// Dotted key of the switch that lets a request load the model it names.
pub const JIT_LOADING_KEY: &str = "server.jit_loading";

/// Dotted key of the switch that gives the models the server loaded their own
/// idle delay.
pub const JIT_AUTO_UNLOAD_KEY: &str = "server.jit_auto_unload";

/// Dotted key of the idle delay of the models the server loaded.
pub const JIT_IDLE_TTL_KEY: &str = "server.jit_idle_ttl_minutes";

/// Dotted key of the switch that keeps only the last model the server loaded.
pub const JIT_KEEP_LAST_ONLY_KEY: &str = "server.jit_keep_last_only";

/// Minutes a model the server loaded stays loaded unused, when the file names
/// no delay.
pub const DEFAULT_JIT_IDLE_TTL_MINUTES: u64 = 60;

fn default_port() -> u16 {
    DEFAULT_COMPAT_SERVER_PORT
}

fn default_true() -> bool {
    true
}

fn default_jit_idle_ttl_minutes() -> u64 {
    DEFAULT_JIT_IDLE_TTL_MINUTES
}

/// The compatible server (`[server]` section in `apollia.toml`).
///
/// A second listener of the runtime, separate from the native API: it serves
/// the endpoints that follow the OpenAI and Anthropic formats, on its own port,
/// and is switched off until someone turns it on. Every field is a named
/// setting, resolved through [`resolve_setting`](super::resolve_setting) under
/// `server.<field>`; starting and stopping the server writes `server.enabled`,
/// so its state survives a restart of the runtime.
///
/// Unknown keys are refused, so a typo fails instead of writing a value nothing
/// reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    /// Whether the compatible server listens. Off by default.
    #[serde(default)]
    pub enabled: bool,
    /// TCP port of the compatible server.
    #[serde(default = "default_port")]
    pub port: u16,
    /// Whether a request must carry a token of the server. Off by default.
    /// Read when the listener opens, so stopping and starting the server
    /// applies a change.
    #[serde(default)]
    pub require_auth: bool,
    /// Whether a request naming a model of this machine that is not loaded
    /// loads it. On by default. Read at every request.
    #[serde(default = "default_true")]
    pub jit_loading: bool,
    /// Whether the models the server loaded are unloaded after their own idle
    /// delay, `jit_idle_ttl_minutes`, rather than the general one of
    /// `[llm.residency]`. On by default. Read at every sweep.
    #[serde(default = "default_true")]
    pub jit_auto_unload: bool,
    /// Minutes without a request after which a model the server loaded, and
    /// nothing holds, is unloaded; `0` keeps it loaded.
    #[serde(default = "default_jit_idle_ttl_minutes")]
    pub jit_idle_ttl_minutes: u64,
    /// Whether loading a model for a request unloads the other models the
    /// server loaded, when nothing holds them and none is answering. On by
    /// default. A model loaded from the application is never unloaded by it.
    #[serde(default = "default_true")]
    pub jit_keep_last_only: bool,
    /// Whether the compatible server listens on every interface, so the other
    /// devices of the local network reach it, rather than on the loopback
    /// interface alone. Off by default, and independent of `require_auth`:
    /// models answer the network without a token when that is off, agents and
    /// workflows never do. The native API stays on the loopback interface
    /// whatever this says.
    #[serde(default)]
    pub serve_on_lan: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: DEFAULT_COMPAT_SERVER_PORT,
            require_auth: false,
            jit_loading: true,
            jit_auto_unload: true,
            jit_idle_ttl_minutes: DEFAULT_JIT_IDLE_TTL_MINUTES,
            jit_keep_last_only: true,
            serve_on_lan: false,
        }
    }
}

impl ServerConfig {
    /// Reads the `[server]` section of a whole `apollia.toml` document.
    ///
    /// A document without the section gives the defaults; the other sections
    /// are not looked at.
    ///
    /// # Errors
    ///
    /// The document is not valid TOML, or the section carries an unknown key or
    /// a value of the wrong type.
    pub fn from_toml_str(content: &str) -> Result<Self, toml::de::Error> {
        #[derive(Deserialize)]
        struct Sections {
            server: Option<ServerConfig>,
        }
        let sections: Sections = toml::from_str(content)?;
        Ok(sections.server.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_absent_section_loads_the_defaults() {
        // GIVEN an apollia.toml without a [server] section
        let content = "[desktop]\ndeveloper_mode = true\n";

        // WHEN the server section is read
        let server = ServerConfig::from_toml_str(content).unwrap();

        // THEN the server is off, on port 7772
        assert!(!server.enabled);
        assert_eq!(server.port, 7772);
        assert!(!server.require_auth);
        assert!(!server.serve_on_lan);
    }

    #[test]
    fn test_server_config_reads_both_keys() {
        // GIVEN a [server] section turning the server on, on another port
        let content = "[server]\nenabled = true\nport = 8123\n";

        // WHEN it is read
        let server = ServerConfig::from_toml_str(content).unwrap();

        // THEN both values are the file's
        assert!(server.enabled);
        assert_eq!(server.port, 8123);
    }

    #[test]
    fn test_server_config_reads_require_auth() {
        // GIVEN a [server] section asking for a token
        let content = "[server]\nrequire_auth = true\n";

        // WHEN it is read
        let server = ServerConfig::from_toml_str(content).unwrap();

        // THEN a token is required
        assert!(server.require_auth);
    }

    #[test]
    fn test_server_config_loads_models_on_demand_by_default() {
        // GIVEN a [server] section that names none of the on-demand keys
        let content = "[server]\nenabled = true\n";

        // WHEN it is read
        let server = ServerConfig::from_toml_str(content).unwrap();

        // THEN a request loads its model, the server's models go after an
        // hour unused, and only the last one it loaded stays
        assert!(server.jit_loading);
        assert!(server.jit_auto_unload);
        assert_eq!(server.jit_idle_ttl_minutes, 60);
        assert!(server.jit_keep_last_only);
    }

    #[test]
    fn test_server_config_reads_the_on_demand_keys() {
        // GIVEN a [server] section turning every on-demand rule off
        let content = "[server]\njit_loading = false\njit_auto_unload = false\n\
                       jit_idle_ttl_minutes = 5\njit_keep_last_only = false\n";

        // WHEN it is read
        let server = ServerConfig::from_toml_str(content).unwrap();

        // THEN each value is the file's
        assert!(!server.jit_loading);
        assert!(!server.jit_auto_unload);
        assert_eq!(server.jit_idle_ttl_minutes, 5);
        assert!(!server.jit_keep_last_only);
    }

    #[test]
    fn test_server_config_reads_serve_on_lan_apart_from_require_auth() {
        // GIVEN a [server] section opening the local network, with no token
        let content = "[server]\nserve_on_lan = true\nrequire_auth = false\n";

        // WHEN it is read
        let server = ServerConfig::from_toml_str(content).unwrap();

        // THEN the network is open and no token is required
        assert!(server.serve_on_lan);
        assert!(!server.require_auth);
    }

    #[test]
    fn test_server_config_refuses_an_unknown_key() {
        // GIVEN a [server] section with a key no field declares
        let content = "[server]\nbind = \"0.0.0.0\"\n";

        // WHEN it is read
        let result = ServerConfig::from_toml_str(content);

        // THEN it is refused, naming the key
        let error = result.unwrap_err().to_string();
        assert!(error.contains("bind"), "{error}");
    }

    #[test]
    fn test_server_config_refuses_a_port_out_of_range() {
        // GIVEN a port no TCP listener can take
        let content = "[server]\nport = 70000\n";

        // WHEN it is read
        let result = ServerConfig::from_toml_str(content);

        // THEN it is refused
        assert!(result.is_err());
    }
}
