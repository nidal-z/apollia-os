//! The onboarding as every surface drives it: the six-phase machine kept in
//! the user memory, the conversation with the onboarding agent that starts,
//! resumes or is dismissed, and the completion that agent records in its own
//! memory.
//!
//! The desktop running the runtime in its own process calls [`Onboarding`]
//! directly; a client of the daemon reaches the same functions through
//! `/api/v1/onboarding`. Both therefore answer the same state and refuse with
//! the same words.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use apollia_core::EventBusSender;
use apollia_memory::user_memory::UserMemoryRepository;

use crate::chat::manager::{ChatSessionManagerHandle, CreateSessionParams};
use crate::chat::{ChatError, ChatMode};

pub mod memory;
pub mod state;

use memory::{
    check_finalized_in, finalize_in, load_state_from_memory, persist_state,
    reset_onboarding_progress,
};
use state::{OnboardingError, OnboardingPhase, OnboardingState, TriggerResult};

/// The five onboarding topics that define a complete (legacy) onboarding.
pub const ONBOARDING_TOPICS: [&str; 5] = ["identity", "preferences", "tools", "domain", "agents"];

/// Name of the agent used for onboarding conversations.
pub const ONBOARDING_AGENT_NAME: &str = "onboarding-agent";

/// What the onboarding reads and writes, borrowed from the runtime for one
/// operation. The repository lock is the one `user_memory.db` was opened
/// behind at startup; this type adds no shared state of its own.
pub struct Onboarding<'a> {
    /// The user memory, `None` when `user_memory.db` did not open.
    pub user_memory: Option<&'a Arc<Mutex<UserMemoryRepository>>>,
    /// The chat manager, `None` when the chat subsystem did not start.
    pub chat_manager: Option<&'a ChatSessionManagerHandle>,
    /// The bus the onboarding announces its start and its completion on.
    pub events: &'a EventBusSender,
    /// The directory holding the per-agent memory databases.
    pub memory_dir: PathBuf,
}

impl Onboarding<'_> {
    fn repo(&self) -> Result<Arc<Mutex<UserMemoryRepository>>, OnboardingError> {
        self.user_memory
            .cloned()
            .ok_or(OnboardingError::RepositoryNotInitialized)
    }

    /// The full state reconstructed from the user memory; `Welcome` on first
    /// launch.
    ///
    /// # Errors
    ///
    /// [`OnboardingError::RepositoryNotInitialized`] without a user memory,
    /// [`OnboardingError::PersistenceError`] when a key cannot be read.
    pub async fn state(&self) -> Result<OnboardingState, OnboardingError> {
        let repo = self.repo()?;
        blocking(move || load_state_from_memory(&*lock(&repo)?)).await
    }

    /// Advances to `target_phase`, persists the state and, on `Done`, announces
    /// the completion. Re-entering the current phase answers the state as is.
    ///
    /// # Errors
    ///
    /// [`OnboardingError::UnknownPhase`] for a name outside the six,
    /// [`OnboardingError::InvalidTransition`] for a phase out of sequence.
    pub async fn advance(&self, target_phase: &str) -> Result<OnboardingState, OnboardingError> {
        let target = OnboardingPhase::parse(target_phase)
            .ok_or_else(|| OnboardingError::UnknownPhase(target_phase.to_owned()))?;
        let repo = self.repo()?;
        let events = self.events.clone();
        blocking(move || {
            let repo = lock(&repo)?;
            let mut onboarding = load_state_from_memory(&repo)?;
            if onboarding.phase == target {
                return Ok(onboarding);
            }
            if !onboarding.can_advance_to(&target) {
                return Err(OnboardingError::InvalidTransition {
                    from: onboarding.phase.clone(),
                    to: target,
                });
            }
            apply_phase_transition(&repo, &mut onboarding, &target);
            persist_state(&repo, &onboarding)?;
            if target == OnboardingPhase::Done {
                let _ = events.send(apollia_core::RuntimeEvent::OnboardingCompleted {
                    duration_sec: onboarding.stats.total_time_sec,
                    actions_count: onboarding.stats.actions_completed,
                });
            }
            tracing::event!(
                tracing::Level::INFO,
                phase = %target.as_str(),
                "onboarding.phase.advanced"
            );
            Ok(onboarding)
        })
        .await
    }

    /// Opens a conversation with the onboarding agent: a full run (`topic`
    /// `None`), one topic again, or, with `resume`, the enrichment of a
    /// profile already collected, which keeps what the agent learnt.
    ///
    /// # Errors
    ///
    /// [`OnboardingError::InvalidTopic`], [`OnboardingError::ChatNotAvailable`],
    /// [`OnboardingError::AgentNotInstalled`], or
    /// [`OnboardingError::SessionCreationFailed`].
    pub async fn trigger(
        &self,
        topic: Option<String>,
        resume: bool,
    ) -> Result<TriggerResult, OnboardingError> {
        if let Some(ref t) = topic {
            if !ONBOARDING_TOPICS.contains(&t.as_str()) {
                return Err(OnboardingError::InvalidTopic(t.clone()));
            }
        }

        // A fresh full run wipes the topic marks and the agent's entries of a
        // previous run, which would otherwise show the progress at 100% before
        // the first message. A resume keeps them.
        if topic.is_none() && !resume {
            if let Ok(repo) = self.repo() {
                if let Ok(repo) = repo.lock() {
                    reset_onboarding_progress(&repo, &self.memory_dir);
                } else {
                    tracing::event!(
                        tracing::Level::WARN,
                        detail = "the reset is skipped",
                        "onboarding.memory.lock.poisoned"
                    );
                }
            }
        }

        // The onboarding agent is a declarative agent: its mission is the
        // system prompt and its tool records the profile, both read when each
        // message is sent, so the session carries no prompt of its own.
        let mode = if resume {
            "resume"
        } else if topic.is_some() {
            "partial"
        } else {
            "full"
        };
        let manager = self.chat_manager.ok_or(OnboardingError::ChatNotAvailable)?;
        let info = manager
            .create_session(CreateSessionParams {
                mode: ChatMode::Agent,
                agent_name: Some(ONBOARDING_AGENT_NAME.to_string()),
                system_prompt: None,
                tools: Vec::new(),
                model: None,
            })
            .await
            .map_err(|e| match e {
                ChatError::AgentNotFound(_) => OnboardingError::AgentNotInstalled,
                other => OnboardingError::SessionCreationFailed(other.to_string()),
            })?;

        let _ = self
            .events
            .send(apollia_core::RuntimeEvent::OnboardingStarted {
                session_id: info.id.clone(),
                mode: mode.to_string(),
                topic: topic.clone(),
            });
        tracing::event!(
            tracing::Level::INFO,
            session_id = %info.id,
            mode = %mode,
            topic = ?topic,
            "onboarding.session.created"
        );
        Ok(TriggerResult {
            session_id: info.id,
            mode: mode.to_string(),
            topic,
        })
    }

    /// Marks the onboarding as skipped by the user.
    ///
    /// # Errors
    ///
    /// [`OnboardingError::RepositoryNotInitialized`] when the mark cannot be
    /// written.
    pub async fn dismiss(&self) -> Result<(), OnboardingError> {
        let repo = self.repo()?;
        blocking(move || {
            let repo = repo.lock().map_err(|e| {
                OnboardingError::SessionCreationFailed(format!("mutex poisoned: {e}"))
            })?;
            repo.set_onboarding_skipped(true)
                .map_err(|_| OnboardingError::RepositoryNotInitialized)
        })
        .await?;
        tracing::event!(tracing::Level::INFO, "onboarding.dismissed");
        Ok(())
    }

    /// Whether the onboarding agent recorded its completion.
    ///
    /// # Errors
    ///
    /// [`OnboardingError::PersistenceError`] when the blocking task fails.
    pub async fn is_finalized(&self) -> Result<bool, OnboardingError> {
        let memory_dir = self.memory_dir.clone();
        blocking(move || Ok(check_finalized_in(&memory_dir))).await
    }

    /// Records the completion of the conversation without a model turn, as
    /// the "skip the optional questions" button asks.
    ///
    /// # Errors
    ///
    /// [`OnboardingError::AgentMemory`] when the agent's memory cannot be
    /// written.
    pub async fn finalize_chat(&self) -> Result<(), OnboardingError> {
        let memory_dir = self.memory_dir.clone();
        blocking(move || finalize_in(&memory_dir)).await
    }
}

fn lock(
    repo: &Mutex<UserMemoryRepository>,
) -> Result<std::sync::MutexGuard<'_, UserMemoryRepository>, OnboardingError> {
    repo.lock()
        .map_err(|e| OnboardingError::PersistenceError(format!("mutex poisoned: {e}")))
}

async fn blocking<T, F>(work: F) -> Result<T, OnboardingError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, OnboardingError> + Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| OnboardingError::PersistenceError(format!("spawn_blocking failed: {e}")))?
}

/// Applies the in-memory side effects of advancing to `target`: `started_at`
/// on the first move out of Welcome, every topic covered once the
/// acquaintance is left, the completion stamped on `Done`.
fn apply_phase_transition(
    repo: &UserMemoryRepository,
    onboarding: &mut OnboardingState,
    target: &OnboardingPhase,
) {
    if onboarding.phase == OnboardingPhase::Welcome && onboarding.started_at.is_none() {
        onboarding.started_at = Some(chrono::Utc::now().to_rfc3339());
    }
    onboarding.phase = target.clone();
    if matches!(
        target,
        OnboardingPhase::GuidedTour | OnboardingPhase::Graduation | OnboardingPhase::Done
    ) {
        for topic in &ONBOARDING_TOPICS {
            let _ = repo.mark_topic_covered(topic);
        }
    }
    if *target == OnboardingPhase::Done {
        onboarding.completed = true;
        onboarding.completed_at = Some(chrono::Utc::now().to_rfc3339());
    }
}
