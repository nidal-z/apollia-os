//! Tauri IPC command — Meta-LLM `GenerateNextSteps` routine.
//!
//! Produces up to 3 actionable "next steps" cards for:
//!   - the operator Dashboard (`context = global_context`),
//!   - the end-of-session debrief on `ChatConversation` (`context = session_end`).
//!
//! Reuses the user's default LLM via [`SharedLlmRouter`]. Never allocates a
//! second backend and always returns a non-empty payload — on any LLM
//! failure the heuristic fallback kicks in and `fromLlm = false` flags the
//! degraded path to the UI.

use apollia_llm::meta::next_steps::{
    generate_next_steps, heuristic_fallback, NextStep, NextStepsRequest, NextStepsResponse,
};
use serde::Serialize;
use tauri::State;

use crate::SharedLlmRouter;

/// Response body — mirrors [`NextStepsResponse`] so the IPC contract stays
/// decoupled from the underlying crate type.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextStepsIpcResponse {
    pub steps: Vec<NextStep>,
    pub from_llm: bool,
}

impl From<NextStepsResponse> for NextStepsIpcResponse {
    fn from(value: NextStepsResponse) -> Self {
        Self {
            steps: value.steps,
            from_llm: value.from_llm,
        }
    }
}

/// Generate up to 3 Next Steps cards.
///
/// Never fails from the caller's perspective: if no LLM backend is configured
/// or the call errors out, the heuristic fallback is returned with
/// `fromLlm = false` so the panel always renders.
#[tauri::command]
pub async fn meta_generate_next_steps(
    shared: State<'_, SharedLlmRouter>,
    request: NextStepsRequest,
) -> Result<NextStepsIpcResponse, String> {
    let router_opt = {
        let guard = shared
            .read()
            .map_err(|e| format!("llm router lock poisoned: {e}"))?;
        guard.as_ref().cloned()
    };

    let response = match router_opt {
        Some(router) => generate_next_steps(&router, request).await,
        None => NextStepsResponse {
            steps: heuristic_fallback(&request),
            from_llm: false,
        },
    };
    Ok(response.into())
}
