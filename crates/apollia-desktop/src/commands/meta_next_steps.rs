//! Tauri IPC command: Meta-LLM `GenerateNextSteps` routine.
//!
//! Produces up to 3 actionable "next steps" cards for:
//!   - the operator Dashboard (`context = global_context`),
//!   - the end-of-session debrief on `ChatConversation` (`context = session_end`).
//!
//! The logic lives in `apollia_runtime::chat_assists`. Embedded, it runs with
//! the live desktop router; attached, the daemon runs it with its own router
//! at `POST /api/v1/assists/next-steps`. On any LLM failure the heuristic
//! fallback kicks in and `fromLlm = false` flags the degraded path to the UI.

use apollia_llm::meta::next_steps::NextStepsRequest;
use apollia_runtime::chat_assists::{self, NextStepsReply};
use tauri::State;

use super::meta_rewrite::desktop_router;
use crate::attached::RuntimeLink;

/// The route of the next steps on the runtime.
const NEXT_STEPS: &str = "/api/v1/assists/next-steps";

async fn next_steps_on(
    link: &RuntimeLink,
    router: Option<std::sync::Arc<apollia_llm::LlmRouter>>,
    request: NextStepsRequest,
) -> Result<NextStepsReply, String> {
    match link {
        RuntimeLink::Embedded(_) => Ok(chat_assists::next_steps(router.as_ref(), request).await),
        RuntimeLink::Attached(remote) => Ok(remote.post(NEXT_STEPS, &request).await?),
    }
}

/// Generate up to 3 Next Steps cards.
///
/// Never fails while the runtime answers: if no LLM backend is configured or
/// the call errors out, the heuristic fallback is returned with
/// `fromLlm = false` so the panel always renders. Attached, a refusal of the
/// daemon comes back in its own words.
#[tauri::command]
pub async fn meta_generate_next_steps(
    link: State<'_, RuntimeLink>,
    app: tauri::AppHandle,
    request: NextStepsRequest,
) -> Result<NextStepsReply, String> {
    let router = match link.inner() {
        RuntimeLink::Embedded(_) => desktop_router(&app)?,
        RuntimeLink::Attached(_) => None,
    };
    next_steps_on(link.inner(), router, request).await
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

    #[tokio::test]
    async fn test_next_steps_reach_the_daemon_with_the_token() {
        // GIVEN a daemon answering one heuristic card
        let server = MockServer::start().await;
        let card = serde_json::json!({
            "id": "capture-note",
            "title": "Save a note",
            "description": "Keep a trace.",
            "actionButton": {
                "label": "Open Memory",
                "action": "navigate",
                "payload": { "route": "/memory?new" },
            },
        });
        Mock::given(method("POST"))
            .and(path(NEXT_STEPS))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .and(body_json(serde_json::json!({
                "context": "global_context",
                "mode": "operator",
                "facts": serde_json::to_value(NextStepsRequest::default().facts).unwrap(),
            })))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "steps": [card], "fromLlm": false })),
            )
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the attached window asks for the dashboard's next steps
        let reply = next_steps_on(&attached(&server), None, NextStepsRequest::default())
            .await
            .unwrap();

        // THEN the daemon's card comes back, flagged as heuristic
        assert!(!reply.from_llm);
        assert_eq!(reply.steps.len(), 1);
        assert_eq!(reply.steps[0].id, "capture-note");
    }

    #[tokio::test]
    async fn test_next_steps_refused_by_the_daemon_show_its_words() {
        // GIVEN a daemon refusing the request
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(NEXT_STEPS))
            .respond_with(
                ResponseTemplate::new(401)
                    .set_body_json(serde_json::json!({ "error": "invalid bearer token" })),
            )
            .mount(&server)
            .await;

        // WHEN the attached window asks for the next steps
        let refused = next_steps_on(&attached(&server), None, NextStepsRequest::default()).await;

        // THEN the refusal reads in the daemon's own words
        assert_eq!(refused.unwrap_err(), "invalid bearer token");
    }
}
