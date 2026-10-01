use super::*;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// An `/api/show` answer as Ollama gives it, trimmed to the keys read here.
fn show(parameters: Option<&str>, trained: u64) -> Value {
    let mut body = serde_json::json!({
        "modelfile": "FROM qwen3:8b",
        "details": { "family": "qwen3", "parameter_size": "8.2B" },
        "model_info": {
            "general.architecture": "qwen3",
            "qwen3.context_length": trained,
            "qwen3.embedding_length": 4096
        }
    });
    if let Some(p) = parameters {
        body["parameters"] = Value::String(p.to_string());
    }
    body
}

async fn ollama_answering(model: &str, body: Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/show"))
        .and(body_json(serde_json::json!({ "model": model })))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    server
}

async fn listing(entries: Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "object": "list", "data": entries })),
        )
        .mount(&server)
        .await;
    server
}

// GIVEN an Ollama server whose model sets its own window in its parameters
// WHEN the window is read by `/api/show`, through the address as stored
//      (with `/v1`) and without it
// THEN the parameter is the window, not the trained length
#[tokio::test]
async fn test_an_ollama_model_window_is_read_from_its_parameters() {
    // GIVEN
    let server = ollama_answering(
        "qwen3:8b",
        show(
            Some("num_ctx                        8192\nstop \"<|im_end|>\""),
            40_960,
        ),
    )
    .await;

    // WHEN
    let with_v1 = ollama_window(&format!("{}/v1", server.uri()), "qwen3:8b", None).await;
    let bare = ollama_window(&server.uri(), "qwen3:8b", None).await;

    // THEN
    assert_eq!(with_v1, Some(8192));
    assert_eq!(bare, Some(8192));
}

// GIVEN an Ollama server whose model sets no window of its own
// WHEN the window is read by `/api/show`
// THEN the trained length is the figure
#[tokio::test]
async fn test_an_ollama_model_without_parameter_reads_its_trained_length() {
    // GIVEN
    let server = ollama_answering("qwen3:8b", show(Some("stop \"<|im_end|>\""), 40_960)).await;

    // WHEN
    let window = ollama_window(&server.uri(), "qwen3:8b", None).await;

    // THEN
    assert_eq!(window, Some(40_960));
}

// GIVEN a vLLM server listing its model with `max_model_len`, and a server
//       listing the same model without it
// WHEN the window of that model is read from `/v1/models`
// THEN the first gives the figure and the second gives none
#[tokio::test]
async fn test_a_listed_window_is_read_when_the_server_exposes_it() {
    // GIVEN
    let vllm = listing(serde_json::json!([
        { "id": "other", "object": "model", "max_model_len": 4096 },
        { "id": "Qwen/Qwen3-8B", "object": "model", "owned_by": "vllm", "max_model_len": 16_384 }
    ]))
    .await;
    let silent = listing(serde_json::json!([{ "id": "Qwen/Qwen3-8B", "object": "model" }])).await;

    // WHEN
    let read = listed_window(&vllm.uri(), "Qwen/Qwen3-8B", None).await;
    let unread = listed_window(&silent.uri(), "Qwen/Qwen3-8B", None).await;

    // THEN
    assert_eq!(read, Some(16_384));
    assert_eq!(unread, None);
}

// GIVEN a server that answers neither `/api/show` nor `/v1/models`
// WHEN its window is read both ways
// THEN neither gives a figure, so the configured value is what remains
#[tokio::test]
async fn test_a_server_exposing_nothing_gives_no_window() {
    // GIVEN
    let server = MockServer::start().await;

    // WHEN
    let shown = ollama_window(&server.uri(), "m", None).await;
    let listed = listed_window(&server.uri(), "m", None).await;

    // THEN
    assert_eq!((shown, listed), (None, None));
}

// GIVEN a listing where one model has a declared window and two do not, on a
//       server that is not Ollama
// WHEN the missing windows are filled
// THEN the declared one is kept, the others stay unknown, and `/api/show` was
//      asked once only
#[tokio::test]
async fn test_filling_stops_at_the_first_model_the_server_cannot_show() {
    // GIVEN
    let server = MockServer::start().await;
    let mut models = vec![
        ListedModel {
            id: "a".into(),
            context_window: Some(65_536),
        },
        ListedModel {
            id: "b".into(),
            context_window: None,
        },
        ListedModel {
            id: "c".into(),
            context_window: None,
        },
    ];

    // WHEN
    fill_windows(&server.uri(), None, &mut models).await;

    // THEN
    let windows: Vec<_> = models.iter().map(|m| m.context_window).collect();
    assert_eq!(windows, vec![Some(65_536), None, None]);
    let shows = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/api/show")
        .count();
    assert_eq!(shows, 1);
}

/// Records the events emitted while it is the default subscriber, as their
/// message and fields rendered to text.
#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<BTreeMap<String, String>>>>);

struct Fields<'a>(&'a mut BTreeMap<String, String>);

impl tracing::field::Visit for Fields<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.insert(
            field.name().to_string(),
            format!("{value:?}").replace('"', ""),
        );
    }
}

impl tracing::Subscriber for Recorder {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let mut fields = BTreeMap::new();
        event.record(&mut Fields(&mut fields));
        self.0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(fields);
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

// GIVEN a window read below the expected one, and one at it
// WHEN each is checked
// THEN only the short one is reported, by a `llm.server.context_short` event
//      naming the server, the model, the window read and the window expected
#[test]
fn test_a_short_window_is_reported_with_its_figures() {
    // GIVEN
    let recorder = Recorder::default();

    // WHEN
    let (short, enough) = tracing::subscriber::with_default(recorder.clone(), || {
        (
            context_short("ollama-box", "qwen3:8b", 8192),
            context_short("ollama-box", "qwen3:32b", EXPECTED_CONTEXT_WINDOW),
        )
    });

    // THEN
    assert!(short);
    assert!(!enough);
    let events = recorder.0.lock().unwrap().clone();
    assert_eq!(events.len(), 1, "{events:?}");
    let event = &events[0];
    assert_eq!(event["message"], "llm.server.context_short");
    assert_eq!(event["server"], "ollama-box");
    assert_eq!(event["model"], "qwen3:8b");
    assert_eq!(event["read_window"], "8192");
    assert_eq!(
        event["expected_window"],
        EXPECTED_CONTEXT_WINDOW.to_string()
    );
}
