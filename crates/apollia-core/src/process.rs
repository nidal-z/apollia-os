use serde::{Deserialize, Serialize};

/// Agent process state machine, aligned with ACP (Agent Communication Protocol).
///
/// Valid transitions:
/// `Initializing` -> `Active` -> `Degraded` -> `Stopping` -> `Stopped`
///                                 ^                            ^
///                            (missing optional            (fatal error,
///                             tools)                       skip to Stopping)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessState {
    /// Tool resolution, manifest validation, SQLite opening.
    /// Any error here is a startup failure (fail fast).
    Initializing,
    /// Ready to accept tasks.
    Active,
    /// Active but with missing or degraded `tools_optional`.
    Degraded,
    /// Draining in-flight tasks (30s timeout). Refuses new tasks.
    Stopping,
    /// Clean stop. No more tasks accepted.
    Stopped,
}

impl ProcessState {
    /// Checks whether the transition to `target` is valid per the state machine.
    ///
    /// Valid transitions:
    /// - `Initializing` -> `Active` (normal startup)
    /// - `Initializing` -> `Stopping` (fail-fast on startup error)
    /// - `Active` -> `Degraded` (missing optional tools)
    /// - `Active` -> `Stopping` (stop requested)
    /// - `Degraded` -> `Active` (recovery)
    /// - `Degraded` -> `Stopping` (stop requested)
    /// - `Stopping` -> `Stopped` (end of task drain)
    pub fn can_transition_to(&self, target: &ProcessState) -> bool {
        matches!(
            (self, target),
            (ProcessState::Initializing, ProcessState::Active)
                | (ProcessState::Initializing, ProcessState::Stopping)
                | (ProcessState::Active, ProcessState::Degraded)
                | (ProcessState::Active, ProcessState::Stopping)
                | (ProcessState::Degraded, ProcessState::Active)
                | (ProcessState::Degraded, ProcessState::Stopping)
                | (ProcessState::Stopping, ProcessState::Stopped)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::result::TaskStatus;

    #[test]
    fn test_process_state_variants_exist() {
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
    fn test_process_state_serializes_snake_case() {
        // GIVEN
        let state = ProcessState::Active;
        // WHEN
        let json = serde_json::to_string(&state).expect("serialize failed");
        // THEN
        assert_eq!(json, "\"active\"");
    }

    #[test]
    fn test_task_status_input_required_serializes() {
        // GIVEN
        let status = TaskStatus::InputRequired;
        // WHEN
        let json = serde_json::to_string(&status).expect("serialize failed");
        // THEN
        assert_eq!(json, "\"input_required\"");
    }

    #[test]
    fn test_can_transition_to_valid() {
        // GIVEN / WHEN / THEN: all valid transitions pass
        assert!(ProcessState::Initializing.can_transition_to(&ProcessState::Active));
        assert!(ProcessState::Initializing.can_transition_to(&ProcessState::Stopping));
        assert!(ProcessState::Active.can_transition_to(&ProcessState::Degraded));
        assert!(ProcessState::Active.can_transition_to(&ProcessState::Stopping));
        assert!(ProcessState::Degraded.can_transition_to(&ProcessState::Active));
        assert!(ProcessState::Degraded.can_transition_to(&ProcessState::Stopping));
        assert!(ProcessState::Stopping.can_transition_to(&ProcessState::Stopped));
    }

    #[test]
    fn test_can_transition_to_invalid() {
        // GIVEN / WHEN / THEN: invalid transitions refused
        assert!(!ProcessState::Stopped.can_transition_to(&ProcessState::Active));
        assert!(!ProcessState::Stopped.can_transition_to(&ProcessState::Initializing));
        assert!(!ProcessState::Initializing.can_transition_to(&ProcessState::Stopped));
        assert!(!ProcessState::Active.can_transition_to(&ProcessState::Initializing));
    }

    #[test]
    fn test_unknown_state_deserializes_to_error() {
        // GIVEN
        let invalid = "\"unknown_state\"";
        // WHEN
        let result: Result<ProcessState, _> = serde_json::from_str(invalid);
        // THEN
        assert!(result.is_err());
    }

    #[test]
    fn test_task_status_all_variants_exist() {
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
