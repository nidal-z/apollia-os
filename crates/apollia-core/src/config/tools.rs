use serde::Deserialize;

use super::{validate_bounds, ConfigError, WebReadConfig, WebSearchConfig};

// ─────────────────────────────────────────────
// ToolsConfig
// ─────────────────────────────────────────────

/// Native tool output configuration (`[tools]` section in `apollia.toml`).
///
/// Controls the limits the runtime applies to native tool outputs before
/// forwarding them to the LLM. Protects the LLM context window against large
/// outputs (one of the non-negotiable safeguards).
#[derive(Debug, Clone, Deserialize)]
pub struct ToolsConfig {
    /// Maximum size of a tool output forwarded to the LLM, in UTF-8 bytes.
    ///
    /// Outputs exceeding this limit are truncated with a "middle-trim" strategy:
    /// the start and end are preserved, the middle is removed and replaced by a
    /// message stating how many lines were lost.
    /// Default: 30 000. Bounds: [10, 1 000 000].
    #[serde(default = "default_max_output_chars")]
    pub max_output_chars: usize,

    /// Regex pattern for extracting paths from bash output.
    ///
    /// Used by `FilePathExtractor` to parse the lightweight LLM response.
    /// Default: `None`, meaning the built-in pattern (Unix paths per
    /// POSIX.1-2017 and Windows UNC per RFC 8089) is applied.
    /// Configurable for environments with custom naming conventions.
    ///
    /// `apollia.toml` example:
    /// ```toml
    /// [tools]
    /// # file_path_extraction_pattern = "(?:/[^\\s]+)"
    /// ```
    #[serde(default)]
    pub file_path_extraction_pattern: Option<String>,

    /// Native tools statically disabled by the operator in `apollia.toml`.
    ///
    /// The names listed here are removed from the dispatcher at boot, so any
    /// invocation results in `UnknownTool`. This list complements the `tools`
    /// table in `governance.db`: a tool disabled in either one is inactive.
    /// Default: `[]`.
    #[serde(default)]
    pub disabled: Vec<String>,

    /// Configuration of the native `web_search` tool.
    #[serde(default)]
    pub web_search: WebSearchConfig,

    /// Configuration of the native `web_read` tool.
    #[serde(default)]
    pub web_read: WebReadConfig,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            max_output_chars: default_max_output_chars(),
            file_path_extraction_pattern: None,
            disabled: Vec::new(),
            web_search: WebSearchConfig::default(),
            web_read: WebReadConfig::default(),
        }
    }
}

impl ToolsConfig {
    /// Validates the tools configuration bounds at startup (fail-fast).
    ///
    /// - `max_output_chars`: must be in [10, 1 000 000].
    /// - `web_search.brave.max_results`: must be in [1, 20].
    /// - `web_search.brave.timeout_secs` and `web_search.duckduckgo.timeout_secs`: `[1, 120]`.
    /// - `web_search.duckduckgo.max_response_kb`: `[16, 16 384]`.
    /// - `web_read.timeout_secs`: `[1, 120]`. `web_read.max_response_kb`: `[64, 32 768]`.
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "tools.max_output_chars",
            self.max_output_chars,
            10_usize,
            1_000_000_usize,
        )?;
        self.web_search.validate()?;
        self.web_read.validate()?;
        Ok(())
    }
}

fn default_max_output_chars() -> usize {
    30_000
}
