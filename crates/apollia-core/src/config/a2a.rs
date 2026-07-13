use serde::{Deserialize, Serialize};

use super::{validate_bounds, ConfigError};

// ─────────────────────────────────────────────
// A2AConfig
// ─────────────────────────────────────────────

/// A2A routing configuration enforced by the runtime.
///
/// Controls the three automatic safeguards triggered during inter-agent
/// invocations: recursion depth, per-invocation timeout, and cumulative chain
/// timeout.
///
/// Defaults are tuned for the majority of use cases: `max_depth = 3`,
/// `invocation_timeout_secs = 120`, `chain_timeout_secs = 300`. Every field can
/// be overridden in `apollia.toml` under `[a2a]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2AConfig {
    /// Maximum allowed A2A recursion depth.
    ///
    /// A value of `3` means a chain can reach three nesting levels before being
    /// blocked. The check is enforced by the runtime before each invocation and
    /// cannot be bypassed from the agent side.
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,

    /// Timeout for a single A2A invocation, in seconds.
    ///
    /// Applied to each `invoke()` call independently of the overall chain. An
    /// invocation exceeding this delay is cancelled.
    #[serde(default = "default_invocation_timeout")]
    pub invocation_timeout_secs: u64,

    /// Cumulative timeout for the whole A2A chain, in seconds.
    ///
    /// Initialized on the first invocation of a chain (`chain_deadline = None`).
    /// The remaining budget is used as the upper bound for every subsequent
    /// invocation in the same chain, preventing long chains from monopolizing
    /// resources beyond this total budget.
    /// Default: 300. Bounds: [10, 3600].
    #[serde(default = "default_chain_timeout")]
    pub chain_timeout_secs: u64,
}

impl Default for A2AConfig {
    fn default() -> Self {
        Self {
            max_depth: default_max_depth(),
            invocation_timeout_secs: default_invocation_timeout(),
            chain_timeout_secs: default_chain_timeout(),
        }
    }
}

impl A2AConfig {
    /// Validates the A2A configuration bounds at startup (fail-fast).
    ///
    /// - `chain_timeout_secs`: must be in [10, 3600].
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds("a2a.chain_timeout_secs", self.chain_timeout_secs, 10, 3600)?;
        Ok(())
    }
}

fn default_max_depth() -> u32 {
    3
}

fn default_invocation_timeout() -> u64 {
    120
}

fn default_chain_timeout() -> u64 {
    300
}
