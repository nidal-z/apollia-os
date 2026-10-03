//! The in-app companion: whether the user keeps it on, and the guide session
//! it opens.
//!
//! Both live where the runtime keeps them: the preference in the internal
//! state of `user_memory.db`, the session in the chat subsystem. The desktop
//! running the runtime in its own process calls these functions directly; a
//! client of the daemon reaches the same functions through
//! `/api/v1/companion/...`. Both therefore answer the same values and refuse
//! with the same words.

use std::sync::Arc;

use apollia_memory::user_memory::UserMemoryRepository;

use crate::chat::manager::CreateSessionParams;
use crate::chat::{ChatError, ChatMode, ChatSessionManagerHandle};
use crate::profile::ProfileStore;

/// Name of the agent that powers the companion panel.
pub const GUIDE_AGENT: &str = "apollia-guide";

/// Internal-state key of the companion preference (stored with the `__`
/// prefix, out of the profile listing).
const ENABLED_KEY: &str = "companion_enabled";

/// Internal-state key the onboarding state reads its companion session from.
const SESSION_KEY: &str = "onboarding_companion_session_id";

/// Why a companion operation did not complete. The display text is what the
/// interface shows, the same whichever process ran the operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CompanionError {
    /// `user_memory.db` did not open at startup.
    #[error("UserMemory database not initialized")]
    MemoryUnavailable,
    /// The chat subsystem did not start.
    #[error("chat subsystem not available")]
    ChatUnavailable,
    /// The guide is not installed in this runtime.
    #[error("apollia-guide agent not found - it powers the companion panel and is provisioned at startup, bundle it and restart the application")]
    GuideNotInstalled,
    /// The chat subsystem refused the session.
    #[error("chat session creation failed: {0}")]
    SessionCreation(String),
    /// The repository refused the read or the write.
    #[error("persistence error: {0}")]
    Storage(String),
    /// A writer panicked while holding the repository.
    #[error("mutex poisoned")]
    Poisoned,
    /// The blocking task did not complete.
    #[error("spawn_blocking failed: {0}")]
    Join(#[source] tokio::task::JoinError),
}

/// Runs `operation` on the repository off the async threads.
async fn with_repo<T, F>(store: Option<ProfileStore<'_>>, operation: F) -> Result<T, CompanionError>
where
    T: Send + 'static,
    F: FnOnce(&UserMemoryRepository) -> Result<T, CompanionError> + Send + 'static,
{
    let repo = Arc::clone(store.ok_or(CompanionError::MemoryUnavailable)?.0);
    tokio::task::spawn_blocking(move || {
        let guard = repo.lock().map_err(|_| CompanionError::Poisoned)?;
        operation(&guard)
    })
    .await
    .map_err(CompanionError::Join)?
}

/// Reads whether the companion is on; `false` when never written.
///
/// # Errors
///
/// [`CompanionError::MemoryUnavailable`] without a repository,
/// [`CompanionError::Storage`] when it cannot be read.
pub async fn read_enabled(store: Option<ProfileStore<'_>>) -> Result<bool, CompanionError> {
    with_repo(store, |repo| {
        repo.get_internal(ENABLED_KEY)
            .map(|value| value.as_deref() == Some("true"))
            .map_err(|e| CompanionError::Storage(e.to_string()))
    })
    .await
}

/// Records whether the companion is on.
///
/// # Errors
///
/// [`CompanionError::MemoryUnavailable`] without a repository,
/// [`CompanionError::Storage`] when it cannot be written.
pub async fn write_enabled(
    store: Option<ProfileStore<'_>>,
    enabled: bool,
) -> Result<(), CompanionError> {
    let value = if enabled { "true" } else { "false" };
    with_repo(store, move |repo| {
        repo.set_internal(ENABLED_KEY, value)
            .map_err(|e| CompanionError::Storage(e.to_string()))
    })
    .await
}

/// Opens an Agent-mode session on the guide and records its identifier where
/// the onboarding state reads it, best effort.
///
/// The guide is a declarative agent whose mission is the prompt, so no caller
/// prompt is supplied; `context` (the route the panel opened on) is logged,
/// not injected.
///
/// # Errors
///
/// [`CompanionError::ChatUnavailable`] without a chat subsystem,
/// [`CompanionError::GuideNotInstalled`] when the guide is not there,
/// [`CompanionError::SessionCreation`] for any other refusal.
pub async fn create_session(
    chat: Option<&ChatSessionManagerHandle>,
    store: Option<ProfileStore<'_>>,
    context: Option<String>,
) -> Result<String, CompanionError> {
    let manager = chat.ok_or(CompanionError::ChatUnavailable)?;
    let info = manager
        .create_session(CreateSessionParams {
            mode: ChatMode::Agent,
            agent_name: Some(GUIDE_AGENT.to_string()),
            system_prompt: None,
            tools: Vec::new(),
            project_id: None,
        })
        .await
        .map_err(|e| match e {
            ChatError::AgentNotFound(_) => CompanionError::GuideNotInstalled,
            other => CompanionError::SessionCreation(other.to_string()),
        })?;
    let session_id = info.id;

    let recorded = session_id.clone();
    let stored = with_repo(store, move |repo| {
        repo.set_internal(SESSION_KEY, &recorded)
            .map_err(|e| CompanionError::Storage(e.to_string()))
    })
    .await;
    if let Err(error) = stored {
        tracing::event!(
            tracing::Level::WARN,
            error = %error,
            session_id = %session_id,
            "companion.session.record.failed"
        );
    }

    tracing::event!(
        tracing::Level::INFO,
        session_id = %session_id,
        agent = %GUIDE_AGENT,
        context = ?context,
        "companion.session.created"
    );
    Ok(session_id)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn repo(dir: &tempfile::TempDir) -> Arc<Mutex<UserMemoryRepository>> {
        let repo = UserMemoryRepository::new(&dir.path().join("user_memory.db")).unwrap();
        Arc::new(Mutex::new(repo))
    }

    #[tokio::test]
    async fn test_the_preference_reads_false_until_written_then_as_written() {
        // GIVEN a repository where the preference was never written
        let dir = tempfile::TempDir::new().unwrap();
        let repo = repo(&dir);

        // WHEN it is read, turned on, read, turned off, read
        let before = read_enabled(Some(ProfileStore(&repo))).await.unwrap();
        write_enabled(Some(ProfileStore(&repo)), true)
            .await
            .unwrap();
        let on = read_enabled(Some(ProfileStore(&repo))).await.unwrap();
        write_enabled(Some(ProfileStore(&repo)), false)
            .await
            .unwrap();
        let off = read_enabled(Some(ProfileStore(&repo))).await.unwrap();

        // THEN it starts off and follows each write, under the key the
        // onboarding helpers of the desktop read
        assert!(!before);
        assert!(on);
        assert!(!off);
        let raw = repo.lock().unwrap().get_internal(ENABLED_KEY).unwrap();
        assert_eq!(raw.as_deref(), Some("false"));
    }

    #[tokio::test]
    async fn test_without_a_repository_or_a_chat_each_operation_says_which() {
        // GIVEN a runtime where neither the repository nor the chat started
        // WHEN each operation runs
        let read = read_enabled(None).await;
        let write = write_enabled(None, true).await;
        let session = create_session(None, None, None).await;

        // THEN each refuses with the words the desktop showed before
        assert_eq!(
            read.unwrap_err().to_string(),
            "UserMemory database not initialized"
        );
        assert!(matches!(write, Err(CompanionError::MemoryUnavailable)));
        assert_eq!(
            session.unwrap_err().to_string(),
            "chat subsystem not available"
        );
    }
}
