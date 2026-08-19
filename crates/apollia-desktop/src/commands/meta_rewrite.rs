//! Tauri IPC command: Meta-LLM `RewriteInput` routine.
//!
//! Rewrites terse or ambiguous user input into clearer, more actionable
//! prompts before sending to an agent. Explicitly triggered by the operator
//! clicking the "Improve prompt" button in a chat composer.
//!
//! Reuses the user's default LLM via [`SharedLlmRouter`]. Never allocates a
//! second backend and always returns a non-empty payload; on any LLM
//! failure, timeout, or empty response, the original text is returned
//! unchanged, carrying the [`RewriteFallbackIpc`] that says which of the four
//! situations produced it. The composer renders one message per situation, so a
//! reachable engine that timed out is never reported as a missing one.

use apollia_llm::meta::rewrite_input::{
    rewrite_input, RewriteFallback, RewriteInputRequest, RewriteInputResponse,
};
use serde::Serialize;
use tauri::State;

use crate::SharedLlmRouter;

/// Why no rewrite happened, as the composer receives it.
///
/// Mirrors [`RewriteFallback`] and adds the one situation this layer owns:
/// no router is mounted at all, which the crate below cannot observe.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RewriteFallbackIpc {
    /// No LLM router is mounted in the application state.
    NoRouter,
    /// A router is mounted and holds no backend under its default name.
    NoBackend,
    /// The backend answered with an error, or the call timed out.
    CallFailed,
    /// The backend answered, and nothing survived the reasoning strip.
    EmptyAnswer,
}

impl From<RewriteFallback> for RewriteFallbackIpc {
    fn from(value: RewriteFallback) -> Self {
        match value {
            RewriteFallback::NoBackend => Self::NoBackend,
            RewriteFallback::CallFailed => Self::CallFailed,
            RewriteFallback::EmptyAnswer => Self::EmptyAnswer,
        }
    }
}

/// Response body, mirrors [`RewriteInputResponse`] so the IPC contract stays
/// decoupled from the underlying crate type.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RewriteInputIpcResponse {
    pub rewritten_text: String,
    pub fallback: Option<RewriteFallbackIpc>,
}

impl From<RewriteInputResponse> for RewriteInputIpcResponse {
    fn from(value: RewriteInputResponse) -> Self {
        Self {
            rewritten_text: value.rewritten_text,
            fallback: value.fallback.map(RewriteFallbackIpc::from),
        }
    }
}

/// Rewrite user input to make it clearer and more actionable.
///
/// Never fails from the caller's perspective: if no LLM backend is configured
/// or the call errors out, the original text is returned unchanged with the
/// fallback that names the cause.
#[tauri::command]
pub async fn meta_rewrite_input(
    shared: State<'_, SharedLlmRouter>,
    request: RewriteInputRequest,
) -> Result<RewriteInputIpcResponse, String> {
    // GIVEN: SharedLlmRouter state
    let router_opt = {
        let guard = shared
            .read()
            .map_err(|e| format!("llm router lock poisoned: {e}"))?;
        guard.as_ref().cloned()
    };

    // WHEN: router exists or None
    let response = match router_opt {
        Some(router) => {
            // THEN: call rewrite_input
            rewrite_input(&router, request).await
        }
        None => {
            // THEN: no router at all, a situation the crate below never sees
            return Ok(RewriteInputIpcResponse {
                rewritten_text: request.original_text,
                fallback: Some(RewriteFallbackIpc::NoRouter),
            });
        }
    };

    // THEN: convert to IPC response
    Ok(response.into())
}
