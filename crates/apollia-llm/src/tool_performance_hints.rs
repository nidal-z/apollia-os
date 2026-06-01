//! Static per-tool performance hints.
//!
//! Table embedded from `tool_performance_hints.toml`: expected duration in ms
//! and an optional faster-alternative suggestion. Consulted before execution
//! to enrich the `ToolCallRationale` without waiting for runtime telemetry.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

/// TOML file embedded at build time.
const HINTS_TOML: &str = include_str!("../tool_performance_hints.toml");

/// Hint entry for a given tool.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolPerformanceHint {
    /// Typical tool duration in milliseconds (estimated p50).
    pub expected_duration_ms: u64,
    /// Optional faster alternative to suggest depending on context.
    #[serde(default)]
    pub faster_alternative: Option<String>,
}

/// Root of the TOML document: a `tool_name -> hint` map.
#[derive(Debug, Deserialize)]
struct HintsDocument {
    #[serde(default)]
    tools: HashMap<String, ToolPerformanceHint>,
}

/// Table parsed once on first access.
fn table() -> &'static HashMap<String, ToolPerformanceHint> {
    static CELL: OnceLock<HashMap<String, ToolPerformanceHint>> = OnceLock::new();
    CELL.get_or_init(|| {
        toml::from_str::<HintsDocument>(HINTS_TOML)
            .map(|doc| doc.tools)
            .unwrap_or_default()
    })
}

/// Look up a hint for a tool; returns `None` if absent.
pub fn lookup(tool_name: &str) -> Option<&'static ToolPerformanceHint> {
    table().get(tool_name)
}

/// Build the formatted hint phrase for a tool, or `None` if absent.
///
/// Format: `"Durée attendue: {ms}ms"` or, when an alternative is present,
/// `"Durée attendue: {ms}ms - alternative: {alt}"` (the phrase shown to the
/// user is French).
pub fn format_hint(tool_name: &str) -> Option<String> {
    lookup(tool_name).map(|h| match &h.faster_alternative {
        Some(alt) => format!(
            "Durée attendue: {}ms - alternative: {}",
            h.expected_duration_ms, alt
        ),
        None => format!("Durée attendue: {}ms", h.expected_duration_ms),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN a tool present in the TOML
    // WHEN format_hint()
    // THEN the phrase is non-empty and contains the duration
    #[test]
    fn format_hint_returns_phrase_for_known_tool() {
        let hint = format_hint("file_read").expect("file_read hint exists");
        assert!(hint.contains("ms"));
    }

    // GIVEN a tool absent from the TOML
    // WHEN lookup()
    // THEN None
    #[test]
    fn lookup_unknown_returns_none() {
        assert!(lookup("unknown_tool_xyz").is_none());
    }
}
