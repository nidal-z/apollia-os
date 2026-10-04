//! The reasoning of a model in the chat completions the server relays, kept
//! apart or put back into `content`, on answers recorded from `llama-server`.
//!
//! The two fixtures were recorded from `llama-server` (release b9870, run with
//! `--jinja --reasoning-format auto`, the flags the runtime passes) answering a
//! thinking model, one as a JSON body and one as an event stream; only the
//! `model` field, which named a path on the recording machine, was shortened
//! to the file name.

use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;
use futures::StreamExt;
use serde_json::{json, Value};
use tempfile::TempDir;
use tower::ServiceExt;

use super::logs::ServerLogsHandle;
use super::models::LocalModels;
use super::reasoning::{merge_answer, merge_stream, StreamMerger};
use super::relay::RelayState;
use super::relay_tests::serve_engine;
use super::router::compat_router;
use crate::llama_server::LlamaServerSupervisor;

const RECORDED_ANSWER: &str =
    include_str!("../../tests/fixtures/compat_server/reasoning_answer.json");
const RECORDED_STREAM: &str =
    include_str!("../../tests/fixtures/compat_server/reasoning_stream.sse");

/// The reasoning and the answer a stream carries, each put end to end, and
/// whether any delta still has a `reasoning_content`.
fn stream_texts(stream: &str) -> (String, String, bool) {
    let mut reasoning = String::new();
    let mut content = String::new();
    let mut field = false;
    for line in stream.lines() {
        let Some(payload) = line.strip_prefix("data: ") else {
            continue;
        };
        let Ok(chunk) = serde_json::from_str::<Value>(payload) else {
            continue;
        };
        let delta = &chunk["choices"][0]["delta"];
        field |= delta.get("reasoning_content").is_some();
        reasoning.push_str(delta["reasoning_content"].as_str().unwrap_or(""));
        content.push_str(delta["content"].as_str().unwrap_or(""));
    }
    (reasoning, content, field)
}

#[test]
fn test_a_recorded_answer_gets_its_reasoning_back_in_content_between_think_tags() {
    // GIVEN a recorded chat completion whose reasoning the engine separated
    let mut answer: Value = serde_json::from_str(RECORDED_ANSWER).unwrap();
    let reasoning = answer["choices"][0]["message"]["reasoning_content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(!reasoning.is_empty());

    // WHEN its reasoning is put back
    merge_answer(&mut answer);

    // THEN the content opens on the reasoning between the tags, then the
    // answer, and the separate field is gone; the rest is untouched
    let message = &answer["choices"][0]["message"];
    assert_eq!(
        message["content"],
        format!("<think>{reasoning}</think>Paris")
    );
    assert!(message.get("reasoning_content").is_none());
    assert_eq!(answer["usage"]["completion_tokens"], 204);
    assert_eq!(answer["choices"][0]["finish_reason"], "stop");
}

#[test]
fn test_an_answer_without_reasoning_is_left_as_it_is() {
    // GIVEN a chat completion of a model that does not think, and a body
    // that is not a chat completion
    let mut plain =
        json!({"choices": [{"index": 0, "message": {"role": "assistant", "content": "Hi"}}]});
    let mut other = json!({"error": {"message": "no"}});
    let (plain_before, other_before) = (plain.clone(), other.clone());

    // WHEN their reasoning is put back
    merge_answer(&mut plain);
    merge_answer(&mut other);

    // THEN neither changes
    assert_eq!(plain, plain_before);
    assert_eq!(other, other_before);
}

#[test]
fn test_a_recorded_stream_gets_its_reasoning_back_whatever_the_chunk_boundaries() {
    // GIVEN the recorded stream, and what it reads as when kept apart
    let (reasoning, answer, field) = stream_texts(RECORDED_STREAM);
    assert!(field && !reasoning.is_empty());
    assert_eq!(answer, "4");

    for size in [1, 7, 64, RECORDED_STREAM.len()] {
        // WHEN it goes through in chunks of `size` bytes
        let mut merger = StreamMerger::default();
        let mut out = Vec::new();
        for chunk in RECORDED_STREAM.as_bytes().chunks(size) {
            out.extend_from_slice(&merger.push(chunk));
        }
        out.extend_from_slice(&merger.finish());
        let merged = String::from_utf8(out).unwrap();

        // THEN the content alone carries the reasoning between the tags and
        // then the answer, no delta keeps the field, and the stream still
        // ends on [DONE]
        let (left, content, field) = stream_texts(&merged);
        assert_eq!(left, "", "chunks of {size}");
        assert!(!field, "chunks of {size}");
        assert_eq!(
            content,
            format!("<think>{reasoning}</think>4"),
            "chunks of {size}"
        );
        assert!(
            merged.trim_end().ends_with("data: [DONE]"),
            "chunks of {size}"
        );
    }
}

#[test]
fn test_a_stream_that_finishes_while_thinking_closes_its_tag() {
    // GIVEN a stream cut by the token limit before any answer text
    let stream = concat!(
        "data: {\"choices\":[{\"index\":0,\"finish_reason\":null,\"delta\":{\"reasoning_content\":\"Let me\"}}]}\n\n",
        "data: {\"choices\":[{\"index\":0,\"finish_reason\":\"length\",\"delta\":{}}]}\n\n",
    );

    // WHEN its reasoning is put back
    let mut merger = StreamMerger::default();
    let merged = String::from_utf8(merger.push(stream.as_bytes()).to_vec()).unwrap();

    // THEN the tag is closed on the finishing event
    let (_, content, _) = stream_texts(&merged);
    assert_eq!(content, "<think>Let me</think>");
}

#[tokio::test]
async fn test_merge_stream_passes_the_whole_recorded_stream_in_order() {
    // GIVEN the recorded stream as the engine's chunks, split mid-event
    let chunks: Vec<Result<Bytes, std::io::Error>> = RECORDED_STREAM
        .as_bytes()
        .chunks(50)
        .map(|chunk| Ok(Bytes::copy_from_slice(chunk)))
        .collect();

    // WHEN it is relayed with the reasoning put back
    let relayed: Vec<Bytes> = merge_stream(futures::stream::iter(chunks))
        .map(Result::unwrap)
        .collect()
        .await;

    // THEN no chunk is empty and the content reads as the model wrote it
    assert!(relayed.iter().all(|chunk| !chunk.is_empty()));
    let merged = String::from_utf8(relayed.concat()).unwrap();
    let (reasoning, _, _) = stream_texts(RECORDED_STREAM);
    let (_, content, _) = stream_texts(&merged);
    assert_eq!(content, format!("<think>{reasoning}</think>4"));
}

/// A relay over one chat model whose engine answers the recorded fixtures,
/// with `apollia.toml` holding `toml`.
struct Recorded {
    dir: TempDir,
    router: Router,
}

async fn recorded_relay(toml: &str) -> Recorded {
    let app = Router::new().fallback(|request: Request<Body>| async move {
        let bytes = axum::body::to_bytes(request.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        if body["stream"] == json!(true) {
            return ([("content-type", "text/event-stream")], RECORDED_STREAM).into_response();
        }
        ([("content-type", "application/json")], RECORDED_ANSWER).into_response()
    });
    let port = serve_engine(app).await;
    let supervisor =
        LlamaServerSupervisor::with_resident_for_test(&[("/models/think.gguf", port)], None).await;
    let dir = TempDir::new().unwrap();
    let index = json!({"version": 1, "models": [{
        "id": "acme/think", "format": "gguf", "path": "/models/think.gguf", "size_bytes": 1,
        "origin": {"kind": "import"}, "category": "language"
    }]});
    std::fs::write(dir.path().join("index.json"), index.to_string()).unwrap();
    let settings = dir.path().join("apollia.toml");
    std::fs::write(&settings, toml).unwrap();
    let models = LocalModels::new(Some(supervisor), None, Some(dir.path().to_path_buf()))
        .with_files(settings, dir.path().join("system.db"));
    let router = compat_router(
        None,
        RelayState::new(models).unwrap(),
        ServerLogsHandle::spawn(&crate::eventbus::EventBus::new().0),
    );
    Recorded { dir, router }
}

async fn chat(router: &Router, stream: bool) -> (StatusCode, String) {
    let body = json!({"model": "acme/think", "messages": [], "stream": stream});
    let response: Response = router
        .clone()
        .oneshot(
            Request::post("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn test_by_default_the_server_relays_the_reasoning_apart_as_the_engine_gave_it() {
    // GIVEN a server whose apollia.toml does not name the setting
    let relay = recorded_relay("[server]\nenabled = true\n").await;

    // WHEN a chat completion is asked for, whole and streamed
    let (status, whole) = chat(&relay.router, false).await;
    let (_, streamed) = chat(&relay.router, true).await;

    // THEN both are the engine's bytes, the reasoning in its own field
    assert_eq!(status, StatusCode::OK);
    assert_eq!(whole, RECORDED_ANSWER);
    assert_eq!(streamed, RECORDED_STREAM);
}

#[tokio::test]
async fn test_with_the_setting_off_the_server_puts_the_reasoning_back_into_content() {
    // GIVEN a server whose apollia.toml turns the separation off
    let relay = recorded_relay("[server]\nseparate_reasoning_content = false\n").await;

    // WHEN a chat completion is asked for, whole and streamed
    let (status, whole) = chat(&relay.router, false).await;
    let (_, streamed) = chat(&relay.router, true).await;

    // THEN the reasoning is in content between the tags, and in no field
    assert_eq!(status, StatusCode::OK);
    let answer: Value = serde_json::from_str(&whole).unwrap();
    let message = &answer["choices"][0]["message"];
    assert!(message["content"]
        .as_str()
        .unwrap()
        .starts_with("<think>Here's a thinking process:"));
    assert!(message["content"]
        .as_str()
        .unwrap()
        .ends_with("</think>Paris"));
    assert!(message.get("reasoning_content").is_none());
    let (left, content, field) = stream_texts(&streamed);
    assert_eq!(left, "");
    assert!(!field);
    assert!(content.starts_with("<think>Here") && content.ends_with("</think>4"));
}

#[tokio::test]
async fn test_the_setting_is_read_at_every_request() {
    // GIVEN a server that keeps the reasoning apart
    let relay = recorded_relay("").await;
    let (_, before) = chat(&relay.router, false).await;
    assert_eq!(before, RECORDED_ANSWER);

    // WHEN the setting is turned off in the file, without a restart
    let settings = relay.dir.path().join("apollia.toml");
    std::fs::write(&settings, "[server]\nseparate_reasoning_content = false\n").unwrap();
    let (_, after) = chat(&relay.router, false).await;

    // THEN the next answer has its reasoning back in content
    let answer: Value = serde_json::from_str(&after).unwrap();
    assert!(answer["choices"][0]["message"]["content"]
        .as_str()
        .unwrap()
        .starts_with("<think>"));
}
