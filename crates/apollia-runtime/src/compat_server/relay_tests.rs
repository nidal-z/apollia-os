//! The relay of the compatible server, against a fake engine.
//!
//! The fake engine is an HTTP server on a port the system chose; a stand-in
//! supervisor holds a resident model whose process answers on that port, so a
//! request goes through the whole path: registry identifier, lease, `model`
//! replaced, body relayed, answer passed back.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{Request, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Router;
use futures::StreamExt;
use serde_json::{json, Value};
use tempfile::TempDir;
use tower::ServiceExt;

use super::models::LocalModels;
use super::relay::{Endpoint, RelayState};
use super::router::compat_router;
use super::tokens::ServerTokensHandle;
use crate::engine_store::Engine;
use crate::llama_server::{LlamaServerSupervisor, ModelKind};

/// What the fake engine received: the path and the JSON body of each request.
type Received = Arc<Mutex<Vec<(String, Value)>>>;

/// The chunks the fake engine streams, in order.
const CHUNKS: [&str; 3] = [
    "data: {\"n\":1}\n\n",
    "data: {\"n\":2}\n\n",
    "data: [DONE]\n\n",
];

/// Answers every request: an event stream when the body asks for one, else a
/// JSON body echoing the path and what was received.
async fn fake_answer(State(received): State<Received>, uri: Uri, body: Bytes) -> Response {
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    received
        .lock()
        .unwrap()
        .push((uri.path().to_owned(), body.clone()));
    if body.get("stream") == Some(&Value::Bool(true)) {
        let chunks = futures::stream::iter(CHUNKS).then(|chunk| async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            Ok::<_, std::io::Error>(Bytes::from_static(chunk.as_bytes()))
        });
        return (
            [("content-type", "text/event-stream")],
            Body::from_stream(chunks),
        )
            .into_response();
    }
    axum::Json(json!({ "path": uri.path(), "received": body })).into_response()
}

/// Starts the fake engine and returns its port and what it received.
async fn fake_engine() -> (u16, Received) {
    let received: Received = Arc::default();
    let app = Router::new()
        .fallback(fake_answer)
        .with_state(received.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (port, received)
}

/// A models folder whose registry names `/models/chat.gguf` `acme/chat` and
/// `/models/vectors.gguf` `acme/vectors`.
fn registry() -> TempDir {
    let dir = TempDir::new().unwrap();
    let entry = |id: &str, path: &str, category: &str| {
        json!({
            "id": id, "format": "gguf", "path": path, "size_bytes": 1,
            "origin": { "kind": "import" }, "category": category
        })
    };
    let index = json!({
        "version": 1,
        "models": [
            entry("acme/chat", "/models/chat.gguf", "language"),
            entry("acme/vectors", "/models/vectors.gguf", "embeddings"),
        ]
    });
    std::fs::write(dir.path().join("index.json"), index.to_string()).unwrap();
    dir
}

/// A relay over a chat model and an embeddings instance, both answered by
/// the fake engine.
struct Relay {
    _registry: TempDir,
    router: Router,
    supervisor: Arc<LlamaServerSupervisor>,
    received: Received,
}

async fn relay_with(tokens: Option<ServerTokensHandle>) -> Relay {
    let (port, received) = fake_engine().await;
    let supervisor = LlamaServerSupervisor::with_resident_for_test(
        &[("/models/chat.gguf", port)],
        Some(("/models/vectors.gguf", port)),
    )
    .await;
    let registry = registry();
    let models = LocalModels::new(
        Some(supervisor.clone()),
        None,
        Some(registry.path().to_path_buf()),
    );
    Relay {
        _registry: registry,
        router: compat_router(tokens, RelayState::new(models).unwrap()),
        supervisor,
        received,
    }
}

async fn send(router: &Router, method: &str, path: &str, body: Value) -> Response {
    send_with(router, method, path, body, &[]).await
}

async fn send_with(
    router: &Router,
    method: &str,
    path: &str,
    body: Value,
    headers: &[(&str, &str)],
) -> Response {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    router
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

async fn json_of(response: Response) -> (StatusCode, Value) {
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn test_each_endpoint_relays_its_body_with_the_model_replaced() {
    // GIVEN a chat model and an embeddings instance, named in the registry
    let relay = relay_with(None).await;

    // WHEN each endpoint is called on the model that serves it
    let calls = [
        ("/v1/chat/completions", "acme/chat"),
        ("/v1/responses", "acme/chat"),
        ("/v1/messages", "acme/chat"),
        ("/v1/messages/count_tokens", "acme/chat"),
        ("/v1/embeddings", "acme/vectors"),
    ];
    for (path, model) in calls {
        let (status, body) = json_of(
            send(
                &relay.router,
                "POST",
                path,
                json!({ "model": model, "input": "hi", "extra": [1, 2] }),
            )
            .await,
        )
        .await;

        // THEN the engine answered on the same path, with the body unchanged
        // but for `model`, set to what llama-server expects
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
        assert_eq!(body["path"], path);
        assert_eq!(body["received"]["input"], "hi");
        assert_eq!(body["received"]["extra"], json!([1, 2]));
        assert_eq!(body["received"]["model"], model);
    }
    assert_eq!(relay.received.lock().unwrap().len(), calls.len());
}

#[tokio::test]
async fn test_a_stream_arrives_chunk_by_chunk_in_order_and_holds_the_model() {
    // GIVEN a chat model whose engine streams three chunks
    let relay = relay_with(None).await;

    // WHEN a streamed chat completion is asked for
    let response = send(
        &relay.router,
        "POST",
        "/v1/chat/completions",
        json!({ "model": "acme/chat", "stream": true, "messages": [] }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "text/event-stream",
        "the stream keeps its type"
    );
    let mut stream = response.into_body().into_data_stream();
    let first = stream.next().await.unwrap().unwrap();

    // THEN the model cannot be unloaded while the stream runs
    assert!(relay
        .supervisor
        .unload_model("/models/chat.gguf")
        .await
        .is_err());

    // AND the chunks arrive in the order the engine sent them
    let mut received = first.to_vec();
    while let Some(chunk) = stream.next().await {
        received.extend_from_slice(&chunk.unwrap());
    }
    assert_eq!(String::from_utf8(received).unwrap(), CHUNKS.concat());

    // AND once the stream is dropped, the model is free again
    drop(stream);
    let held = relay.supervisor.loaded_models().await;
    assert!(held.iter().all(|m| m.active_requests == 0), "{held:?}");
}

#[tokio::test]
async fn test_a_model_that_is_not_loaded_answers_404_model_not_loaded() {
    // GIVEN a relay whose resident models do not include the one asked for,
    // nor a model of a remote backend
    let relay = relay_with(None).await;

    for (path, model) in [
        ("/v1/chat/completions", "acme/absent"),
        ("/v1/chat/completions", "gpt-4o"),
        ("/v1/messages", "claude-sonnet-4-5"),
    ] {
        // WHEN it is asked for
        let (status, body) =
            json_of(send(&relay.router, "POST", path, json!({ "model": model })).await).await;

        // THEN 404 model_not_loaded, in the format of the endpoint, and the
        // engine never saw it
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        if path == "/v1/messages" {
            assert_eq!(body["type"], "error");
            assert_eq!(body["error"]["type"], "not_found_error");
            assert!(body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("not loaded"));
        } else {
            assert_eq!(body["error"]["code"], "model_not_loaded");
            assert_eq!(body["error"]["type"], "invalid_request_error");
        }
    }
    assert!(relay.received.lock().unwrap().is_empty());
}

#[tokio::test]
async fn test_an_endpoint_the_engine_does_not_serve_answers_a_readable_404() {
    // GIVEN a chat model and an embeddings instance
    let relay = relay_with(None).await;

    // WHEN embeddings are asked of the chat model, and a message of the
    // embeddings instance
    let (vectors_status, vectors) = json_of(
        send(
            &relay.router,
            "POST",
            "/v1/embeddings",
            json!({ "model": "acme/chat", "input": "hi" }),
        )
        .await,
    )
    .await;
    let (message_status, message) = json_of(
        send(
            &relay.router,
            "POST",
            "/v1/messages",
            json!({ "model": "acme/vectors", "messages": [] }),
        )
        .await,
    )
    .await;

    // THEN each answers 404 in its family's format, naming what is missing,
    // and nothing reached the engine
    assert_eq!(vectors_status, StatusCode::NOT_FOUND);
    assert_eq!(vectors["error"]["code"], "endpoint_not_served");
    assert!(vectors["error"]["message"]
        .as_str()
        .unwrap()
        .contains("/v1/embeddings"));
    assert_eq!(message_status, StatusCode::NOT_FOUND);
    assert_eq!(message["type"], "error");
    assert_eq!(message["error"]["type"], "not_found_error");
    assert!(relay.received.lock().unwrap().is_empty());
}

#[test]
fn test_the_served_endpoints_follow_the_engine_and_the_kind() {
    // GIVEN the measured table of what each engine serves
    // WHEN each pair is read
    // THEN llama-server serves the four chat endpoints for a chat model and
    // only embeddings for its embeddings instance; vllm-mlx does not count
    // tokens
    let chat = ModelKind::Chat;
    let vectors = ModelKind::Embeddings;
    assert!(Endpoint::CountTokens.served_by(Engine::LlamaServer, chat));
    assert!(!Endpoint::Embeddings.served_by(Engine::LlamaServer, chat));
    assert!(Endpoint::Embeddings.served_by(Engine::LlamaServer, vectors));
    assert!(!Endpoint::ChatCompletions.served_by(Engine::LlamaServer, vectors));
    assert!(Endpoint::Responses.served_by(Engine::VllmMlx, chat));
    assert!(!Endpoint::CountTokens.served_by(Engine::VllmMlx, chat));
}

#[tokio::test]
async fn test_the_model_list_names_each_loaded_model_by_its_registry_id_and_type() {
    // GIVEN a chat model and an embeddings instance, named in the registry
    let relay = relay_with(None).await;

    // WHEN the models are listed
    let (status, body) = json_of(send(&relay.router, "GET", "/v1/models", Value::Null).await).await;

    // THEN the OpenAI list carries both, by registry id, with their type
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["object"], "list");
    let mut listed: Vec<(String, String, String)> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            assert!(m["created"].as_i64().unwrap() > 0);
            (
                m["id"].as_str().unwrap().to_owned(),
                m["type"].as_str().unwrap().to_owned(),
                m["object"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    listed.sort();
    assert_eq!(
        listed,
        vec![
            ("acme/chat".into(), "llm".into(), "model".into()),
            ("acme/vectors".into(), "embeddings".into(), "model".into()),
        ]
    );
}

#[tokio::test]
async fn test_a_body_without_a_model_is_refused_in_the_endpoint_format() {
    // GIVEN a relay
    let relay = relay_with(None).await;

    // WHEN a body names no model, on each family
    let (openai_status, openai) = json_of(
        send(
            &relay.router,
            "POST",
            "/v1/responses",
            json!({ "input": "hi" }),
        )
        .await,
    )
    .await;
    let (anthropic_status, anthropic) = json_of(
        send(
            &relay.router,
            "POST",
            "/v1/messages/count_tokens",
            json!([1]),
        )
        .await,
    )
    .await;

    // THEN 400 in OpenAI's envelope, and in Anthropic's
    assert_eq!(openai_status, StatusCode::BAD_REQUEST);
    assert_eq!(openai["error"]["type"], "invalid_request_error");
    assert_eq!(anthropic_status, StatusCode::BAD_REQUEST);
    assert_eq!(anthropic["type"], "error");
    assert_eq!(anthropic["error"]["type"], "invalid_request_error");
}

#[tokio::test]
async fn test_every_route_asks_for_a_token_when_the_server_requires_one() {
    // GIVEN a relay behind the token check, and one token in use
    let db = TempDir::new().unwrap();
    let tokens = ServerTokensHandle::spawn(db.path().join("system.db"));
    let token = tokens.create("editor".to_owned()).await.unwrap().value;
    let relay = relay_with(Some(tokens)).await;
    let bearer = format!("Bearer {token}");

    for (method, path) in [
        ("GET", "/v1/models"),
        ("POST", "/v1/chat/completions"),
        ("POST", "/v1/responses"),
        ("POST", "/v1/embeddings"),
        ("POST", "/v1/messages"),
        ("POST", "/v1/messages/count_tokens"),
    ] {
        let body = json!({ "model": "acme/absent" });

        // WHEN it is called without a token, then with one
        let (refused, refusal) =
            json_of(send(&relay.router, method, path, body.clone()).await).await;
        let header = if path.starts_with("/v1/messages") {
            ("x-api-key", token.as_str())
        } else {
            ("authorization", bearer.as_str())
        };
        let admitted = send_with(&relay.router, method, path, body, &[header])
            .await
            .status();

        // THEN the first is 401 in the endpoint's format, the second passes
        assert_eq!(refused, StatusCode::UNAUTHORIZED, "{path}");
        if path.starts_with("/v1/messages") {
            assert_eq!(refusal["error"]["type"], "authentication_error", "{path}");
        } else {
            assert_eq!(refusal["error"]["code"], "invalid_api_key", "{path}");
        }
        assert_ne!(admitted, StatusCode::UNAUTHORIZED, "{path}");
    }
}
