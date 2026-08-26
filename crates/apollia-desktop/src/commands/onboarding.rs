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

use apollia_memory::user_memory::UserMemoryRepository;
use apollia_runtime::chat::ChatMode;
use apollia_runtime::embedded::RuntimeHandle;
use tauri::State;

/// The phase machine types live in `state`, their persistence in `memory`, the
/// in-app companion in `companion`, and the model-detection step in `ai_setup`.
pub mod ai_setup;
pub mod companion;
pub mod memory;
pub mod state;

use memory::{
    get_repo, load_state_from_memory, persist_state, reset_onboarding_progress,
    write_profile_to_agent_memory,
};
use state::{validate_profile, OnboardingError, OnboardingPhase, OnboardingState, TriggerResult};

/// The five onboarding topics that define a complete (legacy) onboarding.
const ONBOARDING_TOPICS: [&str; 5] = ["identity", "preferences", "tools", "domain", "agents"];

/// Name of the agent used for onboarding conversations.
const ONBOARDING_AGENT_NAME: &str = "onboarding-agent";

/// Name of the agent that powers the in-app companion panel.
const GUIDE_AGENT_NAME: &str = "apollia-guide";

/// Valid user profiles for the onboarding flow.
const VALID_PROFILES: [&str; 2] = ["operator", "builder"];

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
        tracing::error!(error = %e, "onboarding.state.read.failed");
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
                        detail = "ignored",
                        "onboarding.phase.self_transition"
                    );
                }
                _ => {
                    tracing::error!(error = %e, "onboarding.phase.advance.failed");
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
            tracing::error!(error = %e, "onboarding.profile.set.failed");
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
            tracing::error!(error = %e, "onboarding.trigger.failed");
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
            tracing::error!(error = %e, "onboarding.resume.failed");
            e.to_string()
        })
}

/// Marks the onboarding as dismissed (skipped) by the user.
#[tauri::command]
pub async fn dismiss_onboarding(state: State<'_, RuntimeHandle>) -> Result<(), String> {
    dismiss_onboarding_inner(&state).await.map_err(|e| {
        tracing::error!(error = %e, "onboarding.dismiss.failed");
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
            "onboarding.phase.advanced"
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

        tracing::info!(profile = %profile, "onboarding.profile.set");

        Ok(onboarding_state)
    })
    .await
    .map_err(|e| OnboardingError::PersistenceError(format!("spawn_blocking failed: {e}")))?
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
                tracing::warn!(
                    detail = "the reset is skipped",
                    "onboarding.memory.lock.poisoned"
                );
            }
        }
    }

    // Validate and inject profile before creating the session.
    if let Some(ref p) = profile {
        validate_profile(p)?;
        write_profile_to_agent_memory(p);
        tracing::info!(profile = %p, "onboarding.profile.injected");
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
        "onboarding.session.created"
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

    tracing::info!("onboarding.dismissed");
    Ok(())
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
mod profile_tests {
    use super::state::{validate_profile, OnboardingError};

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
