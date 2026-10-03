//! Parsing and validation of the Apollia OS configuration from `apollia.toml`.
//!
//! The schema and its validation live in `apollia-core`
//! ([`apollia_core::config::validate`]), shared with the desktop application
//! and the embedded runtime: one root, strict at every level, faults placed at
//! their line. This module reads the file for the CLI and the daemon and
//! carries the non-fatal [`validate_llm_config`] check of LLM backends.
//!
//! The operational sections (`[[triggers]]`, `[notifications]`, `[stt]`) are no
//! longer managed by the TOML file: they are stored in SQLite and administered
//! via the REST API or the desktop application. They are withdrawn keys of the
//! registry: a file that still carries one loads, with a `config.key.retired`
//! warning.
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

pub use apollia_core::config::{ApolliaConfig, ConfigDiagnostic};
use apollia_llm::{BackendKind, LlmConfig};

// ─────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────

/// Possible errors when reading `apollia.toml`.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// I/O error while reading the configuration file.
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),

    /// The file was refused: every fault, each at its line.
    #[error("{}", render_diagnostics(.0))]
    Invalid(Vec<ConfigDiagnostic>),
}

/// One diagnostic per line, `line:column: path: message`.
pub fn render_diagnostics(diagnostics: &[ConfigDiagnostic]) -> String {
    diagnostics
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

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
        let home = apollia_core::paths::home_string().unwrap_or_default();
        PathBuf::from(format!("{}{}", home, &s[1..]))
    } else if s == "~" {
        let home = apollia_core::paths::home_string().unwrap_or_default();
        PathBuf::from(home)
    } else {
        path.to_path_buf()
    }
}

/// Reads and validates `apollia.toml` from the given path.
///
/// The `[llm]` section is **optional**: its absence yields `config.llm = None`
/// without an error.
///
/// # Errors
///
/// - [`ConfigError::Io`]: the file is inaccessible or unreadable.
/// - [`ConfigError::Invalid`]: the file is refused by
///   [`apollia_core::config::validate`].
pub fn parse_apollia_toml(path: &Path) -> Result<ApolliaConfig, ConfigError> {
    let content = std::fs::read_to_string(path)?;
    validate_apollia_toml_str(&content)
}

/// Validates TOML `content` with the shared validation of `apollia-core`.
///
/// Same semantics as [`parse_apollia_toml`] on an in-memory string, so callers
/// (e.g. `config set`) can validate a candidate document before persisting it.
///
/// # Errors
///
/// [`ConfigError::Invalid`] with every fault found.
pub fn validate_apollia_toml_str(content: &str) -> Result<ApolliaConfig, ConfigError> {
    apollia_core::config::validate(content).map_err(ConfigError::Invalid)
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
            BackendKind::Api(cfg) => {
                if std::env::var(&cfg.api_key_env).is_err() {
                    tracing::warn!(
                        backend = %backend.name(),
                        env_var = %cfg.api_key_env,
                        detail = "the backend is skipped",
                        "llm.api_key.absent"
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

    // GIVEN an apollia.toml written for an earlier version, still carrying [llm.vertex]
    // WHEN parsing
    // THEN the file loads and its backends are read, the retired table ignored
    #[test]
    fn test_retired_llm_vertex_section_still_loads() {
        // GIVEN
        let toml_str = r#"
[llm]
default = "local"

[[llm.backends]]
name = "local"
type = "api"
api_url = "http://localhost:8080/v1"
api_key_env = "LOCAL_SERVER_KEY"
model = "qwen"

[llm.vertex]
enabled    = true
project_id = "my-gcp-project"
location   = "us-east5"
model_id   = "claude-sonnet-4-6@20251001"
"#;
        let file = write_toml(toml_str);

        // WHEN
        let config = parse_apollia_toml(file.path());

        // THEN
        let llm = config
            .expect("a retired section must not fail the load")
            .llm
            .expect("llm should be present");
        assert_eq!(llm.backends[0].name(), "local");
    }

    // GIVEN a minimal LlmConfig (cloud backend without an API key)
    // WHEN validate_llm_config is called
    // THEN Ok(()) is returned, never a fatal error
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

    // GIVEN a section no structure declares and the registry does not withdraw
    // WHEN parse_apollia_toml is called
    // THEN the file is refused, naming the section at its line
    #[test]
    fn test_an_unknown_section_is_refused_at_its_line() {
        // GIVEN
        let file =
            write_toml("[chat]\nplan_mode_default = true\n\n[agents]\ndirectory = \"agents/\"\n");

        // WHEN
        let result = parse_apollia_toml(file.path());

        // THEN
        let Err(ConfigError::Invalid(diagnostics)) = result else {
            panic!("an unknown section must be refused, got {result:?}");
        };
        assert_eq!(diagnostics[0].line, 4, "{diagnostics:?}");
        assert!(diagnostics[0].message.contains("agents"), "{diagnostics:?}");
    }

    // GIVEN apollia.toml contains [mcp] plus two sections that nothing reads
    // WHEN parse_apollia_toml is called
    // THEN [mcp] is deserialized and the inert sections are filtered out
    #[test]
    fn test_mcp_deserialized_and_inert_sections_filtered() {
        // GIVEN a file mixing a live section with two inert ones
        let toml = r#"
[mcp]
approval_ttl_hours = 48

[permissions]
db_path = "~/.apollia/governance.db"

[filesystem.journal]
max_sessions = 100
"#;
        let file = write_toml(toml);

        // WHEN
        let config = parse_apollia_toml(file.path()).expect("parse should succeed");

        // THEN the live section is honoured
        assert_eq!(config.mcp.approval_ttl_hours, 48);

        // AND the inert section is a withdrawn key of the registry, so no
        // caller can mistake it for a setting that does something.
        assert!(apollia_core::config::keys::withdrawn_covering("permissions").is_some());

        // AND `[filesystem]` is live again: `trusted_paths` reaches the risk
        // classifier, which is what decides whether an operation runs or asks.
        assert!(apollia_core::config::keys::withdrawn_covering("filesystem.journal").is_none());
        assert_eq!(config.filesystem.journal.max_sessions, 100);
    }

    // GIVEN the four sections that deserialized into an unread struct
    // WHEN their keys are looked up in the registry
    // THEN each is withdrawn, so `config set` refuses it and a file gets a warning
    #[test]
    fn test_inert_sections_are_withdrawn_keys() {
        // GIVEN
        let sections = ["a2a", "oria", "registry", "permissions"];

        // WHEN
        let withdrawn: Vec<_> = sections
            .iter()
            .filter(|s| apollia_core::config::keys::withdrawn_covering(&format!("{s}.x")).is_some())
            .collect();

        // THEN
        assert_eq!(withdrawn.len(), sections.len(), "{withdrawn:?}");
    }

    // GIVEN an apollia.toml carrying an inert section
    // WHEN it is validated
    // THEN parsing still succeeds: removing the surface breaks no existing file
    #[test]
    fn test_inert_section_does_not_break_an_existing_file() {
        // GIVEN a file written before the sections were withdrawn
        let toml = r#"
[permissions]
db_path = "~/.apollia/governance.db"

[oria]
max_replans = 3

[hitl]
timeout_hours = 12
"#;

        // WHEN
        let config = validate_apollia_toml_str(toml).expect("parse should succeed");

        // THEN the live section is unaffected
        assert_eq!(config.hitl.timeout_hours, Some(12));
    }

    // GIVEN apollia.toml with a [chat] section enabling plan mode by default
    // WHEN parse_apollia_toml is called
    // THEN the chat config is present and the flag is on
    #[test]
    fn test_chat_section_plan_mode_default_parses() {
        // GIVEN
        let toml = r#"
[chat]
plan_mode_default = true
"#;
        let file = write_toml(toml);

        // WHEN
        let config = parse_apollia_toml(file.path()).expect("parse should succeed");

        // THEN
        assert!(config.chat.plan_mode_default);
    }

    // GIVEN apollia.toml without a [chat] section
    // WHEN parse_apollia_toml is called
    // THEN the chat section has its defaults (plan mode off)
    #[test]
    fn test_chat_section_absent_takes_the_defaults() {
        // GIVEN an empty TOML
        let file = write_toml("");

        // WHEN
        let config = parse_apollia_toml(file.path()).expect("parse should succeed");

        // THEN
        assert!(!config.chat.plan_mode_default);
    }

    // GIVEN an apollia.toml without a [desktop] section
    // WHEN it is parsed
    // THEN it loads, and the desktop section reads as its defaults
    #[test]
    fn test_desktop_section_absent_loads_the_defaults() {
        // GIVEN
        let file = write_toml("[chat]\nplan_mode_default = true\n");

        // WHEN
        let config = parse_apollia_toml(file.path()).expect("parse should succeed");

        // THEN
        assert!(!config.desktop.developer_mode);
    }

    // GIVEN an apollia.toml turning developer mode on
    // WHEN it is parsed
    // THEN the typed desktop section carries it
    #[test]
    fn test_desktop_section_developer_mode_parses() {
        // GIVEN
        let file = write_toml("[desktop]\ndeveloper_mode = true\n");

        // WHEN
        let config = parse_apollia_toml(file.path()).expect("parse should succeed");

        // THEN
        assert!(config.desktop.developer_mode);
    }

    // GIVEN an apollia.toml turning the compatible server on
    // WHEN it is parsed
    // THEN the typed server section carries it, and an unknown key is refused
    #[test]
    fn test_server_section_parses_and_refuses_an_unknown_key() {
        // GIVEN
        let file = write_toml("[server]\nenabled = true\nport = 8123\n");
        let typo = write_toml("[server]\nenabeld = true\n");

        // WHEN
        let config = parse_apollia_toml(file.path()).expect("parse should succeed");
        let refused = parse_apollia_toml(typo.path());

        // THEN
        assert!(config.server.enabled);
        assert_eq!(config.server.port, 8123);
        let diagnostics = match refused.err() {
            Some(ConfigError::Invalid(diagnostics)) => diagnostics,
            _ => Vec::new(),
        };
        assert!(
            diagnostics
                .iter()
                .any(|d| d.path == "server.enabeld" && d.line == 2),
            "{diagnostics:?}"
        );
    }
}
