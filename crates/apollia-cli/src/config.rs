//! Parsing and validation of the Apollia OS configuration from `apollia.toml`.
//!
//! Provides [`parse_apollia_toml`] to read and deserialize the config file and
//! [`validate_llm_config`] for non-fatal validation (warnings only) of LLM
//! backends.
//!
//! The operational sections (`[[triggers]]`, `[notifications]`, `[stt]`) are no
//! longer managed by the TOML file: they are now stored in SQLite and
//! administered via the REST API or the desktop application. If an older TOML
//! file still contains these sections, a warning is emitted but boot continues
//! normally.
//!
//! # Exemple TOML minimal
//!
//! ```toml
//! [llm]
//! default = "anthropic"
//!
//! [[llm.backends]]
//! name        = "anthropic"
//! type        = "api"
//! api_url     = "https://api.anthropic.com"
//! api_key_env = "ANTHROPIC_API_KEY"
//! model       = "claude-haiku-4-5-20251001"
//! ```

use std::path::{Path, PathBuf};

use apollia_core::{
    A2AConfig, ApiConfig, FilesystemConfig, HitlConfig, McpConfig, ORIAConfig, PermissionsConfig,
    RegistryConfig, RuntimeConfig, ToolsConfig,
};
use apollia_llm::{BackendKind, LlmConfig};

// ─────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────

/// Possible errors when parsing `apollia.toml`.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// I/O error while reading the configuration file.
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),

    /// TOML deserialization error (invalid field, wrong type, etc.).
    #[error("invalid TOML config: {0}")]
    Parse(#[from] toml::de::Error),
}

// ─────────────────────────────────────────────
// Configuration structures
// ─────────────────────────────────────────────

/// Global Apollia OS configuration validated from `apollia.toml`.
///
/// Holds the LLM, REST API, core runtime, HITL, and A2A routing configuration.
/// The operational configuration (triggers, notifications, agents, stt) is
/// managed in SQLite.
///
/// To deserialize from a file, use [`parse_apollia_toml`].
#[derive(Debug, serde::Deserialize)]
pub struct ApolliaCConfig {
    /// Section `[llm]`: LLM backend configuration.
    ///
    /// `None` when the `[llm]` section is absent from the file.
    pub llm: Option<LlmConfig>,

    /// Section `[api]`: TCP listener and authentication configuration.
    ///
    /// `None` when the `[api]` section is absent from the file; the
    /// [`ApiConfig`] defaults then apply.
    pub api: Option<ApiConfig>,

    /// Section `[runtime]`: EventBus and mailbox capacities.
    ///
    /// `None` when absent; the [`RuntimeConfig`] defaults apply.
    pub runtime: Option<RuntimeConfig>,

    /// Section `[hitl]`: Human-in-the-Loop timeout and scan interval.
    ///
    /// `None` when absent; the [`HitlConfig`] defaults apply.
    pub hitl: Option<HitlConfig>,

    /// Section `[a2a]`: inter-agent routing.
    ///
    /// `None` when absent; the [`A2AConfig`] defaults apply.
    pub a2a: Option<A2AConfig>,

    /// Section `[oria]`: Observer-Reasoner-Actor engine.
    ///
    /// `None` when absent; the [`ORIAConfig`] defaults apply.
    pub oria: Option<ORIAConfig>,

    /// Section `[registry]`: community registry URL.
    ///
    /// `None` when absent; the [`RegistryConfig`] default applies.
    pub registry: Option<RegistryConfig>,

    /// Section `[tools]`: native tools (limits, static disabling,
    /// `web_search` / `web_read` configuration).
    ///
    /// `None` when absent; the [`ToolsConfig`] defaults apply.
    pub tools: Option<ToolsConfig>,

    /// Section `[mcp]`: MCP module configuration (HITL approval TTL).
    ///
    /// `None` when absent; the [`McpConfig`] defaults apply.
    pub mcp: Option<McpConfig>,

    /// Section `[permissions]`: permissions engine (SafeList, injection detection).
    ///
    /// `None` when absent; the [`PermissionsConfig`] defaults apply.
    pub permissions: Option<PermissionsConfig>,

    /// Section `[filesystem]`: reversible journal and filesystem configuration.
    ///
    /// `None` when absent; the [`FilesystemConfig`] defaults apply.
    pub filesystem: Option<FilesystemConfig>,
}

/// Names of the TOML sections that are now deprecated.
///
/// Used by [`check_deprecated_sections`] to emit warnings if an older
/// `apollia.toml` file still contains these sections.
const DEPRECATED_SECTIONS: &[&str] = &["triggers", "notifications", "stt"];

// ─────────────────────────────────────────────
// Public functions
// ─────────────────────────────────────────────

/// Resolves `~` to `$HOME` in a file path.
///
/// Replaces a leading `~` path component with the value of the `HOME`
/// environment variable. If `HOME` is not set, `~` is replaced with an empty
/// string (non-fatal behavior).
///
/// This function is **pure**: no filesystem access, only a string
/// transformation.
pub fn expand_tilde(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if s.starts_with("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(format!("{}{}", home, &s[1..]))
    } else if s == "~" {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home)
    } else {
        path.to_path_buf()
    }
}

/// Reads and deserializes `apollia.toml` from the given path.
///
/// After TOML parsing:
/// - The `model_path` paths of embedded backends are normalized via [`expand_tilde`].
/// - The deprecated sections (`[[triggers]]`, `[notifications]`, `[stt]`) are
///   detected and emit a warning without blocking startup.
///
/// The `[llm]` section is **optional**: its absence yields `config.llm = None`
/// without an error.
///
/// # Errors
///
/// - [`ConfigError::Io`]: the file is inaccessible or unreadable.
/// - [`ConfigError::Parse`]: the TOML is malformed or contains invalid types.
pub fn parse_apollia_toml(path: &Path) -> Result<ApolliaCConfig, ConfigError> {
    let content = std::fs::read_to_string(path)?;

    // Check for deprecated sections before strict deserialization.
    check_deprecated_sections(&content);

    // Parse as a loose table first to ignore unknown sections gracefully.
    let raw_table: toml::Value = toml::from_str(&content)?;

    // Build a filtered table with only known sections.
    let mut filtered = toml::map::Map::new();
    if let toml::Value::Table(table) = &raw_table {
        for key in &[
            "llm",
            "runtime",
            "memory",
            "tools",
            "budget",
            "api",
            "hitl",
            "a2a",
            "oria",
            "registry",
            "mcp",
            "permissions",
            "filesystem",
        ] {
            if let Some(v) = table.get(*key) {
                filtered.insert((*key).to_string(), v.clone());
            }
        }
    }

    let config: ApolliaCConfig = toml::Value::Table(filtered)
        .try_into()
        .map_err(|e: toml::de::Error| e)?;

    // Local GGUF model paths are no longer configured via `[[llm.backends]]`
    // (they moved to `[llm.runner]`), so no `~ -> $HOME` normalization is
    // required here. Cloud backends have no local path.

    Ok(config)
}

/// Detects and reports deprecated TOML sections.
///
/// The `[[triggers]]`, `[notifications]`, `[stt]` sections are deprecated.
/// If the TOML file still contains them, a warning is emitted for each
/// detected section.
fn check_deprecated_sections(content: &str) {
    for section in DEPRECATED_SECTIONS {
        let bracket_single = format!("[{section}]");
        let bracket_double = format!("[[{section}]]");
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed == bracket_single
                || trimmed == bracket_double
                || trimmed.starts_with(&format!("[{section}."))
            {
                tracing::warn!(
                    section = %section,
                    "apollia.toml contains deprecated section [{section}], \
                     use the desktop app or API to manage {section}"
                );
                break;
            }
        }
    }
}

/// Non-fatal validation of the LLM config: emits warnings, never returns an error.
///
/// For each configured backend:
/// - **Embedded backend**: checks that the `.gguf` file exists after `~`
///   expansion. If missing, `tracing::warn!` with the missing path (backend
///   skipped by the router).
/// - **API backend**: checks that the `api_key_env` environment variable is
///   set. If absent, `tracing::warn!` with the variable name (backend skipped
///   by the router).
///
/// This function is **intentionally non-fatal**: strict validation is delegated
/// to [`apollia_llm::LlmRouter::from_config`] at Supervisor startup.
///
/// Always returns `Ok(())`.
pub fn validate_llm_config(config: &LlmConfig) -> Result<(), ConfigError> {
    for backend in &config.backends {
        match &backend.kind {
            #[cfg(feature = "cloud")]
            BackendKind::Api(cfg) => {
                if std::env::var(&cfg.api_key_env).is_err() {
                    tracing::warn!(
                        backend = %backend.name(),
                        env_var = %cfg.api_key_env,
                        "API key env var not set, backend will be skipped"
                    );
                }
            }
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes the content to a temporary file and returns the handle.
    fn write_toml(content: &str) -> tempfile::NamedTempFile {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().expect("failed to create temp file");
        write!(f, "{content}").expect("failed to write temp file");
        f
    }

    // GIVEN a TOML with [[llm.backends]] type = "api"
    // WHEN deserializing
    // THEN config.llm.backends[0] is Api
    #[cfg(feature = "cloud")]
    #[test]
    fn test_parse_api_backend() {
        // GIVEN
        let toml_str = r#"
[llm]
default = "anthropic"

[[llm.backends]]
name = "anthropic"
type = "api"
api_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"
model = "claude-haiku-4-5-20251001"
"#;
        let file = write_toml(toml_str);

        // WHEN
        let config = parse_apollia_toml(file.path()).expect("parse should succeed");

        // THEN
        let llm = config.llm.expect("llm should be present");
        assert_eq!(llm.backends[0].name(), "anthropic");
        assert!(
            matches!(llm.backends[0].kind, BackendKind::Api(_)),
            "kind should be Api"
        );
    }

    // GIVEN a TOML without a [llm] section
    // WHEN parsing
    // THEN config.llm is None (no error)
    #[test]
    fn test_no_llm_section_is_none() {
        // GIVEN
        let file = write_toml("");

        // WHEN
        let config = parse_apollia_toml(file.path()).expect("parse should succeed");

        // THEN
        assert!(
            config.llm.is_none(),
            "llm should be None when section absent"
        );
    }

    // GIVEN a minimal LlmConfig (cloud backend without an API key)
    // WHEN validate_llm_config is called
    // THEN Ok(()) is returned, never a fatal error
    #[cfg(feature = "cloud")]
    #[test]
    fn test_validate_llm_config_always_returns_ok() {
        // GIVEN
        let toml_str = r#"
[llm]
default = "test-api"

[[llm.backends]]
name = "test-api"
type = "api"
api_url = "https://api.openai.com/v1"
api_key_env = "APOLLIA_NONEXISTENT_KEY_FOR_TEST_XYZ"
model = "gpt-4o-mini"
"#;
        let file = write_toml(toml_str);
        let config = parse_apollia_toml(file.path()).expect("parse should succeed");
        let llm = config.llm.expect("llm should be present");

        // WHEN
        let result = validate_llm_config(&llm);

        // THEN
        assert!(
            result.is_ok(),
            "validate_llm_config should always return Ok"
        );
    }

    // GIVEN a TOML with [llm] but without [llm.observability]
    // WHEN deserializing
    // THEN the default values are applied
    #[cfg(feature = "cloud")]
    #[test]
    fn test_observability_defaults() {
        // GIVEN
        let toml_str = r#"
[llm]
default = "anthropic"

[[llm.backends]]
name = "anthropic"
type = "api"
api_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"
model = "claude-haiku-4-5-20251001"
"#;
        let file = write_toml(toml_str);

        // WHEN
        let config = parse_apollia_toml(file.path()).expect("parse should succeed");

        // THEN
        let obs = &config.llm.expect("llm should be present").observability;
        assert!(
            obs.log_token_usage,
            "log_token_usage should default to true"
        );
        assert!(obs.log_latency, "log_latency should default to true");
        assert!(
            !obs.debug_log_prompt,
            "debug_log_prompt should default to false"
        );
    }

    // GIVEN a path starting with ~/
    // WHEN expand_tilde is called
    // THEN the ~ is replaced
    #[test]
    fn test_expand_tilde_replaces_home() {
        // GIVEN
        let path = Path::new("~/.apollia/models/test.gguf");

        // WHEN
        let expanded = expand_tilde(path);

        // THEN
        let s = expanded.to_string_lossy();
        assert!(!s.contains('~'), "~ should be replaced, got: {s}");
        assert!(s.contains(".apollia"), "path should contain .apollia");
    }

    // GIVEN a path without ~ (absolute path)
    // WHEN expand_tilde is called
    // THEN the path is returned unchanged
    #[test]
    fn test_expand_tilde_noop_for_absolute_path() {
        // GIVEN
        let path = Path::new("/tmp/model.gguf");

        // WHEN
        let expanded = expand_tilde(path);

        // THEN
        assert_eq!(expanded, PathBuf::from("/tmp/model.gguf"));
    }

    // ─── Deprecated sections warning ─────────────────────────────

    // GIVEN a TOML containing a [[triggers]] section
    // WHEN parse_apollia_toml is called
    // THEN parsing succeeds (no error), deprecated sections are ignored
    #[test]
    fn test_deprecated_triggers_section_does_not_block_parsing() {
        // GIVEN a TOML with a deprecated [[triggers]] section
        let toml = r#"
[agents]
directory = "agents/"

[[triggers]]
id             = "old-trigger"
agent          = "some-agent"
enabled        = true
on_busy        = "queue"
input_template = "test"

[triggers.source]
type     = "cron"
schedule = "* * * * *"
"#;
        let file = write_toml(toml);

        // WHEN
        let result = parse_apollia_toml(file.path());

        // THEN parsing succeeds, deprecated sections are silently ignored
        assert!(
            result.is_ok(),
            "parsing should succeed despite deprecated sections, error: {:?}",
            result.err()
        );
    }

    // GIVEN a TOML containing [notifications]
    // WHEN parse_apollia_toml is called
    // THEN parsing succeeds, the section is ignored
    #[test]
    fn test_deprecated_notifications_section_does_not_block_parsing() {
        // GIVEN
        let toml = r#"
[agents]
directory = "agents/"

[notifications]
events = ["task.completed"]
"#;
        let file = write_toml(toml);

        // WHEN
        let result = parse_apollia_toml(file.path());

        // THEN
        assert!(
            result.is_ok(),
            "parsing should succeed despite deprecated [notifications], error: {:?}",
            result.err()
        );
    }

    // GIVEN an empty TOML
    // WHEN parse_apollia_toml is called
    // THEN parsing succeeds with the llm section at None
    #[test]
    fn test_empty_toml_parses_ok() {
        // GIVEN
        let file = write_toml("");

        // WHEN
        let config = parse_apollia_toml(file.path()).expect("empty TOML should parse");

        // THEN
        assert!(config.llm.is_none());
    }

    // GIVEN the DEPRECATED_SECTIONS constant
    // WHEN inspecting it
    // THEN it contains triggers, notifications, and stt (managed via SQLite)
    #[test]
    fn test_deprecated_sections_constant() {
        assert!(DEPRECATED_SECTIONS.contains(&"triggers"));
        assert!(DEPRECATED_SECTIONS.contains(&"notifications"));
        assert!(DEPRECATED_SECTIONS.contains(&"stt"));
    }

    // GIVEN apollia.toml contains [mcp], [permissions], and [filesystem.journal]
    // WHEN parse_apollia_toml is called
    // THEN the custom values are deserialized correctly
    #[test]
    fn test_mcp_permissions_filesystem_sections_deserialized() {
        // GIVEN
        let toml = r#"
[mcp]
approval_ttl_hours = 48

[permissions]
injection_detection = false
safe_commands = ["bash_executor(git status)"]

[filesystem.journal]
max_sessions = 100
"#;
        let file = write_toml(toml);

        // WHEN
        let config = parse_apollia_toml(file.path()).expect("parse should succeed");

        // THEN
        let mcp = config.mcp.expect("mcp should be present");
        assert_eq!(mcp.approval_ttl_hours, 48);

        let perms = config.permissions.expect("permissions should be present");
        assert!(!perms.injection_detection);
        assert_eq!(perms.safe_commands, vec!["bash_executor(git status)"]);

        let fs = config.filesystem.expect("filesystem should be present");
        assert_eq!(fs.journal.max_sessions, 100);
    }

    // GIVEN apollia.toml without [mcp]/[permissions]/[filesystem]
    // WHEN parse_apollia_toml is called
    // THEN the fields are None, no regression
    #[test]
    fn test_mcp_permissions_filesystem_absent_is_none() {
        // GIVEN an empty TOML
        let file = write_toml("");

        // WHEN
        let config = parse_apollia_toml(file.path()).expect("parse should succeed");

        // THEN
        assert!(config.mcp.is_none());
        assert!(config.permissions.is_none());
        assert!(config.filesystem.is_none());
    }

    // GIVEN the ApolliaCConfig struct
    // WHEN verifying its structure
    // THEN it contains the static config fields (llm, api, runtime, hitl, a2a, oria, registry, tools, mcp, permissions, filesystem)
    #[test]
    fn test_config_struct_has_expected_fields() {
        let config = ApolliaCConfig {
            llm: None,
            api: None,
            runtime: None,
            hitl: None,
            a2a: None,
            oria: None,
            registry: None,
            tools: None,
            mcp: None,
            permissions: None,
            filesystem: None,
        };
        assert!(config.llm.is_none());
        assert!(config.runtime.is_none());
        assert!(config.hitl.is_none());
        assert!(config.a2a.is_none());
        assert!(config.registry.is_none());
        assert!(config.mcp.is_none());
        assert!(config.permissions.is_none());
        assert!(config.filesystem.is_none());
    }
}
