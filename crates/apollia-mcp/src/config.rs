//! MCP configuration: parsing, validation, and env-var interpolation.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Abstracts secret retrieval so that `apollia-mcp` does not depend on any
/// Default service slot for MCP static secrets in the OS keychain.
///
/// Kept in sync with the desktop-side `apollia_desktop::mcp::SecretStore`
/// which writes under the same service when the wizard calls
/// `store_mcp_secret`. The constant lives here so the
/// [`DefaultMcpSecretResolver`] can read from the same slot without
/// duplicating the literal.
pub const MCP_STATIC_SECRET_SERVICE: &str = "apollia-mcp";

/// Canonical secret resolver wired into every `McpSession::start` call.
///
/// Stateless ZST: instantiate as `&DefaultMcpSecretResolver` anywhere a
/// `&dyn SecretResolver` is expected. Bridges both placeholder kinds to
/// their concrete storage layer:
/// - `${APOLLIA_SECRET:KEY}` → OS keychain via
///   [`apollia_auth::select_secret_store`] at service [`MCP_STATIC_SECRET_SERVICE`].
/// - `${APOLLIA_OAUTH}` → [`apollia_auth::ensure_fresh_token`], which
///   transparently refreshes tokens with the singleflight lock.
///
/// Removes the need for each caller to plumb its own resolver; there is
/// exactly one production code path now.
pub struct DefaultMcpSecretResolver;

#[async_trait::async_trait]
impl SecretResolver for DefaultMcpSecretResolver {
    fn get_secret(&self, key: &str) -> Result<String, String> {
        let store = apollia_auth::select_secret_store()
            .map_err(|e| format!("secret store unavailable: {e}"))?;
        store
            .get(MCP_STATIC_SECRET_SERVICE, key)
            .map_err(|e| format!("keychain read failed: {e}"))?
            .ok_or_else(|| format!("secret not found: {key}"))
    }

    async fn resolve_oauth_bearer(&self, server_name: &str) -> Result<String, String> {
        let store = apollia_auth::select_secret_store()
            .map_err(|e| format!("secret store unavailable: {e}"))?;
        apollia_auth::ensure_fresh_token(server_name, &*store)
            .await
            .map_err(|e| format!("{e}"))
    }
}

/// Trait for resolving secrets referenced by `${APOLLIA_SECRET:KEY}` placeholders
/// in env values. Decouples the config layer from the runtime's
/// specific keychain implementation.
///
/// Implementors provide access to secrets keyed by the format
/// `"{server_name}:{env_var}"` (e.g. `"notion:NOTION_API_KEY"`) and,
/// optionally, an asynchronous resolver for MCP HTTP OAuth bearer tokens
/// (OAuth Phase 3). The OAuth path has a default implementation that
/// returns an explicit "not configured" error so headless / CLI minimal
/// builds without an orchestrator don't need to plumb it.
#[async_trait::async_trait]
pub trait SecretResolver: Send + Sync {
    /// Retrieve the static secret for `key`.
    ///
    /// Returns `Ok(value)` when the secret is present, or `Err(message)`
    /// when it cannot be retrieved (not found, backend unavailable, etc.).
    fn get_secret(&self, key: &str) -> Result<String, String>;

    /// Resolve the current OAuth access token for `server_name` and return
    /// the value to inject as an `Authorization` header (already
    /// `Bearer`-prefixed).
    ///
    /// The default implementation returns a clear error so callers without
    /// an OAuth orchestrator (e.g. minimal CLI builds) surface a
    /// guidance-bearing error rather than panicking.
    async fn resolve_oauth_bearer(&self, server_name: &str) -> Result<String, String> {
        let _ = server_name;
        Err("MCP OAuth bearer resolver is not configured in this runtime".to_string())
    }
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
    /// configuration. Disabled by default (fail fast: any network-dependent
    /// feature must be explicitly opted into).
    #[serde(default)]
    pub mdns_discovery: bool,
}

/// Current version of the persisted format, and the value read for files
/// written before the key existed.
fn default_format_version() -> u32 {
    1
}

/// Configuration for a single MCP server process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Version of this on-disk format. A file written before versioning has
    /// no key and reads as format 1; a future format bumps the default and the
    /// reader branches on the value it finds.
    #[serde(default = "default_format_version")]
    pub format_version: u32,
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

    /// Maximum number of bytes accepted from a single server response before the
    /// transport aborts the read with an error.
    ///
    /// MCP servers are untrusted: a server that never emits a newline, streams
    /// forever, or returns a giant body would otherwise grow daemon memory
    /// without bound. This cap is enforced by every transport (stdio line read,
    /// HTTP body read, SSE buffer accumulation). Default: 8 MiB. Bounds:
    /// `[1024, 1_073_741_824]` (1 KiB to 1 GiB). Raise it for servers with
    /// legitimately large tool payloads.
    #[serde(default = "default_max_response_bytes")]
    pub max_response_bytes: u64,

    /// Maximum number of tools retained from this server's `tools/list`.
    ///
    /// MCP servers are untrusted: a server advertising thousands of tools would
    /// otherwise flood the tool registry and the model's tool catalogue,
    /// exhausting context and memory. Tools beyond this cap are dropped at
    /// discovery. Default: 256. Bounds: `[1, 8192]`. Raise it for aggregating
    /// servers that legitimately expose many tools.
    #[serde(default = "default_max_tools")]
    pub max_tools: u32,

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

fn default_max_response_bytes() -> u64 {
    8 * 1024 * 1024
}

fn default_max_tools() -> u32 {
    256
}

/// Lower bound for `max_response_bytes` (1 KiB): below this even a single
/// JSON-RPC handshake line would not fit, which is a misconfiguration.
const MIN_MAX_RESPONSE_BYTES: u64 = 1024;

/// Upper bound for `max_response_bytes` (1 GiB): a ceiling generous enough for
/// any realistic tool payload while still bounding memory.
const MAX_MAX_RESPONSE_BYTES: u64 = 1024 * 1024 * 1024;

/// Lower bound for `max_tools`: a server must be allowed to expose at least one
/// tool, and zero would silently disable it.
const MIN_MAX_TOOLS: u32 = 1;

/// Upper bound for `max_tools`: a ceiling generous enough for any realistic
/// aggregating server while still bounding registry and context growth.
const MAX_MAX_TOOLS: u32 = 8192;

/// Errors that can occur while loading or validating MCP configuration.
#[derive(Debug, Error)]
#[non_exhaustive]
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

    /// `max_response_bytes` is outside the valid range `[1024, 1_073_741_824]`.
    #[error("server '{server}': max_response_bytes must be in [1024, 1073741824], got {value}")]
    InvalidMaxResponseBytes {
        /// Server name.
        server: String,
        /// The out-of-range value.
        value: u64,
    },

    /// `max_tools` is outside the valid range `[1, 8192]`.
    #[error("server '{server}': max_tools must be in [1, 8192], got {value}")]
    InvalidMaxTools {
        /// Server name.
        server: String,
        /// The out-of-range value.
        value: u32,
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
    ///
    /// `${APOLLIA_OAUTH}` is a special MCP OAuth marker; when unresolved it means no
    /// OAuth token is stored for the server yet. The display impl surfaces a more
    /// actionable hint for that case.
    #[error("{}", format_unresolved_env_var(server, var))]
    UnresolvedEnvVar { server: String, var: String },

    /// A stdio server's `command` looks like an npm/pypi package identifier
    /// rather than an executable name. The most common cause is a misconfigured
    /// catalog launcher that put the package id in `command` instead of in
    /// `args` behind a launcher like `npx -y` / `uvx`. We catch this at
    /// validation time so the failure mode is a clear error instead of a
    /// confusing `MODULE_NOT_FOUND` from the launched Node/Python process.
    #[error(
        "server '{server}': command '{command}' looks like a package identifier - \
         use a launcher (e.g. `command = \"npx\"`, `args = [\"-y\", \"{command}\", ...]`)"
    )]
    PackageIdAsCommand { server: String, command: String },
}

/// Format the `UnresolvedEnvVar` display, with a dedicated hint for the
/// `APOLLIA_OAUTH` marker that points to the MCP OAuth login flow.
fn format_unresolved_env_var(server: &str, var: &str) -> String {
    if var == "APOLLIA_OAUTH" {
        format!(
            "server '{server}': MCP OAuth token not yet stored - run `apollia-os mcp oauth login {server}` to authenticate"
        )
    } else {
        format!("server '{server}': environment variable ${{{var}}} is not set")
    }
}

impl McpConfig {
    /// Load and validate `mcp.toml` from `path`.
    ///
    /// Returns `Ok(McpConfig { servers: [] })` when the file does not exist;
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
    /// - `max_response_bytes` is in `[1024, 1_073_741_824]`
    /// - `max_tools` is in `[1, 8192]`
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

        if !(MIN_MAX_RESPONSE_BYTES..=MAX_MAX_RESPONSE_BYTES).contains(&self.max_response_bytes) {
            return Err(McpConfigError::InvalidMaxResponseBytes {
                server: self.name.clone(),
                value: self.max_response_bytes,
            });
        }

        if !(MIN_MAX_TOOLS..=MAX_MAX_TOOLS).contains(&self.max_tools) {
            return Err(McpConfigError::InvalidMaxTools {
                server: self.name.clone(),
                value: self.max_tools,
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
    /// `${APOLLIA_SECRET:MY_KEY}` placeholders are resolved from the supplied
    /// `secret_store` using the keychain key `"{server_name}:{MY_KEY}"`.
    /// `${APOLLIA_OAUTH}` placeholders trigger an MCP HTTP OAuth bearer
    /// lookup via [`SecretResolver::resolve_oauth_bearer`] and
    /// expand to the full `"Bearer …"` header value.
    ///
    /// Returns a new map with all placeholders replaced by their resolved values.
    /// Returns [`McpConfigError::UnresolvedEnvVar`] if any placeholder cannot be
    /// resolved, including when `secret_store` is `None` for a placeholder that
    /// requires it.
    pub async fn resolve_env(
        &self,
        secret_store: Option<&dyn SecretResolver>,
    ) -> Result<HashMap<String, String>, McpConfigError> {
        let mut out = HashMap::with_capacity(self.env.len());
        for (key, value) in &self.env {
            let resolved = resolve_placeholders(value, &self.name, secret_store).await?;
            out.insert(key.clone(), resolved);
        }
        Ok(out)
    }
}

/// Heuristic: does this `command` value look like an npm/pypi package
/// identifier that should have been placed in `args` behind a launcher?
///
/// We flag two well-known shapes:
/// - `@scope/name`: scoped npm packages always start with `@`.
/// - `name/sub`: bare slash-segmented identifiers that are *not* absolute or
///   relative filesystem paths (no leading `/`, `./`, `../`, no Windows drive
///   prefix). Real executables resolved from `PATH` don't contain slashes.
///
/// Anything else (single tokens like `npx`, `uvx`, `mcp-server-time`, absolute
/// paths) is accepted; we'd rather false-negative than reject a legitimate
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
    // Windows drive prefix (e.g. `C:\bin\foo.exe`), also a legitimate path.
    if cmd.len() >= 3
        && cmd.as_bytes()[1] == b':'
        && (cmd.as_bytes()[2] == b'\\' || cmd.as_bytes()[2] == b'/')
    {
        return false;
    }
    // Bare slash-segmented identifier (e.g. `io.github.foo/bar-mcp`).
    cmd.contains('/')
}

/// Replace all `${VAR}` occurrences in `value`, dispatching to either the
/// system environment, the static secret store (`APOLLIA_SECRET:` prefix), or
/// the MCP OAuth bearer resolver (`APOLLIA_OAUTH`).
async fn resolve_placeholders(
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

        let var_value = resolve_single_var(server_name, var_name, secret_store).await?;
        result.push_str(&var_value);
    }

    // Append any trailing literal text.
    result.push_str(remaining);
    Ok(result)
}

/// Resolve a single variable reference extracted from a `${…}` placeholder.
///
/// Dispatch precedence:
/// - `APOLLIA_OAUTH`: exact match, no suffix. Resolves the current MCP HTTP
///   OAuth bearer for this server via
///   [`SecretResolver::resolve_oauth_bearer`] and expands to the full
///   `"Bearer …"` header value.
/// - `APOLLIA_SECRET:KEY`: static secret keyed by `"{server_name}:{KEY}"`.
/// - anything else: resolved from the process environment via
///   [`std::env::var`].
///
/// `secret_store` is required for both OAuth and static-secret resolution;
/// absent store + secret/oauth placeholder ⇒ [`McpConfigError::UnresolvedEnvVar`].
async fn resolve_single_var(
    server_name: &str,
    var_name: &str,
    secret_store: Option<&dyn SecretResolver>,
) -> Result<String, McpConfigError> {
    if var_name == "APOLLIA_OAUTH" {
        let store = secret_store.ok_or_else(|| McpConfigError::UnresolvedEnvVar {
            server: server_name.to_string(),
            var: var_name.to_string(),
        })?;
        let token = store.resolve_oauth_bearer(server_name).await.map_err(|_| {
            McpConfigError::UnresolvedEnvVar {
                server: server_name.to_string(),
                var: var_name.to_string(),
            }
        })?;
        // MCP HTTP OAuth always issues Bearer tokens (spec MUST). Prefix here
        // so the placeholder is foolproof in the user-facing config: the
        // operator writes `Authorization = "${APOLLIA_OAUTH}"`, not
        // `"Bearer ${APOLLIA_OAUTH}"`.
        return Ok(format!("Bearer {}", token));
    }
    if let Some(secret_key) = var_name.strip_prefix("APOLLIA_SECRET:") {
        let store = secret_store.ok_or_else(|| McpConfigError::UnresolvedEnvVar {
            server: server_name.to_string(),
            var: var_name.to_string(),
        })?;
        let key = format!("{}:{}", server_name, secret_key);
        return store
            .get_secret(&key)
            .map_err(|_| McpConfigError::UnresolvedEnvVar {
                server: server_name.to_string(),
                var: var_name.to_string(),
            });
    }
    std::env::var(var_name).map_err(|_| McpConfigError::UnresolvedEnvVar {
        server: server_name.to_string(),
        var: var_name.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// Serialises the tests that mutate the process environment (a process
    /// global), and lets each restore the previous value before it returns.
    /// A tokio mutex, because the mutation must span `resolve_env(..).await`.
    static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
            format_version: 1,
            name: "My Server!".to_string(),
            command: "npx".to_string(),
            args: vec![],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            max_response_bytes: default_max_response_bytes(),
            max_tools: 256,
            tags: vec![],
        };
        // WHEN / THEN
        assert!(matches!(
            config.validate(),
            Err(McpConfigError::InvalidServerName(_))
        ));
    }

    #[tokio::test]
    async fn test_resolve_env_interpolates_variable() {
        // GIVEN an env map containing ${TEST_MCP_KEY_327} and that variable set in the process env
        let _guard = ENV_LOCK.lock().await;
        let previous = std::env::var_os("TEST_MCP_KEY_327");
        std::env::set_var("TEST_MCP_KEY_327", "secret123");
        let config = McpServerConfig {
            format_version: 1,
            name: "test".to_string(),
            command: "npx".to_string(),
            args: vec![],
            env: HashMap::from([("API_KEY".to_string(), "${TEST_MCP_KEY_327}".to_string())]),
            transport: "stdio".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            max_response_bytes: default_max_response_bytes(),
            max_tools: 256,
            tags: vec![],
        };
        // WHEN
        let resolved = config.resolve_env(None).await.unwrap();
        // THEN
        assert_eq!(resolved["API_KEY"], "secret123");
        match previous {
            Some(v) => std::env::set_var("TEST_MCP_KEY_327", v),
            None => std::env::remove_var("TEST_MCP_KEY_327"),
        }
    }

    #[tokio::test]
    async fn test_unresolved_env_var_fails() {
        // GIVEN an env map referencing a variable absent from the process environment
        let _guard = ENV_LOCK.lock().await;
        std::env::remove_var("TOTALLY_MISSING_VAR_327");
        let config = McpServerConfig {
            format_version: 1,
            name: "test".to_string(),
            command: "npx".to_string(),
            args: vec![],
            env: HashMap::from([("KEY".to_string(), "${TOTALLY_MISSING_VAR_327}".to_string())]),
            transport: "stdio".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            max_response_bytes: default_max_response_bytes(),
            max_tools: 256,
            tags: vec![],
        };
        // WHEN / THEN
        assert!(matches!(
            config.resolve_env(None).await,
            Err(McpConfigError::UnresolvedEnvVar { .. })
        ));
    }

    #[test]
    fn test_unsupported_transport_fails() {
        // GIVEN a server configured with transport = "http"
        let config = McpServerConfig {
            format_version: 1,
            name: "test".to_string(),
            command: "npx".to_string(),
            args: vec![],
            env: HashMap::new(),
            transport: "http".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            max_response_bytes: default_max_response_bytes(),
            max_tools: 256,
            tags: vec![],
        };
        // WHEN / THEN
        assert!(matches!(
            config.validate(),
            Err(McpConfigError::UnsupportedTransport { .. })
        ));
    }

    #[tokio::test]
    async fn test_partial_interpolation_with_surrounding_text() {
        // GIVEN a value that mixes literal text and a placeholder
        let _guard = ENV_LOCK.lock().await;
        let previous = std::env::var_os("TEST_MCP_HOST_327");
        std::env::set_var("TEST_MCP_HOST_327", "localhost");
        let config = McpServerConfig {
            format_version: 1,
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
            max_response_bytes: default_max_response_bytes(),
            max_tools: 256,
            tags: vec![],
        };
        // WHEN
        let resolved = config.resolve_env(None).await.unwrap();
        // THEN
        assert_eq!(resolved["BASE_URL"], "http://localhost:8080");
        match previous {
            Some(v) => std::env::set_var("TEST_MCP_HOST_327", v),
            None => std::env::remove_var("TEST_MCP_HOST_327"),
        }
    }

    #[test]
    fn test_empty_server_name_fails() {
        // GIVEN a server with an empty name
        let config = McpServerConfig {
            format_version: 1,
            name: String::new(),
            command: "npx".to_string(),
            args: vec![],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            max_response_bytes: default_max_response_bytes(),
            max_tools: 256,
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
            format_version: 1,
            name: "notion".to_string(),
            command: String::new(),
            args: vec![],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            max_response_bytes: default_max_response_bytes(),
            max_tools: 256,
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
            format_version: 1,
            name: "local-files".to_string(),
            command: "@modelcontextprotocol/server-filesystem".to_string(),
            args: vec!["/tmp".to_string()],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            max_response_bytes: default_max_response_bytes(),
            max_tools: 256,
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
            format_version: 1,
            name: "github".to_string(),
            command: "io.github.github/github-mcp-server".to_string(),
            args: vec![],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            max_response_bytes: default_max_response_bytes(),
            max_tools: 256,
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
                format_version: 1,
                name: "ok".to_string(),
                command: cmd.to_string(),
                args: vec![],
                env: HashMap::new(),
                transport: "stdio".to_string(),
                url: None,
                requires_approval: false,
                init_timeout_secs: 30,
                call_timeout_secs: 60,
                max_response_bytes: default_max_response_bytes(),
                max_tools: 256,
                tags: vec![],
            };
            // WHEN / THEN: must accept all of these.
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

    #[async_trait::async_trait]
    impl SecretResolver for MockSecretStore {
        fn get_secret(&self, key: &str) -> Result<String, String> {
            self.secrets
                .get(key)
                .cloned()
                .ok_or_else(|| format!("not found: {key}"))
        }

        async fn resolve_oauth_bearer(&self, server_name: &str) -> Result<String, String> {
            // Mock supports OAuth lookups via a synthetic "oauth:{server_name}" key
            // so tests can exercise the placeholder without spinning up an AS.
            let key = format!("oauth:{}", server_name);
            self.secrets
                .get(&key)
                .cloned()
                .ok_or_else(|| format!("no oauth token for {server_name}"))
        }
    }

    fn server_with_env(name: &str, env: HashMap<String, String>) -> McpServerConfig {
        McpServerConfig {
            format_version: 1,
            name: name.to_string(),
            command: "npx".to_string(),
            args: vec![],
            env,
            transport: "stdio".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            max_response_bytes: default_max_response_bytes(),
            max_tools: 256,
            tags: vec![],
        }
    }

    #[tokio::test]
    async fn test_resolve_apollia_secret_from_store() {
        // GIVEN env = { KEY = "${APOLLIA_SECRET:MY_KEY}" } and a store holding "notion:MY_KEY"
        let store = MockSecretStore::with(&[("notion:MY_KEY", "tok_abc123")]);
        let config = server_with_env(
            "notion",
            HashMap::from([("KEY".to_string(), "${APOLLIA_SECRET:MY_KEY}".to_string())]),
        );
        // WHEN
        let resolved = config.resolve_env(Some(&store)).await.unwrap();
        // THEN the keyring value is injected
        assert_eq!(resolved["KEY"], "tok_abc123");
    }

    #[tokio::test]
    async fn test_resolve_normal_env_var_unchanged() {
        // GIVEN env = { KEY = "${HOME}" } and no secret store
        let config = server_with_env(
            "test",
            HashMap::from([("KEY".to_string(), "${HOME}".to_string())]),
        );
        let home = apollia_core::paths::home_string().unwrap_or_default();
        // WHEN
        let resolved = config.resolve_env(None).await.unwrap();
        // THEN the system env var is returned as before
        assert_eq!(resolved["KEY"], home);
    }

    #[tokio::test]
    async fn test_missing_secret_returns_unresolved_error() {
        // GIVEN env = { KEY = "${APOLLIA_SECRET:MISSING}" } and a store without that key
        let store = MockSecretStore::with(&[]);
        let config = server_with_env(
            "notion",
            HashMap::from([("KEY".to_string(), "${APOLLIA_SECRET:MISSING}".to_string())]),
        );
        // WHEN / THEN
        assert!(matches!(
            config.resolve_env(Some(&store)).await,
            Err(McpConfigError::UnresolvedEnvVar { .. })
        ));
    }

    #[tokio::test]
    async fn test_coexistence_secret_and_env_var() {
        // GIVEN env = { A = "${APOLLIA_SECRET:X}", B = "${HOME}" }
        let store = MockSecretStore::with(&[("svc:X", "from_keychain")]);
        let config = server_with_env(
            "svc",
            HashMap::from([
                ("A".to_string(), "${APOLLIA_SECRET:X}".to_string()),
                ("B".to_string(), "${HOME}".to_string()),
            ]),
        );
        let home = apollia_core::paths::home_string().unwrap_or_default();
        // WHEN
        let resolved = config.resolve_env(Some(&store)).await.unwrap();
        // THEN A comes from the keyring and B from the system environment
        assert_eq!(resolved["A"], "from_keychain");
        assert_eq!(resolved["B"], home);
    }

    #[tokio::test]
    async fn test_apollia_secret_without_store_returns_error() {
        // GIVEN env = { KEY = "${APOLLIA_SECRET:X}" } and secret_store = None
        let config = server_with_env(
            "svc",
            HashMap::from([("KEY".to_string(), "${APOLLIA_SECRET:X}".to_string())]),
        );
        // WHEN / THEN
        assert!(matches!(
            config.resolve_env(None).await,
            Err(McpConfigError::UnresolvedEnvVar { .. })
        ));
    }

    // ${APOLLIA_OAUTH} dispatch (OAuth Phase 3)

    #[tokio::test]
    async fn test_apollia_oauth_resolves_with_bearer_prefix() {
        // GIVEN env = { Authorization = "${APOLLIA_OAUTH}" } and a store with a
        // canned OAuth token for the server.
        let store = MockSecretStore::with(&[("oauth:linear", "at-12345")]);
        let config = server_with_env(
            "linear",
            HashMap::from([("Authorization".to_string(), "${APOLLIA_OAUTH}".to_string())]),
        );
        // WHEN
        let resolved = config.resolve_env(Some(&store)).await.unwrap();
        // THEN the placeholder expands to the full `Bearer …` header value:
        // the user writes only `${APOLLIA_OAUTH}`, no manual "Bearer " prefix.
        assert_eq!(resolved["Authorization"], "Bearer at-12345");
    }

    #[tokio::test]
    async fn test_apollia_oauth_returns_unresolved_when_no_store() {
        // GIVEN env = { Authorization = "${APOLLIA_OAUTH}" } and no store
        let config = server_with_env(
            "linear",
            HashMap::from([("Authorization".to_string(), "${APOLLIA_OAUTH}".to_string())]),
        );
        // WHEN / THEN: clear error, no panic
        assert!(matches!(
            config.resolve_env(None).await,
            Err(McpConfigError::UnresolvedEnvVar { .. })
        ));
    }

    #[tokio::test]
    async fn test_apollia_oauth_returns_unresolved_when_no_token() {
        // GIVEN env = { Authorization = "${APOLLIA_OAUTH}" } and a store
        // without any oauth entry for this server
        let store = MockSecretStore::with(&[]);
        let config = server_with_env(
            "linear",
            HashMap::from([("Authorization".to_string(), "${APOLLIA_OAUTH}".to_string())]),
        );
        // WHEN / THEN
        assert!(matches!(
            config.resolve_env(Some(&store)).await,
            Err(McpConfigError::UnresolvedEnvVar { .. })
        ));
    }

    #[tokio::test]
    async fn test_default_oauth_resolver_returns_error() {
        // GIVEN a SecretResolver implementor that does NOT override
        // resolve_oauth_bearer; it must fall back to the default impl which
        // returns "not configured" so the runtime fails fast rather than
        // silently leaving the header literal.
        struct OnlyStatic;
        #[async_trait::async_trait]
        impl SecretResolver for OnlyStatic {
            fn get_secret(&self, _key: &str) -> Result<String, String> {
                Err("unused".into())
            }
            // No resolve_oauth_bearer override; default impl in play.
        }
        let store = OnlyStatic;
        let config = server_with_env(
            "anyserver",
            HashMap::from([("Authorization".to_string(), "${APOLLIA_OAUTH}".to_string())]),
        );
        // WHEN / THEN
        let err = config.resolve_env(Some(&store)).await.unwrap_err();
        match err {
            McpConfigError::UnresolvedEnvVar { var, .. } => assert_eq!(var, "APOLLIA_OAUTH"),
            other => panic!("expected UnresolvedEnvVar, got {other:?}"),
        }
    }

    #[test]
    fn test_mcp_init_timeout_zero_returns_validation_error() {
        // GIVEN init_timeout_secs = 0 (below minimum of 1)
        let config = McpServerConfig {
            format_version: 1,
            name: "notion".to_string(),
            command: "npx".to_string(),
            args: vec![],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 0,
            call_timeout_secs: 60,
            max_response_bytes: default_max_response_bytes(),
            max_tools: 256,
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
            format_version: 1,
            name: "notion".to_string(),
            command: "npx".to_string(),
            args: vec![],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 301,
            call_timeout_secs: 60,
            max_response_bytes: default_max_response_bytes(),
            max_tools: 256,
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
            format_version: 1,
            name: "notion".to_string(),
            command: "npx".to_string(),
            args: vec![],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 0,
            max_response_bytes: default_max_response_bytes(),
            max_tools: 256,
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
            format_version: 1,
            name: "notion".to_string(),
            command: String::new(),
            args: vec![],
            env: HashMap::new(),
            transport: "streamable-http".to_string(),
            url: Some("localhost:8080".to_string()),
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            max_response_bytes: default_max_response_bytes(),
            max_tools: 256,
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
            format_version: 1,
            name: "local-mcp".to_string(),
            command: String::new(),
            args: vec![],
            env: HashMap::new(),
            transport: "streamable-http".to_string(),
            url: Some("http://localhost:8080/mcp".to_string()),
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            max_response_bytes: default_max_response_bytes(),
            max_tools: 256,
            tags: vec![],
        };
        // WHEN / THEN
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_streamable_http_with_url_passes_validation() {
        // GIVEN a config with transport = "streamable-http" and a valid url
        let config = McpServerConfig {
            format_version: 1,
            name: "notion".to_string(),
            command: String::new(),
            args: vec![],
            env: HashMap::new(),
            transport: "streamable-http".to_string(),
            url: Some("https://mcp.notion.ai/mcp".to_string()),
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            max_response_bytes: default_max_response_bytes(),
            max_tools: 256,
            tags: vec![],
        };
        // WHEN / THEN
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_streamable_http_without_url_fails() {
        // GIVEN a config with transport = "streamable-http" but no url
        let config = McpServerConfig {
            format_version: 1,
            name: "notion".to_string(),
            command: String::new(),
            args: vec![],
            env: HashMap::new(),
            transport: "streamable-http".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            max_response_bytes: default_max_response_bytes(),
            max_tools: 256,
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
            format_version: 1,
            name: "brave".to_string(),
            command: String::new(),
            args: vec![],
            env: HashMap::new(),
            transport: "sse".to_string(),
            url: Some("https://mcp.brave.com/sse".to_string()),
            requires_approval: false,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            max_response_bytes: default_max_response_bytes(),
            max_tools: 256,
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

    #[test]
    fn test_mcp_max_response_bytes_below_min_returns_validation_error() {
        // GIVEN max_response_bytes below the 1024-byte floor
        let mut config = server_with_env("notion", HashMap::new());
        config.max_response_bytes = 512;
        // WHEN / THEN
        assert!(matches!(
            config.validate(),
            Err(McpConfigError::InvalidMaxResponseBytes { value: 512, .. })
        ));
    }

    #[test]
    fn test_mcp_max_response_bytes_above_max_returns_validation_error() {
        // GIVEN max_response_bytes above the 1 GiB ceiling
        let mut config = server_with_env("notion", HashMap::new());
        config.max_response_bytes = 2 * 1024 * 1024 * 1024;
        // WHEN / THEN
        assert!(matches!(
            config.validate(),
            Err(McpConfigError::InvalidMaxResponseBytes { .. })
        ));
    }

    #[test]
    fn test_mcp_max_response_bytes_within_bounds_passes_validation() {
        // GIVEN a max_response_bytes inside [1024, 1 GiB]
        let mut config = server_with_env("notion", HashMap::new());
        config.max_response_bytes = 16 * 1024 * 1024;
        // WHEN / THEN
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_max_response_bytes_defaults_when_absent_from_toml() {
        // GIVEN a mcp.toml entry that omits max_response_bytes
        let toml_content = r#"
            [[servers]]
            name = "notion"
            command = "npx"
        "#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(toml_content.as_bytes()).unwrap();
        // WHEN the config is loaded
        let config = McpConfig::load(file.path()).unwrap();
        // THEN the serde default (8 MiB) is applied
        assert_eq!(config.servers[0].max_response_bytes, 8 * 1024 * 1024);
    }

    #[test]
    fn test_mcp_max_tools_below_min_returns_validation_error() {
        // GIVEN max_tools below the floor of 1 (zero would disable the server)
        let mut config = server_with_env("notion", HashMap::new());
        config.max_tools = 0;
        // WHEN / THEN
        assert!(matches!(
            config.validate(),
            Err(McpConfigError::InvalidMaxTools { value: 0, .. })
        ));
    }

    #[test]
    fn test_mcp_max_tools_above_max_returns_validation_error() {
        // GIVEN max_tools above the 8192 ceiling
        let mut config = server_with_env("notion", HashMap::new());
        config.max_tools = 8193;
        // WHEN / THEN
        assert!(matches!(
            config.validate(),
            Err(McpConfigError::InvalidMaxTools { value: 8193, .. })
        ));
    }

    #[test]
    fn test_mcp_max_tools_within_bounds_passes_validation() {
        // GIVEN a max_tools inside [1, 8192]
        let mut config = server_with_env("notion", HashMap::new());
        config.max_tools = 1024;
        // WHEN / THEN
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_max_tools_defaults_when_absent_from_toml() {
        // GIVEN a mcp.toml entry that omits max_tools
        let toml_content = r#"
            [[servers]]
            name = "notion"
            command = "npx"
        "#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(toml_content.as_bytes()).unwrap();
        // WHEN the config is loaded
        let config = McpConfig::load(file.path()).unwrap();
        // THEN the serde default (256) is applied
        assert_eq!(config.servers[0].max_tools, 256);
    }
}
