//! The onboarding phase machine as the interface sees it: the ordered phases,
//! one per step of the first launch and three that close it, the counters the
//! graduation screen reads, and the refusals a wrong transition earns.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Phase machine types
// ---------------------------------------------------------------------------

/// The ordered phases of the onboarding flow: the seven steps of the first
/// launch, each resumed exactly where it was left, then the three that close
/// it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingPhase {
    /// Initial landing screen, where the language is chosen.
    Welcome,
    /// The language model, local or served: the only step that cannot be passed.
    AiSetup,
    /// The model search by meaning uses.
    Embeddings,
    /// The conversation that proposes a profile.
    Acquaintance,
    /// The first agent: an example installed, or one described in words.
    FirstAgent,
    /// The connection the first agent needs.
    Connection,
    /// The first task handed to that agent.
    FirstTask,
    /// Guided product tour (interactive spotlight steps).
    GuidedTour,
    /// Summary screen showing achievements and stats.
    Graduation,
    /// Terminal state - onboarding fully completed.
    Done,
}

impl OnboardingPhase {
    /// Every phase, in the order the flow walks them.
    pub const ORDER: [Self; 10] = [
        Self::Welcome,
        Self::AiSetup,
        Self::Embeddings,
        Self::Acquaintance,
        Self::FirstAgent,
        Self::Connection,
        Self::FirstTask,
        Self::GuidedTour,
        Self::Graduation,
        Self::Done,
    ];

    /// Returns the lowercase snake_case string stored in UserMemory.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Welcome => "welcome",
            Self::AiSetup => "ai_setup",
            Self::Embeddings => "embeddings",
            Self::Acquaintance => "acquaintance",
            Self::FirstAgent => "first_agent",
            Self::Connection => "connection",
            Self::FirstTask => "first_task",
            Self::GuidedTour => "guided_tour",
            Self::Graduation => "graduation",
            Self::Done => "done",
        }
    }

    /// Parses a phase from its snake_case string representation.
    ///
    /// Returns `None` for unknown strings.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ORDER.into_iter().find(|phase| phase.as_str() == s)
    }
}

/// Cumulative statistics accumulated over the full onboarding flow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, utoipa::ToSchema)]
pub struct OnboardingStats {
    /// Total time spent in the onboarding flow, in seconds.
    pub total_time_sec: u64,
    /// Number of discrete actions completed by the user.
    pub actions_completed: u32,
    /// Number of questions asked to the AI Companion.
    pub companion_questions: u32,
}

/// Full onboarding state maintained server-side and persisted in UserMemory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct OnboardingState {
    /// Current phase of the onboarding flow.
    pub phase: OnboardingPhase,
    /// `true` when at least one LLM backend has been configured.
    pub llm_configured: bool,
    /// `true` when the STT engine has been configured and tested.
    pub stt_configured: bool,
    /// Topics covered so far.
    pub topics_covered: Vec<String>,
    /// `true` when the mandatory name/role fields have been collected.
    pub mandatory_complete: bool,
    /// Session ID of the active Companion chat, if any.
    pub companion_session_id: Option<String>,
    /// `true` when the user has enabled voice commands.
    pub voice_enabled: bool,
    /// `true` if the user explicitly skipped the onboarding.
    pub skipped: bool,
    /// `true` when the phase has reached `Done`.
    pub completed: bool,
    /// ISO 8601 timestamp of the first phase transition (Welcome to AiSetup).
    pub started_at: Option<String>,
    /// ISO 8601 timestamp when the phase reached `Done`.
    pub completed_at: Option<String>,
    /// Accumulated stats for the graduation screen.
    pub stats: OnboardingStats,
}

impl Default for OnboardingState {
    fn default() -> Self {
        Self {
            phase: OnboardingPhase::Welcome,
            llm_configured: false,
            stt_configured: false,
            topics_covered: Vec::new(),
            mandatory_complete: false,
            companion_session_id: None,
            voice_enabled: false,
            skipped: false,
            completed: false,
            started_at: None,
            completed_at: None,
            stats: OnboardingStats::default(),
        }
    }
}

impl OnboardingState {
    /// Returns `true` when advancing from the current phase to `target` is a
    /// legal sequential transition.
    ///
    /// The only valid transitions are the consecutive pairs of
    /// [`OnboardingPhase::ORDER`]: a step passed is still a phase walked.
    pub fn can_advance_to(&self, target: &OnboardingPhase) -> bool {
        OnboardingPhase::ORDER
            .windows(2)
            .any(|pair| pair[0] == self.phase && pair[1] == *target)
    }
}

// ---------------------------------------------------------------------------
// Legacy public types (serialised to Svelte)
// ---------------------------------------------------------------------------

/// Result of triggering an onboarding session.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TriggerResult {
    /// Chat session identifier.
    pub session_id: String,
    /// `"full"` or `"partial"`.
    pub mode: String,
    /// Target topic when mode is `"partial"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors specific to onboarding IPC commands.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OnboardingError {
    /// The onboarding agent is not installed.
    ///
    /// The desktop installs the onboarding package at boot, once the runtime
    /// and its audit journal are up. So this fires when that installation
    /// failed, which is logged as `bundled.package.install_failed`, or when a
    /// profile was swapped in under a live app.
    #[error(
        "onboarding-agent is not installed. The desktop installs it at startup, \
         so restart the application first. If restarting does not help, look \
         for `bundled.package.install_failed` in the logs, which names the real \
         cause."
    )]
    AgentNotInstalled,

    /// The UserMemory database is unavailable.
    #[error("UserMemory database not initialized")]
    RepositoryNotInitialized,

    /// The requested topic is not one of the 5 valid onboarding topics.
    #[error("invalid topic: {0}. Valid topics: identity, preferences, tools, domain, agents")]
    InvalidTopic(String),

    /// The chat subsystem failed to create a session.
    #[error("chat session creation failed: {0}")]
    SessionCreationFailed(String),

    /// The chat subsystem is not available.
    #[error("chat subsystem not available")]
    ChatNotAvailable,

    /// The requested phase transition is not legal.
    #[error("cannot advance from {from:?} to {to:?}")]
    InvalidTransition {
        /// Source phase.
        from: OnboardingPhase,
        /// Attempted target phase.
        to: OnboardingPhase,
    },

    /// A UserMemory read or write operation failed.
    #[error("persistence error: {0}")]
    PersistenceError(String),

    /// The requested phase is not one the flow knows.
    #[error("persistence error: unknown phase: {0}")]
    UnknownPhase(String),

    /// The onboarding agent's own memory could not be written.
    #[error("{0}")]
    AgentMemory(String),
}

#[cfg(test)]
mod tests {
    use super::super::ONBOARDING_TOPICS;
    use super::*;

    #[test]
    fn test_default_state_is_welcome() {
        // GIVEN nothing
        // WHEN creating a default OnboardingState
        let state = OnboardingState::default();
        // THEN phase is Welcome
        assert_eq!(state.phase, OnboardingPhase::Welcome);
        assert!(!state.completed);
    }

    #[test]
    fn test_valid_transition_welcome_to_ai_setup() {
        // GIVEN a state in Welcome phase
        let state = OnboardingState::default();
        // WHEN checking can_advance_to AiSetup
        // THEN it returns true: no profile question stands between them
        assert!(state.can_advance_to(&OnboardingPhase::AiSetup));
    }

    #[test]
    fn test_profile_choice_is_no_longer_a_phase() {
        // GIVEN the name the removed usage-profile step was stored under
        // WHEN it is parsed as a phase
        let parsed = OnboardingPhase::parse("profile_choice");
        // THEN it is not one of the phases
        assert!(parsed.is_none());
    }

    #[test]
    fn test_invalid_transition_welcome_to_guided_tour() {
        // GIVEN a state in Welcome phase
        let state = OnboardingState::default();
        // WHEN checking can_advance_to GuidedTour
        // THEN it returns false
        assert!(!state.can_advance_to(&OnboardingPhase::GuidedTour));
    }

    #[test]
    fn test_serialization_roundtrip() {
        // GIVEN a fully populated OnboardingState
        let state = OnboardingState {
            phase: OnboardingPhase::AiSetup,
            llm_configured: true,
            stt_configured: false,
            topics_covered: vec!["agents".to_string()],
            mandatory_complete: false,
            companion_session_id: Some("sess-123".to_string()),
            voice_enabled: false,
            skipped: false,
            completed: false,
            started_at: Some("2026-04-02T10:00:00Z".to_string()),
            completed_at: None,
            stats: OnboardingStats::default(),
        };
        // WHEN serializing then deserializing
        let json = serde_json::to_string(&state).unwrap();
        let restored: OnboardingState = serde_json::from_str(&json).unwrap();
        // THEN the restored state matches
        assert_eq!(restored.phase, state.phase);
        assert!(!json.contains("\"profile\""));
        assert_eq!(restored.llm_configured, state.llm_configured);
    }

    #[test]
    fn test_the_seven_steps_of_the_first_launch_are_seven_phases_in_order() {
        // GIVEN the names the seven steps are stored under, in their order
        let steps = [
            "welcome",
            "ai_setup",
            "embeddings",
            "acquaintance",
            "first_agent",
            "connection",
            "first_task",
        ];
        // WHEN the first seven phases are read
        let names: Vec<&str> = OnboardingPhase::ORDER[..7]
            .iter()
            .map(OnboardingPhase::as_str)
            .collect();
        // THEN each step has its own phase, so a restart reopens exactly on it
        assert_eq!(names, steps);
        for name in steps {
            assert!(OnboardingPhase::parse(name).is_some(), "{name}");
        }
    }

    #[test]
    fn test_a_step_cannot_be_jumped_over() {
        // GIVEN a flow standing on the AI setup
        let state = OnboardingState {
            phase: OnboardingPhase::AiSetup,
            ..Default::default()
        };
        // WHEN the conversation is asked for without the step between them
        // THEN the move is refused, and the step between them is accepted
        assert!(!state.can_advance_to(&OnboardingPhase::Acquaintance));
        assert!(state.can_advance_to(&OnboardingPhase::Embeddings));
    }

    #[test]
    fn test_all_sequential_transitions_valid() {
        // GIVEN the full transition chain
        let phases = OnboardingPhase::ORDER;
        // WHEN checking each consecutive pair
        // THEN all are valid transitions
        for window in phases.windows(2) {
            let state = OnboardingState {
                phase: window[0].clone(),
                ..Default::default()
            };
            assert!(
                state.can_advance_to(&window[1]),
                "Transition {:?} -> {:?} should be valid",
                window[0],
                window[1]
            );
        }
    }

    #[test]
    fn test_all_non_sequential_transitions_invalid() {
        // GIVEN a state in Welcome phase
        let state = OnboardingState::default();
        // WHEN checking non-sequential targets
        // THEN all are invalid
        let non_targets = [
            OnboardingPhase::Embeddings,
            OnboardingPhase::Acquaintance,
            OnboardingPhase::FirstAgent,
            OnboardingPhase::Connection,
            OnboardingPhase::FirstTask,
            OnboardingPhase::GuidedTour,
            OnboardingPhase::Graduation,
            OnboardingPhase::Done,
        ];
        for target in &non_targets {
            assert!(
                !state.can_advance_to(target),
                "Transition Welcome -> {:?} should be invalid",
                target
            );
        }
    }

    #[test]
    fn test_invalid_transition_error_message() {
        // GIVEN an invalid transition
        let err = OnboardingError::InvalidTransition {
            from: OnboardingPhase::Welcome,
            to: OnboardingPhase::GuidedTour,
        };
        // WHEN it is rendered for the operator
        // THEN the message names both phases
        let msg = err.to_string();
        assert!(msg.contains("Welcome"));
        assert!(msg.contains("GuidedTour"));
    }

    #[test]
    fn test_onboarding_topics_count() {
        // GIVEN the topic list the onboarding walks through
        // WHEN it is read
        // THEN it holds the five named topics, so a rename breaks here and not in the progress bar
        assert_eq!(ONBOARDING_TOPICS.len(), 5);
        assert!(ONBOARDING_TOPICS.contains(&"identity"));
        assert!(ONBOARDING_TOPICS.contains(&"preferences"));
        assert!(ONBOARDING_TOPICS.contains(&"tools"));
        assert!(ONBOARDING_TOPICS.contains(&"domain"));
        assert!(ONBOARDING_TOPICS.contains(&"agents"));
    }

    #[test]
    fn test_completion_pct_calculation() {
        // GIVEN 3 out of 5 topics covered
        let covered = 3_usize;
        let total = ONBOARDING_TOPICS.len();
        let pct = ((covered as f64 / total as f64) * 100.0) as u8;

        // WHEN the completion percentage is computed
        // THEN completion is 60%
        assert_eq!(pct, 60);
    }

    #[test]
    fn test_completion_pct_full() {
        // GIVEN all 5 topics covered
        let covered = 5_usize;
        let total = ONBOARDING_TOPICS.len();
        let pct = ((covered as f64 / total as f64) * 100.0) as u8;

        // WHEN the completion percentage is computed
        // THEN completion is 100%
        assert_eq!(pct, 100);
    }

    #[test]
    fn test_completion_pct_none() {
        // GIVEN no topics covered
        let covered = 0_usize;
        let total = ONBOARDING_TOPICS.len();
        let pct = ((covered as f64 / total as f64) * 100.0) as u8;

        // WHEN the completion percentage is computed
        // THEN completion is 0%
        assert_eq!(pct, 0);
    }

    #[test]
    fn test_invalid_topic_error_message() {
        // GIVEN an invalid topic
        let err = OnboardingError::InvalidTopic("invalid".to_string());

        // WHEN it is rendered for the operator
        // THEN the error message lists valid topics
        let msg = err.to_string();
        assert!(msg.contains("invalid"));
        assert!(msg.contains("identity"));
        assert!(msg.contains("preferences"));
        assert!(msg.contains("tools"));
        assert!(msg.contains("domain"));
        assert!(msg.contains("agents"));
    }

    #[test]
    fn test_agent_not_installed_error_message() {
        // GIVEN AgentNotInstalled error
        let err = OnboardingError::AgentNotInstalled;

        // WHEN it is rendered for the operator
        // THEN the message names the agent, says restarting is the first thing
        // to try because the desktop installs it at startup, and quotes the
        // log line that names the real cause, verbatim, so it can be grepped
        let msg = err.to_string();
        assert!(msg.contains("onboarding-agent"));
        assert!(msg.contains("restart"));
        assert!(msg.contains("installs it at startup"));
        assert!(msg.contains("bundled.package.install_failed"));
    }

    #[test]
    fn test_trigger_result_full_mode() {
        // GIVEN a full onboarding trigger result
        let result = TriggerResult {
            session_id: "test-session-id".into(),
            mode: "full".into(),
            topic: None,
        };

        // WHEN serialized
        let json = serde_json::to_string(&result).expect("serialization should succeed");

        // THEN mode is full and topic is absent
        assert!(json.contains("\"mode\":\"full\""));
        assert!(!json.contains("topic"));
    }

    #[test]
    fn test_trigger_result_partial_mode() {
        // GIVEN a partial onboarding trigger result
        let result = TriggerResult {
            session_id: "test-session-id".into(),
            mode: "partial".into(),
            topic: Some("preferences".into()),
        };

        // WHEN serialized
        let json = serde_json::to_string(&result).expect("serialization should succeed");

        // THEN mode is partial and topic is present
        assert!(json.contains("\"mode\":\"partial\""));
        assert!(json.contains("\"topic\":\"preferences\""));
    }

    #[test]
    fn test_phase_as_str_roundtrip() {
        // GIVEN all phases
        let phases = OnboardingPhase::ORDER;
        // WHEN converting to string and back
        // THEN all phases roundtrip correctly
        for phase in &phases {
            let s = phase.as_str();
            let recovered = OnboardingPhase::parse(s);
            assert_eq!(
                recovered.as_ref(),
                Some(phase),
                "phase {:?} should roundtrip via as_str/parse",
                phase
            );
        }
    }

    #[test]
    fn test_phase_parse_unknown_returns_none() {
        // GIVEN an unknown phase string
        // WHEN parsing
        // THEN None is returned
        assert!(OnboardingPhase::parse("unknown_phase").is_none());
        assert!(OnboardingPhase::parse("").is_none());
    }
}
