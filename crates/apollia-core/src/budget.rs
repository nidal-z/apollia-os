use serde::{Deserialize, Serialize};

/// Configuration du budget d'exécution déclarée par l'agent dans son AgentManifest.
///
/// Ces valeurs sont des suggestions maximales. Le runtime (ORIA StepBudget)
/// applique les valeurs minimales entre la config agent et la config runtime globale.
/// Un agent ne peut PAS dépasser les limites configurées dans apollia.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepBudgetConfig {
    /// Nombre maximum de steps ORIA (appels successifs à l'agent). Défaut: 10.
    pub max_steps: u32,
    /// Nombre maximum d'appels d'outils au total sur la tâche. Défaut: 20.
    pub max_tool_calls: u32,
    /// Durée maximum wall-clock en secondes. Défaut: 300 (5 minutes).
    pub wall_clock_secs: u64,
}

impl Default for StepBudgetConfig {
    fn default() -> Self {
        Self {
            max_steps: 10,
            max_tool_calls: 20,
            wall_clock_secs: 300,
        }
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
        assert_eq!(budget.max_steps, 10);
        assert_eq!(budget.max_tool_calls, 20);
        assert_eq!(budget.wall_clock_secs, 300);
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
