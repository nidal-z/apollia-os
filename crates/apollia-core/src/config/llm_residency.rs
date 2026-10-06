use std::time::Duration;

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
// LlmResidencyConfig
// ─────────────────────────────────────────────

/// Dotted key of the delay after which an unused local model is unloaded.
pub const IDLE_UNLOAD_KEY: &str = "llm.residency.idle_unload_minutes";

/// Dotted key of the rule that unloads the resident models before another
/// one is loaded.
pub const UNLOAD_BEFORE_LOAD_KEY: &str = "llm.residency.unload_before_load";

/// Minutes an unused local model stays loaded when the key is absent.
pub const DEFAULT_IDLE_UNLOAD_MINUTES: u64 = 60;

/// How long a local model stays loaded once nothing uses it
/// (`[llm.residency]` section in `apollia.toml`).
///
/// The runtime reads it at every sweep through
/// [`resolve_setting`](super::resolve_setting) under [`IDLE_UNLOAD_KEY`], so a
/// change applies without a restart. A model pinned by the user, held by an
/// open conversation, or answering a request is never unloaded for idleness.
///
/// Several models stay loaded together, as many as the load guardrail
/// admits. With `unload_before_load` on, loading a model first unloads the
/// resident ones that are not pinned and answer no request, so one model is
/// loaded at a time. The engines read it at every load.
///
/// ```toml
/// [llm.residency]
/// idle_unload_minutes = 30   # 0 keeps an unused model loaded
/// unload_before_load = true  # one model at a time
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LlmResidencyConfig {
    /// Minutes without a request after which a model nothing holds is
    /// unloaded. `0` means never.
    #[serde(default = "default_idle_unload_minutes")]
    pub idle_unload_minutes: u64,
    /// Whether loading a model first unloads the resident ones that are not
    /// pinned and answer no request. Off by default.
    #[serde(default)]
    pub unload_before_load: bool,
}

fn default_idle_unload_minutes() -> u64 {
    DEFAULT_IDLE_UNLOAD_MINUTES
}

impl Default for LlmResidencyConfig {
    fn default() -> Self {
        Self {
            idle_unload_minutes: DEFAULT_IDLE_UNLOAD_MINUTES,
            unload_before_load: false,
        }
    }
}

impl LlmResidencyConfig {
    /// The delay after which an unused model is unloaded, `None` for never.
    #[must_use]
    pub fn idle_unload(&self) -> Option<Duration> {
        (self.idle_unload_minutes > 0)
            .then(|| Duration::from_secs(self.idle_unload_minutes.saturating_mul(60)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_residency_defaults_to_an_hour() {
        // GIVEN the section's defaults
        let config = LlmResidencyConfig::default();

        // WHEN the delay is read
        let delay = config.idle_unload();

        // THEN an unused model is unloaded after sixty minutes
        assert_eq!(delay, Some(Duration::from_secs(3600)));
    }

    #[test]
    fn test_llm_residency_zero_means_never() {
        // GIVEN a delay of zero minutes
        let config = LlmResidencyConfig {
            idle_unload_minutes: 0,
            ..LlmResidencyConfig::default()
        };

        // WHEN the delay is read
        let delay = config.idle_unload();

        // THEN there is none: an unused model stays loaded
        assert_eq!(delay, None);
    }

    #[test]
    fn test_llm_residency_keeps_several_models_loaded_by_default() {
        // GIVEN a section that only sets the idle delay
        let content = "idle_unload_minutes = 30\n";

        // WHEN it is read
        let config: LlmResidencyConfig = toml::from_str(content).unwrap();

        // THEN loading a model does not unload the others first
        assert!(!config.unload_before_load);
        assert!(!LlmResidencyConfig::default().unload_before_load);
    }
}
