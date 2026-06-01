//! Configuration et utilitaires d'observabilité.
//!
//! Fournit [`ObservabilityConfig`] pour contrôler la troncature des données
//! persistées (inputs, outputs, stdout/stderr) et [`truncate_with_marker`]
//! pour tronquer proprement sur une frontière UTF-8.

use serde::{Deserialize, Serialize};

/// Taille max par défaut des inputs tasks/steps en octets.
const DEFAULT_MAX_INPUT_BYTES: usize = 32_768;

/// Taille max par défaut des outputs tasks/steps/completions en octets.
const DEFAULT_MAX_OUTPUT_BYTES: usize = 32_768;

/// Taille max par défaut des stdout/stderr outils en octets.
const DEFAULT_MAX_TOOL_OUTPUT_BYTES: usize = 10_240;

/// Configuration de troncature pour l'observabilité.
///
/// Contrôle les limites de taille pour les données persistées dans SQLite.
/// Les valeurs par défaut sont raisonnables pour un usage courant ;
/// elles peuvent être surchargées via `apollia.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    /// Taille max des inputs tasks/steps en octets (défaut 32768).
    #[serde(default = "default_max_input_bytes")]
    pub max_input_bytes: usize,

    /// Taille max des outputs tasks/steps/completions en octets (défaut 32768).
    #[serde(default = "default_max_output_bytes")]
    pub max_output_bytes: usize,

    /// Taille max des stdout/stderr outils en octets (défaut 10240).
    #[serde(default = "default_max_tool_output_bytes")]
    pub max_tool_output_bytes: usize,

    /// Si true, persiste le prompt_text LLM (défaut false).
    #[serde(default)]
    pub debug_log_prompt: bool,

    // ── ADR-088 - capture granulaire pour `runtime_events` (Lot 2) ──
    /// Si `true`, persiste les `Thought` ReAct sur la trace (défaut `true`).
    /// Désactiver vide les bulles de raisonnement côté UI builder.
    #[serde(default = "default_capture_on")]
    pub capture_thoughts: bool,
    /// Si `true`, persiste les prompts LLM en clair sur la trace (défaut
    /// `true`, local-first). Distinct de `debug_log_prompt` qui contrôle
    /// le `prompt_text` dans `llm_calls.db`.
    #[serde(default = "default_capture_on")]
    pub capture_llm_prompts: bool,
    /// Si `true`, persiste les `args_json` complets des tool calls (défaut
    /// `true`). Désactiver = seul le tool name + duration sont visibles.
    #[serde(default = "default_capture_on")]
    pub capture_tool_args: bool,
    /// Si `true`, persiste les `output_json` complets des tool calls
    /// (défaut `true`). Désactiver = seul le succès/échec est visible.
    #[serde(default = "default_capture_on")]
    pub capture_tool_outputs: bool,
    /// Si `true`, persiste les `ctx.log()` Python sur la trace (défaut
    /// `true`). Désactiver = `tracing::*` continue mais aucun record dans
    /// `runtime_events.db`.
    #[serde(default = "default_capture_on")]
    pub capture_agent_logs: bool,
    /// Nombre de jours de rétention des `runtime_events` avant purge
    /// automatique (défaut 90, cohérent avec audit.db). Lot 5 implémente
    /// la purge ; les Lots 1-4 ignorent ce paramètre.
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
            // Local-first : on capture tout par défaut. L'opérateur peut
            // désactiver granulairement dans Settings → Observability.
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

/// Trouve la plus grande frontière de caractère UTF-8 ≤ `index`.
///
/// Équivalent stable de `str::floor_char_boundary` (nightly).
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

/// Tronque un texte si sa taille dépasse `max_bytes`.
///
/// Retourne `(texte_tronqué_ou_original, was_truncated)`.
/// La coupure est garantie sur une frontière UTF-8 valide.
/// Le marqueur de troncature indique la taille totale originale.
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
        // GIVEN un texte court
        let text = "hello world";
        // WHEN troncature avec limite large
        let (result, truncated) = truncate_with_marker(text, 1024);
        // THEN texte inchangé, pas tronqué
        assert_eq!(result, "hello world");
        assert!(!truncated);
    }

    #[test]
    fn test_truncate_exact_boundary_unchanged() {
        // GIVEN un texte de taille exacte
        let text = "abcde";
        // WHEN troncature avec limite = len
        let (result, truncated) = truncate_with_marker(text, 5);
        // THEN texte inchangé
        assert_eq!(result, "abcde");
        assert!(!truncated);
    }

    #[test]
    fn test_truncate_over_limit_adds_marker() {
        // GIVEN un texte de 500 octets
        let text = "x".repeat(500);
        // WHEN troncature avec limite 100
        let (result, truncated) = truncate_with_marker(&text, 100);
        // THEN résultat tronqué avec marqueur
        assert!(truncated);
        assert!(result.len() <= 100 + 50, "result should be near max_bytes");
        assert!(result.ends_with("octets total]"));
        assert!(result.contains("500"));
    }

    #[test]
    fn test_truncate_utf8_safe_boundary() {
        // GIVEN un texte avec des caractères multi-octets (é = 2 octets)
        let text = "ééééé"; // 10 octets pour 5 caractères
                            // WHEN troncature à 5 octets (au milieu d'un é)
        let (result, truncated) = truncate_with_marker(text, 5);
        // THEN pas de panic, texte valide UTF-8
        assert!(truncated);
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    }

    #[test]
    fn test_truncate_zero_limit() {
        // GIVEN un texte quelconque
        let text = "hello";
        // WHEN troncature avec limite 0
        let (result, truncated) = truncate_with_marker(text, 0);
        // THEN tronqué, contient le marqueur
        assert!(truncated);
        assert!(result.contains("5 octets total"));
    }

    #[test]
    fn test_observability_config_default_values() {
        // GIVEN config par défaut
        let config = ObservabilityConfig::default();
        // THEN valeurs attendues
        assert_eq!(config.max_input_bytes, 32_768);
        assert_eq!(config.max_output_bytes, 32_768);
        assert_eq!(config.max_tool_output_bytes, 10_240);
        assert!(!config.debug_log_prompt);
    }

    #[test]
    fn test_observability_config_serde_defaults() {
        // GIVEN un JSON vide
        let json = "{}";
        // WHEN désérialisation
        let config: ObservabilityConfig = serde_json::from_str(json).expect("deser failed");
        // THEN valeurs par défaut appliquées
        assert_eq!(config.max_input_bytes, 32_768);
        assert_eq!(config.max_output_bytes, 32_768);
        assert_eq!(config.max_tool_output_bytes, 10_240);
        assert!(!config.debug_log_prompt);
    }

    #[test]
    fn test_observability_config_serde_override() {
        // GIVEN un JSON avec des valeurs custom
        let json = r#"{"max_input_bytes": 1024, "debug_log_prompt": true}"#;
        // WHEN désérialisation
        let config: ObservabilityConfig = serde_json::from_str(json).expect("deser failed");
        // THEN valeurs custom appliquées, défauts pour le reste
        assert_eq!(config.max_input_bytes, 1024);
        assert_eq!(config.max_output_bytes, 32_768);
        assert!(config.debug_log_prompt);
    }
}
