//! Where the onboarding state lives between two launches: the `onboarding_*`
//! context keys of the user memory, read and written one typed value at a time,
//! the reset that empties them, and the onboarding agent's own memory under the
//! memory directory, where its progress and its completion are kept.

use std::path::Path;

use apollia_memory::user_memory::UserMemoryRepository;

use super::state::{OnboardingError, OnboardingPhase, OnboardingState, OnboardingStats};
use super::ONBOARDING_TOPICS;

// ---------------------------------------------------------------------------
// Persistence helpers
// ---------------------------------------------------------------------------

/// The phase name an earlier version stored while the usage-profile question
/// was on screen.
const LEGACY_PROFILE_PHASE: &str = "profile_choice";

/// Loads the full `OnboardingState` from UserMemory context keys.
///
/// Returns `OnboardingState::default()` when no persisted phase is found
/// (first launch).
pub fn load_state_from_memory(
    repo: &UserMemoryRepository,
) -> Result<OnboardingState, OnboardingError> {
    let phase_str = read_str(repo, "onboarding_phase")?;
    let Some(phase_str) = phase_str else {
        return Ok(OnboardingState::default());
    };

    // An install that stopped on the usage-profile step, which no longer
    // exists, resumes on the step that followed it.
    let phase = match phase_str.as_str() {
        LEGACY_PROFILE_PHASE => OnboardingPhase::AiSetup,
        other => OnboardingPhase::parse(other).ok_or_else(|| {
            OnboardingError::PersistenceError(format!("unrecognised persisted phase: {other}"))
        })?,
    };

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
pub fn persist_state(
    repo: &UserMemoryRepository,
    state: &OnboardingState,
) -> Result<(), OnboardingError> {
    write_str(repo, "onboarding_phase", state.phase.as_str())?;

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

pub fn read_bool(repo: &UserMemoryRepository, key: &str) -> Result<bool, OnboardingError> {
    Ok(read_str(repo, key)?.map(|v| v == "true").unwrap_or(false))
}

pub fn write_bool(
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
// The onboarding agent's memory
// ---------------------------------------------------------------------------

/// Wipes stale onboarding progress before starting a fresh session.
///
/// Without this, two sources of stale state make a fresh session look already
/// finished on arrival:
///   1. `UserMemoryRepository` keeps `onboarding_topic_*` marks across runs.
///   2. The memory of the onboarding agent keeps the completion it recorded
///      at the end of the previous conversation.
///
/// This helper forgets every `onboarding_topic_{topic}` entry in the user
/// repo, then calls [`forget_agent_entries`]. The profiles are not touched:
/// the agent never writes them.
pub fn reset_onboarding_progress(repo: &UserMemoryRepository, memory_dir: &Path) {
    for topic in &ONBOARDING_TOPICS {
        let key = format!("onboarding_topic_{topic}");
        let _ = repo.forget(&key);
    }
    forget_agent_entries(memory_dir);
}

/// Forgets what the onboarding agent recorded in its own memory: the meta
/// `onboarding.*` keys, the completion among them, and the `user.*` entries an
/// older agent kept there.
///
/// Every conversation starts with it, the one that completes a profile
/// included: a completion left by the previous conversation would otherwise
/// close the new one before the person has answered.
pub fn forget_agent_entries(memory_dir: &Path) {
    // Both filename/namespace pairs are wiped - the manifest namespace was
    // renamed from "onboarding-agent" to "onboarding" in v2.x and an install
    // upgraded across that change can have entries in either file. Forgetting
    // to clean the new file caused stale `onboarding.completed_at` to leak
    // into fresh sessions and trigger the wrap-up panel before the user
    // could answer a single question.
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
            let is_meta = entry.key.starts_with("onboarding.");
            if !is_user && !is_meta {
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

/// Whether `onboarding.completed_at` exists in the onboarding agent's semantic
/// memory under `memory_dir`. Read side of the finalization contract: whatever
/// [`finalize_in`] writes must be found here.
///
/// Looks under both the current namespace (`onboarding`, since manifest
/// v2.x) and the legacy one (`onboarding-agent`, kept for installs that ran an
/// older agent).
pub fn check_finalized_in(memory_dir: &Path) -> bool {
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
///
/// # Errors
///
/// [`OnboardingError::AgentMemory`] when the directory, the store or a key
/// cannot be written.
pub fn finalize_in(memory_dir: &Path) -> Result<(), OnboardingError> {
    std::fs::create_dir_all(memory_dir)
        .map_err(|e| OnboardingError::AgentMemory(format!("cannot create memory dir: {e}")))?;
    let db_path = memory_dir.join("onboarding.db");
    let store = apollia_memory::store::MemoryStore::open(&db_path)
        .map_err(|e| OnboardingError::AgentMemory(format!("cannot open onboarding memory: {e}")))?;
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
        .map_err(|e| OnboardingError::AgentMemory(format!("cannot write {key}: {e}")))?;
    }
    Ok(())
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
            stats: OnboardingStats {
                total_time_sec: 720,
                actions_completed: 12,
                companion_questions: 3,
            },
            ..OnboardingState::default()
        };
        // WHEN constructing the OnboardingCompleted event (mirrors advance_onboarding_phase_inner)
        let event = apollia_core::RuntimeEvent::OnboardingCompleted {
            duration_sec: state.stats.total_time_sec,
            actions_count: state.stats.actions_completed,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        // THEN the event carries the correct values, and no usage profile
        assert!(!json.contains("profile"), "event should carry no profile");
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
        // THEN the stats and the phase match what was persisted
        assert_eq!(loaded.stats.total_time_sec, 480);
        assert_eq!(loaded.stats.actions_completed, 9);
        assert_eq!(loaded.stats.companion_questions, 5);
        assert_eq!(loaded.phase, OnboardingPhase::Graduation);
    }

    #[test]
    fn test_legacy_profile_phase_resumes_on_ai_setup() {
        // GIVEN a user memory left on the usage-profile step by an earlier version
        let (repo, _dir) = make_repo();
        write_str(&repo, "onboarding_phase", "profile_choice").expect("write");
        // WHEN the state is loaded
        let loaded = load_state_from_memory(&repo).expect("load");
        // THEN the onboarding resumes on the step that followed it
        assert_eq!(loaded.phase, OnboardingPhase::AiSetup);
    }

    #[test]
    fn test_reset_forgets_every_onboarding_meta_key() {
        // GIVEN an agent memory holding a stale usage profile and a user fact
        let (repo, dir) = make_repo();
        let store = apollia_memory::store::MemoryStore::open(&dir.path().join("onboarding.db"))
            .expect("open");
        let sem = apollia_memory::semantic::SemanticMemory::new(&store);
        for key in ["onboarding.active_profile", "user.name"] {
            sem.remember(apollia_memory::semantic::RememberInput {
                namespace: "onboarding",
                key,
                value: &serde_json::Value::String("x".to_string()),
                confidence: 0.9,
                source: Some("onboarding"),
                expires_at: None,
            })
            .expect("remember");
        }
        // WHEN a fresh run resets the progress
        reset_onboarding_progress(&repo, dir.path());
        // THEN neither survives
        let left = sem.recall_all("onboarding", None).expect("recall");
        assert!(left.is_empty(), "left: {left:?}");
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

    #[test]
    fn test_forgetting_the_agent_entries_reopens_the_gate() {
        // GIVEN a memory directory where a previous conversation was completed
        let dir = tempfile::tempdir().expect("tempdir");
        finalize_in(dir.path()).expect("finalize");
        assert!(check_finalized_in(dir.path()));

        // WHEN the entries of the agent are forgotten, as every new
        // conversation does
        forget_agent_entries(dir.path());

        // THEN the gate is closed again
        assert!(!check_finalized_in(dir.path()));
    }
}
