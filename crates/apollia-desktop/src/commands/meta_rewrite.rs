//! Tauri IPC command: Meta-LLM `RewriteInput` routine.
//!
//! Rewrites terse or ambiguous user input into clearer, more actionable
//! prompts before sending to an agent. Explicitly triggered by the operator
//! clicking the "Improve prompt" button in a chat composer.
//!
//! Reuses the user's default LLM via [`SharedLlmRouter`]. Never allocates a
//! second backend and always returns a non-empty payload; on any LLM
//! failure, timeout, or empty response, the original text is returned
//! unchanged with `fromLlm = false`.

use apollia_llm::meta::rewrite_input::{rewrite_input, RewriteInputRequest, RewriteInputResponse};
use serde::Serialize;
use tauri::State;

use crate::SharedLlmRouter;

/// Response body, mirrors [`RewriteInputResponse`] so the IPC contract stays
/// decoupled from the underlying crate type.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RewriteInputIpcResponse {
    pub rewritten_text: String,
    pub from_llm: bool,
}

impl From<RewriteInputResponse> for RewriteInputIpcResponse {
    fn from(value: RewriteInputResponse) -> Self {
        Self {
            rewritten_text: value.rewritten_text,
            from_llm: value.from_llm,
        }
    }
}

/// Rewrite user input to make it clearer and more actionable.
///
/// Never fails from the caller's perspective: if no LLM backend is configured
/// or the call errors out, the original text is returned unchanged with
/// `fromLlm = false`.
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
            // THEN: no router, return original text
            RewriteInputResponse {
                rewritten_text: request.original_text,
                from_llm: false,
            }
        }
    };

    // THEN: convert to IPC response
    Ok(response.into())
}
