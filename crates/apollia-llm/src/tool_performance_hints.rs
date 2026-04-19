//! Hints statiques de performance par outil (US-SP42-038).
//!
//! Table embarquée depuis `tool_performance_hints.toml` : durée attendue en ms
//! et éventuelle suggestion d'alternative plus rapide. Consultée avant l'exec
//! pour enrichir la `ToolCallRationale` sans attendre de télémétrie runtime.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

/// Fichier TOML embarqué au build.
const HINTS_TOML: &str = include_str!("../tool_performance_hints.toml");

/// Entrée de hint pour un outil donné.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolPerformanceHint {
    /// Durée typique de l'outil en millisecondes (p50 estimé).
    pub expected_duration_ms: u64,
    /// Optionnel : alternative plus rapide à suggérer selon le contexte.
    #[serde(default)]
    pub faster_alternative: Option<String>,
}

/// Racine du document TOML — map `tool_name -> hint`.
#[derive(Debug, Deserialize)]
struct HintsDocument {
    #[serde(default)]
    tools: HashMap<String, ToolPerformanceHint>,
}

/// Table parsée une seule fois au premier accès.
fn table() -> &'static HashMap<String, ToolPerformanceHint> {
    static CELL: OnceLock<HashMap<String, ToolPerformanceHint>> = OnceLock::new();
    CELL.get_or_init(|| {
        toml::from_str::<HintsDocument>(HINTS_TOML)
            .map(|doc| doc.tools)
            .unwrap_or_default()
    })
}

/// Lookup d'un hint pour un outil ; retourne `None` si absent.
pub fn lookup(tool_name: &str) -> Option<&'static ToolPerformanceHint> {
    table().get(tool_name)
}

/// Construit la phrase de hint formatée pour un outil, ou `None` si absent.
///
/// Format : `"Durée attendue: {ms}ms"` ou, si alternative présente,
/// `"Durée attendue: {ms}ms — alternative: {alt}"`.
pub fn format_hint(tool_name: &str) -> Option<String> {
    lookup(tool_name).map(|h| match &h.faster_alternative {
        Some(alt) => format!(
            "Durée attendue: {}ms — alternative: {}",
            h.expected_duration_ms, alt
        ),
        None => format!("Durée attendue: {}ms", h.expected_duration_ms),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN un outil présent dans le TOML
    // WHEN format_hint()
    // THEN la phrase est non vide et contient la durée
    #[test]
    fn format_hint_returns_phrase_for_known_tool() {
        let hint = format_hint("file_read").expect("file_read hint exists");
        assert!(hint.contains("ms"));
    }

    // GIVEN un outil absent du TOML
    // WHEN lookup()
    // THEN None
    #[test]
    fn lookup_unknown_returns_none() {
        assert!(lookup("unknown_tool_xyz").is_none());
    }
}
