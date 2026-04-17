use serde::{Deserialize, Serialize};

/// Configuration du budget d'exécution déclarée par l'agent dans son AgentManifest.
///
/// Ces valeurs sont des suggestions maximales. Le runtime (ORIA StepBudget)
/// applique les valeurs minimales entre la config agent et la config runtime globale.
/// Un agent ne peut PAS dépasser les limites configurées dans apollia.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepBudgetConfig {
    /// Nombre maximum de steps ORIA (appels successifs à l'agent). Défaut: 30.
    pub max_steps: u32,
    /// Nombre maximum d'appels d'outils au total sur la tâche. Défaut: 60.
    pub max_tool_calls: u32,
    /// Durée maximum wall-clock en secondes. Défaut: 600 (10 minutes).
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
    /// Budget par défaut pour les sessions de chat interactives.
    ///
    /// Les sessions de chat sont conversationnelles et impliquent souvent
    /// de nombreux appels d'outils successifs (recherche web, fetch HTTP, etc.).
    /// Les limites sont donc plus généreuses que pour l'exécution ORIA.
    pub fn chat_default() -> Self {
        Self {
            max_steps: 100,
            max_tool_calls: 200,
            wall_clock_secs: 1200,
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
        assert_eq!(budget.max_steps, 30);
        assert_eq!(budget.max_tool_calls, 60);
        assert_eq!(budget.wall_clock_secs, 600);
    }

    #[test]
    fn test_chat_default_more_generous_than_default() {
        // GIVEN / WHEN
        let chat = StepBudgetConfig::chat_default();
        let oria = StepBudgetConfig::default();
        // THEN — chat limits are strictly higher
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
