//! Tauri IPC commands for the Apollia Guide meta-chat (US-SP42-057).
//!
//! Two entry points exposed to the frontend:
//!
//! - [`apollia_coach_invoke`] — the unified coach routine used by the
//!   dedicated chat surface (`/chat?agent=apollia-guide`) AND the contextual
//!   onboarding widget. Always reuses the user's configured LLM backend via
//!   [`SharedLlmRouter`]; never opens a second model or a cloud call beyond
//!   what the user already consented to (ADR-073).
//! - [`apollia_guide_bootstrap`] — a no-op on subsequent calls ; returns
//!   basic metadata about the bundled Apollia Guide agent. Kept separate
//!   from the agents command surface so the frontend sidebar entry can
//!   probe readiness without listing every installed agent.
//!
//! The heavy lifting lives in [`apollia_llm::meta::apollia_coach`] —
//! this module is strictly a thin IPC adapter.

use apollia_llm::meta::apollia_coach::{
    invoke_apollia_coach, ActionButton, CoachContext, CoachMode, CoachResponse, CoachTurn,
};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::SharedLlmRouter;

/// Request body for [`apollia_coach_invoke`].
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoachInvokeRequest {
    /// `operator`, `builder`, or `onboarding`.
    #[serde(default)]
    pub mode: CoachMode,
    /// Live context used to tailor the reply.
    #[serde(default)]
    pub context: CoachContext,
    /// Past conversation turns — capped server-side to the last 12.
    #[serde(default)]
    pub history: Vec<CoachTurn>,
    /// The message the user just typed.
    pub user_message: String,
}

/// Response returned to the frontend.
///
/// Mirrors [`CoachResponse`] one-to-one. A dedicated type is kept here to
/// keep the IPC surface decoupled from the underlying crate.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoachInvokeResponse {
    pub text: String,
    pub action_buttons: Vec<ActionButton>,
}

impl From<CoachResponse> for CoachInvokeResponse {
    fn from(value: CoachResponse) -> Self {
        Self {
            text: value.text,
            action_buttons: value.action_buttons,
        }
    }
}

/// Invoke the Apollia Guide coach.
///
/// Always routes through the user's configured default LLM backend. If no
/// backend is configured, returns `Err(...)` so the frontend can prompt the
/// user to complete LLM setup first.
#[tauri::command]
pub async fn apollia_coach_invoke(
    shared: State<'_, SharedLlmRouter>,
    request: CoachInvokeRequest,
) -> Result<CoachInvokeResponse, String> {
    let router = {
        let guard = shared
            .read()
            .map_err(|e| format!("llm router lock poisoned: {e}"))?;
        guard
            .as_ref()
            .cloned()
            .ok_or_else(|| "LLM backend not configured — open Settings → LLM".to_string())?
    };

    let response = invoke_apollia_coach(
        &router,
        request.mode,
        request.context,
        request.history,
        request.user_message,
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(response.into())
}

/// Summary surfaced by [`apollia_guide_bootstrap`] so the sidebar entry can
/// render instantly without querying the full agent list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApolliaGuideStatus {
    /// Stable identifier — always `"apollia-guide"`.
    pub agent_id: String,
    /// Bundled version pinned in `agents/system/apollia-guide/manifest.toml`.
    pub version: String,
    /// `true` when a default LLM backend is configured. The sidebar entry is
    /// always present; when `false` the UI steers the user to Settings → LLM
    /// the first time they open the coach.
    pub llm_ready: bool,
}

/// Bundled Apollia Guide version — kept in sync with the manifest at
/// `agents/system/apollia-guide/manifest.toml`. Update both in lockstep.
const APOLLIA_GUIDE_VERSION: &str = "0.1.0";

/// Lightweight readiness probe for the sidebar Apollia entry.
///
/// Returns basic metadata about the bundled Apollia Guide agent plus a flag
/// indicating whether the user's LLM is ready. Idempotent and cheap — safe
/// to call on every sidebar mount.
#[tauri::command]
pub async fn apollia_guide_bootstrap(
    shared: State<'_, SharedLlmRouter>,
) -> Result<ApolliaGuideStatus, String> {
    let llm_ready = shared.read().map(|g| g.is_some()).unwrap_or(false);

    Ok(ApolliaGuideStatus {
        agent_id: "apollia-guide".to_string(),
        version: APOLLIA_GUIDE_VERSION.to_string(),
        llm_ready,
    })
}
