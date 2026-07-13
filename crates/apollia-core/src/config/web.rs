use serde::Deserialize;

use super::{validate_bounds, ConfigError};

// ─────────────────────────────────────────────
// WebSearchConfig
// ─────────────────────────────────────────────

/// Configuration of the `web_search` tool (`[tools.web_search]` section).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct WebSearchConfig {
    /// Preferred backend: `auto`, `duckduckgo`, or `brave`. Default: `auto`.
    #[serde(default)]
    pub backend: WebSearchBackend,

    /// If `true`, boot fails when the selected backend is not operational
    /// (e.g. `backend = "brave"` without an API key). Default: `false`.
    #[serde(default)]
    pub require_configured: bool,

    /// Brave Search backend configuration.
    #[serde(default)]
    pub brave: BraveBackendConfig,

    /// DuckDuckGo backend configuration.
    #[serde(default)]
    pub duckduckgo: DuckDuckGoBackendConfig,
}

impl WebSearchConfig {
    /// Validates the bounds of the backend sub-configurations.
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.brave.validate()?;
        self.duckduckgo.validate()?;
        Ok(())
    }
}

/// `web_search` backend choice exposed by `apollia.toml`.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WebSearchBackend {
    /// Automatic selection: DuckDuckGo first, Brave if configured.
    #[default]
    Auto,
    /// Force DuckDuckGo (zero-config, always available).
    DuckDuckGo,
    /// Force Brave Search, requires a valid API key.
    Brave,
}

/// Brave backend configuration (`[tools.web_search.brave]` section).
#[derive(Debug, Clone, Deserialize)]
pub struct BraveBackendConfig {
    /// Environment variable holding the Brave API key.
    /// Default: `"BRAVE_SEARCH_API_KEY"`.
    #[serde(default = "default_brave_env_var")]
    pub api_key_env_var: String,

    /// HTTP request timeout in seconds. Default: 15. Bounds: [1, 120].
    #[serde(default = "default_web_timeout_secs")]
    pub timeout_secs: u64,

    /// Maximum number of results requested from Brave per query.
    /// Default: 10. Bounds: [1, 20].
    #[serde(default = "default_brave_max_results")]
    pub max_results: u8,
}

impl Default for BraveBackendConfig {
    fn default() -> Self {
        Self {
            api_key_env_var: default_brave_env_var(),
            timeout_secs: default_web_timeout_secs(),
            max_results: default_brave_max_results(),
        }
    }
}

impl BraveBackendConfig {
    /// Validates the bounds of the numeric fields.
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "tools.web_search.brave.timeout_secs",
            self.timeout_secs,
            1_u64,
            120_u64,
        )?;
        validate_bounds(
            "tools.web_search.brave.max_results",
            self.max_results,
            1_u8,
            20_u8,
        )?;
        Ok(())
    }
}

/// DuckDuckGo backend configuration (`[tools.web_search.duckduckgo]` section).
#[derive(Debug, Clone, Deserialize)]
pub struct DuckDuckGoBackendConfig {
    /// HTTP request timeout in seconds. Default: 15. Bounds: [1, 120].
    #[serde(default = "default_web_timeout_secs")]
    pub timeout_secs: u64,

    /// Maximum HTTP response size in kilobytes before giving up.
    /// Default: 1024. Bounds: [16, 16 384].
    #[serde(default = "default_ddg_max_response_kb")]
    pub max_response_kb: u32,
}

impl Default for DuckDuckGoBackendConfig {
    fn default() -> Self {
        Self {
            timeout_secs: default_web_timeout_secs(),
            max_response_kb: default_ddg_max_response_kb(),
        }
    }
}

impl DuckDuckGoBackendConfig {
    /// Validates the bounds of the numeric fields.
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "tools.web_search.duckduckgo.timeout_secs",
            self.timeout_secs,
            1_u64,
            120_u64,
        )?;
        validate_bounds(
            "tools.web_search.duckduckgo.max_response_kb",
            self.max_response_kb,
            16_u32,
            16_384_u32,
        )?;
        Ok(())
    }
}

/// Configuration of the `web_read` tool (`[tools.web_read]` section).
#[derive(Debug, Clone, Deserialize)]
pub struct WebReadConfig {
    /// HTTP request timeout in seconds. Default: 20. Bounds: [1, 120].
    #[serde(default = "default_webread_timeout_secs")]
    pub timeout_secs: u64,

    /// Maximum HTTP response size in kilobytes before giving up.
    /// Default: 2048 (2 MB). Bounds: [64, 32 768].
    #[serde(default = "default_webread_max_response_kb")]
    pub max_response_kb: u32,

    /// Enables the anti-SSRF guard (rejects private and loopback hosts). Default: `true`.
    #[serde(default = "default_true")]
    pub ssrf_guard: bool,
}

impl Default for WebReadConfig {
    fn default() -> Self {
        Self {
            timeout_secs: default_webread_timeout_secs(),
            max_response_kb: default_webread_max_response_kb(),
            ssrf_guard: true,
        }
    }
}

impl WebReadConfig {
    /// Validates the bounds of the numeric fields.
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "tools.web_read.timeout_secs",
            self.timeout_secs,
            1_u64,
            120_u64,
        )?;
        validate_bounds(
            "tools.web_read.max_response_kb",
            self.max_response_kb,
            64_u32,
            32_768_u32,
        )?;
        Ok(())
    }
}

fn default_brave_env_var() -> String {
    "BRAVE_SEARCH_API_KEY".to_string()
}

fn default_web_timeout_secs() -> u64 {
    15
}

fn default_brave_max_results() -> u8 {
    10
}

fn default_ddg_max_response_kb() -> u32 {
    1024
}

fn default_webread_timeout_secs() -> u64 {
    20
}

fn default_webread_max_response_kb() -> u32 {
    2048
}

fn default_true() -> bool {
    true
}
