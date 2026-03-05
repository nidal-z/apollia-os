use serde::{Deserialize, Serialize};

/// Machine d'état du processus agent, alignée ACP (Agent Communication Protocol).
///
/// Transitions valides :
/// `Initializing` → `Active` → `Degraded` → `Stopping` → `Stopped`
///                               ↑                           ↑
///                          (outils optionnels          (erreur fatale,
///                            manquants)                skip vers Stopping)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessState {
    /// Résolution des outils, validation du manifest, ouverture SQLite.
    /// Toute erreur ici = échec de démarrage (Principe #4 — Fail fast).
    Initializing,
    /// Prêt à accepter des tâches.
    Active,
    /// Actif mais avec des `tools_optional` manquants ou dégradés.
    Degraded,
    /// Drain des tâches en cours (timeout 30s). Refuse les nouvelles tâches.
    Stopping,
    /// Arrêt propre. Plus aucune tâche acceptée.
    Stopped,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::result::TaskStatus;

    #[test]
    fn test_ac1_process_state_variants_exist() {
        // GIVEN / WHEN / THEN
        let states = [
            ProcessState::Initializing,
            ProcessState::Active,
            ProcessState::Degraded,
            ProcessState::Stopping,
            ProcessState::Stopped,
        ];
        assert_eq!(states.len(), 5);
    }

    #[test]
    fn test_ac3_process_state_serializes_snake_case() {
        // GIVEN
        let state = ProcessState::Active;
        // WHEN
        let json = serde_json::to_string(&state).expect("serialize failed");
        // THEN
        assert_eq!(json, "\"active\"");
    }

    #[test]
    fn test_ac3_task_status_input_required_serializes() {
        // GIVEN
        let status = TaskStatus::InputRequired;
        // WHEN
        let json = serde_json::to_string(&status).expect("serialize failed");
        // THEN
        assert_eq!(json, "\"input_required\"");
    }

    #[test]
    fn test_ac4_unknown_state_deserializes_to_error() {
        // GIVEN
        let invalid = "\"unknown_state\"";
        // WHEN
        let result: Result<ProcessState, _> = serde_json::from_str(invalid);
        // THEN
        assert!(result.is_err());
    }

    #[test]
    fn test_ac2_task_status_all_variants_exist() {
        // GIVEN / WHEN / THEN
        let statuses = [
            TaskStatus::Submitted,
            TaskStatus::Working,
            TaskStatus::Completed,
            TaskStatus::Failed,
            TaskStatus::InputRequired,
            TaskStatus::Canceled,
        ];
        assert_eq!(statuses.len(), 6);
    }
}
