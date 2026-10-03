//! Tauri IPC command: Meta-LLM `RewriteInput` routine.
//!
//! Rewrites terse or ambiguous user input into clearer, more actionable
//! prompts before sending to an agent. Explicitly triggered by the operator
//! clicking the "Improve prompt" button in a chat composer.
//!
//! The logic lives in `apollia_runtime::chat_assists`. Embedded, it runs with
//! the live desktop router; attached, the daemon runs it with its own router
//! at `POST /api/v1/assists/rewrite`. It always returns a payload: on any LLM
//! failure, timeout, or empty response, the original text comes back with the
//! [`RewriteFallbackReason`] that says which of the four situations produced
//! it, so a reachable engine that timed out is never reported as a missing one.

use std::sync::Arc;

use apollia_llm::meta::rewrite_input::RewriteInputRequest;
use apollia_llm::LlmRouter;
use apollia_runtime::chat_assists::{self, RewriteReply};
use tauri::{Manager, State};

use crate::attached::RuntimeLink;
use crate::SharedLlmRouter;

/// The route of the rewrite on the runtime.
pub(crate) const REWRITE: &str = "/api/v1/assists/rewrite";

/// The live desktop router, read from the application rather than taken as a
/// state argument, because the attached application never builds one.
pub(crate) fn desktop_router(app: &tauri::AppHandle) -> Result<Option<Arc<LlmRouter>>, String> {
    match app.try_state::<SharedLlmRouter>() {
        Some(shared) => Ok(shared
            .read()
            .map_err(|e| format!("llm router lock poisoned: {e}"))?
            .clone()),
        None => Ok(None),
    }
}

async fn rewrite_on(
    link: &RuntimeLink,
    router: Option<Arc<LlmRouter>>,
    request: RewriteInputRequest,
) -> Result<RewriteReply, String> {
    match link {
        RuntimeLink::Embedded(_) => Ok(chat_assists::rewrite(router.as_ref(), request).await),
        RuntimeLink::Attached(remote) => Ok(remote.post(REWRITE, &request).await?),
    }
}

/// Rewrite user input to make it clearer and more actionable.
///
/// Never fails while the runtime answers: if no LLM backend is configured or
/// the call errors out, the original text is returned unchanged with the
/// fallback that names the cause. Attached, a refusal of the daemon comes back
/// in its own words.
#[tauri::command]
pub async fn meta_rewrite_input(
    link: State<'_, RuntimeLink>,
    app: tauri::AppHandle,
    request: RewriteInputRequest,
) -> Result<RewriteReply, String> {
    let router = match link.inner() {
        RuntimeLink::Embedded(_) => desktop_router(&app)?,
        RuntimeLink::Attached(_) => None,
    };
    rewrite_on(link.inner(), router, request).await
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::attached::AttachedClient;

    const TOKEN: &str = "token-of-this-installation";

    fn attached(server: &MockServer) -> RuntimeLink {
        RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap())
    }

    fn request(text: &str) -> RewriteInputRequest {
        RewriteInputRequest {
            original_text: text.into(),
            work_context: None,
        }
    }

    #[tokio::test]
    async fn test_a_rewrite_reaches_the_daemon_with_the_token() {
        // GIVEN a daemon that rewrites "fix it"
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(REWRITE))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .and(body_json(serde_json::json!({
                "originalText": "fix it",
                "workContext": null,
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rewrittenText": "Fix the failing test in the parser.",
                "fallback": null,
            })))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the attached window asks for a rewrite
        let reply = rewrite_on(&attached(&server), None, request("fix it"))
            .await
            .unwrap();

        // THEN the daemon's rewrite comes back
        assert_eq!(reply.rewritten_text, "Fix the failing test in the parser.");
        assert_eq!(reply.fallback, None);
    }

    #[tokio::test]
    async fn test_a_rewrite_refused_by_the_daemon_shows_its_words() {
        // GIVEN a daemon refusing the request
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(REWRITE))
            .respond_with(
                ResponseTemplate::new(503)
                    .set_body_json(serde_json::json!({ "error": "runtime is shutting down" })),
            )
            .mount(&server)
            .await;

        // WHEN the attached window asks for a rewrite
        let refused = rewrite_on(&attached(&server), None, request("fix it")).await;

        // THEN the refusal reads in the daemon's own words
        assert_eq!(refused.unwrap_err(), "runtime is shutting down");
    }
}
