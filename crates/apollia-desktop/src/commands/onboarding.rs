//! Tauri IPC commands for onboarding lifecycle management.
//!
//! The state machine (6 phases, persisted in the `onboarding_*` keys of the
//! user memory) and the chat-based onboarding live in
//! `apollia_runtime::onboarding`, which serves both modes: embedded, these
//! commands call it in this process; attached to a daemon, [`remote`] reaches
//! the same functions over `/api/v1/onboarding`.
//!
//! - [`get_onboarding_state`] - full machine state (new)
//! - [`advance_onboarding_phase`] - validated phase transition (new)
//! - [`trigger_onboarding`] - creates an agent-backed chat session
//! - [`dismiss_onboarding`] - marks onboarding as skipped
//!
//! All data stays local (Principle #1). Structs are serde-typed for the
//! frontend (Principle #8). Invalid transitions are rejected immediately
//! (Principle #4).

use apollia_runtime::embedded::RuntimeHandle;
use apollia_runtime::onboarding::state::TriggerResult;
use apollia_runtime::onboarding::Onboarding;
use tauri::State;

use crate::attached::RuntimeLink;

/// The model-detection step in `ai_setup`, the in-app companion in
/// `companion`, the attached arm of the commands below in `remote`.
pub mod ai_setup;
pub mod companion;
pub(crate) mod remote;

/// The phase machine types, which the runtime defines and serves.
pub mod state {
    pub use apollia_runtime::onboarding::state::OnboardingState;
}

use state::OnboardingState;

/// The onboarding of the runtime running in this process.
fn embedded(state: &RuntimeHandle) -> Onboarding<'_> {
    Onboarding {
        user_memory: state.user_memory.as_ref(),
        chat_manager: state.chat_manager.as_ref(),
        events: &state.event_sender,
        memory_dir: onboarding_memory_dir(),
    }
}

/// Directory holding the per-agent memory databases (`~/.apollia/memory`).
fn onboarding_memory_dir() -> std::path::PathBuf {
    apollia_core::paths::data_dir_under(apollia_core::paths::home_dir_or_temp()).join("memory")
}

/// Logs a refusal under `event` and turns it into the text the interface
/// shows.
fn shown(event: &'static str) -> impl FnOnce(String) -> String {
    move |error| {
        tracing::event!(tracing::Level::ERROR, error = %error, event = event, "onboarding.command.failed");
        error
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
pub async fn get_onboarding_state(link: State<'_, RuntimeLink>) -> Result<OnboardingState, String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => embedded(state).state().await.map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => remote::state(remote).await,
    }
    .map_err(shown("onboarding.state.read.failed"))
}

/// Checks whether the onboarding-agent has written `onboarding.completed_at`
/// in its semantic memory.
///
/// Used by the desktop's onboarding chat step to detect that the agent has
/// finished its calibration and finalized the user profile, so the modal can
/// switch to the wrap-up screen and offer the "Terminer" CTA.
#[tauri::command]
pub async fn check_onboarding_finalized(link: State<'_, RuntimeLink>) -> Result<bool, String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => embedded(state)
            .is_finalized()
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => remote::is_finalized(remote).await,
    }
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
pub async fn finalize_onboarding_chat(link: State<'_, RuntimeLink>) -> Result<(), String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => embedded(state)
            .finalize_chat()
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => remote::finalize(remote).await,
    }
}

/// Attempts to advance the onboarding flow to `target_phase`.
///
/// The transition is rejected if it does not follow the legal sequential
/// chain. On success, the new state is persisted and, when the phase reaches
/// `Done`, a `RuntimeEvent::OnboardingCompleted` is emitted on the EventBus.
#[tauri::command]
pub async fn advance_onboarding_phase(
    target_phase: String,
    link: State<'_, RuntimeLink>,
) -> Result<OnboardingState, String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => embedded(state)
            .advance(&target_phase)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => remote::advance(remote, &target_phase).await,
    }
    .map_err(shown("onboarding.phase.advance.failed"))
}

// ---------------------------------------------------------------------------
// Tauri commands - legacy
// ---------------------------------------------------------------------------

/// Triggers a full or partial onboarding session.
///
/// Pass `topic = None` for a full onboarding (all 5 topics).
/// Pass `topic = Some("preferences")` for a single-topic re-run.
#[tauri::command]
pub async fn trigger_onboarding(
    topic: Option<String>,
    link: State<'_, RuntimeLink>,
) -> Result<TriggerResult, String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => embedded(state)
            .trigger(topic, false)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => remote::trigger(remote, topic).await,
    }
    .map_err(shown("onboarding.trigger.failed"))
}

/// Resumes onboarding for profile enrichment without wiping collected facts.
///
/// Unlike `trigger_onboarding` (which resets progress for a fresh run), this
/// keeps the already-collected Tier 1 + Tier 2 profile so the agent continues
/// the optional enrichment where it left off. Used by the "complete your
/// profile" entry point after a user finished calibration but skipped Tier 2.
#[tauri::command]
pub async fn resume_onboarding(link: State<'_, RuntimeLink>) -> Result<TriggerResult, String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => embedded(state)
            .trigger(None, true)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => remote::resume(remote).await,
    }
    .map_err(shown("onboarding.resume.failed"))
}

/// Marks the onboarding as dismissed (skipped) by the user.
#[tauri::command]
pub async fn dismiss_onboarding(link: State<'_, RuntimeLink>) -> Result<(), String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => embedded(state).dismiss().await.map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => remote::dismiss(remote).await,
    }
    .map_err(shown("onboarding.dismiss.failed"))
}
