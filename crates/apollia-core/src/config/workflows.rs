use serde::{Deserialize, Serialize};

use super::{validate_bounds, ConfigError};
use crate::budget::StepBudgetConfig;

// ─────────────────────────────────────────────
// WorkflowsConfig
// ─────────────────────────────────────────────

/// Workflow configuration (`[workflows]` section in `apollia.toml`).
///
/// Holds the budget a workflow run starts with. The budget is shared: every
/// agent a step runs and every workflow a step calls draws from the budget of
/// the run that called it, so a workflow and everything under it stop
/// together once it is spent.
///
/// ```toml
/// [workflows]
/// max_steps = 200
/// max_tool_calls = 400
/// wall_clock_secs = 3600
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowsConfig {
    /// Steps a run may take, its own declared steps and the loop steps of the
    /// agents it runs together. Default: 200. Bounds: [1, 100000].
    #[serde(default = "default_max_steps")]
    pub max_steps: u32,

    /// Tool calls a run may make, those of the agents it runs included.
    /// Default: 400. Bounds: [1, 100000].
    #[serde(default = "default_max_tool_calls")]
    pub max_tool_calls: u32,

    /// Wall-clock duration of a run, in seconds. Default: 3600 (one hour).
    /// Bounds: [1, 86400].
    #[serde(default = "default_wall_clock_secs")]
    pub wall_clock_secs: u64,
}

fn default_max_steps() -> u32 {
    200
}

fn default_max_tool_calls() -> u32 {
    400
}

fn default_wall_clock_secs() -> u64 {
    3600
}

impl Default for WorkflowsConfig {
    fn default() -> Self {
        Self {
            max_steps: default_max_steps(),
            max_tool_calls: default_max_tool_calls(),
            wall_clock_secs: default_wall_clock_secs(),
        }
    }
}

impl WorkflowsConfig {
    /// The budget a workflow run starts with.
    pub fn budget(&self) -> StepBudgetConfig {
        StepBudgetConfig {
            max_steps: self.max_steps,
            max_tool_calls: self.max_tool_calls,
            wall_clock_secs: self.wall_clock_secs,
        }
    }

    /// Validates the bounds at startup (fail-fast).
    ///
    /// # Errors
    ///
    /// [`ConfigError::OutOfBounds`] for a value outside its bounds.
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds("workflows.max_steps", self.max_steps, 1, 100_000)?;
        validate_bounds("workflows.max_tool_calls", self.max_tool_calls, 1, 100_000)?;
        validate_bounds("workflows.wall_clock_secs", self.wall_clock_secs, 1, 86_400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflows_section_defaults_fill_what_is_absent() {
        // GIVEN a [workflows] section that names the steps alone
        let toml = "max_steps = 50";
        // WHEN it is read
        let config: WorkflowsConfig = toml::from_str(toml).unwrap();
        // THEN the steps are those given and the other dimensions their defaults
        let budget = config.budget();
        assert_eq!(budget.max_steps, 50);
        assert_eq!(budget.max_tool_calls, 400);
        assert_eq!(budget.wall_clock_secs, 3600);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_workflows_budget_out_of_bounds_is_refused() {
        // GIVEN a run allowed no step at all
        let config = WorkflowsConfig {
            max_steps: 0,
            ..WorkflowsConfig::default()
        };
        // WHEN it is validated
        let err = config.validate().unwrap_err();
        // THEN the key is named
        assert!(err.to_string().contains("workflows.max_steps"), "{err}");
    }
}
