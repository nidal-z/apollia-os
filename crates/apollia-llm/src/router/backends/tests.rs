//! A configured remote backend, from its row to the request its server receives.

use super::*;
use apollia_core::{LlmBackendConfig, LlmProvider};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::types::ChatMessage;

fn completion_body() -> serde_json::Value {
    serde_json::json!({
        "id": "chatcmpl-1",
        "object": "chat.completion",
        "created": 1,
        "model": "qwen3:8b",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": "ok" },
            "finish_reason": "stop"
        }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
    })
}

fn row(provider: LlmProvider, config_json: serde_json::Value) -> LlmBackendConfig {
    LlmBackendConfig {
        name: "row".into(),
        provider,
        model: "qwen3:8b".into(),
        config_json,
        enabled: true,
        is_default: true,
    }
}

/// A server answering chat completions on the OpenAI path, and `/api/show`
/// with `show` when given.
async fn server(show: Option<serde_json::Value>) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(completion_body()))
        .mount(&server)
        .await;
    if let Some(show) = show {
        Mock::given(method("POST"))
            .and(path("/api/show"))
            .respond_with(ResponseTemplate::new(200).set_body_json(show))
            .mount(&server)
            .await;
    }
    server
}

async fn complete_once(cfg: &LlmBackendConfig) -> Arc<dyn CompletionModel> {
    let backend = instantiate_from_config(cfg, CancellationToken::new())
        .await
        .unwrap();
    backend
        .complete(CompletionRequest {
            messages: vec![ChatMessage::user("hi")],
            max_tokens: Some(16),
            ..Default::default()
        })
        .await
        .unwrap();
    backend
}

// GIVEN an `ollama` row pointing at a server that records what it receives
// WHEN a completion goes through it
// THEN the server receives a plain OpenAI chat completion, with no `options`
//      object, the native field through which a client chose the server's
//      context window
#[tokio::test]
async fn test_an_ollama_row_sends_no_context_window_to_its_server() {
    // GIVEN
    let server = server(None).await;
    let cfg = row(
        LlmProvider::Ollama,
        serde_json::json!({ "base_url": format!("{}/v1", server.uri()) }),
    );

    // WHEN
    complete_once(&cfg).await;

    // THEN
    let received = server.received_requests().await.unwrap();
    let chat: Vec<_> = received
        .iter()
        .filter(|r| r.url.path().ends_with("/chat/completions"))
        .collect();
    assert_eq!(chat.len(), 1, "one chat completion reached the server");
    let body: serde_json::Value = serde_json::from_slice(&chat[0].body).unwrap();
    assert!(body.get("options").is_none(), "{body}");
    assert_eq!(body["model"], "qwen3:8b");
    assert!(body.get("messages").is_some(), "{body}");
}

// GIVEN an existing `ollama` row stored with an address without `/v1`
// WHEN a completion goes through it
// THEN it reaches `/v1/chat/completions` on the same host
#[tokio::test]
async fn test_an_existing_ollama_row_stays_routable_on_the_same_host() {
    // GIVEN
    let server = server(None).await;
    let cfg = row(
        LlmProvider::Ollama,
        serde_json::json!({ "base_url": server.uri() }),
    );

    // WHEN
    complete_once(&cfg).await;

    // THEN
    let received = server.received_requests().await.unwrap();
    assert!(
        received
            .iter()
            .any(|r| r.method.as_str() == "POST" && r.url.path() == "/v1/chat/completions"),
        "paths received: {:?}",
        received
            .iter()
            .map(|r| r.url.path().to_string())
            .collect::<Vec<_>>()
    );
}

// GIVEN an `ollama` row with a configured window, on a server whose
//       `/api/show` says the model is served with 8192 tokens
// WHEN the backend is instantiated
// THEN it declares the window read on the server, not the configured one
#[tokio::test]
async fn test_an_ollama_backend_declares_the_window_its_server_shows() {
    // GIVEN
    let server = server(Some(serde_json::json!({
        "parameters": "num_ctx 8192",
        "model_info": { "qwen3.context_length": 40_960 }
    })))
    .await;
    let cfg = row(
        LlmProvider::Ollama,
        serde_json::json!({ "base_url": format!("{}/v1", server.uri()), "context_window": 65_536 }),
    );

    // WHEN
    let backend = complete_once(&cfg).await;

    // THEN
    assert_eq!(backend.context_window(), Some(8192));
}

// GIVEN an OpenAI-compatible row on a server whose `/v1/models` gives the
//       model's `max_model_len`, and the same row on a server that gives none
// WHEN each backend is instantiated
// THEN the first declares the listed window, the second keeps the configured one
#[tokio::test]
async fn test_another_server_declares_its_listed_window_or_keeps_the_configured_one() {
    // GIVEN
    let listing = |entry: serde_json::Value| async move {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "object": "list", "data": [entry] })),
            )
            .mount(&server)
            .await;
        server
    };
    let vllm = listing(serde_json::json!({ "id": "qwen3:8b", "max_model_len": 131_072 })).await;
    let silent = listing(serde_json::json!({ "id": "qwen3:8b" })).await;
    let configured = |server: &MockServer| {
        row(
            LlmProvider::OpenAi,
            serde_json::json!({ "base_url": server.uri(), "context_window": 16_384 }),
        )
    };

    // WHEN
    let read = instantiate_from_config(&configured(&vllm), CancellationToken::new())
        .await
        .unwrap();
    let kept = instantiate_from_config(&configured(&silent), CancellationToken::new())
        .await
        .unwrap();

    // THEN
    assert_eq!(read.context_window(), Some(131_072));
    assert_eq!(kept.context_window(), Some(16_384));
}
