//! Tauri IPC commands for onboarding lifecycle management.
//!
//! Provides three commands consumed by the Svelte frontend:
//! - [`get_onboarding_status`] — reads completion state from UserMemory
//! - [`trigger_onboarding`] — creates an agent-backed chat session
//! - [`dismiss_onboarding`] — marks onboarding as skipped
//!
//! All data stays local (Principle #1). Structs are serde-typed for the
//! frontend (Principle #8).

use std::sync::Arc;

use apollia_memory::user_memory::UserMemoryRepository;
use apollia_runtime::chat::ChatMode;
use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

/// The five onboarding topics that define a complete onboarding.
const ONBOARDING_TOPICS: [&str; 5] = ["identity", "preferences", "tools", "domain", "agents"];

/// Name of the agent used for onboarding conversations.
const ONBOARDING_AGENT_NAME: &str = "onboarding-agent";

// ---------------------------------------------------------------------------
// Public types (serialised to Svelte)
// ---------------------------------------------------------------------------

/// Onboarding completion status returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingStatus {
    /// `true` if all 5 topics have been covered.
    pub completed: bool,
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
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Returns the current onboarding status.
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
// Inner logic (testable without Tauri context)
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

/// Reads onboarding status from UserMemory.
async fn get_onboarding_status_inner(
    state: &RuntimeHandle,
) -> Result<OnboardingStatus, OnboardingError> {
    let repo = get_repo(state)?;

    let status = tokio::task::spawn_blocking(move || {
        let repo = repo
            .lock()
            .map_err(|e| OnboardingError::SessionCreationFailed(format!("mutex poisoned: {e}")))?;

        let topics_covered = repo
            .get_covered_topics()
            .map_err(|_| OnboardingError::RepositoryNotInitialized)?;

        let total = ONBOARDING_TOPICS.len();
        let covered = topics_covered.len().min(total);
        let completion_pct = ((covered as f64 / total as f64) * 100.0) as u8;
        let completed = completion_pct == 100;

        let last_session_at = repo.get_last_onboarding_session().unwrap_or(None);

        let skipped = repo.get_onboarding_skipped().unwrap_or(false);

        Ok::<OnboardingStatus, OnboardingError>(OnboardingStatus {
            completed,
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
        None => format!(
            "You are an onboarding assistant. Cover these topics naturally through conversation: {}. \
             Ask questions one at a time. Be conversational, not rigid. \
             Adapt based on what the user shares.",
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
}
