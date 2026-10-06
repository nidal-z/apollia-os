use std::pin::Pin;
use std::sync::Arc;

use apollia_llm::LlmRouter;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::super::tests::{chat_state, MockBackend};
use crate::api::server::{APIServer, AppState};

/// A model that answers the same text to every request.
struct FixedAnswerModel {
    answer: String,
}

#[async_trait::async_trait]
impl apollia_llm::CompletionModel for FixedAnswerModel {
    async fn complete(
        &self,
        _req: apollia_llm::CompletionRequest,
    ) -> Result<apollia_llm::CompletionResponse, apollia_llm::LlmError> {
        Ok(apollia_llm::CompletionResponse {
            content: self.answer.clone(),
            tool_calls: Vec::new(),
            usage: apollia_llm::TokenUsage::default(),
            finish_reason: apollia_llm::FinishReason::Stop,
            latency_ms: 1,
            ttft_ms: None,
            engine_timings: None,
        })
    }

    async fn stream(
        &self,
        _req: apollia_llm::CompletionRequest,
    ) -> Result<
        Pin<
            Box<
                dyn futures::Stream<Item = Result<apollia_llm::StreamChunk, apollia_llm::LlmError>>
                    + Send,
            >,
        >,
        apollia_llm::LlmError,
    > {
        Err(apollia_llm::LlmError::InferenceError("not used".into()))
    }

    fn is_available(&self) -> bool {
        true
    }

    fn backend_name(&self) -> &str {
        "fixed"
    }

    fn model_id(&self) -> &str {
        "fixed-model"
    }
}

async fn call(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    let payload = match body {
        Some(body) => {
            request = request.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&body).unwrap())
        }
        None => Body::empty(),
    };
    let response = router
        .clone()
        .oneshot(request.body(payload).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

async fn router_with_session(state: AppState<MockBackend>) -> (axum::Router, String) {
    let router = APIServer::build_router_for_test(state);
    let (status, created) = call(
        &router,
        "POST",
        "/api/v1/sessions",
        Some(json!({ "mode": "libre" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id = created["id"].as_str().unwrap().to_owned();
    (router, id)
}

#[tokio::test]
async fn test_patch_session_changes_its_instructions() {
    // GIVEN a runtime holding one session
    let dir = tempfile::tempdir().unwrap();
    let (router, id) = router_with_session(chat_state(&dir)).await;

    // WHEN its instructions are patched
    let (status, _) = call(
        &router,
        "PATCH",
        &format!("/api/v1/sessions/{id}"),
        Some(json!({ "system_prompt": "Answer in three lines." })),
    )
    .await;

    // THEN the detail carries the new instructions, in the settings of the
    // conversation
    assert_eq!(status, StatusCode::OK);
    let (_, detail) = call(&router, "GET", &format!("/api/v1/sessions/{id}"), None).await;
    assert_eq!(
        detail["session"]["settings"]["instructions"], "Answer in three lines.",
        "{detail}"
    );
}

#[tokio::test]
async fn test_patch_of_an_unknown_session_is_a_404() {
    // GIVEN a runtime holding no session
    let dir = tempfile::tempdir().unwrap();
    let router = APIServer::build_router_for_test(chat_state(&dir));

    // WHEN a session nobody created is patched
    let (status, body) = call(
        &router,
        "PATCH",
        "/api/v1/sessions/ghost",
        Some(json!({ "tools": [] })),
    )
    .await;

    // THEN the refusal names it
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].as_str().unwrap().contains("ghost"), "{body}");
}

#[tokio::test]
async fn test_put_title_stores_the_trimmed_title() {
    // GIVEN a runtime holding one session
    let dir = tempfile::tempdir().unwrap();
    let (router, id) = router_with_session(chat_state(&dir)).await;

    // WHEN the user's title is put, padded with spaces
    let (status, _) = call(
        &router,
        "PUT",
        &format!("/api/v1/sessions/{id}/title"),
        Some(json!({ "title": "  Migration plan  " })),
    )
    .await;

    // THEN the list shows it trimmed
    assert_eq!(status, StatusCode::OK);
    let (_, sessions) = call(&router, "GET", "/api/v1/sessions", None).await;
    assert_eq!(sessions[0]["title"], "Migration plan", "{sessions}");
}

#[tokio::test]
async fn test_a_blank_title_is_refused_with_the_embedded_message() {
    // GIVEN a runtime holding one session
    let dir = tempfile::tempdir().unwrap();
    let (router, id) = router_with_session(chat_state(&dir)).await;

    // WHEN a blank title is put
    let (status, body) = call(
        &router,
        "PUT",
        &format!("/api/v1/sessions/{id}/title"),
        Some(json!({ "title": "   " })),
    )
    .await;

    // THEN it is a bad request carrying the message the embedded mode shows
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "title must not be empty");
}

#[tokio::test]
async fn test_generate_title_names_the_session_with_the_model_answer() {
    // GIVEN a runtime holding one session, whose model answers a quoted title
    let dir = tempfile::tempdir().unwrap();
    let state = chat_state(&dir);
    let model: Arc<dyn apollia_llm::CompletionModel> = Arc::new(FixedAnswerModel {
        answer: "\"CV writing help.\"".to_owned(),
    });
    let backends = std::collections::HashMap::from([("fixed".to_owned(), model)]);
    *state.llm_router.write().await = Some(Arc::new(LlmRouter::with_backends(backends, "fixed")));
    let (router, id) = router_with_session(state).await;

    // WHEN a title is generated from the first message
    let (status, body) = call(
        &router,
        "POST",
        &format!("/api/v1/sessions/{id}/title/generate"),
        Some(json!({ "first_message": "Help me write my CV" })),
    )
    .await;

    // THEN the sanitised title comes back and is stored on the session
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["title"], "CV writing help");
    let (_, sessions) = call(&router, "GET", "/api/v1/sessions", None).await;
    assert_eq!(sessions[0]["title"], "CV writing help");
}

#[tokio::test]
async fn test_generate_title_without_a_model_is_a_503() {
    // GIVEN a runtime holding one session and no inference server
    let dir = tempfile::tempdir().unwrap();
    let state = chat_state(&dir);
    *state.llm_router.write().await = None;
    let (router, id) = router_with_session(state).await;

    // WHEN a title is asked for
    let (status, body) = call(
        &router,
        "POST",
        &format!("/api/v1/sessions/{id}/title/generate"),
        Some(json!({ "first_message": "Hello" })),
    )
    .await;

    // THEN the refusal says no model is configured
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "no LLM router configured");
}

#[tokio::test]
async fn test_erase_removes_the_session_and_its_history() {
    // GIVEN a runtime holding one session
    let dir = tempfile::tempdir().unwrap();
    let (router, id) = router_with_session(chat_state(&dir)).await;

    // WHEN it is erased
    let (status, _) = call(
        &router,
        "POST",
        &format!("/api/v1/sessions/{id}/erase"),
        None,
    )
    .await;

    // THEN it is gone, not closed
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(&router, "GET", &format!("/api/v1/sessions/{id}"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_erase_of_an_unknown_session_is_a_404() {
    // GIVEN a runtime holding no session
    let dir = tempfile::tempdir().unwrap();
    let router = APIServer::build_router_for_test(chat_state(&dir));

    // WHEN a session nobody created is erased
    let (status, _) = call(&router, "POST", "/api/v1/sessions/ghost/erase", None).await;

    // THEN the runtime says it does not know it
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_a_session_can_no_longer_be_linked_to_a_project() {
    // GIVEN a runtime holding one session
    let dir = tempfile::tempdir().unwrap();
    let (router, id) = router_with_session(chat_state(&dir)).await;

    // WHEN a client creates a session naming a project, links one to a
    // project, or lists the sessions of a project
    let (created, session) = call(
        &router,
        "POST",
        "/api/v1/sessions",
        Some(json!({ "mode": "libre", "project_id": "p-1" })),
    )
    .await;
    let uri = format!("/api/v1/sessions/{id}/project");
    let (linked, _) = call(&router, "PUT", &uri, Some(json!({ "project_id": "p-1" }))).await;
    let (listed, _) = call(&router, "GET", "/api/v1/projects/p-1/sessions", None).await;

    // THEN the session is created without a project, and neither route
    // exists any more
    assert_eq!(created, StatusCode::CREATED, "{session}");
    assert!(session.get("project_id").is_none(), "{session}");
    assert!(linked.is_client_error(), "{linked}");
    assert_eq!(listed, StatusCode::NOT_FOUND);
}
