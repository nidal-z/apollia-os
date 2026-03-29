//! MCP configuration: parsing, validation, and env-var interpolation.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Abstracts secret retrieval so that `apollia-mcp` does not depend on any
/// specific keychain implementation.
///
/// Implementors provide access to secrets keyed by the format
/// `"{server_name}:{env_var}"` (e.g. `"notion:NOTION_API_KEY"`).
pub trait SecretResolver: Send + Sync {
    /// Retrieve the secret for `key`.
    ///
    /// Returns `Ok(value)` when the secret is present, or `Err(message)`
    /// when it cannot be retrieved (not found, backend unavailable, etc.).
    fn get_secret(&self, key: &str) -> Result<String, String>;
}

/// Top-level MCP configuration loaded from `~/.apollia/mcp.toml`.
#[derive(Debug, Serialize, Deserialize)]
pub struct McpConfig {
    /// Ordered list of MCP server definitions.
    #[serde(default)]
    pub servers: Vec<McpServerConfig>,
}

/// Configuration for a single MCP server process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Unique server name (e.g. `"notion"`). Used in tool name prefixes: `mcp:notion/search`.
    /// Allowed characters: `[a-z0-9_-]`.
    pub name: String,

    /// Executable to spawn (e.g. `"npx"`, `"uvx"`, `"python3"`).
    pub command: String,

    /// Arguments forwarded to the command.
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables injected into the server process.
    /// Values may contain `${VAR}` placeholders resolved from the system environment.
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Transport protocol. Only `"stdio"` is supported in V1.
    #[serde(default = "default_transport")]
    pub transport: String,

    /// When `true`, every tool call to this server requires HITL approval before execution.
    #[serde(default)]
    pub requires_approval: bool,

    /// Timeout for the MCP `initialize` handshake, in seconds.
    #[serde(default = "default_init_timeout")]
    pub init_timeout_secs: u64,

    /// Timeout for individual `tools/call` requests, in seconds.
    #[serde(default = "default_call_timeout")]
    pub call_timeout_secs: u64,

    /// Additional tags applied to every tool advertised by this server.
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_transport() -> String {
    "stdio".to_string()
}

fn default_init_timeout() -> u64 {
    30
}

fn default_call_timeout() -> u64 {
    60
}

/// Errors that can occur while loading or validating MCP configuration.
#[derive(Debug, Error)]
pub enum McpConfigError {
    /// The `mcp.toml` file could not be read.
    #[error("failed to read mcp.toml: {0}")]
    ReadFailed(String),

    /// The `mcp.toml` file could not be parsed as TOML.
    #[error("failed to parse mcp.toml: {0}")]
    ParseFailed(String),

    /// A server name is the empty string.
    #[error("server name is empty")]
    EmptyServerName,

    /// A server name contains characters outside `[a-z0-9_-]`.
    #[error("server name '{0}' contains invalid characters (allowed: a-z, 0-9, _, -)")]
    InvalidServerName(String),

    /// Two servers share the same name.
    #[error("duplicate server name: '{0}'")]
    DuplicateServerName(String),

    /// A server's `command` field is the empty string.
    #[error("server '{server}': command is empty")]
    EmptyCommand { server: String },

    /// A server specifies a transport other than `"stdio"`.
    #[error("server '{server}': unsupported transport '{transport}' (V1 supports 'stdio' only)")]
    UnsupportedTransport { server: String, transport: String },

    /// An `${VAR}` placeholder in an env value has no corresponding environment variable.
    #[error("server '{server}': unresolved environment variable: ${{{var}}}")]
    UnresolvedEnvVar { server: String, var: String },
}

impl McpConfig {
    /// Load and validate `mcp.toml` from `path`.
    ///
    /// Returns `Ok(McpConfig { servers: [] })` when the file does not exist —
    /// absent config is not an error (the runtime runs without MCP servers).
    pub fn load(path: &Path) -> Result<Self, McpConfigError> {
        if !path.exists() {
            return Ok(McpConfig {
                servers: Vec::new(),
            });
        }

        let raw =
            std::fs::read_to_string(path).map_err(|e| McpConfigError::ReadFailed(e.to_string()))?;

        let config: McpConfig =
            toml::from_str(&raw).map_err(|e| McpConfigError::ParseFailed(e.to_string()))?;

        config.validate()?;
        Ok(config)
    }

    /// Validate every server entry in the configuration.
    ///
    /// Enforces name uniqueness and delegates per-server validation to
    /// [`McpServerConfig::validate`].
    pub fn validate(&self) -> Result<(), McpConfigError> {
        let mut seen: HashSet<&str> = HashSet::new();

        for server in &self.servers {
            server.validate()?;

            if !seen.insert(server.name.as_str()) {
                return Err(McpConfigError::DuplicateServerName(server.name.clone()));
            }
        }

        Ok(())
    }
}

impl McpServerConfig {
    /// Validate this server's fields independently of other servers.
    ///
    /// Checks that `name` matches `[a-z0-9_-]+`, `command` is non-empty,
    /// and `transport` is `"stdio"`.
    pub fn validate(&self) -> Result<(), McpConfigError> {
        if self.name.is_empty() {
            return Err(McpConfigError::EmptyServerName);
        }

        if !self
            .name
            .chars()
            .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_' | '-'))
        {
            return Err(McpConfigError::InvalidServerName(self.name.clone()));
        }

        if self.command.is_empty() {
            return Err(McpConfigError::EmptyCommand {
                server: self.name.clone(),
            });
        }

        if self.transport != "stdio" {
            return Err(McpConfigError::UnsupportedTransport {
                server: self.name.clone(),
                transport: self.transport.clone(),
            });
        }

        Ok(())
    }

    /// Resolve `${VAR}` placeholders in every env value.
    ///
    /// Plain `${VAR}` placeholders are resolved from the system environment.
    /// Placeholders prefixed with `APOLLIA_SECRET:` (e.g. `${APOLLIA_SECRET:MY_KEY}`)
    /// are resolved from the supplied `secret_store` using the keychain key
    /// `"{server_name}:{MY_KEY}"`.
    ///
    /// Returns a new map with all placeholders replaced by their resolved values.
    /// Returns [`McpConfigError::UnresolvedEnvVar`] if any placeholder cannot be
    /// resolved, including when `secret_store` is `None` for an `APOLLIA_SECRET:`
    /// placeholder.
    pub fn resolve_env(
        &self,
        secret_store: Option<&dyn SecretResolver>,
    ) -> Result<HashMap<String, String>, McpConfigError> {
        self.env
            .iter()
            .map(|(key, value)| {
                let resolved = resolve_placeholders(value, &self.name, secret_store)?;
                Ok((key.clone(), resolved))
            })
            .collect()
    }
}

/// Replace all `${VAR}` occurrences in `value`, dispatching to either the system
/// environment or the secret store depending on the `APOLLIA_SECRET:` prefix.
fn resolve_placeholders(
    value: &str,
    server_name: &str,
    secret_store: Option<&dyn SecretResolver>,
) -> Result<String, McpConfigError> {
    let mut result = String::with_capacity(value.len());
    let mut remaining = value;

    while let Some(start) = remaining.find("${") {
        // Append literal prefix before the placeholder.
        result.push_str(&remaining[..start]);
        remaining = &remaining[start + 2..];

        let end = remaining
            .find('}')
            .ok_or_else(|| McpConfigError::UnresolvedEnvVar {
                server: server_name.to_string(),
                var: remaining.to_string(),
            })?;

        let var_name = &remaining[..end];
        remaining = &remaining[end + 1..];

        let var_value = resolve_single_var(server_name, var_name, secret_store)?;
        result.push_str(&var_value);
    }

    // Append any trailing literal text.
    result.push_str(remaining);
    Ok(result)
}

/// Resolve a single variable reference extracted from a `${…}` placeholder.
///
/// When `var_name` starts with `APOLLIA_SECRET:`, the remainder is used as the
/// secret key and the value is fetched from `secret_store` using the composite
/// key `"{server_name}:{secret_key}"`. When `secret_store` is `None` the call
/// fails immediately with [`McpConfigError::UnresolvedEnvVar`].
///
/// All other variable names are resolved from the process environment via
/// [`std::env::var`].
fn resolve_single_var(
    server_name: &str,
    var_name: &str,
    secret_store: Option<&dyn SecretResolver>,
) -> Result<String, McpConfigError> {
    if let Some(secret_key) = var_name.strip_prefix("APOLLIA_SECRET:") {
        let store = secret_store.ok_or_else(|| McpConfigError::UnresolvedEnvVar {
            server: server_name.to_string(),
            var: var_name.to_string(),
        })?;
        let key = format!("{}:{}", server_name, secret_key);
        store
            .get_secret(&key)
            .map_err(|_| McpConfigError::UnresolvedEnvVar {
                server: server_name.to_string(),
                var: var_name.to_string(),
            })
    } else {
        std::env::var(var_name).map_err(|_| McpConfigError::UnresolvedEnvVar {
            server: server_name.to_string(),
            var: var_name.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_valid_toml() {
        // GIVEN a valid mcp.toml with two configured servers
        let toml_content = r#"
            [[servers]]
            name = "notion"
            command = "npx"
            args = ["-y", "@notionhq/notion-mcp-server"]
            transport = "stdio"

            [[servers]]
            name = "sqlite"
            command = "uvx"
            args = ["mcp-server-sqlite"]
            transport = "stdio"
        "#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(toml_content.as_bytes()).unwrap();
        // WHEN
        let config = McpConfig::load(file.path()).unwrap();
        // THEN
        assert_eq!(config.servers.len(), 2);
        assert_eq!(config.servers[0].name, "notion");
        assert_eq!(config.servers[1].name, "sqlite");
    }

    #[test]
    fn test_missing_file_returns_empty_config() {
        // GIVEN a path pointing to a non-existent file
        let path = Path::new("/tmp/nonexistent-mcp-toml-12345.toml");
        // WHEN
        let config = McpConfig::load(path).unwrap();
        // THEN
        assert!(config.servers.is_empty());
    }

    #[test]
    fn test_duplicate_server_name_fails() {
        // GIVEN two servers sharing the same name
        let toml_content = r#"
            [[servers]]
            name = "notion"
            command = "npx"
            [[servers]]
            name = "notion"
            command = "uvx"
        "#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(toml_content.as_bytes()).unwrap();
        // WHEN / THEN
        assert!(matches!(
            McpConfig::load(file.path()),
            Err(McpConfigError::DuplicateServerName(_))
        ));
    }

    #[test]
    fn test_invalid_server_name_fails() {
        // GIVEN a server name containing characters outside [a-z0-9_-]
        let config = McpServerConfig {
            name: "My Server!".to_string(),
            command: "npx".to_string(),
            args: vec![],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            tags: vec![],
        };
        // WHEN / THEN
        assert!(matches!(
            config.validate(),
            Err(McpConfigError::InvalidServerName(_))
        ));
    }

    #[test]
    fn test_resolve_env_interpolates_variable() {
        // GIVEN an env map containing ${TEST_MCP_KEY_327} and that variable set in the process env
        std::env::set_var("TEST_MCP_KEY_327", "secret123");
        let config = McpServerConfig {
            name: "test".to_string(),
            command: "npx".to_string(),
            args: vec![],
            env: HashMap::from([("API_KEY".to_string(), "${TEST_MCP_KEY_327}".to_string())]),
            transport: "stdio".to_string(),
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            tags: vec![],
        };
        // WHEN
        let resolved = config.resolve_env(None).unwrap();
        // THEN
        assert_eq!(resolved["API_KEY"], "secret123");
        std::env::remove_var("TEST_MCP_KEY_327");
    }

    #[test]
    fn test_unresolved_env_var_fails() {
        // GIVEN an env map referencing a variable absent from the process environment
        std::env::remove_var("TOTALLY_MISSING_VAR_327");
        let config = McpServerConfig {
            name: "test".to_string(),
            command: "npx".to_string(),
            args: vec![],
            env: HashMap::from([("KEY".to_string(), "${TOTALLY_MISSING_VAR_327}".to_string())]),
            transport: "stdio".to_string(),
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            tags: vec![],
        };
        // WHEN / THEN
        assert!(matches!(
            config.resolve_env(None),
            Err(McpConfigError::UnresolvedEnvVar { .. })
        ));
    }

    #[test]
    fn test_unsupported_transport_fails() {
        // GIVEN a server configured with transport = "http"
        let config = McpServerConfig {
            name: "test".to_string(),
            command: "npx".to_string(),
            args: vec![],
            env: HashMap::new(),
            transport: "http".to_string(),
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            tags: vec![],
        };
        // WHEN / THEN
        assert!(matches!(
            config.validate(),
            Err(McpConfigError::UnsupportedTransport { .. })
        ));
    }

    #[test]
    fn test_partial_interpolation_with_surrounding_text() {
        // GIVEN a value that mixes literal text and a placeholder
        std::env::set_var("TEST_MCP_HOST_327", "localhost");
        let config = McpServerConfig {
            name: "test".to_string(),
            command: "npx".to_string(),
            args: vec![],
            env: HashMap::from([(
                "BASE_URL".to_string(),
                "http://${TEST_MCP_HOST_327}:8080".to_string(),
            )]),
            transport: "stdio".to_string(),
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            tags: vec![],
        };
        // WHEN
        let resolved = config.resolve_env(None).unwrap();
        // THEN
        assert_eq!(resolved["BASE_URL"], "http://localhost:8080");
        std::env::remove_var("TEST_MCP_HOST_327");
    }

    #[test]
    fn test_empty_server_name_fails() {
        // GIVEN a server with an empty name
        let config = McpServerConfig {
            name: String::new(),
            command: "npx".to_string(),
            args: vec![],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            tags: vec![],
        };
        // WHEN / THEN
        assert!(matches!(
            config.validate(),
            Err(McpConfigError::EmptyServerName)
        ));
    }

    #[test]
    fn test_empty_command_fails() {
        // GIVEN a server with a valid name but empty command
        let config = McpServerConfig {
            name: "notion".to_string(),
            command: String::new(),
            args: vec![],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            tags: vec![],
        };
        // WHEN / THEN
        assert!(matches!(
            config.validate(),
            Err(McpConfigError::EmptyCommand { .. })
        ));
    }

    // ── SecretResolver tests ─────────────────────────────────────────────────

    /// In-memory secret store used exclusively in tests.
    struct MockSecretStore {
        secrets: HashMap<String, String>,
    }

    impl MockSecretStore {
        fn with(pairs: &[(&str, &str)]) -> Self {
            Self {
                secrets: pairs
                    .iter()
                    .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                    .collect(),
            }
        }
    }

    impl SecretResolver for MockSecretStore {
        fn get_secret(&self, key: &str) -> Result<String, String> {
            self.secrets
                .get(key)
                .cloned()
                .ok_or_else(|| format!("not found: {key}"))
        }
    }

    fn server_with_env(name: &str, env: HashMap<String, String>) -> McpServerConfig {
        McpServerConfig {
            name: name.to_string(),
            command: "npx".to_string(),
            args: vec![],
            env,
            transport: "stdio".to_string(),
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            tags: vec![],
        }
    }

    #[test]
    fn test_resolve_apollia_secret_from_store() {
        // GIVEN env = { KEY = "${APOLLIA_SECRET:MY_KEY}" } and a store holding "notion:MY_KEY"
        let store = MockSecretStore::with(&[("notion:MY_KEY", "tok_abc123")]);
        let config = server_with_env(
            "notion",
            HashMap::from([("KEY".to_string(), "${APOLLIA_SECRET:MY_KEY}".to_string())]),
        );
        // WHEN
        let resolved = config.resolve_env(Some(&store)).unwrap();
        // THEN the keyring value is injected
        assert_eq!(resolved["KEY"], "tok_abc123");
    }

    #[test]
    fn test_resolve_normal_env_var_unchanged() {
        // GIVEN env = { KEY = "${HOME}" } and no secret store
        let config = server_with_env(
            "test",
            HashMap::from([("KEY".to_string(), "${HOME}".to_string())]),
        );
        let home = std::env::var("HOME").unwrap_or_default();
        // WHEN
        let resolved = config.resolve_env(None).unwrap();
        // THEN the system env var is returned as before
        assert_eq!(resolved["KEY"], home);
    }

    #[test]
    fn test_missing_secret_returns_unresolved_error() {
        // GIVEN env = { KEY = "${APOLLIA_SECRET:MISSING}" } and a store without that key
        let store = MockSecretStore::with(&[]);
        let config = server_with_env(
            "notion",
            HashMap::from([("KEY".to_string(), "${APOLLIA_SECRET:MISSING}".to_string())]),
        );
        // WHEN / THEN
        assert!(matches!(
            config.resolve_env(Some(&store)),
            Err(McpConfigError::UnresolvedEnvVar { .. })
        ));
    }

    #[test]
    fn test_coexistence_secret_and_env_var() {
        // GIVEN env = { A = "${APOLLIA_SECRET:X}", B = "${HOME}" }
        let store = MockSecretStore::with(&[("svc:X", "from_keychain")]);
        let config = server_with_env(
            "svc",
            HashMap::from([
                ("A".to_string(), "${APOLLIA_SECRET:X}".to_string()),
                ("B".to_string(), "${HOME}".to_string()),
            ]),
        );
        let home = std::env::var("HOME").unwrap_or_default();
        // WHEN
        let resolved = config.resolve_env(Some(&store)).unwrap();
        // THEN A comes from the keyring and B from the system environment
        assert_eq!(resolved["A"], "from_keychain");
        assert_eq!(resolved["B"], home);
    }

    #[test]
    fn test_apollia_secret_without_store_returns_error() {
        // GIVEN env = { KEY = "${APOLLIA_SECRET:X}" } and secret_store = None
        let config = server_with_env(
            "svc",
            HashMap::from([("KEY".to_string(), "${APOLLIA_SECRET:X}".to_string())]),
        );
        // WHEN / THEN
        assert!(matches!(
            config.resolve_env(None),
            Err(McpConfigError::UnresolvedEnvVar { .. })
        ));
    }
}
