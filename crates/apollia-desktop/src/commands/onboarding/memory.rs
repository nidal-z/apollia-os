//! Where the onboarding state lives between two launches: the `onboarding_*`
//! context keys of the user memory, read and written one typed value at a time,
//! and the reset that empties them.

use std::sync::Arc;

use apollia_memory::user_memory::UserMemoryRepository;
use apollia_runtime::embedded::RuntimeHandle;

use super::state::{OnboardingError, OnboardingPhase, OnboardingState, OnboardingStats};
use super::ONBOARDING_TOPICS;

// ---------------------------------------------------------------------------
// Persistence helpers
// ---------------------------------------------------------------------------

/// Loads the full `OnboardingState` from UserMemory context keys.
///
/// Returns `OnboardingState::default()` when no persisted phase is found
/// (first launch).
pub(super) fn load_state_from_memory(
    repo: &UserMemoryRepository,
) -> Result<OnboardingState, OnboardingError> {
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
pub(super) fn persist_state(
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

pub(super) fn read_bool(repo: &UserMemoryRepository, key: &str) -> Result<bool, OnboardingError> {
    Ok(read_str(repo, key)?.map(|v| v == "true").unwrap_or(false))
}

pub(super) fn write_bool(
    repo: &UserMemoryRepository,
    key: &str,
    value: bool,
) -> Result<(), OnboardingError> {
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
pub(super) fn get_repo(
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
pub(super) fn write_profile_to_agent_memory(profile: &str) {
    let db_path = apollia_core::paths::data_dir_under(apollia_core::paths::home_dir_or_temp())
        .join("memory")
        .join("onboarding-agent.db");

    let Ok(store) = apollia_memory::store::MemoryStore::open(&db_path) else {
        tracing::warn!(
            detail = "the profile is not injected",
            "onboarding.memory.store.absent"
        );
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
        tracing::warn!(error = %e, "onboarding.profile.write.failed");
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
pub(super) fn reset_onboarding_progress(repo: &UserMemoryRepository) {
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
                detail = "stale entries may persist",
                "onboarding.memory.store.unreadable"
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
                    "onboarding.memory.entry.wipe.failed",
                );
            }
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
