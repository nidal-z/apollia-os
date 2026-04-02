//! Tauri IPC commands for onboarding lifecycle management.
//!
//! Provides the full onboarding state machine (7 phases) with persistence
//! in UserMemory (`context.onboarding_*` keys), plus the legacy helpers that
//! drive the existing chat-based onboarding:
//!
//! - [`get_onboarding_state`] — full machine state (new)
//! - [`advance_onboarding_phase`] — validated phase transition (new)
//! - [`set_onboarding_profile`] — profile selection (new)
//! - [`get_onboarding_status`] — backward-compatible completion flag
//! - [`trigger_onboarding`] — creates an agent-backed chat session
//! - [`dismiss_onboarding`] — marks onboarding as skipped
//!
//! All data stays local (Principle #1). Structs are serde-typed for the
//! frontend (Principle #8). Invalid transitions are rejected immediately
//! (Principle #4).

use std::sync::Arc;

use apollia_memory::user_memory::{
    UserMemoryCategory, UserMemoryRepository, UserMemorySource,
};
use apollia_runtime::chat::ChatMode;
use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

/// The five onboarding topics that define a complete (legacy) onboarding.
const ONBOARDING_TOPICS: [&str; 5] = ["identity", "preferences", "tools", "domain", "agents"];

/// Name of the agent used for onboarding conversations.
const ONBOARDING_AGENT_NAME: &str = "onboarding-agent";

/// Valid user profiles for the onboarding flow.
const VALID_PROFILES: [&str; 2] = ["operator", "builder"];

// ---------------------------------------------------------------------------
// Phase machine types
// ---------------------------------------------------------------------------

/// The seven ordered phases of the Sprint 33 onboarding flow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingPhase {
    /// Initial landing screen — shown on first launch.
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
    /// Terminal state — onboarding fully completed.
    Done,
}

impl OnboardingPhase {
    /// Returns the lowercase snake_case string stored in UserMemory.
    fn as_str(&self) -> &'static str {
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
    fn from_str(s: &str) -> Option<Self> {
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
    /// Number of voice commands used during the tour.
    pub voice_commands_used: u32,
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
    /// Topics covered so far (legacy compatibility with `OnboardingStatus`).
    pub topics_covered: Vec<String>,
    /// `true` when the mandatory name/role fields have been collected.
    pub mandatory_complete: bool,
    /// Index of the current guided-tour step (0-based).
    pub tour_step_index: u32,
    /// Total number of steps in the guided tour for the chosen profile.
    pub tour_total_steps: u32,
    /// `true` when all guided-tour steps have been visited.
    pub tour_completed: bool,
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
            tour_step_index: 0,
            tour_total_steps: 0,
            tour_completed: false,
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

/// Onboarding completion status returned to the frontend (backward-compatible).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingStatus {
    /// `true` if all 5 topics have been covered.
    pub completed: bool,
    /// `true` if the mandatory fields (name, role) have been collected.
    pub mandatory_complete: bool,
    /// Topics already covered by the user.
    pub topics_covered: Vec<String>,
    /// Completion percentage (0–100).
    pub completion_pct: u8,
    /// ISO 8601 timestamp of the last onboarding session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_session_at: Option<String>,
    /// `true` if the user explicitly dismissed the onboarding.
    pub skipped: bool,
}

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
pub enum OnboardingError {
    /// The onboarding agent is not registered in the runtime.
    #[error("onboarding-agent not found — it should be provisioned automatically at startup. Check that Python is available and restart the application.")]
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
    #[error("Cannot advance from {from:?} to {to:?}")]
    InvalidTransition {
        /// Source phase.
        from: OnboardingPhase,
        /// Attempted target phase.
        to: OnboardingPhase,
    },

    /// The supplied profile name is not one of the accepted values.
    #[error("Invalid profile: {0}. Expected 'operator' or 'builder'")]
    InvalidProfile(String),

    /// A UserMemory read or write operation failed.
    #[error("Persistence error: {0}")]
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

// ---------------------------------------------------------------------------
// Tauri commands — phase machine
// ---------------------------------------------------------------------------

/// Returns the full onboarding state reconstructed from UserMemory.
///
/// On first launch (no persisted keys), returns a default state with
/// `phase: Welcome`.
#[tauri::command]
pub async fn get_onboarding_state(
    state: State<'_, RuntimeHandle>,
) -> Result<OnboardingState, String> {
    get_onboarding_state_inner(&state).await.map_err(|e| {
        tracing::error!(error = %e, "get_onboarding_state failed");
        e.to_string()
    })
}

/// Attempts to advance the onboarding flow to `target_phase`.
///
/// The transition is rejected with [`OnboardingError::InvalidTransition`]
/// if it does not follow the legal sequential chain.  On success, the new
/// state is persisted and, when the phase reaches `Done`, a
/// `RuntimeEvent::OnboardingCompleted` is emitted on the EventBus.
#[tauri::command]
pub async fn advance_onboarding_phase(
    target_phase: String,
    state: State<'_, RuntimeHandle>,
) -> Result<OnboardingState, String> {
    advance_onboarding_phase_inner(target_phase, &state)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "advance_onboarding_phase failed");
            e.to_string()
        })
}

/// Sets the user's onboarding profile to `"operator"` or `"builder"`.
///
/// Returns `Err` if the profile string is not one of the two accepted values.
/// The state is persisted to UserMemory on success.
#[tauri::command]
pub async fn set_onboarding_profile(
    profile: String,
    state: State<'_, RuntimeHandle>,
) -> Result<OnboardingState, String> {
    set_onboarding_profile_inner(profile, &state)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "set_onboarding_profile failed");
            e.to_string()
        })
}

// ---------------------------------------------------------------------------
// Tauri commands — legacy
// ---------------------------------------------------------------------------

/// Returns the current onboarding status (backward-compatible).
#[tauri::command]
pub async fn get_onboarding_status(
    state: State<'_, RuntimeHandle>,
) -> Result<OnboardingStatus, String> {
    get_onboarding_status_inner(&state).await.map_err(|e| {
        tracing::error!(error = %e, "get_onboarding_status failed");
        e.to_string()
    })
}

/// Triggers a full or partial onboarding session.
///
/// Pass `topic = None` for a full onboarding (all 5 topics).
/// Pass `topic = Some("preferences")` for a single-topic re-run.
#[tauri::command]
pub async fn trigger_onboarding(
    topic: Option<String>,
    state: State<'_, RuntimeHandle>,
) -> Result<TriggerResult, String> {
    trigger_onboarding_inner(topic, &state).await.map_err(|e| {
        tracing::error!(error = %e, "trigger_onboarding failed");
        e.to_string()
    })
}

/// Marks the onboarding as dismissed (skipped) by the user.
#[tauri::command]
pub async fn dismiss_onboarding(state: State<'_, RuntimeHandle>) -> Result<(), String> {
    dismiss_onboarding_inner(&state).await.map_err(|e| {
        tracing::error!(error = %e, "dismiss_onboarding failed");
        e.to_string()
    })
}

// ---------------------------------------------------------------------------
// Inner logic — phase machine
// ---------------------------------------------------------------------------

async fn get_onboarding_state_inner(
    state: &RuntimeHandle,
) -> Result<OnboardingState, OnboardingError> {
    let repo = get_repo(state)?;

    tokio::task::spawn_blocking(move || {
        let repo = repo
            .lock()
            .map_err(|e| OnboardingError::PersistenceError(format!("mutex poisoned: {e}")))?;
        load_state_from_memory(&repo)
    })
    .await
    .map_err(|e| OnboardingError::PersistenceError(format!("spawn_blocking failed: {e}")))?
}

async fn advance_onboarding_phase_inner(
    target_phase: String,
    state: &RuntimeHandle,
) -> Result<OnboardingState, OnboardingError> {
    let target = OnboardingPhase::from_str(&target_phase)
        .ok_or_else(|| OnboardingError::PersistenceError(format!("unknown phase: {target_phase}")))?;

    let repo = get_repo(state)?;
    let event_sender = state.event_sender.clone();

    tokio::task::spawn_blocking(move || {
        let repo = repo
            .lock()
            .map_err(|e| OnboardingError::PersistenceError(format!("mutex poisoned: {e}")))?;

        let mut onboarding_state = load_state_from_memory(&repo)?;

        if !onboarding_state.can_advance_to(&target) {
            return Err(OnboardingError::InvalidTransition {
                from: onboarding_state.phase.clone(),
                to: target,
            });
        }

        // Set started_at on the first transition out of Welcome.
        if onboarding_state.phase == OnboardingPhase::Welcome
            && onboarding_state.started_at.is_none()
        {
            onboarding_state.started_at = Some(chrono::Utc::now().to_rfc3339());
        }

        onboarding_state.phase = target.clone();

        if target == OnboardingPhase::Done {
            onboarding_state.completed = true;
            onboarding_state.completed_at = Some(chrono::Utc::now().to_rfc3339());
        }

        persist_state(&repo, &onboarding_state)?;

        // Emit the completion event outside the lock (fire-and-forget).
        if target == OnboardingPhase::Done {
            let profile = onboarding_state
                .profile
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            let _ = event_sender.send(apollia_core::RuntimeEvent::OnboardingCompleted {
                profile,
                duration_sec: onboarding_state.stats.total_time_sec,
                actions_count: onboarding_state.stats.actions_completed,
            });
        }

        tracing::info!(
            phase = %target.as_str(),
            "onboarding phase advanced"
        );

        Ok(onboarding_state)
    })
    .await
    .map_err(|e| OnboardingError::PersistenceError(format!("spawn_blocking failed: {e}")))?
}

async fn set_onboarding_profile_inner(
    profile: String,
    state: &RuntimeHandle,
) -> Result<OnboardingState, OnboardingError> {
    validate_profile(&profile)?;

    let repo = get_repo(state)?;

    tokio::task::spawn_blocking(move || {
        let repo = repo
            .lock()
            .map_err(|e| OnboardingError::PersistenceError(format!("mutex poisoned: {e}")))?;

        let mut onboarding_state = load_state_from_memory(&repo)?;
        onboarding_state.profile = Some(profile.clone());
        persist_state(&repo, &onboarding_state)?;

        tracing::info!(profile = %profile, "onboarding profile set");

        Ok(onboarding_state)
    })
    .await
    .map_err(|e| OnboardingError::PersistenceError(format!("spawn_blocking failed: {e}")))?
}

// ---------------------------------------------------------------------------
// Persistence helpers
// ---------------------------------------------------------------------------

/// Loads the full `OnboardingState` from UserMemory context keys.
///
/// Returns `OnboardingState::default()` when no persisted phase is found
/// (first launch).
fn load_state_from_memory(repo: &UserMemoryRepository) -> Result<OnboardingState, OnboardingError> {
    let ctx = UserMemoryCategory::Context;

    let phase_str = repo
        .recall_by_key(ctx, "onboarding_phase")
        .map_err(|e| OnboardingError::PersistenceError(e.to_string()))?
        .map(|e| e.value);

    let Some(phase_str) = phase_str else {
        return Ok(OnboardingState::default());
    };

    let phase = OnboardingPhase::from_str(&phase_str).ok_or_else(|| {
        OnboardingError::PersistenceError(format!("unrecognised persisted phase: {phase_str}"))
    })?;

    let profile = repo
        .recall_by_key(ctx, "onboarding_profile")
        .map_err(|e| OnboardingError::PersistenceError(e.to_string()))?
        .map(|e| e.value);

    let llm_configured = read_bool(repo, "onboarding_llm_configured")?;
    let stt_configured = read_bool(repo, "onboarding_stt_configured")?;

    let topics_covered = repo
        .recall_by_key(ctx, "onboarding_topics_covered")
        .map_err(|e| OnboardingError::PersistenceError(e.to_string()))?
        .map(|e| {
            serde_json::from_str::<Vec<String>>(&e.value).unwrap_or_default()
        })
        .unwrap_or_default();

    let mandatory_complete = read_bool(repo, "onboarding_mandatory_complete")?;
    let tour_step_index = read_u32(repo, "onboarding_tour_step_index")?;
    let tour_total_steps = read_u32(repo, "onboarding_tour_total_steps")?;
    let tour_completed = read_bool(repo, "onboarding_tour_completed")?;

    let companion_session_id = repo
        .recall_by_key(ctx, "onboarding_companion_session_id")
        .map_err(|e| OnboardingError::PersistenceError(e.to_string()))?
        .map(|e| e.value)
        .filter(|v| !v.is_empty());

    let voice_enabled = read_bool(repo, "onboarding_voice_enabled")?;
    let skipped = read_bool(repo, "onboarding_skipped")?;
    let completed = read_bool(repo, "onboarding_completed")?;

    let started_at = repo
        .recall_by_key(ctx, "onboarding_started_at")
        .map_err(|e| OnboardingError::PersistenceError(e.to_string()))?
        .map(|e| e.value)
        .filter(|v| v != "none");

    let completed_at = repo
        .recall_by_key(ctx, "onboarding_completed_at")
        .map_err(|e| OnboardingError::PersistenceError(e.to_string()))?
        .map(|e| e.value)
        .filter(|v| v != "none");

    let stats = OnboardingStats {
        total_time_sec: read_u64(repo, "onboarding_stats_total_time_sec")?,
        actions_completed: read_u32(repo, "onboarding_stats_actions_completed")?,
        companion_questions: read_u32(repo, "onboarding_stats_companion_questions")?,
        voice_commands_used: read_u32(repo, "onboarding_stats_voice_commands_used")?,
    };

    Ok(OnboardingState {
        phase,
        profile,
        llm_configured,
        stt_configured,
        topics_covered,
        mandatory_complete,
        tour_step_index,
        tour_total_steps,
        tour_completed,
        companion_session_id,
        voice_enabled,
        skipped,
        completed,
        started_at,
        completed_at,
        stats,
    })
}

/// Persists the full `OnboardingState` to UserMemory context keys.
fn persist_state(
    repo: &UserMemoryRepository,
    state: &OnboardingState,
) -> Result<(), OnboardingError> {
    let ctx = UserMemoryCategory::Context;
    let src = UserMemorySource::Onboarding;

    repo.store(ctx, "onboarding_phase", state.phase.as_str(), src)
        .map_err(|e| OnboardingError::PersistenceError(e.to_string()))?;

    repo.store(
        ctx,
        "onboarding_profile",
        state.profile.as_deref().unwrap_or(""),
        src,
    )
    .map_err(|e| OnboardingError::PersistenceError(e.to_string()))?;

    write_bool(repo, "onboarding_llm_configured", state.llm_configured)?;
    write_bool(repo, "onboarding_stt_configured", state.stt_configured)?;

    let topics_json = serde_json::to_string(&state.topics_covered)
        .map_err(|e| OnboardingError::PersistenceError(e.to_string()))?;
    repo.store(ctx, "onboarding_topics_covered", &topics_json, src)
        .map_err(|e| OnboardingError::PersistenceError(e.to_string()))?;

    write_bool(repo, "onboarding_mandatory_complete", state.mandatory_complete)?;
    write_u32(repo, "onboarding_tour_step_index", state.tour_step_index)?;
    write_u32(repo, "onboarding_tour_total_steps", state.tour_total_steps)?;
    write_bool(repo, "onboarding_tour_completed", state.tour_completed)?;

    repo.store(
        ctx,
        "onboarding_companion_session_id",
        state.companion_session_id.as_deref().unwrap_or(""),
        src,
    )
    .map_err(|e| OnboardingError::PersistenceError(e.to_string()))?;

    write_bool(repo, "onboarding_voice_enabled", state.voice_enabled)?;
    write_bool(repo, "onboarding_skipped", state.skipped)?;
    write_bool(repo, "onboarding_completed", state.completed)?;

    repo.store(
        ctx,
        "onboarding_started_at",
        state.started_at.as_deref().unwrap_or("none"),
        src,
    )
    .map_err(|e| OnboardingError::PersistenceError(e.to_string()))?;

    repo.store(
        ctx,
        "onboarding_completed_at",
        state.completed_at.as_deref().unwrap_or("none"),
        src,
    )
    .map_err(|e| OnboardingError::PersistenceError(e.to_string()))?;

    write_u64(repo, "onboarding_stats_total_time_sec", state.stats.total_time_sec)?;
    write_u32(repo, "onboarding_stats_actions_completed", state.stats.actions_completed)?;
    write_u32(repo, "onboarding_stats_companion_questions", state.stats.companion_questions)?;
    write_u32(repo, "onboarding_stats_voice_commands_used", state.stats.voice_commands_used)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Low-level read/write helpers
// ---------------------------------------------------------------------------

fn read_bool(repo: &UserMemoryRepository, key: &str) -> Result<bool, OnboardingError> {
    let entry = repo
        .recall_by_key(UserMemoryCategory::Context, key)
        .map_err(|e| OnboardingError::PersistenceError(e.to_string()))?;
    Ok(entry.map(|e| e.value == "true").unwrap_or(false))
}

fn write_bool(
    repo: &UserMemoryRepository,
    key: &str,
    value: bool,
) -> Result<(), OnboardingError> {
    repo.store(
        UserMemoryCategory::Context,
        key,
        if value { "true" } else { "false" },
        UserMemorySource::Onboarding,
    )
    .map_err(|e| OnboardingError::PersistenceError(e.to_string()))
}

fn read_u32(repo: &UserMemoryRepository, key: &str) -> Result<u32, OnboardingError> {
    let entry = repo
        .recall_by_key(UserMemoryCategory::Context, key)
        .map_err(|e| OnboardingError::PersistenceError(e.to_string()))?;
    Ok(entry
        .and_then(|e| e.value.parse::<u32>().ok())
        .unwrap_or(0))
}

fn write_u32(repo: &UserMemoryRepository, key: &str, value: u32) -> Result<(), OnboardingError> {
    repo.store(
        UserMemoryCategory::Context,
        key,
        &value.to_string(),
        UserMemorySource::Onboarding,
    )
    .map_err(|e| OnboardingError::PersistenceError(e.to_string()))
}

fn read_u64(repo: &UserMemoryRepository, key: &str) -> Result<u64, OnboardingError> {
    let entry = repo
        .recall_by_key(UserMemoryCategory::Context, key)
        .map_err(|e| OnboardingError::PersistenceError(e.to_string()))?;
    Ok(entry
        .and_then(|e| e.value.parse::<u64>().ok())
        .unwrap_or(0))
}

fn write_u64(repo: &UserMemoryRepository, key: &str, value: u64) -> Result<(), OnboardingError> {
    repo.store(
        UserMemoryCategory::Context,
        key,
        &value.to_string(),
        UserMemorySource::Onboarding,
    )
    .map_err(|e| OnboardingError::PersistenceError(e.to_string()))
}

// ---------------------------------------------------------------------------
// Inner logic — legacy
// ---------------------------------------------------------------------------

/// Acquires the `UserMemoryRepository` from the runtime handle.
fn get_repo(
    state: &RuntimeHandle,
) -> Result<Arc<std::sync::Mutex<UserMemoryRepository>>, OnboardingError> {
    state
        .user_memory
        .as_ref()
        .cloned()
        .ok_or(OnboardingError::RepositoryNotInitialized)
}

/// Maps a memory key written by the onboarding agent to a topic name.
///
/// The agent writes keys like `user.name`, `user.tools.ide`, `user.domain.stack`
/// etc. This function maps them to the five onboarding topics.
fn topic_for_memory_key(key: &str) -> Option<&'static str> {
    if key.starts_with("user.name")
        || key.starts_with("user.role")
        || key.starts_with("user.languages")
        || key.starts_with("user.expertise")
        || key.starts_with("user.industry")
        || key.starts_with("user.goals")
    {
        Some("identity")
    } else if key.starts_with("user.preferences") {
        Some("preferences")
    } else if key.starts_with("user.tools") {
        Some("tools")
    } else if key.starts_with("user.domain") {
        Some("domain")
    } else if key.starts_with("user.agents") || key.starts_with("user.challenges") {
        Some("agents")
    } else {
        None
    }
}

/// Checks whether the mandatory onboarding fields (name, role) have been collected.
///
/// Scans the onboarding agent's semantic memory namespace for keys `user.name`
/// and `user.role` with confidence >= 0.5.
fn mandatory_fields_collected(agent_db_path: &std::path::Path) -> bool {
    if !agent_db_path.exists() {
        return false;
    }
    let Ok(store) = apollia_memory::store::MemoryStore::open(agent_db_path) else {
        return false;
    };
    let sem = apollia_memory::semantic::SemanticMemory::new(&store);
    let Ok(entries) = sem.recall_all("onboarding-agent") else {
        return false;
    };
    let has_name = entries
        .iter()
        .any(|e| e.key.starts_with("user.name") && e.confidence >= 0.5);
    let has_role = entries
        .iter()
        .any(|e| e.key.starts_with("user.role") && e.confidence >= 0.5);
    has_name && has_role
}

/// Reads onboarding status from UserMemory.
///
/// Also scans the `onboarding-agent` semantic memory namespace to auto-detect
/// covered topics from the keys the agent has written (`user.name`, `user.tools.ide`,
/// etc.) and marks them in `UserMemoryRepository` so the progress bar advances.
async fn get_onboarding_status_inner(
    state: &RuntimeHandle,
) -> Result<OnboardingStatus, OnboardingError> {
    let repo = get_repo(state)?;

    // Open the agent's memory store to scan for written keys.
    let memory_dir = {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        std::path::PathBuf::from(home)
            .join(".apollia")
            .join("memory")
    };
    let agent_db_path = memory_dir.join("onboarding-agent.db");

    let status = tokio::task::spawn_blocking(move || {
        let repo = repo
            .lock()
            .map_err(|e| OnboardingError::SessionCreationFailed(format!("mutex poisoned: {e}")))?;

        // Auto-detect covered topics from the agent's semantic memory.
        if agent_db_path.exists() {
            if let Ok(agent_store) = apollia_memory::store::MemoryStore::open(&agent_db_path) {
                let sem = apollia_memory::semantic::SemanticMemory::new(&agent_store);
                if let Ok(entries) = sem.recall_all("onboarding-agent") {
                    let mut discovered = std::collections::HashSet::new();
                    for entry in &entries {
                        if let Some(topic) = topic_for_memory_key(&entry.key) {
                            discovered.insert(topic);
                        }
                    }
                    // Mark newly discovered topics.
                    for topic in discovered {
                        let _ = repo.mark_topic_covered(topic);
                    }
                }
            }
        }

        let topics_covered = repo
            .get_covered_topics()
            .map_err(|_| OnboardingError::RepositoryNotInitialized)?;

        let total = ONBOARDING_TOPICS.len();
        let covered = topics_covered.len().min(total);
        let completion_pct = ((covered as f64 / total as f64) * 100.0) as u8;
        let completed = completion_pct == 100;

        let last_session_at = repo.get_last_onboarding_session().unwrap_or(None);

        let skipped = repo.get_onboarding_skipped().unwrap_or(false);

        let mandatory_complete = mandatory_fields_collected(&agent_db_path);

        Ok::<OnboardingStatus, OnboardingError>(OnboardingStatus {
            completed,
            mandatory_complete,
            topics_covered,
            completion_pct,
            last_session_at,
            skipped,
        })
    })
    .await
    .map_err(|e| OnboardingError::SessionCreationFailed(format!("spawn_blocking failed: {e}")))?;

    status
}

/// Creates an onboarding chat session.
async fn trigger_onboarding_inner(
    topic: Option<String>,
    state: &RuntimeHandle,
) -> Result<TriggerResult, OnboardingError> {
    // Validate topic if provided
    if let Some(ref t) = topic {
        if !ONBOARDING_TOPICS.contains(&t.as_str()) {
            return Err(OnboardingError::InvalidTopic(t.clone()));
        }
    }

    // Verify the onboarding agent is registered
    let found = state
        .registry_handle
        .find_by_name(ONBOARDING_AGENT_NAME)
        .await
        .map_err(|_| OnboardingError::AgentNotInstalled)?;

    if found.is_none() {
        return Err(OnboardingError::AgentNotInstalled);
    }

    // Build system prompt based on mode
    let mode = if topic.is_some() { "partial" } else { "full" };
    let system_prompt = build_onboarding_prompt(&topic);

    // Create chat session via ChatSessionManager
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or(OnboardingError::ChatNotAvailable)?;

    let info = manager
        .create_session(
            ChatMode::Agent,
            Some(ONBOARDING_AGENT_NAME.to_string()),
            Some(system_prompt),
            Vec::new(),
        )
        .await
        .map_err(|e| OnboardingError::SessionCreationFailed(e.to_string()))?;

    let session_id = info.id.clone();

    // Emit OnboardingStarted event
    let _ = state
        .event_sender
        .send(apollia_core::RuntimeEvent::OnboardingStarted {
            session_id: session_id.clone(),
            mode: mode.to_string(),
            topic: topic.clone(),
        });

    tracing::info!(
        session_id = %session_id,
        mode = %mode,
        topic = ?topic,
        "onboarding session created"
    );

    Ok(TriggerResult {
        session_id,
        mode: mode.to_string(),
        topic,
    })
}

/// Marks onboarding as skipped in UserMemory.
async fn dismiss_onboarding_inner(state: &RuntimeHandle) -> Result<(), OnboardingError> {
    let repo = get_repo(state)?;

    tokio::task::spawn_blocking(move || {
        let repo = repo
            .lock()
            .map_err(|e| OnboardingError::SessionCreationFailed(format!("mutex poisoned: {e}")))?;

        repo.set_onboarding_skipped(true)
            .map_err(|_| OnboardingError::RepositoryNotInitialized)?;

        Ok::<(), OnboardingError>(())
    })
    .await
    .map_err(|e| OnboardingError::SessionCreationFailed(format!("spawn_blocking failed: {e}")))?
    ?;

    tracing::info!("onboarding dismissed by user");
    Ok(())
}

/// Builds the system prompt for the onboarding agent.
fn build_onboarding_prompt(topic: &Option<String>) -> String {
    match topic {
        Some(t) => format!(
            "You are an onboarding assistant. Focus exclusively on the topic: {t}. \
             Ask natural questions to learn about the user's {t}. \
             Do not cover other topics.",
        ),
        None => format!(
            "You are an onboarding assistant for all professionals (not just developers). \
             First, ALWAYS collect the user's name and role/profession — these are mandatory. \
             Then cover these topics naturally through conversation: {}. \
             Ask questions one at a time. Be conversational, not rigid. \
             Adapt your questions to the user's profession.",
            ONBOARDING_TOPICS.join(", "),
        ),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
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
            tour_step_index: 0,
            tour_total_steps: 12,
            tour_completed: false,
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
        let phases = vec![
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
            let mut state = OnboardingState::default();
            state.phase = window[0].clone();
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
    fn test_build_onboarding_prompt_full() {
        // GIVEN no specific topic
        let prompt = build_onboarding_prompt(&None);

        // THEN the prompt mentions all 5 topics
        for topic in &ONBOARDING_TOPICS {
            assert!(
                prompt.contains(topic),
                "full prompt should mention topic: {topic}"
            );
        }
    }

    #[test]
    fn test_build_onboarding_prompt_partial() {
        // GIVEN a specific topic
        let prompt = build_onboarding_prompt(&Some("preferences".to_string()));

        // THEN the prompt focuses on that topic
        assert!(prompt.contains("preferences"));
        assert!(prompt.contains("Focus exclusively"));
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

        // THEN the message is actionable
        let msg = err.to_string();
        assert!(msg.contains("onboarding-agent"));
        assert!(msg.contains("restart"));
    }

    #[test]
    fn test_onboarding_status_serialization() {
        // GIVEN a complete onboarding status
        let status = OnboardingStatus {
            completed: true,
            mandatory_complete: true,
            topics_covered: vec![
                "identity".into(),
                "preferences".into(),
                "tools".into(),
                "domain".into(),
                "agents".into(),
            ],
            completion_pct: 100,
            last_session_at: Some("2026-06-15T14:30:00Z".into()),
            skipped: false,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_string(&status).expect("serialization should succeed");

        // THEN all fields are present
        assert!(json.contains("\"completed\":true"));
        assert!(json.contains("\"completion_pct\":100"));
        assert!(json.contains("\"skipped\":false"));
        assert!(json.contains("\"last_session_at\":\"2026-06-15T14:30:00Z\""));
    }

    #[test]
    fn test_onboarding_status_new_user_serialization() {
        // GIVEN a new user status
        let status = OnboardingStatus {
            completed: false,
            mandatory_complete: false,
            topics_covered: vec![],
            completion_pct: 0,
            last_session_at: None,
            skipped: false,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_string(&status).expect("serialization should succeed");

        // THEN completed is false and last_session_at is absent
        assert!(json.contains("\"completed\":false"));
        assert!(json.contains("\"completion_pct\":0"));
        assert!(!json.contains("last_session_at"));
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
            assert_eq!(recovered.as_ref(), Some(phase), "phase {:?} should roundtrip via as_str/from_str", phase);
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
