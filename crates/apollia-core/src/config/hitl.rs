use serde::Deserialize;

use super::{validate_bounds, ConfigError};

// ─────────────────────────────────────────────
// HitlConfig
// ─────────────────────────────────────────────

/// Human-in-the-Loop configuration (`[hitl]` section in `apollia.toml`).
///
/// Controls the `TimeoutWatcher` behavior: maximum wait for human approval and
/// the scan frequency for expired tasks. Every field has a sane default via
/// [`Default`].
#[derive(Debug, Clone, Deserialize)]
pub struct HitlConfig {
    /// Maximum wait for human approval, in hours.
    ///
    /// `None` (default): the task stays paused indefinitely until the operator
    /// responds. `Some(n)`: automatic cancellation after `n` hours.
    /// Bounds when `Some`: [1, 168] (1 hour to 7 days).
    ///
    /// Do not set a global timeout unless an agent explicitly requests one.
    #[serde(default)]
    pub timeout_hours: Option<u64>,

    /// Scan interval for expired HITL tasks, in seconds.
    ///
    /// How often the `TimeoutWatcher` checks suspended tasks.
    /// Default: 60. Bounds: [10, 3600].
    /// Ignored when `timeout_hours` is `None`.
    #[serde(default = "default_scan_interval_secs")]
    pub scan_interval_secs: u64,
}

impl Default for HitlConfig {
    fn default() -> Self {
        Self {
            timeout_hours: None,
            scan_interval_secs: default_scan_interval_secs(),
        }
    }
}

impl HitlConfig {
    /// Validates the HITL configuration bounds at startup (fail-fast).
    ///
    /// - `timeout_hours`: if `Some`, must be in [1, 168].
    /// - `scan_interval_secs`: must be in [10, 3600].
    pub fn validate(&self) -> Result<(), ConfigError> {
        if let Some(h) = self.timeout_hours {
            validate_bounds("hitl.timeout_hours", h, 1, 168)?;
        }
        validate_bounds("hitl.scan_interval_secs", self.scan_interval_secs, 10, 3600)?;
        Ok(())
    }
}

fn default_scan_interval_secs() -> u64 {
    60
}
