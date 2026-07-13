use serde::{Deserialize, Serialize};

use crate::budget::StepBudgetConfig;

use super::{validate_bounds, ConfigError};

// ─────────────────────────────────────────────
// Autonomy tiers
// ─────────────────────────────────────────────

/// Execution autonomy tier: opt-in, explicit, auditable.
///
/// Each variant maps to an effective [`StepBudgetConfig`] and two behavioral
/// flags (`inject_memory`, `run_verification`). The effective budget is always
/// capped by the runtime ceiling via `StepBudget::from_capped`, so a tier can
/// never raise the budget above the runtime bound (principle #7).
/// Gate policy for plan review: whether the engine pauses after plan generation
/// and waits for human approval before starting the `ActorLoop`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatePolicy {
    /// The plan must be explicitly approved before execution starts.
    Active,
    /// The plan executes immediately without waiting for approval.
    Bypass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyLevel {
    /// Default conversational mode. Low budget, no memory injection, no verification.
    Assisted,
    /// Supervised mode. Slightly raised budget, verification on, memory off.
    Supervised,
    /// Bounded autonomous mode. Raised budget, verification on, memory off by default.
    BoundedAutonomous,
    /// Long-horizon autonomous mode. High budget, verification on, memory injection on.
    LongAutonomous,
}

impl AutonomyLevel {
    /// All variants in canonical order. Used by validation and round-trip tests.
    pub const ALL: [AutonomyLevel; 4] = [
        AutonomyLevel::Assisted,
        AutonomyLevel::Supervised,
        AutonomyLevel::BoundedAutonomous,
        AutonomyLevel::LongAutonomous,
    ];

    /// Canonical snake_case identifier (round-trips with the [`std::str::FromStr`] impl).
    pub fn as_str(self) -> &'static str {
        match self {
            AutonomyLevel::Assisted => "assisted",
            AutonomyLevel::Supervised => "supervised",
            AutonomyLevel::BoundedAutonomous => "bounded_autonomous",
            AutonomyLevel::LongAutonomous => "long_autonomous",
        }
    }

    /// Plan gate policy for this autonomy tier.
    ///
    /// | Tier               | Gate   |
    /// |--------------------|--------|
    /// | Assisted           | Active |
    /// | Supervised         | Active |
    /// | BoundedAutonomous  | Bypass |
    /// | LongAutonomous     | Bypass |
    ///
    /// The safe default is `Active`: only the explicitly autonomous tiers bypass
    /// the gate. The match is exhaustive, so an unrepresentable tier cannot slip
    /// through to `Bypass`.
    pub fn gate_policy(self) -> GatePolicy {
        match self {
            AutonomyLevel::Assisted | AutonomyLevel::Supervised => GatePolicy::Active,
            AutonomyLevel::BoundedAutonomous | AutonomyLevel::LongAutonomous => GatePolicy::Bypass,
        }
    }
}

impl std::str::FromStr for AutonomyLevel {
    type Err = AutonomyLevelParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "assisted" => Ok(AutonomyLevel::Assisted),
            "supervised" => Ok(AutonomyLevel::Supervised),
            "bounded_autonomous" => Ok(AutonomyLevel::BoundedAutonomous),
            "long_autonomous" => Ok(AutonomyLevel::LongAutonomous),
            other => Err(AutonomyLevelParseError {
                given: other.to_string(),
            }),
        }
    }
}

impl std::fmt::Display for AutonomyLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned when an unknown autonomy level string is supplied.
#[derive(Debug, thiserror::Error)]
#[error(
    "unknown autonomy level '{given}'; accepted values: assisted, supervised, bounded_autonomous, long_autonomous"
)]
pub struct AutonomyLevelParseError {
    /// The string that failed to parse.
    pub given: String,
}

/// Per-level configuration: effective budget plus behavioral flags.
///
/// When absent from `apollia.toml`, each level uses the values defined by
/// [`AutonomyLevelConfig::default_for`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyLevelConfig {
    /// Effective budget for this tier. Always capped by the runtime ceiling.
    pub budget: StepBudgetConfig,
    /// Inject user memory context into the system prompt for this tier.
    /// Default: false for all tiers except `long_autonomous`.
    #[serde(default)]
    pub inject_memory: bool,
    /// Run the verification loop before declaring the task done for this tier.
    /// Default: false for `assisted`; true for the other tiers.
    #[serde(default)]
    pub run_verification: bool,
}

impl AutonomyLevelConfig {
    /// Canonical default configuration for a given tier.
    ///
    /// Default tier matrix (suggested budgets, capped by the runtime ceiling at
    /// execution time):
    ///
    /// | tier               | max_steps | max_tool_calls | wall_clock_secs | inject_memory | run_verification |
    /// |--------------------|-----------|----------------|-----------------|---------------|------------------|
    /// | assisted           | 100       | 200            | 1200            | false         | false            |
    /// | supervised         | 200       | 400            | 2400            | false         | true             |
    /// | bounded_autonomous | 300       | 600            | 3600            | false         | true             |
    /// | long_autonomous    | 500       | 1000           | 7200            | true          | true             |
    pub fn default_for(level: AutonomyLevel) -> Self {
        match level {
            AutonomyLevel::Assisted => Self {
                budget: StepBudgetConfig::chat_default(),
                inject_memory: false,
                run_verification: false,
            },
            AutonomyLevel::Supervised => Self {
                budget: StepBudgetConfig {
                    max_steps: 200,
                    max_tool_calls: 400,
                    wall_clock_secs: 2400,
                },
                inject_memory: false,
                run_verification: true,
            },
            AutonomyLevel::BoundedAutonomous => Self {
                budget: StepBudgetConfig {
                    max_steps: 300,
                    max_tool_calls: 600,
                    wall_clock_secs: 3600,
                },
                inject_memory: false,
                run_verification: true,
            },
            AutonomyLevel::LongAutonomous => Self {
                budget: StepBudgetConfig {
                    max_steps: 500,
                    max_tool_calls: 1000,
                    wall_clock_secs: 7200,
                },
                inject_memory: true,
                run_verification: true,
            },
        }
    }
}

/// Default tier applied when `--autonomy` is not specified.
fn default_autonomy_level() -> AutonomyLevel {
    AutonomyLevel::Assisted
}

/// Autonomy tiers configuration (`[autonomy]` section in `apollia.toml`).
///
/// Absent per-tier fields fall back to [`AutonomyLevelConfig::default_for`].
/// Each tier is validated against the runtime ceiling in [`AutonomyConfig::validate`]:
/// no tier may declare a budget above the runtime bound, which keeps the
/// `StepBudget` a non-bypassable ceiling at every tier (principle #7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyConfig {
    /// Default tier applied when `--autonomy` is not specified. Default: `assisted`.
    #[serde(default = "default_autonomy_level")]
    pub default_level: AutonomyLevel,
    /// Optional override for the `assisted` tier.
    #[serde(default)]
    pub assisted: Option<AutonomyLevelConfig>,
    /// Optional override for the `supervised` tier.
    #[serde(default)]
    pub supervised: Option<AutonomyLevelConfig>,
    /// Optional override for the `bounded_autonomous` tier.
    #[serde(default)]
    pub bounded_autonomous: Option<AutonomyLevelConfig>,
    /// Optional override for the `long_autonomous` tier.
    #[serde(default)]
    pub long_autonomous: Option<AutonomyLevelConfig>,
}

impl Default for AutonomyConfig {
    fn default() -> Self {
        Self {
            default_level: default_autonomy_level(),
            assisted: None,
            supervised: None,
            bounded_autonomous: None,
            long_autonomous: None,
        }
    }
}

impl AutonomyConfig {
    /// Effective config for the requested level (TOML override, or the default).
    pub fn level_config(&self, level: AutonomyLevel) -> AutonomyLevelConfig {
        let override_config = match level {
            AutonomyLevel::Assisted => &self.assisted,
            AutonomyLevel::Supervised => &self.supervised,
            AutonomyLevel::BoundedAutonomous => &self.bounded_autonomous,
            AutonomyLevel::LongAutonomous => &self.long_autonomous,
        };
        override_config
            .clone()
            .unwrap_or_else(|| AutonomyLevelConfig::default_for(level))
    }

    /// Validate all tiers against the runtime ceiling (fail-fast at startup).
    ///
    /// Returns [`ConfigError::OutOfBounds`] if any tier declares a budget that
    /// exceeds the runtime ceiling along any dimension. The reported `key`
    /// identifies the offending tier and dimension, e.g.
    /// `"autonomy.supervised.budget.max_steps"`.
    pub fn validate(&self, runtime_ceiling: &StepBudgetConfig) -> Result<(), ConfigError> {
        for level in AutonomyLevel::ALL {
            let config = self.level_config(level);
            validate_bounds(
                &format!("autonomy.{level}.budget.max_steps"),
                config.budget.max_steps,
                0,
                runtime_ceiling.max_steps,
            )?;
            validate_bounds(
                &format!("autonomy.{level}.budget.max_tool_calls"),
                config.budget.max_tool_calls,
                0,
                runtime_ceiling.max_tool_calls,
            )?;
            validate_bounds(
                &format!("autonomy.{level}.budget.wall_clock_secs"),
                config.budget.wall_clock_secs,
                0,
                runtime_ceiling.wall_clock_secs,
            )?;
        }
        Ok(())
    }
}
