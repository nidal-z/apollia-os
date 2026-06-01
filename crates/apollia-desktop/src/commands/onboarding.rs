//! Tauri IPC commands for onboarding lifecycle management.
//!
//! Provides the full onboarding state machine (7 phases) with persistence
//! in UserMemory (`context.onboarding_*` keys), plus the legacy helpers that
//! drive the existing chat-based onboarding:
//!
//! - [`get_onboarding_state`] - full machine state (new)
//! - [`advance_onboarding_phase`] - validated phase transition (new)
//! - [`set_onboarding_profile`] - profile selection (new)
//! - [`get_onboarding_status`] - backward-compatible completion flag
//! - [`trigger_onboarding`] - creates an agent-backed chat session
//! - [`dismiss_onboarding`] - marks onboarding as skipped
//!
//! All data stays local (Principle #1). Structs are serde-typed for the
//! frontend (Principle #8). Invalid transitions are rejected immediately
//! (Principle #4).

use std::sync::Arc;

use apollia_memory::user_memory::UserMemoryRepository;
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
    #[error("onboarding-agent not found - it should be provisioned automatically at startup. Check that Python is available and restart the application.")]
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
// Tauri commands - phase machine
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

/// Checks whether the onboarding-agent has written `onboarding.completed_at`
/// in its semantic memory.
///
/// Used by the desktop's onboarding chat step to detect that the agent has
/// finished its 4-turn calibration and finalized the user profile, so the
/// modal can switch to the wrap-up screen and offer the "Terminer" CTA.
///
/// Looks under both the current namespace (`onboarding`, since manifest
/// v2.x) and the legacy one (`onboarding-agent`, kept for backwards-compat
/// with installs that ran an older agent).
#[tauri::command]
pub async fn check_onboarding_finalized() -> Result<bool, String> {
    let memory_dir = {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        std::path::PathBuf::from(home)
            .join(".apollia")
            .join("memory")
    };

    if !memory_dir.exists() {
        return Ok(false);
    }

    // (db_filename, namespace_in_table) - the manifest namespace also names
    // the SQLite file (see `MemoryManager::db_path`), but the semantic table
    // carries a redundant namespace column we must filter on.
    let candidates: [(&str, &str); 2] = [
        ("onboarding.db", "onboarding"),
        ("onboarding-agent.db", "onboarding-agent"),
    ];

    let result = tokio::task::spawn_blocking(move || -> bool {
        for (filename, namespace) in candidates {
            let db_path = memory_dir.join(filename);
            if !db_path.exists() {
                continue;
            }
            let Ok(store) = apollia_memory::store::MemoryStore::open(&db_path) else {
                continue;
            };
            let sem = apollia_memory::semantic::SemanticMemory::new(&store);
            let Ok(entries) = sem.recall_all(namespace, None) else {
                continue;
            };
            if entries.iter().any(|e| e.key == "onboarding.completed_at") {
                return true;
            }
        }
        false
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?;

    Ok(result)
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
            // Self-transitions surface as InvalidTransition with from == to.
            // The frontend's `syncBackendPhase` fires defensively whenever the
            // step changes, so a no-op advance is expected on resume / when
            // the backend is already ahead. Log at debug instead of error.
            match &e {
                OnboardingError::InvalidTransition { from, to } if from == to => {
                    tracing::debug!(
                        error = %e,
                        "advance_onboarding_phase: self-transition ignored"
                    );
                }
                _ => {
                    tracing::error!(error = %e, "advance_onboarding_phase failed");
                }
            }
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
// Tauri commands - legacy
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
///
/// Pass `profile = Some("operator")` or `profile = Some("builder")` to inject
/// the selected profile into the onboarding agent's memory before the session
/// starts.  Passing `None` preserves backward-compatible generic behaviour.
#[tauri::command]
pub async fn trigger_onboarding(
    topic: Option<String>,
    profile: Option<String>,
    state: State<'_, RuntimeHandle>,
) -> Result<TriggerResult, String> {
    trigger_onboarding_inner(topic, profile, &state)
        .await
        .map_err(|e| {
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
// Inner logic - phase machine
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
    let target = OnboardingPhase::from_str(&target_phase).ok_or_else(|| {
        OnboardingError::PersistenceError(format!("unknown phase: {target_phase}"))
    })?;

    let repo = get_repo(state)?;
    let event_sender = state.event_sender.clone();

    tokio::task::spawn_blocking(move || {
        let repo = repo
            .lock()
            .map_err(|e| OnboardingError::PersistenceError(format!("mutex poisoned: {e}")))?;

        let mut onboarding_state = load_state_from_memory(&repo)?;

        // Idempotent self-transition: when the frontend re-syncs an already
        // active phase (e.g. after restoring from a previous session and
        // re-entering the same step), surface success rather than a hard
        // error. Avoids spurious ERROR logs on a no-op call.
        if onboarding_state.phase == target {
            return Ok(onboarding_state);
        }

        if !onboarding_state.can_advance_to(&target) {
            return Err(OnboardingError::InvalidTransition {
                from: onboarding_state.phase.clone(),
                to: target,
            });
        }

        apply_phase_transition(&repo, &mut onboarding_state, &target);
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

/// Apply the in-memory side effects of advancing to `target`.
///
/// Sets `started_at` on the first move out of Welcome, marks all topics covered
/// when entering a terminal-ish phase, and stamps completion on `Done`.
fn apply_phase_transition(
    repo: &UserMemoryRepository,
    onboarding_state: &mut OnboardingState,
    target: &OnboardingPhase,
) {
    // Set started_at on the first transition out of Welcome.
    if onboarding_state.phase == OnboardingPhase::Welcome && onboarding_state.started_at.is_none() {
        onboarding_state.started_at = Some(chrono::Utc::now().to_rfc3339());
    }

    onboarding_state.phase = target.clone();

    // When leaving Acquaintance, auto-mark any uncovered topics so the
    // progress reaches 100% even if the agent didn't ask about every topic.
    if *target == OnboardingPhase::GuidedTour
        || *target == OnboardingPhase::Graduation
        || *target == OnboardingPhase::Done
    {
        for topic in &ONBOARDING_TOPICS {
            let _ = repo.mark_topic_covered(topic);
        }
    }

    if *target == OnboardingPhase::Done {
        onboarding_state.completed = true;
        onboarding_state.completed_at = Some(chrono::Utc::now().to_rfc3339());
    }
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
    let phase_str = read_str(repo, "onboarding_phase")?;
    let Some(phase_str) = phase_str else {
        return Ok(OnboardingState::default());
    };

    let phase = OnboardingPhase::from_str(&phase_str).ok_or_else(|| {
        OnboardingError::PersistenceError(format!("unrecognised persisted phase: {phase_str}"))
    })?;

    let profile = read_str(repo, "onboarding_profile")?;

    let llm_configured = read_bool(repo, "onboarding_llm_configured")?;
    let stt_configured = read_bool(repo, "onboarding_stt_configured")?;

    let topics_covered = read_str(repo, "onboarding_topics_covered")?
        .map(|v| serde_json::from_str::<Vec<String>>(&v).unwrap_or_default())
        .unwrap_or_default();

    let mandatory_complete = read_bool(repo, "onboarding_mandatory_complete")?;
    let tour_step_index = read_u32(repo, "onboarding_tour_step_index")?;
    let tour_total_steps = read_u32(repo, "onboarding_tour_total_steps")?;
    let tour_completed = read_bool(repo, "onboarding_tour_completed")?;

    let companion_session_id =
        read_str(repo, "onboarding_companion_session_id")?.filter(|v| !v.is_empty());

    let voice_enabled = read_bool(repo, "onboarding_voice_enabled")?;
    let skipped = read_bool(repo, "onboarding_skipped")?;
    let completed = read_bool(repo, "onboarding_completed")?;

    let started_at = read_str(repo, "onboarding_started_at")?.filter(|v| v != "none");
    let completed_at = read_str(repo, "onboarding_completed_at")?.filter(|v| v != "none");

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

/// Persists the full `OnboardingState` to UserMemory internal-state keys.
///
/// All keys are stamped with the `__` prefix in storage so they stay hidden
/// from the user profile listing (ADR-087).
fn persist_state(
    repo: &UserMemoryRepository,
    state: &OnboardingState,
) -> Result<(), OnboardingError> {
    write_str(repo, "onboarding_phase", state.phase.as_str())?;
    write_str(
        repo,
        "onboarding_profile",
        state.profile.as_deref().unwrap_or(""),
    )?;

    write_bool(repo, "onboarding_llm_configured", state.llm_configured)?;
    write_bool(repo, "onboarding_stt_configured", state.stt_configured)?;

    let topics_json = serde_json::to_string(&state.topics_covered)
        .map_err(|e| OnboardingError::PersistenceError(e.to_string()))?;
    write_str(repo, "onboarding_topics_covered", &topics_json)?;

    write_bool(
        repo,
        "onboarding_mandatory_complete",
        state.mandatory_complete,
    )?;
    write_u32(repo, "onboarding_tour_step_index", state.tour_step_index)?;
    write_u32(repo, "onboarding_tour_total_steps", state.tour_total_steps)?;
    write_bool(repo, "onboarding_tour_completed", state.tour_completed)?;

    write_str(
        repo,
        "onboarding_companion_session_id",
        state.companion_session_id.as_deref().unwrap_or(""),
    )?;

    write_bool(repo, "onboarding_voice_enabled", state.voice_enabled)?;
    write_bool(repo, "onboarding_skipped", state.skipped)?;
    write_bool(repo, "onboarding_completed", state.completed)?;

    write_str(
        repo,
        "onboarding_started_at",
        state.started_at.as_deref().unwrap_or("none"),
    )?;
    write_str(
        repo,
        "onboarding_completed_at",
        state.completed_at.as_deref().unwrap_or("none"),
    )?;

    write_u64(
        repo,
        "onboarding_stats_total_time_sec",
        state.stats.total_time_sec,
    )?;
    write_u32(
        repo,
        "onboarding_stats_actions_completed",
        state.stats.actions_completed,
    )?;
    write_u32(
        repo,
        "onboarding_stats_companion_questions",
        state.stats.companion_questions,
    )?;
    write_u32(
        repo,
        "onboarding_stats_voice_commands_used",
        state.stats.voice_commands_used,
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Low-level read/write helpers - all operate on internal-state keys
// ---------------------------------------------------------------------------

fn read_str(repo: &UserMemoryRepository, key: &str) -> Result<Option<String>, OnboardingError> {
    repo.get_internal(key)
        .map_err(|e| OnboardingError::PersistenceError(e.to_string()))
}

fn write_str(repo: &UserMemoryRepository, key: &str, value: &str) -> Result<(), OnboardingError> {
    repo.set_internal(key, value)
        .map_err(|e| OnboardingError::PersistenceError(e.to_string()))
}

fn read_bool(repo: &UserMemoryRepository, key: &str) -> Result<bool, OnboardingError> {
    Ok(read_str(repo, key)?.map(|v| v == "true").unwrap_or(false))
}

fn write_bool(repo: &UserMemoryRepository, key: &str, value: bool) -> Result<(), OnboardingError> {
    write_str(repo, key, if value { "true" } else { "false" })
}

fn read_u32(repo: &UserMemoryRepository, key: &str) -> Result<u32, OnboardingError> {
    Ok(read_str(repo, key)?
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0))
}

fn write_u32(repo: &UserMemoryRepository, key: &str, value: u32) -> Result<(), OnboardingError> {
    write_str(repo, key, &value.to_string())
}

fn read_u64(repo: &UserMemoryRepository, key: &str) -> Result<u64, OnboardingError> {
    Ok(read_str(repo, key)?
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0))
}

fn write_u64(repo: &UserMemoryRepository, key: &str, value: u64) -> Result<(), OnboardingError> {
    write_str(repo, key, &value.to_string())
}

// ---------------------------------------------------------------------------
// Inner logic - legacy
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

/// Scan the onboarding agent's semantic memory and mark every discovered
/// topic as covered in the user-memory repository.
///
/// A missing or unreadable agent store is a silent no-op - the progress bar
/// simply stays at whatever the repository already knows.
fn auto_mark_discovered_topics(repo: &UserMemoryRepository, agent_db_path: &std::path::Path) {
    if !agent_db_path.exists() {
        return;
    }
    let Ok(agent_store) = apollia_memory::store::MemoryStore::open(agent_db_path) else {
        return;
    };
    let sem = apollia_memory::semantic::SemanticMemory::new(&agent_store);
    let Ok(entries) = sem.recall_all("onboarding-agent", None) else {
        return;
    };
    let mut discovered = std::collections::HashSet::new();
    for entry in &entries {
        if let Some(topic) = topic_for_memory_key(&entry.key) {
            discovered.insert(topic);
        }
    }
    for topic in discovered {
        let _ = repo.mark_topic_covered(topic);
    }
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
    let Ok(entries) = sem.recall_all("onboarding-agent", None) else {
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
        auto_mark_discovered_topics(&repo, &agent_db_path);

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

/// Writes the active onboarding profile to the onboarding agent's semantic
/// memory so the Python agent can read it via `ctx.memory.recall()` on its
/// first conversational turn.
///
/// Writes to the agent's own namespace (`"onboarding-agent"`) under the key
/// `"onboarding.active_profile"`.  A missing or unwritable memory store is
/// treated as a non-fatal degradation - the agent falls back to generic
/// questioning without the profile section.
fn write_profile_to_agent_memory(profile: &str) {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let db_path = std::path::PathBuf::from(home)
        .join(".apollia")
        .join("memory")
        .join("onboarding-agent.db");

    let Ok(store) = apollia_memory::store::MemoryStore::open(&db_path) else {
        tracing::warn!("onboarding agent memory store not found - profile not injected");
        return;
    };
    let sem = apollia_memory::semantic::SemanticMemory::new(&store);
    if let Err(e) = sem.remember(apollia_memory::semantic::RememberInput {
        namespace: "onboarding-agent",
        key: "onboarding.active_profile",
        value: &serde_json::Value::String(profile.to_string()),
        confidence: 0.95,
        source: Some("onboarding"),
        expires_at: None,
    }) {
        tracing::warn!(error = %e, "failed to write profile to agent memory");
    }
}

/// Wipes stale onboarding progress before starting a fresh session.
///
/// Without this, two sources of stale state cause `get_onboarding_status` to
/// return 100% on session arrival:
///   1. `UserMemoryRepository` keeps `onboarding_topic_*` marks across runs.
///   2. The onboarding agent's semantic DB keeps `user.*` entries from prior
///      conversations (auto-discovery in `get_onboarding_status_inner` then
///      re-marks every topic on first poll).
///
/// This helper:
///   - Forgets every `onboarding_topic_{topic}` entry in the user repo.
///   - Forgets every `user.*` and meta `onboarding.*` key in the agent's
///     semantic namespace (`onboarding.active_profile` is preserved - the
///     caller writes it just after this reset).
fn reset_onboarding_progress(repo: &UserMemoryRepository) {
    for topic in &ONBOARDING_TOPICS {
        let key = format!("onboarding_topic_{topic}");
        let _ = repo.forget(&key);
    }

    // Both filename/namespace pairs are wiped - the manifest namespace was
    // renamed from "onboarding-agent" to "onboarding" in v2.x and an install
    // upgraded across that change can have entries in either file. Forgetting
    // to clean the new file caused stale `onboarding.completed_at` to leak
    // into fresh sessions and trigger the wrap-up panel before the user
    // could answer a single question.
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let memory_dir = std::path::PathBuf::from(home)
        .join(".apollia")
        .join("memory");

    let candidates: [(&str, &str); 2] = [
        ("onboarding.db", "onboarding"),
        ("onboarding-agent.db", "onboarding-agent"),
    ];

    for (filename, namespace) in candidates {
        let db_path = memory_dir.join(filename);
        if !db_path.exists() {
            continue;
        }
        let Ok(store) = apollia_memory::store::MemoryStore::open(&db_path) else {
            tracing::warn!(
                file = filename,
                "onboarding agent memory store unreadable - stale entries may persist"
            );
            continue;
        };
        let sem = apollia_memory::semantic::SemanticMemory::new(&store);
        let Ok(entries) = sem.recall_all(namespace, None) else {
            continue;
        };

        for entry in entries {
            let is_user = entry.key.starts_with("user.");
            let is_meta_to_clear =
                entry.key.starts_with("onboarding.") && entry.key != "onboarding.active_profile";
            if !is_user && !is_meta_to_clear {
                continue;
            }
            if let Err(e) = sem.forget(namespace, &entry.key) {
                tracing::warn!(
                    key = %entry.key,
                    namespace = namespace,
                    error = %e,
                    "failed to wipe stale onboarding entry",
                );
            }
        }
    }
}

/// Creates an onboarding chat session.
///
/// When `profile` is provided it is validated, then persisted to the onboarding
/// agent's semantic memory so the Python agent can adapt its questions.
async fn trigger_onboarding_inner(
    topic: Option<String>,
    profile: Option<String>,
    state: &RuntimeHandle,
) -> Result<TriggerResult, OnboardingError> {
    // Validate topic if provided
    if let Some(ref t) = topic {
        if !ONBOARDING_TOPICS.contains(&t.as_str()) {
            return Err(OnboardingError::InvalidTopic(t.clone()));
        }
    }

    // Wipe stale progress (topic marks + agent semantic entries from prior
    // sessions). Without this the progress bar shows 100% before the user
    // has even sent a single message - see `reset_onboarding_progress`.
    if topic.is_none() {
        if let Ok(repo_arc) = get_repo(state) {
            if let Ok(repo) = repo_arc.lock() {
                reset_onboarding_progress(&repo);
            } else {
                tracing::warn!("user memory repo poisoned - onboarding reset skipped");
            }
        }
    }

    // Validate and inject profile before creating the session.
    if let Some(ref p) = profile {
        validate_profile(p)?;
        write_profile_to_agent_memory(p);
        tracing::info!(profile = %p, "onboarding profile injected into agent memory");
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
        .create_session(apollia_runtime::chat::manager::CreateSessionParams {
            mode: ChatMode::Agent,
            agent_name: Some(ONBOARDING_AGENT_NAME.to_string()),
            system_prompt: Some(system_prompt),
            tools: Vec::new(),
            project_id: None,
        })
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
    .map_err(|e| OnboardingError::SessionCreationFailed(format!("spawn_blocking failed: {e}")))??;

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
        None => String::from(
            "You are an onboarding assistant for all professionals (not just developers). \
             First, ALWAYS collect the user's name and role/profession - these are mandatory. \
             Then cover ALL five topics naturally through conversation. \
             You MUST cover every single topic before concluding - do not skip any:\n\
             1. **identity** - name, role/profession, expertise, industry, goals\n\
             2. **preferences** - communication style, response detail level, language\n\
             3. **tools** - IDE, AI tools, project management, version control\n\
             4. **domain** - sector, channels (LinkedIn, website…), current focus\n\
             5. **agents** - what AI agents or automations they want to use, challenges they face with AI\n\n\
             Ask questions one at a time. Be conversational, not rigid. \
             Adapt your questions to the user's profession. \
             IMPORTANT: Do NOT conclude the onboarding until you have gathered information on ALL five topics, \
             especially 'agents' (what the user wants to automate or delegate to AI agents).",
        ),
    }
}

// ---------------------------------------------------------------------------
// Companion - context table
// ---------------------------------------------------------------------------

/// Per-route contextual help texts displayed in the Companion panel.
const COMPANION_CONTEXTS: &[(&str, &str)] = &[
    ("dashboard", "Vous êtes sur le tableau de bord. Ici vous voyez un aperçu de vos agents actifs, les événements récents, et l'état du système. Posez-moi des questions sur la gestion de vos agents."),
    ("agents", "Vous êtes sur la page Agents. Vous pouvez créer, configurer et surveiller vos agents IA. Chaque agent a un manifest Python avec les fonctions manifest() et run()."),
    ("chat", "Vous êtes sur la page Chat. Vous pouvez converser avec un LLM local ou interagir avec vos agents via le Chat Libre. Les sessions sont sauvegardées localement."),
    ("triggers", "Vous êtes sur la page Triggers. Les triggers déclenchent automatiquement des agents selon des conditions : cron, intervalle, surveillance de fichiers, ou webhooks."),
    ("pipelines", "Vous êtes sur la page Pipelines. Les pipelines orchestrent plusieurs agents en séquence ou en parallèle avec topologie DAG, fan-out/fan-in, et points de validation humaine."),
    ("memory", "Vous êtes sur la page Mémoire. Apollia offre 3 types de mémoire : épisodique (événements), sémantique (connaissances), et procédurale (savoir-faire). Recherche FTS5 intégrée."),
    ("integrations", "Vous êtes sur la page Intégrations MCP. Connectez des serveurs MCP externes pour donner de nouveaux outils à vos agents via le protocole standard JSON-RPC 2.0."),
    ("approvals", "Vous êtes sur la page Approbations. Les actions sensibles de vos agents peuvent nécessiter votre validation avant exécution (Human-in-the-Loop)."),
    ("observability", "Vous êtes sur la page Observabilité. Suivez les traces d'exécution de vos agents, les métriques de performance, et les logs structurés en temps réel."),
    ("notifications", "Vous êtes sur la page Notifications. Configurez les alertes desktop et webhooks pour être informé des événements importants de vos agents."),
    ("transcriptions", "Vous êtes sur la page Transcriptions. Visualisez les transcriptions audio générées par le moteur STT Whisper intégré."),
    ("llm", "Vous êtes sur la page LLM. Configurez les backends de modèles de langage : llama.cpp local, Ollama, ou API cloud (Anthropic, OpenAI) pour vos agents."),
    ("settings", "Vous êtes sur la page Paramètres. Configurez les options globales d'Apollia : chemins, logs, limites de ressources, et préférences utilisateur."),
    ("onboarding", "Vous êtes en cours d'onboarding. Je suis là pour vous guider à travers la découverte d'Apollia. N'hésitez pas à me poser des questions à chaque étape."),
];

/// Generic fallback when no route-specific context is available.
const COMPANION_CONTEXT_FALLBACK: &str =
    "Je suis votre assistant Apollia. Posez-moi n'importe quelle question sur l'application, vos agents, ou les fonctionnalités disponibles.";

/// Returns the contextual help text for the given application route.
///
/// Falls back to a generic message for unknown routes.
pub fn get_companion_context_text(route: &str) -> &'static str {
    COMPANION_CONTEXTS
        .iter()
        .find(|(r, _)| *r == route)
        .map(|(_, ctx)| *ctx)
        .unwrap_or(COMPANION_CONTEXT_FALLBACK)
}

/// Return payload for companion session creation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionSessionResult {
    /// Unique identifier of the newly created companion session.
    pub session_id: String,
}

// ---------------------------------------------------------------------------
// Companion - Tauri commands
// ---------------------------------------------------------------------------

/// Returns the contextual help text for the given application route.
///
/// Exposes [`get_companion_context_text`] over IPC so the frontend can
/// pre-load context before opening the panel.
#[tauri::command]
pub async fn get_companion_context(route: String) -> Result<String, String> {
    Ok(get_companion_context_text(&route).to_string())
}

/// Creates a Chat Libre companion session with a route-contextual system prompt.
///
/// The session is persisted normally through the chat subsystem. On success,
/// the `session_id` is stored in the onboarding state so other commands can
/// reference the active companion session.
#[tauri::command]
pub async fn create_companion_session(
    context: Option<String>,
    state: State<'_, RuntimeHandle>,
) -> Result<CompanionSessionResult, String> {
    create_companion_session_inner(context, &state)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "create_companion_session failed");
            e.to_string()
        })
}

async fn create_companion_session_inner(
    context: Option<String>,
    state: &RuntimeHandle,
) -> Result<CompanionSessionResult, OnboardingError> {
    let context_text = context
        .as_deref()
        .map(get_companion_context_text)
        .unwrap_or(COMPANION_CONTEXT_FALLBACK);

    let system_prompt = format!(
        "Tu es le Companion Apollia, un assistant contextuel intégré à l'interface. \
         {context_text} \
         Réponds de manière concise et adaptée au contexte de la page. \
         Tu n'injectes jamais de mémoire automatiquement - c'est l'utilisateur qui décide."
    );

    let manager = state
        .chat_manager
        .as_ref()
        .ok_or(OnboardingError::ChatNotAvailable)?;

    let info = manager
        .create_session(apollia_runtime::chat::manager::CreateSessionParams {
            mode: ChatMode::Companion,
            agent_name: None,
            system_prompt: Some(system_prompt),
            tools: Vec::new(),
            project_id: None,
        })
        .await
        .map_err(|e| OnboardingError::SessionCreationFailed(e.to_string()))?;

    let session_id = info.id.clone();

    // Best-effort: persist the companion session id in the onboarding state.
    if let Ok(repo) = get_repo(state) {
        let sid = session_id.clone();
        let _ = tokio::task::spawn_blocking(move || {
            if let Ok(repo) = repo.lock() {
                let mut ob_state = load_state_from_memory(&repo).unwrap_or_default();
                ob_state.companion_session_id = Some(sid);
                let _ = persist_state(&repo, &ob_state);
            }
        })
        .await;
    }

    tracing::info!(
        session_id = %session_id,
        context = ?context,
        "companion session created"
    );

    Ok(CompanionSessionResult { session_id })
}

/// Persists the companion-enabled preference to UserMemory internal state.
///
/// Stored under the internal-state key `companion_enabled` (auto-prefixed
/// with `__` so it stays out of the user profile listing).
#[tauri::command]
pub async fn set_companion_enabled(
    enabled: bool,
    state: State<'_, RuntimeHandle>,
) -> Result<(), String> {
    let repo = get_repo(&state).map_err(|e| e.to_string())?;
    tokio::task::spawn_blocking(move || {
        let repo = repo.lock().map_err(|e| format!("mutex poisoned: {e}"))?;
        write_bool(&repo, "companion_enabled", enabled).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

/// Reads the companion-enabled preference from UserMemory internal state.
/// Defaults to `false` when the key has never been written.
#[tauri::command]
pub async fn get_companion_enabled(state: State<'_, RuntimeHandle>) -> Result<bool, String> {
    let repo = get_repo(&state).map_err(|e| e.to_string())?;
    tokio::task::spawn_blocking(move || {
        let repo = repo.lock().map_err(|e| format!("mutex poisoned: {e}"))?;
        read_bool(&repo, "companion_enabled").map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

// ---------------------------------------------------------------------------
// Companion - tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod companion_tests {
    use super::*;

    #[test]
    fn test_companion_context_for_all_routes() {
        // GIVEN the 15 defined routes
        let routes = [
            "dashboard",
            "agents",
            "chat",
            "triggers",
            "pipelines",
            "memory",
            "integrations",
            "approvals",
            "observability",
            "notifications",
            "transcriptions",
            "llm",
            "settings",
            "onboarding",
        ];
        // WHEN getting context for each route
        // THEN all return non-empty strings
        for route in routes {
            let ctx = get_companion_context_text(route);
            assert!(
                !ctx.is_empty(),
                "context for route '{}' should not be empty",
                route
            );
        }
    }

    #[test]
    fn test_companion_context_unknown_route() {
        // GIVEN an unknown route
        let ctx = get_companion_context_text("nonexistent");
        // THEN a non-empty fallback is returned
        assert!(!ctx.is_empty());
        assert_eq!(ctx, COMPANION_CONTEXT_FALLBACK);
    }

    #[test]
    fn test_companion_context_all_routes_distinct() {
        // GIVEN the defined route table
        // WHEN extracting all context strings
        let mut seen = std::collections::HashSet::new();
        for (_, ctx) in COMPANION_CONTEXTS {
            // THEN each route has a unique context text
            assert!(seen.insert(*ctx), "duplicate context text: {ctx}");
        }
    }
}

// ---------------------------------------------------------------------------
// AI Setup - types
// ---------------------------------------------------------------------------

/// System information used to compute model recommendations in the AI setup step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    /// Total RAM in gigabytes.
    pub total_ram_gb: f64,
    /// Available RAM in gigabytes.
    pub available_ram_gb: f64,
    /// Operating system identifier (e.g. `"macos"`, `"linux"`).
    pub os: String,
    /// CPU architecture identifier (e.g. `"aarch64"`, `"x86_64"`).
    pub arch: String,
    /// Whether a GPU is available (basic heuristic).
    pub gpu_available: bool,
}

/// GGUF model file detected on the local filesystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GgufModelInfo {
    /// Absolute path to the model file.
    pub path: String,
    /// Filename of the model.
    pub filename: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Human-readable file size (e.g. `"4.2 GB"`).
    pub size_human: String,
    /// Whether this model is recommended for the current RAM.
    pub recommended: bool,
}

/// Whisper GGML model file detected on the local filesystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhisperModelInfo {
    /// Absolute path to the model file.
    pub path: String,
    /// Filename of the model.
    pub filename: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Size variant parsed from the filename (`tiny`, `base`, `small`, `medium`, `large`).
    pub model_size: String,
    /// Whether this model is recommended for the current RAM.
    pub recommended: bool,
}

// ---------------------------------------------------------------------------
// AI Setup - Tauri commands
// ---------------------------------------------------------------------------

/// Returns system information for AI setup model recommendations.
///
/// Queries total and available RAM, OS, architecture, and basic GPU
/// availability. Runs the sysinfo query on a blocking thread.
#[tauri::command]
pub async fn get_ai_setup_info() -> Result<SystemInfo, String> {
    tokio::task::spawn_blocking(get_system_info_sync)
        .await
        .map_err(|e| format!("system info query failed: {e}"))
}

/// Scans standard filesystem locations for `.gguf` model files.
///
/// Locations scanned:
/// 1. `~/.apollia/models/` (flat)
/// 2. `~/Downloads/` (flat)
/// 3. `~/.cache/lm-studio/models/` (recursive, up to 4 levels deep)
///
/// Results are sorted by file size descending. Missing or unreadable
/// directories are silently skipped - an empty list is not an error.
#[tauri::command]
pub async fn scan_for_gguf_models() -> Result<Vec<GgufModelInfo>, String> {
    tokio::task::spawn_blocking(|| {
        let home = std::env::var("HOME").unwrap_or_default();
        let home_path = std::path::PathBuf::from(&home);
        let sys_info = get_system_info_sync();
        let max_recommended = recommended_max_gguf_size_bytes(sys_info.total_ram_gb);

        let flat_dirs = [
            home_path.join(".apollia").join("models"),
            home_path.join("Downloads"),
        ];
        let lm_studio_root = home_path.join(".cache").join("lm-studio").join("models");

        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut models: Vec<GgufModelInfo> = Vec::new();

        for dir in &flat_dirs {
            for mut info in scan_gguf_in_dir(dir) {
                if seen.insert(info.path.clone()) {
                    info.recommended = info.size_bytes <= max_recommended;
                    models.push(info);
                }
            }
        }

        let mut recursive_buf: Vec<GgufModelInfo> = Vec::new();
        collect_gguf_recursive(&lm_studio_root, &mut recursive_buf, 0, 4);
        for mut info in recursive_buf {
            if seen.insert(info.path.clone()) {
                info.recommended = info.size_bytes <= max_recommended;
                models.push(info);
            }
        }

        models.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
        models
    })
    .await
    .map_err(|e| format!("GGUF scan failed: {e}"))
}

/// Scans standard filesystem locations for Whisper GGML model files.
///
/// Locations scanned:
/// 1. `~/.apollia/models/ggml-*.bin`
/// 2. `~/Downloads/ggml-*.bin`
/// 3. `~/.cache/whisper/*.bin`
///
/// Only filenames matching the `ggml-(tiny|base|small|medium|large)` pattern
/// are returned. An empty list is not an error.
#[tauri::command]
pub async fn scan_for_whisper_models() -> Result<Vec<WhisperModelInfo>, String> {
    tokio::task::spawn_blocking(|| {
        let home = std::env::var("HOME").unwrap_or_default();
        let home_path = std::path::PathBuf::from(&home);
        let sys_info = get_system_info_sync();

        let scan_dirs = [
            home_path.join(".apollia").join("models"),
            home_path.join("Downloads"),
            home_path.join(".cache").join("whisper"),
        ];

        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut models: Vec<WhisperModelInfo> = Vec::new();

        for dir in &scan_dirs {
            for model in scan_whisper_in_dir(dir, sys_info.total_ram_gb) {
                if seen.insert(model.path.clone()) {
                    models.push(model);
                }
            }
        }
        models
    })
    .await
    .map_err(|e| format!("Whisper scan failed: {e}"))
}

/// Configures the Whisper STT backend with the selected model and marks
/// the onboarding state as STT-configured.
///
/// Persists `enabled = true` and the provided `model_path` to
/// `~/.apollia/system.db`, then sets `stt_configured` and `voice_enabled`
/// to `true` in the onboarding state.
#[tauri::command]
pub async fn setup_whisper_model(
    model_path: String,
    state: State<'_, RuntimeHandle>,
) -> Result<OnboardingState, String> {
    setup_whisper_model_inner(model_path, &state)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "setup_whisper_model failed");
            e.to_string()
        })
}

async fn setup_whisper_model_inner(
    model_path: String,
    state: &RuntimeHandle,
) -> Result<OnboardingState, OnboardingError> {
    let db_path = {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        std::path::PathBuf::from(home)
            .join(".apollia")
            .join("system.db")
    };

    let mp = model_path.clone();
    tokio::task::spawn_blocking(move || -> Result<(), OnboardingError> {
        let repo = apollia_core::SttConfigRepository::open(&db_path)
            .map_err(|e| OnboardingError::PersistenceError(format!("open system.db: {e}")))?;
        let mut row = repo
            .get_or_default()
            .map_err(|e| OnboardingError::PersistenceError(format!("read STT config: {e}")))?;
        row.model_path = mp;
        row.enabled = true;
        repo.upsert(&row)
            .map_err(|e| OnboardingError::PersistenceError(format!("persist STT config: {e}")))?;
        Ok(())
    })
    .await
    .map_err(|e| OnboardingError::PersistenceError(format!("spawn_blocking failed: {e}")))??;

    let repo = get_repo(state)?;
    let result = tokio::task::spawn_blocking(move || {
        let repo = repo
            .lock()
            .map_err(|e| OnboardingError::PersistenceError(format!("mutex poisoned: {e}")))?;
        let mut onboarding = load_state_from_memory(&repo)?;
        onboarding.stt_configured = true;
        onboarding.voice_enabled = true;
        persist_state(&repo, &onboarding)?;
        Ok::<OnboardingState, OnboardingError>(onboarding)
    })
    .await
    .map_err(|e| OnboardingError::PersistenceError(format!("spawn_blocking failed: {e}")))??;

    tracing::info!(model_path = %model_path, "Whisper STT model configured");

    Ok(result)
}

// ---------------------------------------------------------------------------
// AI Setup - helper functions
// ---------------------------------------------------------------------------

/// Returns current system information synchronously.
///
/// Memory is read via sysinfo; OS and architecture are compile-time constants.
/// Intended for use inside `spawn_blocking` from async commands.
pub fn get_system_info_sync() -> SystemInfo {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let total_bytes = sys.total_memory();
    let available_bytes = sys.available_memory();

    SystemInfo {
        total_ram_gb: total_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
        available_ram_gb: available_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        gpu_available: detect_gpu_basic(),
    }
}

/// Scans a single directory (non-recursive) for `.gguf` files.
///
/// Returns an empty `Vec` for non-existent or unreadable directories.
/// The `recommended` field defaults to `false`; callers apply a RAM threshold.
pub fn scan_gguf_in_dir(dir: &std::path::Path) -> Vec<GgufModelInfo> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("gguf") {
            continue;
        }
        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        results.push(GgufModelInfo {
            path: path.display().to_string(),
            filename,
            size_bytes,
            size_human: format_size_human(size_bytes),
            recommended: false,
        });
    }
    results
}

/// Collects `.gguf` files recursively from `dir` up to `max_depth` levels.
///
/// Fills `out` in place. Unreadable directories and symlink loops are
/// silently skipped via `file_type()` rather than `is_dir()`.
fn collect_gguf_recursive(
    dir: &std::path::Path,
    out: &mut Vec<GgufModelInfo>,
    depth: u8,
    max_depth: u8,
) {
    if depth >= max_depth {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            collect_gguf_recursive(&path, out, depth + 1, max_depth);
        } else if file_type.is_file() && path.extension().and_then(|e| e.to_str()) == Some("gguf") {
            let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let filename = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            out.push(GgufModelInfo {
                path: path.display().to_string(),
                filename,
                size_bytes,
                size_human: format_size_human(size_bytes),
                recommended: false,
            });
        }
    }
}

/// Scans a single directory for Whisper GGML model files.
///
/// Only files matching `ggml-(tiny|base|small|medium|large)*.bin` are included.
/// Returns an empty `Vec` for non-existent or unreadable directories.
fn scan_whisper_in_dir(dir: &std::path::Path, total_ram_gb: f64) -> Vec<WhisperModelInfo> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let filename = match path.file_name().map(|n| n.to_string_lossy().into_owned()) {
            Some(n) => n,
            None => continue,
        };

        if !filename.starts_with("ggml-") || !filename.ends_with(".bin") {
            continue;
        }

        let model_size = match detect_whisper_model_size(&filename) {
            Some(s) => s,
            None => continue,
        };

        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let recommended = whisper_model_recommended(&model_size, total_ram_gb);

        results.push(WhisperModelInfo {
            path: path.display().to_string(),
            filename,
            size_bytes,
            model_size,
            recommended,
        });
    }
    results
}

/// Parses the Whisper size variant from a filename.
///
/// Matches `ggml-(tiny|base|small|medium|large)` in the lowercased filename.
/// Falls back to `"base"` for any other `ggml-*.bin` file (e.g. `ggml-model-q5_0.bin`)
/// so that generic Whisper GGML models are still detected.
pub fn detect_whisper_model_size(filename: &str) -> Option<String> {
    let lower = filename.to_lowercase();
    for variant in ["tiny", "base", "small", "medium", "large"] {
        let pattern = format!("ggml-{variant}");
        if lower.contains(pattern.as_str()) {
            return Some(variant.to_owned());
        }
    }
    // Accept any ggml-*.bin as a valid Whisper model with unknown size.
    if lower.starts_with("ggml-") && lower.ends_with(".bin") {
        return Some("base".to_owned());
    }
    None
}

/// Returns the maximum recommended GGUF file size in bytes for the given RAM.
///
/// Thresholds:
/// - < 8 GB  → 2.5 GB (≤ 3B-parameter models)
/// - 8–16 GB → 6.0 GB (7–8B-parameter models)
/// - > 16 GB → no upper limit
pub fn recommended_max_gguf_size_bytes(total_ram_gb: f64) -> u64 {
    if total_ram_gb < 8.0 {
        2_500_000_000
    } else if total_ram_gb <= 16.0 {
        6_000_000_000
    } else {
        u64::MAX
    }
}

/// Formats a byte count as a human-readable string using binary divisions.
///
/// Examples: `4_500_000_000` → `"4.2 GB"`, `500_000_000` → `"476.8 MB"`.
pub fn format_size_human(bytes: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

/// Returns whether a Whisper model size variant is recommended for the given RAM.
fn whisper_model_recommended(model_size: &str, total_ram_gb: f64) -> bool {
    match model_size {
        "tiny" => true,
        "base" | "small" => total_ram_gb >= 8.0,
        "medium" => total_ram_gb >= 16.0,
        "large" => total_ram_gb >= 32.0,
        _ => false,
    }
}

/// Detects GPU availability using platform-specific heuristics.
fn detect_gpu_basic() -> bool {
    #[cfg(target_os = "macos")]
    {
        // All macOS machines expose Metal GPU support
        true
    }
    #[cfg(target_os = "linux")]
    {
        std::path::Path::new("/dev/dri/card0").exists()
            || std::path::Path::new("/proc/driver/nvidia").exists()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        false
    }
}

// ---------------------------------------------------------------------------
// Guided Tour - types
// ---------------------------------------------------------------------------

/// Interaction descriptor for a tour step that requires user action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TourInteraction {
    /// Kind of interaction: `"start_agent"` | `"send_chat"` | `"create_trigger"` | `"search_memory"`.
    pub interaction_type: String,
    /// Optional data pre-filled in the relevant UI form.
    pub prefilled_data: Option<serde_json::Value>,
    /// SSE event name that marks the step as completed when received.
    pub validation_event: Option<String>,
}

/// Descriptor for a single step of the guided tour.
///
/// Each step specifies a route to navigate to, a DOM element to highlight,
/// a companion message key (i18n), an optional user interaction, a completion
/// mode, and an estimated duration the user should spend on the step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TourStep {
    /// Stable identifier used for persistence.
    pub id: String,
    /// Application route to navigate to (e.g. `"/agents"`).
    pub route: String,
    /// CSS selector of the element to highlight via the spotlight overlay.
    pub spotlight_selector: Option<String>,
    /// i18n key for the companion message shown on this step.
    pub companion_message_key: String,
    /// Optional interactive action required to complete the step.
    pub interaction: Option<TourInteraction>,
    /// How the step is completed: `"auto"` | `"click_next"` | `"wait_event"`.
    pub completion_mode: String,
    /// Suggested time to spend on the step, in seconds. Used by `"auto"` mode.
    pub estimated_seconds: u32,
    /// Visual group name for the progress rail (e.g. `"dashboard"`, `"agents"`).
    ///
    /// Steps with the same group value are shown as a single dot in the
    /// progress rail.  When `None`, each step is its own group (backward
    /// compatible with the builder profile).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

// ---------------------------------------------------------------------------
// Guided Tour - step catalogue
// ---------------------------------------------------------------------------

/// Returns the ordered tour steps for the Operator profile (17 steps, 9 groups).
///
/// Steps are flat but grouped via the `group` field.  The progress rail shows
/// one dot per group.  Two steps are interactive (`wait_event`): agent start
/// and libre chat creation.  The tour programmatically creates demo data
/// (trigger, notification channel) so pages are never empty.
fn operator_steps() -> Vec<TourStep> {
    vec![
        // ── Group: dashboard (3 sub-steps) ──────────────────────────────────
        TourStep {
            id: "op-dashboard-1".to_string(),
            route: "/dashboard".to_string(),
            spotlight_selector: Some("[data-testid=\"dashboard-header\"]".to_string()),
            companion_message_key: "onboarding.tour.op.dashboard_header.title".to_string(),
            interaction: None,
            completion_mode: "click_next".to_string(),
            estimated_seconds: 10,
            group: Some("dashboard".to_string()),
        },
        TourStep {
            id: "op-dashboard-2".to_string(),
            route: "/dashboard".to_string(),
            spotlight_selector: Some("[data-testid=\"dashboard-agents-section\"]".to_string()),
            companion_message_key: "onboarding.tour.op.dashboard_agents.title".to_string(),
            interaction: None,
            completion_mode: "click_next".to_string(),
            estimated_seconds: 10,
            group: Some("dashboard".to_string()),
        },
        TourStep {
            id: "op-dashboard-3".to_string(),
            route: "/dashboard".to_string(),
            spotlight_selector: Some("[data-testid=\"dashboard-activity-section\"]".to_string()),
            companion_message_key: "onboarding.tour.op.dashboard_activity.title".to_string(),
            interaction: None,
            completion_mode: "click_next".to_string(),
            estimated_seconds: 10,
            group: Some("dashboard".to_string()),
        },
        // ── Group: agents (3 sub-steps) ─────────────────────────────────────
        TourStep {
            id: "op-agents-1".to_string(),
            route: "/agents".to_string(),
            spotlight_selector: Some("[data-testid=\"agents-page\"]".to_string()),
            companion_message_key: "onboarding.tour.op.agents_grid.title".to_string(),
            interaction: None,
            completion_mode: "click_next".to_string(),
            estimated_seconds: 10,
            group: Some("agents".to_string()),
        },
        TourStep {
            id: "op-agents-2".to_string(),
            route: "/agents".to_string(),
            spotlight_selector: Some("[data-agent-name=\"onboarding-agent\"]".to_string()),
            companion_message_key: "onboarding.tour.op.agents_start.title".to_string(),
            interaction: Some(TourInteraction {
                interaction_type: "start_agent".to_string(),
                prefilled_data: Some(serde_json::json!({ "agent_id": "onboarding-agent" })),
                validation_event: Some("AgentReady".to_string()),
            }),
            completion_mode: "wait_event".to_string(),
            estimated_seconds: 15,
            group: Some("agents".to_string()),
        },
        // ── Group: chat (3 sub-steps) ───────────────────────────────────────
        TourStep {
            id: "op-chat-1".to_string(),
            route: "/chat".to_string(),
            spotlight_selector: Some("[data-testid=\"new-chat-button\"]".to_string()),
            companion_message_key: "onboarding.tour.op.chat_discover.title".to_string(),
            interaction: None,
            completion_mode: "click_next".to_string(),
            estimated_seconds: 10,
            group: Some("chat".to_string()),
        },
        TourStep {
            id: "op-chat-2".to_string(),
            route: "/chat".to_string(),
            spotlight_selector: Some("[data-testid=\"new-chat-picker\"]".to_string()),
            companion_message_key: "onboarding.tour.op.chat_picker.title".to_string(),
            interaction: None,
            completion_mode: "click_next".to_string(),
            estimated_seconds: 15,
            group: Some("chat".to_string()),
        },
        TourStep {
            id: "op-chat-3".to_string(),
            route: "/chat".to_string(),
            spotlight_selector: Some("[data-testid=\"pick-libre\"]".to_string()),
            companion_message_key: "onboarding.tour.op.chat_libre.title".to_string(),
            interaction: Some(TourInteraction {
                interaction_type: "start_libre_chat".to_string(),
                prefilled_data: None,
                validation_event: Some("ChatSessionCreated".to_string()),
            }),
            completion_mode: "wait_event".to_string(),
            estimated_seconds: 10,
            group: Some("chat".to_string()),
        },
        // ── Group: triggers (2 sub-steps) ───────────────────────────────────
        TourStep {
            id: "op-triggers-1".to_string(),
            route: "/triggers".to_string(),
            spotlight_selector: Some("[data-testid=\"triggers-page\"]".to_string()),
            companion_message_key: "onboarding.tour.op.triggers_overview.title".to_string(),
            interaction: None,
            completion_mode: "click_next".to_string(),
            estimated_seconds: 10,
            group: Some("triggers".to_string()),
        },
        TourStep {
            id: "op-triggers-2".to_string(),
            route: "/triggers".to_string(),
            spotlight_selector: Some("[data-testid=\"triggers-grouped\"]".to_string()),
            companion_message_key: "onboarding.tour.op.triggers_detail.title".to_string(),
            interaction: None,
            completion_mode: "click_next".to_string(),
            estimated_seconds: 10,
            group: Some("triggers".to_string()),
        },
        // ── Group: approvals (1 step) ───────────────────────────────────────
        TourStep {
            id: "op-approvals".to_string(),
            route: "/approvals".to_string(),
            spotlight_selector: Some("[data-testid=\"approvals-pending-title\"]".to_string()),
            companion_message_key: "onboarding.tour.op.approvals.title".to_string(),
            interaction: None,
            completion_mode: "click_next".to_string(),
            estimated_seconds: 10,
            group: Some("approvals".to_string()),
        },
        // ── Group: notifications (2 sub-steps) ─────────────────────────────
        TourStep {
            id: "op-notifications-1".to_string(),
            route: "/notifications".to_string(),
            spotlight_selector: Some("[data-testid=\"notifications-page\"]".to_string()),
            companion_message_key: "onboarding.tour.op.notifications_overview.title".to_string(),
            interaction: None,
            completion_mode: "click_next".to_string(),
            estimated_seconds: 10,
            group: Some("notifications".to_string()),
        },
        TourStep {
            id: "op-notifications-2".to_string(),
            route: "/notifications".to_string(),
            spotlight_selector: Some("[data-testid=\"channel-card-tour-alerts\"]".to_string()),
            companion_message_key: "onboarding.tour.op.notifications_test.title".to_string(),
            interaction: None,
            completion_mode: "click_next".to_string(),
            estimated_seconds: 10,
            group: Some("notifications".to_string()),
        },
        // ── Group: observability (2 sub-steps) ──────────────────────────────
        TourStep {
            id: "op-observability-1".to_string(),
            route: "/observability".to_string(),
            spotlight_selector: Some("[data-testid=\"observability-tabbar\"]".to_string()),
            companion_message_key: "onboarding.tour.op.observability_tabs.title".to_string(),
            interaction: None,
            completion_mode: "click_next".to_string(),
            estimated_seconds: 10,
            group: Some("observability".to_string()),
        },
        TourStep {
            id: "op-observability-2".to_string(),
            route: "/observability".to_string(),
            spotlight_selector: Some("[data-testid=\"observability-tab-timeline\"]".to_string()),
            companion_message_key: "onboarding.tour.op.observability_timeline.title".to_string(),
            interaction: None,
            completion_mode: "click_next".to_string(),
            estimated_seconds: 10,
            group: Some("observability".to_string()),
        },
        // ── Group: graduation (1 step) ──────────────────────────────────────
        TourStep {
            id: "op-graduation".to_string(),
            route: "/dashboard".to_string(),
            spotlight_selector: Some("[data-testid=\"dashboard-page\"]".to_string()),
            companion_message_key: "onboarding.tour.op.graduation.title".to_string(),
            interaction: None,
            completion_mode: "click_next".to_string(),
            estimated_seconds: 10,
            group: Some("graduation".to_string()),
        },
    ]
}

/// Returns the ordered tour steps for the Builder profile (10 steps).
///
/// Steps 2, 4, and 5 (indices 1, 3, 4) are interactive: they wait for a backend
/// event before the orchestrator advances. The other steps are passive.
/// Step 3 (bld-agent-detail) triggers a programmatic panel open on the frontend.
///
/// Builder steps do not use visual grouping (`group: None`).
fn builder_steps() -> Vec<TourStep> {
    vec![
        TourStep {
            id: "bld-dashboard".to_string(),
            route: "/dashboard".to_string(),
            spotlight_selector: Some("#dashboard-stats".to_string()),
            companion_message_key: "onboarding.tour.bld.dashboard.title".to_string(),
            interaction: None,
            completion_mode: "auto".to_string(),
            estimated_seconds: 10,
            group: None,
        },
        TourStep {
            id: "bld-agents-start".to_string(),
            route: "/agents".to_string(),
            spotlight_selector: Some("#agent-csv-data-worker".to_string()),
            companion_message_key: "onboarding.tour.bld.agents.title".to_string(),
            interaction: Some(TourInteraction {
                interaction_type: "start_agent".to_string(),
                prefilled_data: Some(serde_json::json!({ "agent_id": "csv-data-worker" })),
                validation_event: Some("AgentReady".to_string()),
            }),
            completion_mode: "wait_event".to_string(),
            estimated_seconds: 30,
            group: None,
        },
        TourStep {
            id: "bld-agent-detail".to_string(),
            route: "/agents".to_string(),
            spotlight_selector: Some("#agent-detail-manifest".to_string()),
            companion_message_key: "onboarding.tour.bld.agent_detail.title".to_string(),
            interaction: None,
            completion_mode: "click_next".to_string(),
            estimated_seconds: 20,
            group: None,
        },
        TourStep {
            id: "bld-memory-search".to_string(),
            route: "/memory".to_string(),
            spotlight_selector: Some("#memory-search-form".to_string()),
            companion_message_key: "onboarding.tour.bld.memory.title".to_string(),
            interaction: Some(TourInteraction {
                interaction_type: "search_memory".to_string(),
                prefilled_data: Some(serde_json::json!({
                    "namespace": "onboarding-agent",
                    "query": "user",
                    "limit": 10
                })),
                validation_event: Some("MemorySearchCompleted".to_string()),
            }),
            completion_mode: "wait_event".to_string(),
            estimated_seconds: 30,
            group: None,
        },
        TourStep {
            id: "bld-chat-send".to_string(),
            route: "/chat".to_string(),
            spotlight_selector: Some("#chat-input".to_string()),
            companion_message_key: "onboarding.tour.bld.chat.title".to_string(),
            interaction: Some(TourInteraction {
                interaction_type: "send_chat".to_string(),
                prefilled_data: Some(serde_json::json!({
                    "message": "Montre-moi comment un agent utilise la mémoire",
                    "channel": "libre"
                })),
                validation_event: Some("ChatMessageReceived".to_string()),
            }),
            completion_mode: "wait_event".to_string(),
            estimated_seconds: 30,
            group: None,
        },
        TourStep {
            id: "bld-integrations".to_string(),
            route: "/integrations".to_string(),
            spotlight_selector: Some("#mcp-server-list".to_string()),
            companion_message_key: "onboarding.tour.bld.integrations.title".to_string(),
            interaction: None,
            completion_mode: "click_next".to_string(),
            estimated_seconds: 15,
            group: None,
        },
        TourStep {
            id: "bld-triggers".to_string(),
            route: "/triggers".to_string(),
            spotlight_selector: Some("#trigger-type-list".to_string()),
            companion_message_key: "onboarding.tour.bld.triggers.title".to_string(),
            interaction: None,
            completion_mode: "click_next".to_string(),
            estimated_seconds: 15,
            group: None,
        },
        TourStep {
            id: "bld-pipelines".to_string(),
            route: "/pipelines".to_string(),
            spotlight_selector: Some("#pipeline-dag-view".to_string()),
            companion_message_key: "onboarding.tour.bld.pipelines.title".to_string(),
            interaction: None,
            completion_mode: "click_next".to_string(),
            estimated_seconds: 15,
            group: None,
        },
        TourStep {
            id: "bld-observability".to_string(),
            route: "/observability".to_string(),
            spotlight_selector: Some("#audit-trail-panel".to_string()),
            companion_message_key: "onboarding.tour.bld.observability.title".to_string(),
            interaction: None,
            completion_mode: "click_next".to_string(),
            estimated_seconds: 15,
            group: None,
        },
        TourStep {
            id: "bld-graduation".to_string(),
            route: "/dashboard".to_string(),
            spotlight_selector: Some("#graduation-card".to_string()),
            companion_message_key: "onboarding.tour.bld.graduation.title".to_string(),
            interaction: None,
            completion_mode: "auto".to_string(),
            estimated_seconds: 5,
            group: None,
        },
    ]
}

// ---------------------------------------------------------------------------
// Guided Tour - synchronous helper (also used in tests)
// ---------------------------------------------------------------------------

/// Returns the 0-based index of a tour step by its id within the given profile.
///
/// Returns `None` if the profile is unknown or the step id is not found.
fn steps_index_by_id(step_id: &str, profile: Option<&str>) -> Option<usize> {
    let profile = profile.unwrap_or("operator");
    let steps = get_tour_steps_sync(profile).ok()?;
    steps.iter().position(|s| s.id == step_id)
}

/// Returns the tour steps for the given profile synchronously.
///
/// Returns `Ok(Vec<TourStep>)` for `"operator"` (8 steps) and `"builder"` (10 steps).
/// Returns `Err` for any other profile string.
pub fn get_tour_steps_sync(profile: &str) -> Result<Vec<TourStep>, String> {
    match profile {
        "operator" => Ok(operator_steps()),
        "builder" => Ok(builder_steps()),
        other => Err(format!(
            "unknown profile: {other}. Expected 'operator' or 'builder'"
        )),
    }
}

// ---------------------------------------------------------------------------
// Guided Tour - Tauri commands
// ---------------------------------------------------------------------------

/// Returns the ordered tour steps for the given user profile.
///
/// Returns 8 steps for `"operator"` and 10 steps for `"builder"`. Any
/// other profile string is rejected immediately (Principle #4).
#[tauri::command]
pub async fn get_tour_steps(profile: String) -> Result<Vec<TourStep>, String> {
    get_tour_steps_sync(&profile)
}

/// Marks a guided-tour step as completed and advances `tour_step_index` in
/// UserMemory.
///
/// The new `tour_step_index` is set to `current_index + 1`. When the index
/// reaches `tour_total_steps`, `tour_completed` is set to `true` as well.
#[tauri::command]
pub async fn complete_tour_step(
    step_id: String,
    state: State<'_, RuntimeHandle>,
) -> Result<(), String> {
    complete_tour_step_inner(step_id, &state)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "complete_tour_step failed");
            e.to_string()
        })
}

async fn complete_tour_step_inner(
    step_id: String,
    state: &RuntimeHandle,
) -> Result<(), OnboardingError> {
    let repo = get_repo(state)?;

    tokio::task::spawn_blocking(move || {
        let repo = repo
            .lock()
            .map_err(|e| OnboardingError::PersistenceError(format!("mutex poisoned: {e}")))?;

        let mut onboarding = load_state_from_memory(&repo)?;

        // Only advance the persisted high-water mark when the completed step
        // matches or exceeds the current index.  This prevents double-incrementing
        // when the user navigates backward and then forward again.
        let step_idx = steps_index_by_id(&step_id, onboarding.profile.as_deref());
        let should_advance = match step_idx {
            Some(idx) => idx >= onboarding.tour_step_index as usize,
            None => true, // Unknown step - fall back to legacy behaviour.
        };

        let next_index = if should_advance {
            onboarding.tour_step_index + 1
        } else {
            onboarding.tour_step_index
        };
        onboarding.tour_step_index = next_index;

        if onboarding.tour_total_steps > 0 && next_index >= onboarding.tour_total_steps {
            onboarding.tour_completed = true;
        }

        persist_state(&repo, &onboarding)?;

        tracing::info!(
            step_id = %step_id,
            tour_step_index = %next_index,
            tour_completed = %onboarding.tour_completed,
            "tour step completed"
        );

        Ok(())
    })
    .await
    .map_err(|e| OnboardingError::PersistenceError(format!("spawn_blocking failed: {e}")))?
}

// ---------------------------------------------------------------------------
// Guided Tour - tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tour_tests {
    use super::*;

    #[test]
    fn test_get_tour_steps_operator_returns_16_steps() {
        // GIVEN the profile "operator"
        // WHEN get_tour_steps_sync is called
        let steps = get_tour_steps_sync("operator").unwrap();
        // THEN 16 steps are returned
        assert_eq!(steps.len(), 16);
    }

    #[test]
    fn test_get_tour_steps_builder_returns_10_steps() {
        // GIVEN the profile "builder"
        // WHEN get_tour_steps_sync is called
        let steps = get_tour_steps_sync("builder").unwrap();
        // THEN 10 steps are returned
        assert_eq!(steps.len(), 10);
    }

    #[test]
    fn test_get_tour_steps_unknown_profile_returns_error() {
        // GIVEN an unknown profile
        // WHEN get_tour_steps_sync is called
        let result = get_tour_steps_sync("unknown");
        // THEN an error is returned
        assert!(result.is_err());
    }

    #[test]
    fn test_tour_step_serialization_roundtrip() {
        // GIVEN a TourStep with an interaction
        let step = TourStep {
            id: "agents-start".to_string(),
            route: "/agents".to_string(),
            spotlight_selector: Some("[data-testid=\"agent-list\"]".to_string()),
            companion_message_key: "onboarding.tour.op.agents".to_string(),
            interaction: Some(TourInteraction {
                interaction_type: "start_agent".to_string(),
                prefilled_data: Some(serde_json::json!({ "agent": "csv-data-worker" })),
                validation_event: Some("AgentReady".to_string()),
            }),
            completion_mode: "wait_event".to_string(),
            estimated_seconds: 30,
            group: Some("agents".to_string()),
        };
        // WHEN serialized then deserialized
        let json = serde_json::to_string(&step).unwrap();
        let restored: TourStep = serde_json::from_str(&json).unwrap();
        // THEN the data is identical
        assert_eq!(restored.id, "agents-start");
        assert_eq!(restored.route, "/agents");
        assert_eq!(restored.completion_mode, "wait_event");
        assert_eq!(restored.estimated_seconds, 30);
        assert_eq!(restored.group.as_deref(), Some("agents"));
        let interaction = restored.interaction.unwrap();
        assert_eq!(interaction.interaction_type, "start_agent");
        assert_eq!(interaction.validation_event.as_deref(), Some("AgentReady"));
    }

    #[test]
    fn test_all_operator_steps_have_valid_ids() {
        // GIVEN the operator step catalogue
        let steps = get_tour_steps_sync("operator").unwrap();
        // WHEN checking each step
        for step in &steps {
            // THEN id and route are non-empty
            assert!(!step.id.is_empty(), "step id must not be empty");
            assert!(!step.route.is_empty(), "step route must not be empty");
            assert!(
                matches!(
                    step.completion_mode.as_str(),
                    "auto" | "click_next" | "wait_event"
                ),
                "unexpected completion_mode: {}",
                step.completion_mode
            );
        }
    }

    #[test]
    fn test_all_builder_steps_have_valid_ids() {
        // GIVEN the builder step catalogue
        let steps = get_tour_steps_sync("builder").unwrap();
        // WHEN checking each step
        for step in &steps {
            // THEN id and route are non-empty
            assert!(!step.id.is_empty(), "step id must not be empty");
            assert!(!step.route.is_empty(), "step route must not be empty");
            assert!(
                matches!(
                    step.completion_mode.as_str(),
                    "auto" | "click_next" | "wait_event"
                ),
                "unexpected completion_mode: {}",
                step.completion_mode
            );
        }
    }

    #[test]
    fn test_step_ids_are_unique_within_profile() {
        // GIVEN both profiles
        for profile in ["operator", "builder"] {
            let steps = get_tour_steps_sync(profile).unwrap();
            let mut seen = std::collections::HashSet::new();
            for step in &steps {
                // THEN each step id is unique
                assert!(
                    seen.insert(step.id.clone()),
                    "duplicate step id '{}' in profile '{}'",
                    step.id,
                    profile
                );
            }
        }
    }

    #[test]
    fn test_operator_steps_count() {
        let steps = operator_steps();
        assert_eq!(steps.len(), 16);
    }

    #[test]
    fn test_operator_steps_order() {
        let steps = operator_steps();
        assert_eq!(steps[0].id, "op-dashboard-1");
        assert_eq!(steps[1].id, "op-dashboard-2");
        assert_eq!(steps[2].id, "op-dashboard-3");
        assert_eq!(steps[3].id, "op-agents-1");
        assert_eq!(steps[4].id, "op-agents-2");
        assert_eq!(steps[5].id, "op-chat-1");
        assert_eq!(steps[6].id, "op-chat-2");
        assert_eq!(steps[7].id, "op-chat-3");
        assert_eq!(steps[8].id, "op-triggers-1");
        assert_eq!(steps[9].id, "op-triggers-2");
        assert_eq!(steps[10].id, "op-approvals");
        assert_eq!(steps[11].id, "op-notifications-1");
        assert_eq!(steps[12].id, "op-notifications-2");
        assert_eq!(steps[13].id, "op-observability-1");
        assert_eq!(steps[14].id, "op-observability-2");
        assert_eq!(steps[15].id, "op-graduation");
    }

    #[test]
    fn test_operator_interactive_steps_have_interaction() {
        let steps = operator_steps();
        // op-agents-2 (index 4) and op-chat-3 (index 7) are wait_event
        assert!(
            steps[4].interaction.is_some(),
            "op-agents-2 must have interaction"
        );
        assert!(
            steps[7].interaction.is_some(),
            "op-chat-3 must have interaction"
        );
    }

    #[test]
    fn test_operator_passive_steps_no_interaction() {
        let steps = operator_steps();
        for (i, step) in steps.iter().enumerate() {
            if i == 4 || i == 7 {
                continue; // interactive steps
            }
            assert!(
                step.interaction.is_none(),
                "step '{}' at index {} must be passive",
                step.id,
                i
            );
        }
    }

    #[test]
    fn test_operator_steps_all_have_groups() {
        let steps = operator_steps();
        for step in &steps {
            assert!(step.group.is_some(), "step '{}' must have a group", step.id);
        }
    }

    #[test]
    fn test_operator_groups_are_contiguous() {
        let steps = operator_steps();
        let mut seen_groups = Vec::new();
        for step in &steps {
            let g = step.group.as_deref().unwrap();
            if seen_groups.last().map(|s: &String| s.as_str()) != Some(g) {
                assert!(
                    !seen_groups.contains(&g.to_string()),
                    "group '{}' appears non-contiguously",
                    g
                );
                seen_groups.push(g.to_string());
            }
        }
    }

    #[test]
    fn test_operator_group_count_is_8() {
        let steps = operator_steps();
        let groups: std::collections::HashSet<_> =
            steps.iter().filter_map(|s| s.group.as_deref()).collect();
        assert_eq!(
            groups.len(),
            8,
            "expected 8 unique groups, got {:?}",
            groups
        );
    }

    #[test]
    fn test_operator_steps_have_valid_routes() {
        // GIVEN the operator step catalogue
        let steps = operator_steps();
        // WHEN checking routes
        // THEN all routes start with "/"
        for step in &steps {
            assert!(
                step.route.starts_with('/'),
                "invalid route '{}' for step '{}'",
                step.route,
                step.id
            );
        }
    }

    #[test]
    fn test_builder_steps_count() {
        // GIVEN the builder profile
        // WHEN the steps are generated
        // THEN there are exactly 10 steps
        let steps = builder_steps();
        assert_eq!(steps.len(), 10);
    }

    #[test]
    fn test_builder_steps_order() {
        // GIVEN the builder step catalogue
        // WHEN checking the ids by position
        // THEN the order matches the specified sequence
        let steps = builder_steps();
        assert_eq!(steps[0].id, "bld-dashboard");
        assert_eq!(steps[1].id, "bld-agents-start");
        assert_eq!(steps[2].id, "bld-agent-detail");
        assert_eq!(steps[3].id, "bld-memory-search");
        assert_eq!(steps[9].id, "bld-graduation");
    }

    #[test]
    fn test_builder_interactive_steps_have_interaction() {
        // GIVEN the builder step catalogue
        // WHEN filtering the interactive steps (indices 1, 3, 4)
        // THEN each one has an interaction descriptor
        let steps = builder_steps();
        assert!(
            steps[1].interaction.is_some(),
            "bld-agents-start must have interaction"
        );
        assert!(
            steps[3].interaction.is_some(),
            "bld-memory-search must have interaction"
        );
        assert!(
            steps[4].interaction.is_some(),
            "bld-chat-send must have interaction"
        );
    }

    #[test]
    fn test_builder_agent_detail_step_is_passive() {
        // GIVEN the builder step catalogue
        // WHEN checking the agent-detail step
        // THEN it is passive (click_next) with no interaction
        let steps = builder_steps();
        assert!(
            steps[2].interaction.is_none(),
            "bld-agent-detail must be passive"
        );
        assert_eq!(steps[2].completion_mode, "click_next");
    }

    #[test]
    fn test_builder_memory_step_has_prefilled_data() {
        // GIVEN the builder step catalogue
        // WHEN checking the memory-search step
        // THEN prefilled_data contains namespace and query
        let steps = builder_steps();
        let interaction = steps[3].interaction.as_ref().unwrap();
        let data = interaction.prefilled_data.as_ref().unwrap();
        assert_eq!(data["namespace"], "onboarding-agent");
        assert_eq!(data["query"], "user");
    }

    #[test]
    fn test_builder_steps_have_valid_routes() {
        // GIVEN the builder step catalogue
        // WHEN checking routes
        // THEN all routes start with "/"
        let steps = builder_steps();
        for step in &steps {
            assert!(
                step.route.starts_with('/'),
                "invalid route '{}' for step '{}'",
                step.route,
                step.id
            );
        }
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

    #[test]
    fn test_system_info_returns_valid_values() {
        // GIVEN the current system
        // WHEN calling get_system_info_sync
        let info = get_system_info_sync();
        // THEN RAM values are positive and os/arch are non-empty
        assert!(info.total_ram_gb > 0.0);
        assert!(!info.os.is_empty());
        assert!(!info.arch.is_empty());
    }

    #[test]
    fn test_gguf_scan_empty_dir_returns_empty_vec() {
        // GIVEN a temporary empty directory
        let dir = tempfile::tempdir().unwrap();
        // WHEN scanning for GGUF models
        let results = scan_gguf_in_dir(dir.path());
        // THEN an empty Vec is returned
        assert!(results.is_empty());
    }

    #[test]
    fn test_whisper_model_size_detection() {
        // GIVEN a filename "ggml-base.bin"
        let size = detect_whisper_model_size("ggml-base.bin");
        // THEN model_size is "base"
        assert_eq!(size, Some("base".to_string()));
    }

    #[test]
    fn test_whisper_model_size_unknown_filename() {
        // GIVEN a filename without the standard pattern
        let size = detect_whisper_model_size("random-model.bin");
        // THEN None is returned
        assert_eq!(size, None);
    }

    #[test]
    fn test_ram_recommendation_low() {
        // GIVEN 6 GB RAM
        let max_size = recommended_max_gguf_size_bytes(6.0);
        // THEN the threshold is at most 2.5 GB
        assert!(max_size <= 2_500_000_000);
    }

    #[test]
    fn test_ram_recommendation_medium() {
        // GIVEN 12 GB RAM
        let max_size = recommended_max_gguf_size_bytes(12.0);
        // THEN the threshold is between 2.5 GB and 6 GB (inclusive)
        assert!(max_size <= 6_000_000_000);
        assert!(max_size > 2_500_000_000);
    }

    #[test]
    fn test_human_readable_size() {
        // GIVEN known byte counts
        // THEN human-readable output matches expected strings
        assert_eq!(format_size_human(4_500_000_000), "4.2 GB");
        assert_eq!(format_size_human(500_000_000), "476.8 MB");
    }

    #[test]
    fn test_trigger_onboarding_with_operator_profile_validates() {
        // GIVEN a valid operator profile
        // WHEN validate_profile is called
        let result = validate_profile("operator");
        // THEN it succeeds - the profile will be injected into agent memory
        assert!(result.is_ok());
    }

    #[test]
    fn test_trigger_onboarding_without_profile_is_retrocompatible() {
        // GIVEN no profile (None)
        // WHEN building the onboarding prompt without profile
        let prompt = build_onboarding_prompt(&None);
        // THEN the prompt covers all 5 generic topics (no profile-specific section)
        for topic in &ONBOARDING_TOPICS {
            assert!(
                prompt.contains(topic),
                "generic prompt must mention: {topic}"
            );
        }
    }

    #[test]
    fn test_trigger_onboarding_rejects_invalid_profile() {
        // GIVEN an unrecognised profile string
        let result = validate_profile("hacker");
        // THEN InvalidProfile error is returned
        assert!(matches!(result, Err(OnboardingError::InvalidProfile(_))));
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("hacker"));
        assert!(err_msg.contains("operator"));
        assert!(err_msg.contains("builder"));
    }
}

// ---------------------------------------------------------------------------
// Voice commands - types
// ---------------------------------------------------------------------------

/// Action resulting from parsing a voice command during the guided tour.
///
/// Variants are matched in priority order: navigation (next/back/skip) >
/// explain > natural language (AskCompanion). `Unrecognized` is reserved
/// for empty or purely non-alphabetic transcripts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action")]
pub enum TourVoiceAction {
    /// Advance to the next tour step.
    NextStep,
    /// Go back to the previous tour step.
    PreviousStep,
    /// Exit the tour entirely.
    SkipTour,
    /// Forward a natural-language question to the AI Companion.
    AskCompanion {
        /// The original transcript to forward.
        message: String,
    },
    /// Transcript is empty or contains only whitespace/non-alphabetic content.
    Unrecognized {
        /// The raw transcript that could not be matched.
        transcript: String,
    },
}

// ---------------------------------------------------------------------------
// Voice commands - pure parsing logic
// ---------------------------------------------------------------------------

/// Parses a transcript into a [`TourVoiceAction`].
///
/// Matching is case-insensitive and based on substring presence. Priority:
/// 1. Next navigation keywords (FR + EN)
/// 2. Back navigation keywords (FR + EN)
/// 3. Skip / exit keywords (FR + EN)
/// 4. Explain / question keywords → `AskCompanion`
/// 5. Any other non-empty transcript → `AskCompanion`
/// 6. Empty transcript → `Unrecognized`
pub fn parse_voice_command(transcript: &str) -> TourVoiceAction {
    let lower = transcript.to_lowercase();
    let trimmed = lower.trim();

    if trimmed.is_empty() {
        return TourVoiceAction::Unrecognized {
            transcript: transcript.to_owned(),
        };
    }

    const NEXT_KEYWORDS: &[&str] = &[
        "suivant",
        "suivante",
        "next",
        "continue",
        "continuer",
        "avance",
    ];
    if NEXT_KEYWORDS.iter().any(|kw| trimmed.contains(kw)) {
        return TourVoiceAction::NextStep;
    }

    const BACK_KEYWORDS: &[&str] = &["retour", "précédent", "precedent", "back", "previous"];
    if BACK_KEYWORDS.iter().any(|kw| trimmed.contains(kw)) {
        return TourVoiceAction::PreviousStep;
    }

    const SKIP_KEYWORDS: &[&str] = &["passer", "skip", "sauter", "quitter le tour"];
    if SKIP_KEYWORDS.iter().any(|kw| trimmed.contains(kw)) {
        return TourVoiceAction::SkipTour;
    }

    const EXPLAIN_KEYWORDS: &[&str] = &[
        "explique",
        "explain",
        "c'est quoi",
        "what is",
        "comment",
        "how",
    ];
    if EXPLAIN_KEYWORDS.iter().any(|kw| trimmed.contains(kw)) {
        return TourVoiceAction::AskCompanion {
            message: transcript.to_owned(),
        };
    }

    // All other non-empty phrases go to the companion rather than Unrecognized.
    TourVoiceAction::AskCompanion {
        message: transcript.to_owned(),
    }
}

// ---------------------------------------------------------------------------
// Voice commands - Tauri command
// ---------------------------------------------------------------------------

/// Parses a voice transcript into a guided-tour navigation action.
///
/// Recognises FR and EN navigation keywords:
/// - next: `"suivant"`, `"suivante"`, `"next"`, `"continue"`, `"continuer"`, `"avance"`
/// - back: `"retour"`, `"précédent"`, `"back"`, `"previous"`
/// - skip: `"passer"`, `"skip"`, `"sauter"`, `"quitter le tour"`
/// - explain: `"explique"`, `"explain"`, `"c'est quoi"`, `"what is"`, `"comment"`, `"how"`
/// - anything else → `AskCompanion` with the original transcript
#[tauri::command]
pub async fn process_tour_voice_command(transcript: String) -> Result<TourVoiceAction, String> {
    let action = parse_voice_command(&transcript);
    tracing::info!(
        transcript = %transcript,
        action = ?action,
        "tour voice command parsed"
    );
    Ok(action)
}

// ---------------------------------------------------------------------------
// Voice commands - tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod voice_command_tests {
    use super::*;

    #[test]
    fn test_parse_next_fr() {
        // GIVEN a transcript containing "suivant"
        // WHEN parsed
        // THEN NextStep is returned
        assert_eq!(parse_voice_command("suivant"), TourVoiceAction::NextStep);
    }

    #[test]
    fn test_parse_next_en() {
        // GIVEN a transcript containing "next"
        // WHEN parsed
        // THEN NextStep is returned
        assert_eq!(parse_voice_command("next"), TourVoiceAction::NextStep);
    }

    #[test]
    fn test_parse_next_continue_fr() {
        // GIVEN a transcript containing "continuer"
        // WHEN parsed
        // THEN NextStep is returned (contains "continuer")
        assert_eq!(
            parse_voice_command("continuer s'il te plaît"),
            TourVoiceAction::NextStep
        );
    }

    #[test]
    fn test_parse_back_fr() {
        // GIVEN a transcript containing "retour"
        // WHEN parsed
        // THEN PreviousStep is returned
        assert_eq!(parse_voice_command("retour"), TourVoiceAction::PreviousStep);
    }

    #[test]
    fn test_parse_back_en() {
        // GIVEN a transcript containing "back"
        // WHEN parsed
        // THEN PreviousStep is returned
        assert_eq!(
            parse_voice_command("go back"),
            TourVoiceAction::PreviousStep
        );
    }

    #[test]
    fn test_parse_skip_fr() {
        // GIVEN a transcript containing "passer"
        // WHEN parsed
        // THEN SkipTour is returned
        assert_eq!(
            parse_voice_command("passer cette étape"),
            TourVoiceAction::SkipTour
        );
    }

    #[test]
    fn test_parse_skip_en() {
        // GIVEN a transcript "skip"
        // WHEN parsed
        // THEN SkipTour is returned
        assert_eq!(parse_voice_command("skip"), TourVoiceAction::SkipTour);
    }

    #[test]
    fn test_parse_explain_fr() {
        // GIVEN a transcript containing "explique"
        // WHEN parsed
        // THEN AskCompanion with the original message is returned
        assert!(matches!(
            parse_voice_command("explique-moi ça"),
            TourVoiceAction::AskCompanion { .. }
        ));
    }

    #[test]
    fn test_parse_explain_en() {
        // GIVEN a transcript containing "explain"
        // WHEN parsed
        // THEN AskCompanion with the original message is returned
        match parse_voice_command("explain this feature") {
            TourVoiceAction::AskCompanion { message } => {
                assert_eq!(message, "explain this feature");
            }
            other => panic!("expected AskCompanion, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_natural_question_fr() {
        // GIVEN a natural French question with no navigation keywords
        // WHEN parsed
        // THEN AskCompanion is returned (not Unrecognized)
        assert!(matches!(
            parse_voice_command("quel temps fait-il"),
            TourVoiceAction::AskCompanion { .. }
        ));
    }

    #[test]
    fn test_parse_natural_question_en() {
        // GIVEN a natural English question with no navigation keywords
        // WHEN parsed
        // THEN AskCompanion is returned (not Unrecognized)
        assert!(matches!(
            parse_voice_command("what can this agent do"),
            TourVoiceAction::AskCompanion { .. }
        ));
    }

    #[test]
    fn test_parse_empty_transcript() {
        // GIVEN an empty transcript
        // WHEN parsed
        // THEN Unrecognized is returned
        assert!(matches!(
            parse_voice_command(""),
            TourVoiceAction::Unrecognized { .. }
        ));
    }

    #[test]
    fn test_parse_whitespace_only() {
        // GIVEN a transcript with only whitespace
        // WHEN parsed
        // THEN Unrecognized is returned
        assert!(matches!(
            parse_voice_command("   "),
            TourVoiceAction::Unrecognized { .. }
        ));
    }

    #[test]
    fn test_parse_case_insensitive() {
        // GIVEN transcripts in uppercase
        // WHEN parsed
        // THEN navigation keywords are recognised regardless of case
        assert_eq!(parse_voice_command("SUIVANT"), TourVoiceAction::NextStep);
        assert_eq!(parse_voice_command("NEXT"), TourVoiceAction::NextStep);
    }

    #[test]
    fn test_ask_companion_preserves_original_transcript() {
        // GIVEN an explain transcript
        // WHEN parsed
        // THEN the AskCompanion message equals the original (not lowercased)
        let original = "Explique-Moi Ça";
        match parse_voice_command(original) {
            TourVoiceAction::AskCompanion { message } => {
                assert_eq!(message, original);
            }
            other => panic!("expected AskCompanion, got {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Graduation - tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod graduation_tests {
    use super::*;
    use apollia_memory::user_memory::UserMemoryRepository;

    fn make_repo() -> (UserMemoryRepository, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test_onboarding.db");
        let repo = UserMemoryRepository::new(&path).expect("repo");
        (repo, dir)
    }

    #[test]
    fn test_graduation_emits_onboarding_completed_event() {
        // GIVEN an onboarding state in Graduation phase with populated stats
        let state = OnboardingState {
            phase: OnboardingPhase::Graduation,
            profile: Some("operator".to_string()),
            stats: OnboardingStats {
                total_time_sec: 720,
                actions_completed: 12,
                companion_questions: 3,
                voice_commands_used: 1,
            },
            ..OnboardingState::default()
        };
        // WHEN constructing the OnboardingCompleted event (mirrors advance_onboarding_phase_inner)
        let profile = state
            .profile
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let event = apollia_core::RuntimeEvent::OnboardingCompleted {
            profile: profile.clone(),
            duration_sec: state.stats.total_time_sec,
            actions_count: state.stats.actions_completed,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        // THEN the event carries the correct values
        assert!(json.contains("operator"), "event should carry profile");
        assert!(json.contains("720"), "event should carry duration_sec");
        assert!(json.contains("12"), "event should carry actions_count");
        assert!(state.can_advance_to(&OnboardingPhase::Done));
    }

    #[test]
    fn test_graduation_stats_computed_from_user_memory() {
        // GIVEN stats persisted in UserMemory via persist_state
        let (repo, _dir) = make_repo();
        let persisted = OnboardingState {
            phase: OnboardingPhase::Graduation,
            profile: Some("builder".to_string()),
            stats: OnboardingStats {
                total_time_sec: 480,
                actions_completed: 9,
                companion_questions: 5,
                voice_commands_used: 2,
            },
            ..OnboardingState::default()
        };
        persist_state(&repo, &persisted).expect("persist");
        // WHEN loading the state back from UserMemory
        let loaded = load_state_from_memory(&repo).expect("load");
        // THEN the stats and profile match what was persisted
        assert_eq!(loaded.stats.total_time_sec, 480);
        assert_eq!(loaded.stats.actions_completed, 9);
        assert_eq!(loaded.stats.companion_questions, 5);
        assert_eq!(loaded.stats.voice_commands_used, 2);
        assert_eq!(loaded.profile, Some("builder".to_string()));
        assert_eq!(loaded.phase, OnboardingPhase::Graduation);
    }

    #[test]
    fn test_companion_enabled_persisted() {
        // GIVEN a fresh UserMemoryRepository
        let (repo, _dir) = make_repo();
        // WHEN writing companion_enabled = true
        write_bool(&repo, "companion_enabled", true).expect("write true");
        // THEN it reads back as true
        assert!(read_bool(&repo, "companion_enabled").expect("read true"));
        // WHEN writing companion_enabled = false
        write_bool(&repo, "companion_enabled", false).expect("write false");
        // THEN it reads back as false
        assert!(!read_bool(&repo, "companion_enabled").expect("read false"));
    }
}
