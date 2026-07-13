use serde::Deserialize;

use super::{validate_bounds, ConfigError};

// ─────────────────────────────────────────────
// TriggersConfig
// ─────────────────────────────────────────────

/// Trigger engine configuration (`[triggers]` section in `apollia.toml`).
///
/// Controls the bounded queue used by [`OnBusyPolicy::Queue`] when an agent is
/// busy at fire time. Every field has a sane default via [`Default`].
#[derive(Debug, Clone, Deserialize)]
pub struct TriggersConfig {
    /// Maximum capacity of the per-agent bounded FIFO queue.
    ///
    /// Used by `OnBusyPolicy::Queue { max_depth }` to cap the number of pending
    /// triggers per agent. When the queue is full, the trigger is dropped and
    /// `RuntimeEvent::TriggerQueueFull` is emitted.
    /// `0` disables the bound (not recommended in production).
    /// Default: 10. Bounds: [0, 10000].
    #[serde(default = "default_trigger_queue_max_depth")]
    pub queue_max_depth: usize,
}

impl Default for TriggersConfig {
    fn default() -> Self {
        Self {
            queue_max_depth: default_trigger_queue_max_depth(),
        }
    }
}

impl TriggersConfig {
    /// Validates the trigger configuration bounds at startup (fail-fast).
    ///
    /// - `queue_max_depth`: must be in [0, 10000].
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds("triggers.queue_max_depth", self.queue_max_depth, 0, 10_000)?;
        Ok(())
    }
}

fn default_trigger_queue_max_depth() -> usize {
    10
}
