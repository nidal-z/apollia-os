use serde::{Deserialize, Serialize};

use crate::config::{AutonomyLevel, AutonomyLevelConfig};

/// Execution budget configuration declared by the agent in its AgentManifest.
///
/// These values are maximum suggestions. The runtime (ORIA StepBudget) applies
/// the minimum between the agent config and the global runtime config. An agent
/// CANNOT exceed the limits configured in apollia.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepBudgetConfig {
    /// Maximum number of ORIA steps (successive calls to the agent). Default: 30.
    pub max_steps: u32,
    /// Maximum number of tool calls in total over the task. Default: 60.
    pub max_tool_calls: u32,
    /// Maximum wall-clock duration in seconds. Default: 600 (10 minutes).
    /// The `wall_clock_timeout_secs` alias is accepted for doc compatibility.
    #[serde(alias = "wall_clock_timeout_secs")]
    pub wall_clock_secs: u64,
}

impl Default for StepBudgetConfig {
    fn default() -> Self {
        Self {
            max_steps: 30,
            max_tool_calls: 60,
            wall_clock_secs: 600,
        }
    }
}

impl StepBudgetConfig {
    /// Default budget for interactive chat sessions.
    ///
    /// Chat sessions are conversational and often involve many successive tool
    /// calls (web search, HTTP fetch, etc.). The limits are therefore more
    /// generous than for ORIA execution.
    pub fn chat_default() -> Self {
        Self {
            max_steps: 100,
            max_tool_calls: 200,
            wall_clock_secs: 1200,
        }
    }
}

impl AutonomyLevel {
    /// Effective (uncapped) budget suggested for this tier.
    ///
    /// This is the default per-tier budget (see [`AutonomyLevelConfig::default_for`]).
    /// It is always capped afterwards by `StepBudget::from_capped` against the
    /// runtime ceiling, so it never raises the budget above the runtime bound.
    pub fn effective_budget(self) -> StepBudgetConfig {
        AutonomyLevelConfig::default_for(self).budget
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ac1_step_budget_defaults() {
        // GIVEN / WHEN
        let budget = StepBudgetConfig::default();
        // THEN
        assert_eq!(budget.max_steps, 30);
        assert_eq!(budget.max_tool_calls, 60);
        assert_eq!(budget.wall_clock_secs, 600);
    }

    #[test]
    fn test_chat_default_more_generous_than_default() {
        // GIVEN / WHEN
        let chat = StepBudgetConfig::chat_default();
        let oria = StepBudgetConfig::default();
        // THEN: chat limits are strictly higher
        assert!(chat.max_steps > oria.max_steps);
        assert!(chat.max_tool_calls > oria.max_tool_calls);
        assert!(chat.wall_clock_secs > oria.wall_clock_secs);
    }

    #[test]
    fn test_ac2_step_budget_round_trip_json() {
        // GIVEN
        let budget = StepBudgetConfig {
            max_steps: 5,
            max_tool_calls: 10,
            wall_clock_secs: 60,
        };
        // WHEN
        let json = serde_json::to_string(&budget).expect("serialize");
        let restored: StepBudgetConfig = serde_json::from_str(&json).expect("deserialize");
        // THEN
        assert_eq!(restored.max_steps, 5);
        assert_eq!(restored.max_tool_calls, 10);
        assert_eq!(restored.wall_clock_secs, 60);
    }
}
