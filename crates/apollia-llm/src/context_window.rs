//! The context window an operator chose for a backend.
//!
//! One reader for every provider, so the engine launch, the Ollama request, the
//! router's compaction and the context gauge agree on the same figure.

/// The window stored in a backend's `config_json`, in tokens.
///
/// `context_window` is the canonical key, written by onboarding, the settings
/// dialog and `apollia llm add --context-window`. Settings dialogs before
/// 2026-09-22 wrote `context_size`, which nothing read; it is accepted so those
/// rows take effect instead of being silently ignored. Zero, negative and
/// non-numeric values read as unset.
#[must_use]
pub fn configured(config_json: &serde_json::Value) -> Option<u32> {
    ["context_window", "context_size"].iter().find_map(|key| {
        config_json
            .get(*key)
            .and_then(serde_json::Value::as_u64)
            .filter(|v| *v > 0)
            .and_then(|v| u32::try_from(v).ok())
    })
}

/// The window to run a model at, given what was asked and what it was trained on.
///
/// A window past the training length is not refused by the engines: llama.cpp
/// logs a "training context overflow" warning and carries on, and Ollama does
/// the same. Positions past what the model saw in training are ones its
/// attention was never fitted to, so answers degrade exactly when a long
/// conversation most needs them, and the extra cache costs memory for nothing.
/// So the asked window is capped at the trained one. A GGUF whose metadata
/// declares RoPE scaling reports the extended length as its context length,
/// which is why no separate case is needed for it.
///
/// An unknown training length leaves the asked window alone.
#[must_use]
pub fn cap_at_trained(asked: u32, trained: Option<u64>) -> u32 {
    match trained
        .and_then(|t| u32::try_from(t).ok())
        .filter(|t| *t > 0)
    {
        Some(trained) => asked.min(trained),
        None => asked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_past_the_training_length_is_capped() {
        // GIVEN a model trained on 8192 tokens and one trained on 131072
        // WHEN a 32768-token window is asked of each
        // THEN the first runs at its trained length and the second as asked
        assert_eq!(cap_at_trained(32_768, Some(8_192)), 8_192);
        assert_eq!(cap_at_trained(32_768, Some(131_072)), 32_768);
    }

    #[test]
    fn an_unknown_training_length_leaves_the_window_alone() {
        // GIVEN a header without a context length, or with a zero one
        // WHEN a window is asked
        // THEN it is kept
        assert_eq!(cap_at_trained(65_536, None), 65_536);
        assert_eq!(cap_at_trained(65_536, Some(0)), 65_536);
    }

    #[test]
    fn the_canonical_key_wins_over_the_legacy_one() {
        // GIVEN a row carrying both keys
        let cfg = serde_json::json!({ "context_window": 65_536, "context_size": 8192 });

        // WHEN the window is read
        // THEN the canonical key decides
        assert_eq!(configured(&cfg), Some(65_536));
    }

    #[test]
    fn a_legacy_row_still_takes_effect() {
        // GIVEN a row saved by an older settings dialog
        let cfg = serde_json::json!({ "context_size": 16_384 });

        // WHEN the window is read
        // THEN the legacy key is honoured
        assert_eq!(configured(&cfg), Some(16_384));
    }

    #[test]
    fn nonsense_reads_as_unset() {
        // GIVEN zero, a string and an absent key
        // WHEN each is read
        // THEN none of them sets a window
        assert_eq!(
            configured(&serde_json::json!({ "context_window": 0 })),
            None
        );
        assert_eq!(
            configured(&serde_json::json!({ "context_window": "32k" })),
            None
        );
        assert_eq!(configured(&serde_json::json!({})), None);
    }
}
