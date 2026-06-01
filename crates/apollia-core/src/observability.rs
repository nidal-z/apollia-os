//! Observability configuration and utilities.
//!
//! Provides [`ObservabilityConfig`] to control truncation of persisted data
//! (inputs, outputs, stdout/stderr) and [`truncate_with_marker`] to truncate
//! cleanly on a UTF-8 boundary.

use serde::{Deserialize, Serialize};

/// Default max size of task/step inputs in bytes.
const DEFAULT_MAX_INPUT_BYTES: usize = 32_768;

/// Default max size of task/step/completion outputs in bytes.
const DEFAULT_MAX_OUTPUT_BYTES: usize = 32_768;

/// Default max size of tool stdout/stderr in bytes.
const DEFAULT_MAX_TOOL_OUTPUT_BYTES: usize = 10_240;

/// Truncation configuration for observability.
///
/// Controls the size limits for data persisted in SQLite. The defaults are
/// reasonable for common usage; they can be overridden via `apollia.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    /// Max size of task/step inputs in bytes (default 32768).
    #[serde(default = "default_max_input_bytes")]
    pub max_input_bytes: usize,

    /// Max size of task/step/completion outputs in bytes (default 32768).
    #[serde(default = "default_max_output_bytes")]
    pub max_output_bytes: usize,

    /// Max size of tool stdout/stderr in bytes (default 10240).
    #[serde(default = "default_max_tool_output_bytes")]
    pub max_tool_output_bytes: usize,

    /// If true, persists the LLM prompt_text (default false).
    #[serde(default)]
    pub debug_log_prompt: bool,

    // Granular capture for `runtime_events`.
    /// If `true`, persists ReAct `Thought` records on the trace (default
    /// `true`). Disabling empties the reasoning bubbles in the builder UI.
    #[serde(default = "default_capture_on")]
    pub capture_thoughts: bool,
    /// If `true`, persists LLM prompts in clear on the trace (default `true`,
    /// local-first). Distinct from `debug_log_prompt`, which controls the
    /// `prompt_text` in `llm_calls.db`.
    #[serde(default = "default_capture_on")]
    pub capture_llm_prompts: bool,
    /// If `true`, persists the full `args_json` of tool calls (default
    /// `true`). Disabling leaves only the tool name and duration visible.
    #[serde(default = "default_capture_on")]
    pub capture_tool_args: bool,
    /// If `true`, persists the full `output_json` of tool calls (default
    /// `true`). Disabling leaves only success/failure visible.
    #[serde(default = "default_capture_on")]
    pub capture_tool_outputs: bool,
    /// If `true`, persists Python `ctx.log()` calls on the trace (default
    /// `true`). Disabling keeps `tracing::*` working but writes no record in
    /// `runtime_events.db`.
    #[serde(default = "default_capture_on")]
    pub capture_agent_logs: bool,
    /// Retention period in days for `runtime_events` before automatic purge
    /// (default 90, consistent with audit.db).
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            max_input_bytes: DEFAULT_MAX_INPUT_BYTES,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            max_tool_output_bytes: DEFAULT_MAX_TOOL_OUTPUT_BYTES,
            debug_log_prompt: false,
            // Local-first: capture everything by default. The operator can
            // disable individual captures in Settings, Observability.
            capture_thoughts: true,
            capture_llm_prompts: true,
            capture_tool_args: true,
            capture_tool_outputs: true,
            capture_agent_logs: true,
            retention_days: 90,
        }
    }
}

fn default_capture_on() -> bool {
    true
}

fn default_retention_days() -> u32 {
    90
}

fn default_max_input_bytes() -> usize {
    DEFAULT_MAX_INPUT_BYTES
}

fn default_max_output_bytes() -> usize {
    DEFAULT_MAX_OUTPUT_BYTES
}

fn default_max_tool_output_bytes() -> usize {
    DEFAULT_MAX_TOOL_OUTPUT_BYTES
}

/// Finds the largest UTF-8 character boundary at or below `index`.
///
/// Stable equivalent of the nightly `str::floor_char_boundary`.
fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut i = index;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Truncates a text if its size exceeds `max_bytes`.
///
/// Returns `(truncated_or_original_text, was_truncated)`. The cut is
/// guaranteed to fall on a valid UTF-8 boundary. The truncation marker
/// reports the original total size.
pub fn truncate_with_marker(text: &str, max_bytes: usize) -> (String, bool) {
    if text.len() <= max_bytes {
        return (text.to_string(), false);
    }
    let total = text.len();
    let marker = format!("\n[TRONQUÉ - {} octets total]", total);
    let keep = max_bytes.saturating_sub(marker.len());
    let safe_end = floor_char_boundary(text, keep);
    let mut result = text[..safe_end].to_string();
    result.push_str(&marker);
    (result, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short_text_unchanged() {
        // GIVEN a short text
        let text = "hello world";
        // WHEN truncating with a large limit
        let (result, truncated) = truncate_with_marker(text, 1024);
        // THEN text is unchanged, not truncated
        assert_eq!(result, "hello world");
        assert!(!truncated);
    }

    #[test]
    fn test_truncate_exact_boundary_unchanged() {
        // GIVEN a text of exact size
        let text = "abcde";
        // WHEN truncating with limit = len
        let (result, truncated) = truncate_with_marker(text, 5);
        // THEN text is unchanged
        assert_eq!(result, "abcde");
        assert!(!truncated);
    }

    #[test]
    fn test_truncate_over_limit_adds_marker() {
        // GIVEN a 500-byte text
        let text = "x".repeat(500);
        // WHEN truncating with limit 100
        let (result, truncated) = truncate_with_marker(&text, 100);
        // THEN result is truncated with a marker
        assert!(truncated);
        assert!(result.len() <= 100 + 50, "result should be near max_bytes");
        assert!(result.ends_with("octets total]"));
        assert!(result.contains("500"));
    }

    #[test]
    fn test_truncate_utf8_safe_boundary() {
        // GIVEN a text with multi-byte characters (é = 2 bytes)
        let text = "ééééé"; // 10 bytes for 5 characters
                            // WHEN truncating at 5 bytes (in the middle of an é)
        let (result, truncated) = truncate_with_marker(text, 5);
        // THEN no panic, valid UTF-8 text
        assert!(truncated);
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    }

    #[test]
    fn test_truncate_zero_limit() {
        // GIVEN an arbitrary text
        let text = "hello";
        // WHEN truncating with limit 0
        let (result, truncated) = truncate_with_marker(text, 0);
        // THEN truncated, contains the marker
        assert!(truncated);
        assert!(result.contains("5 octets total"));
    }

    #[test]
    fn test_observability_config_default_values() {
        // GIVEN the default config
        let config = ObservabilityConfig::default();
        // THEN expected values
        assert_eq!(config.max_input_bytes, 32_768);
        assert_eq!(config.max_output_bytes, 32_768);
        assert_eq!(config.max_tool_output_bytes, 10_240);
        assert!(!config.debug_log_prompt);
    }

    #[test]
    fn test_observability_config_serde_defaults() {
        // GIVEN an empty JSON
        let json = "{}";
        // WHEN deserializing
        let config: ObservabilityConfig = serde_json::from_str(json).expect("deser failed");
        // THEN defaults are applied
        assert_eq!(config.max_input_bytes, 32_768);
        assert_eq!(config.max_output_bytes, 32_768);
        assert_eq!(config.max_tool_output_bytes, 10_240);
        assert!(!config.debug_log_prompt);
    }

    #[test]
    fn test_observability_config_serde_override() {
        // GIVEN a JSON with custom values
        let json = r#"{"max_input_bytes": 1024, "debug_log_prompt": true}"#;
        // WHEN deserializing
        let config: ObservabilityConfig = serde_json::from_str(json).expect("deser failed");
        // THEN custom values applied, defaults for the rest
        assert_eq!(config.max_input_bytes, 1024);
        assert_eq!(config.max_output_bytes, 32_768);
        assert!(config.debug_log_prompt);
    }
}
