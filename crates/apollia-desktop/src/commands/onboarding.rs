//! Tauri IPC commands for onboarding lifecycle management.
//!
//! Provides the full onboarding state machine (7 phases) with persistence
//! in UserMemory (`context.onboarding_*` keys), plus the legacy helpers that
//! drive the existing chat-based onboarding:
//!
//! - [`get_onboarding_state`] - full machine state (new)
//! - [`advance_onboarding_phase`] - validated phase transition (new)
//! - [`set_onboarding_profile`] - profile selection (new)
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

/// Name of the agent that powers the in-app companion panel.
const GUIDE_AGENT_NAME: &str = "apollia-guide";

/// Valid user profiles for the onboarding flow.
const VALID_PROFILES: [&str; 2] = ["operator", "builder"];

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
    /// warning nobody reads: `Failed to load installed agent at boot`.
    #[error(
        "onboarding-agent is not in the runtime registry. The registry is built \
         once at startup, so restart the application first: if the agent was \
         installed, or a profile swapped in, while it was running, it cannot \
         have been picked up. If restarting does not help, look for `Failed to \
         load installed agent at boot` in the logs, which names the real cause."
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
    let memory_dir = onboarding_memory_dir();
    let result = tokio::task::spawn_blocking(move || check_finalized_in(&memory_dir))
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?;
    Ok(result)
}

/// Directory holding the per-agent memory databases (`~/.apollia/memory`).
fn onboarding_memory_dir() -> std::path::PathBuf {
    apollia_core::paths::data_dir_under(apollia_core::paths::home_dir_or_temp()).join("memory")
}

/// Whether `onboarding.completed_at` exists in the onboarding agent's semantic
/// memory under `memory_dir`. Read side of the finalization contract: whatever
/// [`finalize_in`] writes must be found here.
fn check_finalized_in(memory_dir: &std::path::Path) -> bool {
    if !memory_dir.exists() {
        return false;
    }

    // (db_filename, namespace_in_table) - the manifest namespace also names
    // the SQLite file (see `MemoryManager::db_path`), but the semantic table
    // carries a redundant namespace column we must filter on.
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
}

/// Stamps `onboarding.completed_at` in the onboarding agent's semantic memory
/// under `memory_dir`, plus `onboarding.finalized_by = "operator_skip"` so the
/// audit trail distinguishes an operator skip from the agent's own wrap-up.
///
/// Writes the same store and key that [`check_finalized_in`] reads, in the
/// current namespace (`onboarding`); `MemoryStore::open` creates and migrates
/// the database when the agent has not opened it yet.
fn finalize_in(memory_dir: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(memory_dir).map_err(|e| format!("cannot create memory dir: {e}"))?;
    let db_path = memory_dir.join("onboarding.db");
    let store = apollia_memory::store::MemoryStore::open(&db_path)
        .map_err(|e| format!("cannot open onboarding memory: {e}"))?;
    let sem = apollia_memory::semantic::SemanticMemory::new(&store);
    let now = chrono::Utc::now().to_rfc3339();
    for (key, value) in [
        ("onboarding.completed_at", now.as_str()),
        ("onboarding.finalized_by", "operator_skip"),
    ] {
        sem.remember(apollia_memory::semantic::RememberInput {
            namespace: "onboarding",
            key,
            value: &serde_json::Value::String(value.to_string()),
            confidence: 1.0,
            source: Some("onboarding"),
            expires_at: None,
        })
        .map_err(|e| format!("cannot write {key}: {e}"))?;
    }
    Ok(())
}

/// Finalizes the onboarding acquaintance chat without a model turn.
///
/// Backs the "skip the optional questions" button: it stamps the completion
/// key the wrap-up gate reads ([`check_onboarding_finalized`]), so the modal
/// moves straight to the permission proposals instead of spending one or more
/// inference turns nudging the agent to close the interview.
///
/// Deliberately writes the agent's semantic memory rather than widening the
/// gate to the `UserMemoryRepository` phase machine: `resume_onboarding` (the
/// profile-enrichment re-entry) runs after a previous full completion, and a
/// phase-machine OR in the gate would instantly finalize that resumed chat.
#[tauri::command]
pub async fn finalize_onboarding_chat() -> Result<(), String> {
    let memory_dir = onboarding_memory_dir();
    tokio::task::spawn_blocking(move || finalize_in(&memory_dir))
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
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
    trigger_onboarding_inner(topic, profile, false, &state)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "trigger_onboarding failed");
            e.to_string()
        })
}

/// Resumes onboarding for profile enrichment without wiping collected facts.
///
/// Unlike `trigger_onboarding` (which resets progress for a fresh run), this
/// keeps the already-collected Tier 1 + Tier 2 profile so the agent continues
/// the optional enrichment where it left off. Used by the "complete your
/// profile" entry point after a user finished calibration but skipped Tier 2.
#[tauri::command]
pub async fn resume_onboarding(state: State<'_, RuntimeHandle>) -> Result<TriggerResult, String> {
    trigger_onboarding_inner(None, None, true, &state)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "resume_onboarding failed");
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
    };

    Ok(OnboardingState {
        phase,
        profile,
        llm_configured,
        stt_configured,
        topics_covered,
        mandatory_complete,
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
/// from the user profile listing.
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

/// Writes the active onboarding profile to the onboarding agent's semantic
/// memory so the Python agent can read it via `ctx.memory.recall()` on its
/// first conversational turn.
///
/// Writes to the agent's own namespace (`"onboarding-agent"`) under the key
/// `"onboarding.active_profile"`.  A missing or unwritable memory store is
/// treated as a non-fatal degradation - the agent falls back to generic
/// questioning without the profile section.
fn write_profile_to_agent_memory(profile: &str) {
    let db_path = apollia_core::paths::data_dir_under(apollia_core::paths::home_dir_or_temp())
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
/// Without this, two sources of stale state make a fresh session look already
/// finished on arrival:
///   1. `UserMemoryRepository` keeps `onboarding_topic_*` marks across runs.
///   2. The onboarding agent's semantic DB keeps `user.*` entries from prior
///      conversations, which mark the matching topics as covered again.
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
    let memory_dir =
        apollia_core::paths::data_dir_under(apollia_core::paths::home_dir_or_temp()).join("memory");

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
    resume: bool,
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
    // A resume run keeps already-collected Tier 1 + Tier 2 facts so the agent
    // continues the profile enrichment where it left off, so it never resets.
    if topic.is_none() && !resume {
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

    // The onboarding agent supplies its own system prompt from its
    // `@on_message` handler; in `ChatMode::Agent` the session `system_prompt`
    // is not forwarded to the agent (see `session_to_task`), so we pass `None`
    // rather than a second, divergent prompt.
    let mode = if resume {
        "resume"
    } else if topic.is_some() {
        "partial"
    } else {
        "full"
    };

    // Create chat session via ChatSessionManager
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or(OnboardingError::ChatNotAvailable)?;

    let info = manager
        .create_session(apollia_runtime::chat::manager::CreateSessionParams {
            mode: ChatMode::Agent,
            agent_name: Some(ONBOARDING_AGENT_NAME.to_string()),
            system_prompt: None,
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

// ---------------------------------------------------------------------------
// Companion - context table
// ---------------------------------------------------------------------------

/// Per-route contextual help texts displayed in the Companion panel.
const COMPANION_CONTEXTS: &[(&str, &str)] = &[
    ("dashboard", "You are on the dashboard. It gives you an overview of your active agents, the recent events and the state of the system. Ask me anything about running your agents."),
    ("agents", "You are on the Agents page. You can create, configure and watch your AI agents here. Each agent is a Python module exposing manifest() and run()."),
    ("chat", "You are on the Chat page. You can talk to a local model or work with your agents through the free chat. Sessions are stored on this machine."),
    ("triggers", "You are on the Triggers page. Triggers start agents on their own, on a cron schedule, on an interval, when a file changes, or on an incoming webhook."),
    ("pipelines", "You are on the Pipelines page. Pipelines chain several agents in sequence or in parallel, with a DAG topology, fan-out and fan-in, and human checkpoints."),
    ("memory", "You are on the Memory page. Apollia keeps three kinds of memory: episodic for events, semantic for knowledge, procedural for know-how. Full-text search is built in."),
    ("integrations", "You are on the MCP integrations page. Connect external MCP servers to give your agents new tools over the standard JSON-RPC 2.0 protocol."),
    ("approvals", "You are on the Approvals page. Sensitive agent actions can require your go-ahead before they run, which is the human-in-the-loop safeguard."),
    ("observability", "You are on the Observability page. Follow your agents' execution traces, their performance metrics and the structured logs as they happen."),
    ("notifications", "You are on the Notifications page. Set up desktop alerts and webhooks so you hear about the events that matter on your agents."),
    ("transcriptions", "You are on the Transcriptions page. Read back the audio transcripts produced by the built-in Whisper speech engine."),
    ("llm", "You are on the LLM page. Configure the language model backends your agents use: local llama.cpp, Ollama, or a cloud API such as Anthropic or OpenAI."),
    ("settings", "You are on the Settings page. Configure Apollia's global options: paths, logs, resource limits and your own preferences."),
    ("onboarding", "You are in the middle of onboarding. I am here to walk you through Apollia. Ask me anything at any step."),
];

/// Generic fallback when no route-specific context is available.
const COMPANION_CONTEXT_FALLBACK: &str =
    "I am your Apollia assistant. Ask me anything about the application, your agents, or what it can do.";

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

/// Creates a companion session backed by the apollia-guide agent.
///
/// The session runs in Agent mode so the panel is driven by the single
/// product coach (knowledge base, live environment listing, action buttons).
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
    // The companion panel is powered by the apollia-guide agent: a
    // knowledge-base-grounded coach that inspects the live environment and
    // suggests allowlisted deep-links. Verify it is registered first so the
    // panel can surface a clear install hint instead of an opaque failure.
    let found = state
        .registry_handle
        .find_by_name(GUIDE_AGENT_NAME)
        .await
        .map_err(|_| OnboardingError::GuideAgentNotInstalled)?;
    if found.is_none() {
        return Err(OnboardingError::GuideAgentNotInstalled);
    }

    let manager = state
        .chat_manager
        .as_ref()
        .ok_or(OnboardingError::ChatNotAvailable)?;

    // Agent mode: the agent owns its own system prompt, environment listing,
    // and action-block contract, so no caller prompt is supplied. The
    // `context` route hint is retained on the IPC surface for a future
    // page-aware enhancement but is not injected here.
    let info = manager
        .create_session(apollia_runtime::chat::manager::CreateSessionParams {
            mode: ChatMode::Agent,
            agent_name: Some(GUIDE_AGENT_NAME.to_string()),
            system_prompt: None,
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
        agent = %GUIDE_AGENT_NAME,
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
mod finalize_tests {
    use super::*;

    #[test]
    fn test_finalize_writes_key_check_reads() {
        // GIVEN an empty memory directory (fresh install, agent never ran)
        let dir = tempfile::tempdir().expect("tempdir");

        // WHEN the operator skip finalizes the chat
        finalize_in(dir.path()).expect("finalize");

        // THEN the wrap-up gate reads the finalization from the same store
        assert!(check_finalized_in(dir.path()));
        let store = apollia_memory::store::MemoryStore::open(&dir.path().join("onboarding.db"))
            .expect("open");
        let sem = apollia_memory::semantic::SemanticMemory::new(&store);
        let entries = sem.recall_all("onboarding", None).expect("recall");
        assert!(entries.iter().any(|e| e.key == "onboarding.completed_at"));
        assert!(entries.iter().any(
            |e| e.key == "onboarding.finalized_by" && e.value.as_str() == Some("operator_skip")
        ));
    }

    #[test]
    fn test_check_finalized_false_on_missing_db() {
        // GIVEN an empty memory directory
        let dir = tempfile::tempdir().expect("tempdir");

        // WHEN checking finalization without any database
        // THEN the gate stays closed
        assert!(!check_finalized_in(dir.path()));
    }
}

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

    #[test]
    fn test_companion_contexts_are_written_in_one_language() {
        // GIVEN the per-route Companion blurbs
        //
        // WHEN each is read
        //
        // THEN it reads in the codebase language. These strings are not
        // translated anywhere: `get_companion_context` hands them to the
        // frontend, which pushes them into the Companion session's system
        // prompt, so a French blurb makes the model answer in French inside an
        // English window. The assertion is on the opening words rather than on
        // a non-ASCII scan, because the old fallback ("Je suis votre assistant
        // Apollia...") was pure ASCII and a scan would have waved it through.
        for (route, ctx) in COMPANION_CONTEXTS {
            assert!(
                ctx.starts_with("You are "),
                "context for route '{route}' does not open in English: {ctx}"
            );
        }
        assert!(COMPANION_CONTEXT_FALLBACK.starts_with("I am your Apollia assistant"));

        // The negative case the scan would miss, spelled out so this test is
        // known to be able to fail.
        assert!(!"Je suis votre assistant Apollia.".starts_with("I am your Apollia assistant"));
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
        let home = apollia_core::paths::home_string().unwrap_or_default();
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

        models.sort_by_key(|b| std::cmp::Reverse(b.size_bytes));
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
        let home = apollia_core::paths::home_string().unwrap_or_default();
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
    language: Option<String>,
    state: State<'_, RuntimeHandle>,
    app: tauri::AppHandle,
    stt_flow_state: State<'_, crate::commands::stt::SttFlowState>,
) -> Result<OnboardingState, String> {
    let result = setup_whisper_model_inner(model_path, language, &state)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "setup_whisper_model failed");
            e.to_string()
        })?;

    // Hot-load the freshly-configured model so onboarding "Tester" and the
    // dictation hotkey work immediately, without restarting the app.
    if let Err(e) = crate::commands::stt::reload_stt_inner(&state, &app, &stt_flow_state).await {
        tracing::warn!(error = %e, "whisper model configured but STT reload failed");
    }

    Ok(result)
}

async fn setup_whisper_model_inner(
    model_path: String,
    language: Option<String>,
    state: &RuntimeHandle,
) -> Result<OnboardingState, OnboardingError> {
    let db_path = {
        let home = apollia_core::paths::home_dir_or_temp()
            .display()
            .to_string();
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
        // Pre-set the transcription language from the user's app locale so
        // dictation transcribes in their language instead of defaulting to
        // English. Only when unset, to preserve an explicit Settings choice.
        if row.language.is_none() {
            if let Some(lang) = language.filter(|l| !l.trim().is_empty()) {
                row.language = Some(lang);
            }
        }
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
            msg.contains("Failed to load installed agent at boot"),
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
