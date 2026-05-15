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

    /// Enable mDNS-based discovery of MCP servers on the local network.
    ///
    /// When `true`, `discover_mcp_servers()` may be called at startup to
    /// find servers advertising `_apollia-mcp._tcp.local.` without manual
    /// configuration. Disabled by default (Principle #4 — Fail fast: any
    /// network-dependent feature must be explicitly opted into).
    #[serde(default)]
    pub mdns_discovery: bool,
}

/// Configuration for a single MCP server process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Unique server name (e.g. `"notion"`). Used in tool name prefixes: `mcp:notion/search`.
    /// Allowed characters: `[a-z0-9_-]`.
    pub name: String,

    /// Executable to spawn (e.g. `"npx"`, `"uvx"`, `"python3"`).
    /// Empty for network-based transports (`"streamable-http"`, `"sse"`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,

    /// Arguments forwarded to the command.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,

    /// Environment variables injected into the server process.
    /// Values may contain `${VAR}` placeholders resolved from the system environment.
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Transport protocol. Supported values: `"stdio"`, `"streamable-http"`, `"sse"`.
    #[serde(default = "default_transport")]
    pub transport: String,

    /// Remote server URL for network-based transports (`"streamable-http"`, `"sse"`).
    /// Required when `transport` is not `"stdio"`. Not used by `"stdio"` transport.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

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

    /// A server specifies an unrecognised transport.
    #[error("server '{server}': unsupported transport '{transport}' (supported: 'stdio', 'streamable-http', 'sse')")]
    UnsupportedTransport { server: String, transport: String },

    /// A network transport was configured without the required `url` field.
    #[error("server '{server}': transport '{transport}' requires a 'url' field")]
    MissingUrl { server: String, transport: String },

    /// `init_timeout_secs` is outside the valid range `[1, 300]`.
    #[error("server '{server}': init_timeout_secs must be in [1, 300], got {value}")]
    InvalidInitTimeout {
        /// Server name.
        server: String,
        /// The out-of-range value.
        value: u64,
    },

    /// `call_timeout_secs` is outside the valid range `[1, 600]`.
    #[error("server '{server}': call_timeout_secs must be in [1, 600], got {value}")]
    InvalidCallTimeout {
        /// Server name.
        server: String,
        /// The out-of-range value.
        value: u64,
    },

    /// The `url` field does not carry an `http://` or `https://` scheme.
    #[error("server '{server}': URL must have http:// or https:// scheme, got: {url}")]
    InvalidUrlScheme {
        /// Server name.
        server: String,
        /// The malformed URL value.
        url: String,
    },

    /// An `${VAR}` placeholder in an env value has no corresponding environment variable.
    #[error("server '{server}': unresolved environment variable: ${{{var}}}")]
    UnresolvedEnvVar { server: String, var: String },

    /// A stdio server's `command` looks like an npm/pypi package identifier
    /// rather than an executable name. The most common cause is a misconfigured
    /// catalog launcher that put the package id in `command` instead of in
    /// `args` behind a launcher like `npx -y` / `uvx`. We catch this at
    /// validation time so the failure mode is a clear error instead of a
    /// confusing `MODULE_NOT_FOUND` from the launched Node/Python process.
    #[error(
        "server '{server}': command '{command}' looks like a package identifier — \
         use a launcher (e.g. `command = \"npx\"`, `args = [\"-y\", \"{command}\", ...]`)"
    )]
    PackageIdAsCommand { server: String, command: String },
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
                mdns_discovery: false,
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
    /// Checks:
    /// - `name` matches `[a-z0-9_-]+`
    /// - `command` is non-empty for stdio transport
    /// - `transport` is a supported value
    /// - `url` is present and carries an `http://` or `https://` scheme for network transports
    /// - `init_timeout_secs` is in `[1, 300]`
    /// - `call_timeout_secs` is in `[1, 600]`
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

        if !(1..=300).contains(&self.init_timeout_secs) {
            return Err(McpConfigError::InvalidInitTimeout {
                server: self.name.clone(),
                value: self.init_timeout_secs,
            });
        }

        if !(1..=600).contains(&self.call_timeout_secs) {
            return Err(McpConfigError::InvalidCallTimeout {
                server: self.name.clone(),
                value: self.call_timeout_secs,
            });
        }

        match self.transport.as_str() {
            "stdio" => {
                if self.command.is_empty() {
                    return Err(McpConfigError::EmptyCommand {
                        server: self.name.clone(),
                    });
                }
                if looks_like_package_identifier(&self.command) {
                    return Err(McpConfigError::PackageIdAsCommand {
                        server: self.name.clone(),
                        command: self.command.clone(),
                    });
                }
            }
            "streamable-http" | "sse" => {
                let url = match self.url.as_deref() {
                    Some(u) if !u.is_empty() => u,
                    _ => {
                        return Err(McpConfigError::MissingUrl {
                            server: self.name.clone(),
                            transport: self.transport.clone(),
                        });
                    }
                };
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    return Err(McpConfigError::InvalidUrlScheme {
                        server: self.name.clone(),
                        url: url.to_string(),
                    });
                }
            }
            other => {
                return Err(McpConfigError::UnsupportedTransport {
                    server: self.name.clone(),
                    transport: other.to_string(),
                });
            }
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

/// Heuristic: does this `command` value look like an npm/pypi package
/// identifier that should have been placed in `args` behind a launcher?
///
/// We flag two well-known shapes:
/// - `@scope/name` — scoped npm packages always start with `@`.
/// - `name/sub` — bare slash-segmented identifiers that are *not* absolute or
///   relative filesystem paths (no leading `/`, `./`, `../`, no Windows drive
///   prefix). Real executables resolved from `PATH` don't contain slashes.
///
/// Anything else (single tokens like `npx`, `uvx`, `mcp-server-time`, absolute
/// paths) is accepted — we'd rather false-negative than reject a legitimate
/// custom command.
fn looks_like_package_identifier(command: &str) -> bool {
    let cmd = command.trim();
    if cmd.starts_with('@') {
        return true;
    }
    // Absolute or relative paths are legitimate executables.
    if cmd.starts_with('/') || cmd.starts_with("./") || cmd.starts_with("../") {
        return false;
    }
    // Windows drive prefix (e.g. `C:\bin\foo.exe`) — also a legitimate path.
    if cmd.len() >= 3
        && cmd.as_bytes()[1] == b':'
        && (cmd.as_bytes()[2] == b'\\' || cmd.as_bytes()[2] == b'/')
    {
        return false;
    }
    // Bare slash-segmented identifier (e.g. `io.github.foo/bar-mcp`).
    cmd.contains('/')
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
            url: None,
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
            url: None,
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
            url: None,
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
            url: None,
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
            url: None,
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
            url: None,
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
            url: None,
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

    #[test]
    fn test_scoped_npm_package_as_command_is_rejected() {
        // GIVEN a stdio server whose `command` is the scoped npm identifier
        // (the bug shape produced by older catalog wizard versions).
        let config = McpServerConfig {
            name: "local-files".to_string(),
            command: "@modelcontextprotocol/server-filesystem".to_string(),
            args: vec!["/tmp".to_string()],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            tags: vec![],
        };
        // WHEN / THEN validation rejects with a guidance-bearing error.
        match config.validate() {
            Err(McpConfigError::PackageIdAsCommand { command, .. }) => {
                assert_eq!(command, "@modelcontextprotocol/server-filesystem");
            }
            other => panic!("expected PackageIdAsCommand, got {other:?}"),
        }
    }

    #[test]
    fn test_dotted_reverse_dns_package_as_command_is_rejected() {
        // GIVEN a Java-style reverse-DNS package identifier in `command`.
        let config = McpServerConfig {
            name: "github".to_string(),
            command: "io.github.github/github-mcp-server".to_string(),
            args: vec![],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            tags: vec![],
        };
        // WHEN / THEN
        assert!(matches!(
            config.validate(),
            Err(McpConfigError::PackageIdAsCommand { .. })
        ));
    }

    #[test]
    fn test_legitimate_launchers_pass_validation() {
        // GIVEN well-formed configs using the conventional launchers / paths.
        for cmd in [
            "npx",
            "uvx",
            "bunx",
            "mcp-server-time",
            "/usr/local/bin/my-mcp",
            "./local-bin/mcp",
        ] {
            let config = McpServerConfig {
                name: "ok".to_string(),
                command: cmd.to_string(),
                args: vec![],
                env: HashMap::new(),
                transport: "stdio".to_string(),
                url: None,
                requires_approval: false,
                init_timeout_secs: 30,
                call_timeout_secs: 60,
                tags: vec![],
            };
            // WHEN / THEN — must accept all of these.
            assert!(
                config.validate().is_ok(),
                "command `{cmd}` should be accepted but was rejected",
            );
        }
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
            url: None,
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

    #[test]
    fn test_mcp_init_timeout_zero_returns_validation_error() {
        // GIVEN init_timeout_secs = 0 (below minimum of 1)
        let config = McpServerConfig {
            name: "notion".to_string(),
            command: "npx".to_string(),
            args: vec![],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 0,
            call_timeout_secs: 60,
            tags: vec![],
        };
        // WHEN / THEN
        assert!(matches!(
            config.validate(),
            Err(McpConfigError::InvalidInitTimeout { value: 0, .. })
        ));
    }

    #[test]
    fn test_mcp_init_timeout_301_returns_validation_error() {
        // GIVEN init_timeout_secs = 301 (above maximum of 300)
        let config = McpServerConfig {
            name: "notion".to_string(),
            command: "npx".to_string(),
            args: vec![],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 301,
            call_timeout_secs: 60,
            tags: vec![],
        };
        // WHEN / THEN
        assert!(matches!(
            config.validate(),
            Err(McpConfigError::InvalidInitTimeout { value: 301, .. })
        ));
    }

    #[test]
    fn test_mcp_call_timeout_zero_returns_validation_error() {
        // GIVEN call_timeout_secs = 0 (below minimum of 1)
        let config = McpServerConfig {
            name: "notion".to_string(),
            command: "npx".to_string(),
            args: vec![],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 0,
            tags: vec![],
        };
        // WHEN / THEN
        assert!(matches!(
            config.validate(),
            Err(McpConfigError::InvalidCallTimeout { value: 0, .. })
        ));
    }

    #[test]
    fn test_mcp_url_without_scheme_returns_validation_error() {
        // GIVEN transport = "streamable-http" with a URL that has no scheme
        let config = McpServerConfig {
            name: "notion".to_string(),
            command: String::new(),
            args: vec![],
            env: HashMap::new(),
            transport: "streamable-http".to_string(),
            url: Some("localhost:8080".to_string()),
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            tags: vec![],
        };
        // WHEN / THEN
        assert!(matches!(
            config.validate(),
            Err(McpConfigError::InvalidUrlScheme { .. })
        ));
    }

    #[test]
    fn test_mcp_url_with_http_scheme_passes_validation() {
        // GIVEN transport = "streamable-http" with an http:// URL
        let config = McpServerConfig {
            name: "local-mcp".to_string(),
            command: String::new(),
            args: vec![],
            env: HashMap::new(),
            transport: "streamable-http".to_string(),
            url: Some("http://localhost:8080/mcp".to_string()),
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            tags: vec![],
        };
        // WHEN / THEN
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_streamable_http_with_url_passes_validation() {
        // GIVEN a config with transport = "streamable-http" and a valid url
        let config = McpServerConfig {
            name: "notion".to_string(),
            command: String::new(),
            args: vec![],
            env: HashMap::new(),
            transport: "streamable-http".to_string(),
            url: Some("https://mcp.notion.ai/mcp".to_string()),
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            tags: vec![],
        };
        // WHEN / THEN
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_streamable_http_without_url_fails() {
        // GIVEN a config with transport = "streamable-http" but no url
        let config = McpServerConfig {
            name: "notion".to_string(),
            command: String::new(),
            args: vec![],
            env: HashMap::new(),
            transport: "streamable-http".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            tags: vec![],
        };
        // WHEN / THEN
        assert!(matches!(
            config.validate(),
            Err(McpConfigError::MissingUrl { .. })
        ));
    }

    #[test]
    fn test_sse_with_url_passes_validation() {
        // GIVEN a config with transport = "sse" and a valid url
        let config = McpServerConfig {
            name: "brave".to_string(),
            command: String::new(),
            args: vec![],
            env: HashMap::new(),
            transport: "sse".to_string(),
            url: Some("https://mcp.brave.com/sse".to_string()),
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            tags: vec![],
        };
        // WHEN / THEN
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_parse_remote_config_toml() {
        // GIVEN a mcp.toml declaring a streamable-http server with a url and no command
        let toml_content = r#"
            [[servers]]
            name = "notion"
            transport = "streamable-http"
            url = "https://mcp.notion.com/mcp"
        "#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(toml_content.as_bytes()).unwrap();
        // WHEN
        let config = McpConfig::load(file.path()).unwrap();
        // THEN
        assert_eq!(config.servers.len(), 1);
        let server = &config.servers[0];
        assert_eq!(server.transport, "streamable-http");
        assert_eq!(server.url, Some("https://mcp.notion.com/mcp".to_string()));
        assert!(
            server.command.is_empty(),
            "command must be absent for remote transports"
        );
    }

    #[test]
    fn test_parse_stdio_config_toml_unchanged() {
        // GIVEN a mcp.toml with a stdio server (no url field)
        let toml_content = r#"
            [[servers]]
            name = "sqlite"
            command = "npx"
            args = ["mcp-server-sqlite"]
        "#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(toml_content.as_bytes()).unwrap();
        // WHEN
        let config = McpConfig::load(file.path()).unwrap();
        // THEN transport defaults to "stdio", command is set, url is absent
        assert_eq!(config.servers.len(), 1);
        let server = &config.servers[0];
        assert_eq!(server.transport, "stdio");
        assert_eq!(server.command, "npx");
        assert_eq!(server.args, vec!["mcp-server-sqlite"]);
        assert!(server.url.is_none());
    }
}
