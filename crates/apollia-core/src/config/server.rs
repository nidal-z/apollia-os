use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
// ServerConfig
// ─────────────────────────────────────────────

/// Port the compatible server listens on when `apollia.toml` names none.
///
/// Distinct from the native API port (7771), which the compatible server never
/// shares.
pub const DEFAULT_COMPAT_SERVER_PORT: u16 = 7772;

fn default_port() -> u16 {
    DEFAULT_COMPAT_SERVER_PORT
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
    /// TCP port of the compatible server, on the loopback interface.
    #[serde(default = "default_port")]
    pub port: u16,
    /// Whether a request must carry a token of the server. Off by default.
    /// Read when the listener opens, so stopping and starting the server
    /// applies a change.
    #[serde(default)]
    pub require_auth: bool,
    /// Whether a web page of another origin may call the server: when on,
    /// the server answers browser preflights and marks every answer with the
    /// allowed origin. Off by default, and read each time the server starts.
    #[serde(default)]
    pub cors: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: DEFAULT_COMPAT_SERVER_PORT,
            require_auth: false,
            cors: false,
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

        // THEN the server is off, on port 7772, without cross-origin answers
        assert!(!server.enabled);
        assert_eq!(server.port, 7772);
        assert!(!server.cors);
    }

    #[test]
    fn test_server_config_reads_cors() {
        // GIVEN a [server] section turning cross-origin answers on
        let content = "[server]\ncors = true\n";

        // WHEN it is read
        let server = ServerConfig::from_toml_str(content).unwrap();

        // THEN cors is on and the other keys keep their defaults
        assert!(server.cors);
        assert!(!server.enabled);
        assert_eq!(server.port, 7772);
        assert!(!server.require_auth);
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
