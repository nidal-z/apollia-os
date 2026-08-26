//! The onboarding phase machine as the frontend sees it: the seven ordered
//! phases, the counters the graduation screen reads, and the refusals a wrong
//! transition earns.

use serde::{Deserialize, Serialize};

use super::VALID_PROFILES;

// ---------------------------------------------------------------------------
// Phase machine types
// ---------------------------------------------------------------------------

/// The seven ordered phases of the onboarding flow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingPhase {
    /// Initial landing screen - shown on first launch.
    Welcome,
    /// Profile selection: Operator or Builder.
    ProfileChoice,
    /// LLM and STT configuration.
    AiSetup,
    /// AI Companion introduction and first interaction.
    Acquaintance,
    /// Guided product tour (interactive spotlight steps).
    GuidedTour,
    /// Summary screen showing achievements and stats.
    Graduation,
    /// Terminal state - onboarding fully completed.
    Done,
}

impl OnboardingPhase {
    /// Returns the lowercase snake_case string stored in UserMemory.
    pub(super) fn as_str(&self) -> &'static str {
        match self {
            Self::Welcome => "welcome",
            Self::ProfileChoice => "profile_choice",
            Self::AiSetup => "ai_setup",
            Self::Acquaintance => "acquaintance",
            Self::GuidedTour => "guided_tour",
            Self::Graduation => "graduation",
            Self::Done => "done",
        }
    }

    /// Parses a phase from its snake_case string representation.
    ///
    /// Returns `None` for unknown strings.
    pub(super) fn from_str(s: &str) -> Option<Self> {
        match s {
            "welcome" => Some(Self::Welcome),
            "profile_choice" => Some(Self::ProfileChoice),
            "ai_setup" => Some(Self::AiSetup),
            "acquaintance" => Some(Self::Acquaintance),
            "guided_tour" => Some(Self::GuidedTour),
            "graduation" => Some(Self::Graduation),
            "done" => Some(Self::Done),
            _ => None,
        }
    }
}

/// Cumulative statistics accumulated over the full onboarding flow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OnboardingStats {
    /// Total time spent in the onboarding flow, in seconds.
    pub total_time_sec: u64,
    /// Number of discrete actions completed by the user.
    pub actions_completed: u32,
    /// Number of questions asked to the AI Companion.
    pub companion_questions: u32,
}

/// Full onboarding state maintained server-side and persisted in UserMemory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OnboardingState {
    /// Current phase of the onboarding flow.
    pub phase: OnboardingPhase,
    /// Chosen user profile (`"operator"` or `"builder"`); set during ProfileChoice.
    pub profile: Option<String>,
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
    /// ISO 8601 timestamp of the first phase transition (Welcome → ProfileChoice).
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
            profile: None,
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
    /// The only valid transitions are the six consecutive steps in the ordered
    /// chain: `Welcome → ProfileChoice → AiSetup → Acquaintance → GuidedTour
    /// → Graduation → Done`.
    pub fn can_advance_to(&self, target: &OnboardingPhase) -> bool {
        matches!(
            (&self.phase, target),
            (OnboardingPhase::Welcome, OnboardingPhase::ProfileChoice)
                | (OnboardingPhase::ProfileChoice, OnboardingPhase::AiSetup)
                | (OnboardingPhase::AiSetup, OnboardingPhase::Acquaintance)
                | (OnboardingPhase::Acquaintance, OnboardingPhase::GuidedTour)
                | (OnboardingPhase::GuidedTour, OnboardingPhase::Graduation)
                | (OnboardingPhase::Graduation, OnboardingPhase::Done)
        )
    }
}

// ---------------------------------------------------------------------------
// Legacy public types (serialised to Svelte)
// ---------------------------------------------------------------------------

/// Result of triggering an onboarding session.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// The onboarding agent is not registered in the runtime.
    ///
    /// The registry is filled once, at boot, from `agents.db`. So this fires
    /// whenever the application has been running since before the agent was
    /// installed, which is the common case when a seeded profile is swapped in
    /// under a live app. The previous wording blamed Python, which sent the
    /// reader looking at the interpreter while the real cause was the boot
    /// order, and the actual load failure, when there is one, is logged as a
    /// warning nobody reads: `agent.load.failed`.
    #[error(
        "onboarding-agent is not in the runtime registry. The registry is built \
         once at startup, so restart the application first: if the agent was \
         installed, or a profile swapped in, while it was running, it cannot \
         have been picked up. If restarting does not help, look for \
         `agent.load.failed` in the logs, which names the real cause."
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

    /// The apollia-guide companion agent is not registered in the runtime.
    #[error("apollia-guide agent not found - it powers the companion panel and is provisioned at startup, bundle it and restart the application")]
    GuideAgentNotInstalled,

    /// The requested phase transition is not legal.
    #[error("cannot advance from {from:?} to {to:?}")]
    InvalidTransition {
        /// Source phase.
        from: OnboardingPhase,
        /// Attempted target phase.
        to: OnboardingPhase,
    },

    /// The supplied profile name is not one of the accepted values.
    #[error("invalid profile: {0}, expected 'operator' or 'builder'")]
    InvalidProfile(String),

    /// A UserMemory read or write operation failed.
    #[error("persistence error: {0}")]
    PersistenceError(String),
}

// ---------------------------------------------------------------------------
// Profile validation
// ---------------------------------------------------------------------------

/// Validates that `profile` is one of the two accepted values.
///
/// Returns `Ok(())` for `"operator"` and `"builder"`, and
/// `Err(OnboardingError::InvalidProfile)` for anything else.
pub fn validate_profile(profile: &str) -> Result<(), OnboardingError> {
    if VALID_PROFILES.contains(&profile) {
        Ok(())
    } else {
        Err(OnboardingError::InvalidProfile(profile.to_owned()))
    }
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
    fn test_valid_transition_welcome_to_profile_choice() {
        // GIVEN a state in Welcome phase
        let state = OnboardingState::default();
        // WHEN checking can_advance_to ProfileChoice
        // THEN it returns true
        assert!(state.can_advance_to(&OnboardingPhase::ProfileChoice));
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
    fn test_profile_validation_rejects_invalid() {
        // GIVEN an invalid profile name
        let result = validate_profile("hacker");
        // THEN it returns an error
        assert!(result.is_err());
    }

    #[test]
    fn test_profile_validation_accepts_operator() {
        // GIVEN a valid profile name
        let result = validate_profile("operator");
        // THEN it succeeds
        assert!(result.is_ok());
    }

    #[test]
    fn test_profile_validation_accepts_builder() {
        // GIVEN a valid profile name
        let result = validate_profile("builder");
        // THEN it succeeds
        assert!(result.is_ok());
    }

    #[test]
    fn test_serialization_roundtrip() {
        // GIVEN a fully populated OnboardingState
        let state = OnboardingState {
            phase: OnboardingPhase::AiSetup,
            profile: Some("builder".to_string()),
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
        assert_eq!(restored.profile, state.profile);
        assert_eq!(restored.llm_configured, state.llm_configured);
    }

    #[test]
    fn test_all_sequential_transitions_valid() {
        // GIVEN the full transition chain
        let phases = [
            OnboardingPhase::Welcome,
            OnboardingPhase::ProfileChoice,
            OnboardingPhase::AiSetup,
            OnboardingPhase::Acquaintance,
            OnboardingPhase::GuidedTour,
            OnboardingPhase::Graduation,
            OnboardingPhase::Done,
        ];
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
            OnboardingPhase::AiSetup,
            OnboardingPhase::Acquaintance,
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
        // THEN the message names both phases
        let msg = err.to_string();
        assert!(msg.contains("Welcome"));
        assert!(msg.contains("GuidedTour"));
    }

    #[test]
    fn test_invalid_profile_error_message() {
        // GIVEN an invalid profile
        let err = OnboardingError::InvalidProfile("hacker".to_string());
        // THEN the message names the offending profile and valid ones
        let msg = err.to_string();
        assert!(msg.contains("hacker"));
        assert!(msg.contains("operator"));
        assert!(msg.contains("builder"));
    }

    #[test]
    fn test_onboarding_topics_count() {
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

        // THEN completion is 60%
        assert_eq!(pct, 60);
    }

    #[test]
    fn test_completion_pct_full() {
        // GIVEN all 5 topics covered
        let covered = 5_usize;
        let total = ONBOARDING_TOPICS.len();
        let pct = ((covered as f64 / total as f64) * 100.0) as u8;

        // THEN completion is 100%
        assert_eq!(pct, 100);
    }

    #[test]
    fn test_completion_pct_none() {
        // GIVEN no topics covered
        let covered = 0_usize;
        let total = ONBOARDING_TOPICS.len();
        let pct = ((covered as f64 / total as f64) * 100.0) as u8;

        // THEN completion is 0%
        assert_eq!(pct, 0);
    }

    #[test]
    fn test_invalid_topic_error_message() {
        // GIVEN an invalid topic
        let err = OnboardingError::InvalidTopic("invalid".to_string());

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

        // THEN the message names the first suspect, the boot-order one, and
        // points at the log line that carries the real cause when restarting is
        // not enough. It used to say "check that Python is available", which
        // sent a maintainer into the interpreter for two hours while the app
        // had simply been running since before the profile was swapped in.
        let msg = err.to_string();
        assert!(msg.contains("onboarding-agent"));
        assert!(msg.contains("restart"));
        assert!(
            msg.contains("built") && msg.contains("startup"),
            "the message must say the registry is built once at startup, which \
             is what makes restarting the first thing to try"
        );
        assert!(
            msg.contains("agent.load.failed"),
            "the message must quote the log line that names the real cause, \
             verbatim, so it can be grepped"
        );
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
        let phases = [
            OnboardingPhase::Welcome,
            OnboardingPhase::ProfileChoice,
            OnboardingPhase::AiSetup,
            OnboardingPhase::Acquaintance,
            OnboardingPhase::GuidedTour,
            OnboardingPhase::Graduation,
            OnboardingPhase::Done,
        ];
        // WHEN converting to string and back
        // THEN all phases roundtrip correctly
        for phase in &phases {
            let s = phase.as_str();
            let recovered = OnboardingPhase::from_str(s);
            assert_eq!(
                recovered.as_ref(),
                Some(phase),
                "phase {:?} should roundtrip via as_str/from_str",
                phase
            );
        }
    }

    #[test]
    fn test_phase_from_str_unknown_returns_none() {
        // GIVEN an unknown phase string
        // WHEN parsing
        // THEN None is returned
        assert!(OnboardingPhase::from_str("unknown_phase").is_none());
        assert!(OnboardingPhase::from_str("").is_none());
    }
}
