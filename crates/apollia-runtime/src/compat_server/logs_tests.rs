//! The logs of the compatible server: the ring, its levels, the request lines
//! and the engine lines.
//!
//! Every write reaches the actor through the same channel as the read that
//! follows it, so a read placed after a write sees it without waiting.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use super::logs::{ServerLogLevel, ServerLogLine, ServerLogSource, ServerLogsHandle};
use super::models::LocalModels;
use super::relay::RelayState;
use super::router::compat_router;
use crate::eventbus::{EventBus, EventBusSender};

fn bus() -> EventBusSender {
    EventBus::new().0
}

async fn held(logs: &ServerLogsHandle, level: ServerLogLevel) -> Vec<ServerLogLine> {
    logs.subscribe(level).await.unwrap().backlog
}

fn messages(lines: &[ServerLogLine]) -> Vec<&str> {
    lines
        .iter()
        .map(|line| line.message.as_deref().unwrap_or(""))
        .collect()
}

#[tokio::test]
async fn test_the_ring_keeps_the_order_and_drops_the_oldest_when_full() {
    // GIVEN a ring of three lines
    let bus = bus();
    let logs = ServerLogsHandle::spawn_with_capacity(&bus, 3);

    // WHEN five lines are recorded
    for n in 1..=5 {
        logs.engine_line(&format!("line {n}"));
    }

    // THEN the three most recent remain, oldest first, numbered in order
    let lines = held(&logs, ServerLogLevel::Debug).await;
    assert_eq!(messages(&lines), ["line 3", "line 4", "line 5"]);
    let seqs: Vec<u64> = lines.iter().map(|line| line.seq).collect();
    assert_eq!(seqs, [2, 3, 4]);
}

#[tokio::test]
async fn test_a_subscriber_gets_the_lines_at_its_level_then_the_new_ones() {
    // GIVEN a ring holding an information line and an error line
    let bus = bus();
    let logs = ServerLogsHandle::spawn(&bus);
    logs.engine_line("srv  load_model: loading model");
    logs.engine_line("error: failed to open the model");

    // WHEN a reader subscribes at the warning level and two more lines arrive
    let mut subscription = logs.subscribe(ServerLogLevel::Warn).await.unwrap();
    logs.engine_line("slot launch_slot_: id 0");
    logs.engine_line("warning: context shifted");
    logs.clear().await.unwrap();

    // THEN the backlog holds the error only, and the live channel the warning
    // only
    assert_eq!(
        messages(&subscription.backlog),
        ["error: failed to open the model"]
    );
    assert_eq!(subscription.backlog[0].level, ServerLogLevel::Error);
    let live = subscription.live.try_recv().unwrap();
    assert_eq!(live.message.as_deref(), Some("warning: context shifted"));
    assert_eq!(live.level, ServerLogLevel::Warn);
    assert!(subscription.live.try_recv().is_err());
}

#[tokio::test]
async fn test_clearing_empties_the_ring_and_keeps_the_numbering_and_the_readers() {
    // GIVEN a ring with two lines and a reader following it
    let bus = bus();
    let logs = ServerLogsHandle::spawn(&bus);
    logs.engine_line("one");
    logs.engine_line("two");
    let mut subscription = logs.subscribe(ServerLogLevel::Debug).await.unwrap();

    // WHEN the ring is cleared and a line arrives
    let cleared = logs.clear().await.unwrap();
    logs.engine_line("three");

    // THEN two lines were cleared, the ring holds the new line only, numbered
    // after the cleared ones, and the reader still receives it
    assert_eq!(cleared, 2);
    let lines = held(&logs, ServerLogLevel::Debug).await;
    assert_eq!(messages(&lines), ["three"]);
    assert_eq!(lines[0].seq, 2);
    assert_eq!(
        subscription.live.try_recv().unwrap().message.as_deref(),
        Some("three")
    );
}

#[tokio::test]
async fn test_each_request_leaves_one_line_with_its_endpoint_model_status_and_duration() {
    // GIVEN the compatible server's router, with no model loaded
    let bus = bus();
    let logs = ServerLogsHandle::spawn(&bus);
    let router = compat_router(
        None,
        RelayState::new(LocalModels::default()).unwrap(),
        logs.clone(),
    );

    // WHEN a client lists the models, then asks a model that is not loaded
    let list = Request::get("/v1/models").body(Body::empty()).unwrap();
    let listed = router.clone().oneshot(list).await.unwrap();
    let chat = Request::post("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"model":"acme/chat","messages":[]}"#))
        .unwrap();
    let refused = router.oneshot(chat).await.unwrap();

    // THEN one line per request, in order, each with its method, endpoint,
    // status and duration, the model named when the body named one, and the
    // refusal at the warning level
    assert_eq!(listed.status(), StatusCode::OK);
    assert_eq!(refused.status(), StatusCode::NOT_FOUND);
    let lines = held(&logs, ServerLogLevel::Debug).await;
    assert_eq!(lines.len(), 2);
    let (first, second) = (&lines[0], &lines[1]);
    assert_eq!(first.source, ServerLogSource::Request);
    assert_eq!(first.method.as_deref(), Some("GET"));
    assert_eq!(first.endpoint.as_deref(), Some("/v1/models"));
    assert_eq!(first.model, None);
    assert_eq!(first.status, Some(200));
    assert_eq!(first.level, ServerLogLevel::Info);
    assert!(first.duration_ms.is_some());
    assert_eq!(second.method.as_deref(), Some("POST"));
    assert_eq!(second.endpoint.as_deref(), Some("/v1/chat/completions"));
    assert_eq!(second.model.as_deref(), Some("acme/chat"));
    assert_eq!(second.status, Some(404));
    assert_eq!(second.level, ServerLogLevel::Warn);
    assert!(chrono::DateTime::parse_from_rfc3339(&second.timestamp).is_ok());
}

#[tokio::test]
async fn test_the_lines_an_engine_prints_enter_the_ring_in_order() {
    // GIVEN the logs, and an engine pipe printing three lines, one of them
    // not valid UTF-8 and one an error
    let bus = bus();
    let logs = ServerLogsHandle::spawn(&bus);
    let printed: &[u8] = b"main: server is listening\nbad \xff byte\r\nerror: out of memory\n";

    // WHEN the pipe is drained as the supervisor drains it
    crate::llama_server::drain_pipe(printed, Some(logs.clone())).await;

    // THEN the three lines are in the ring, in order, as engine lines, the
    // error at the error level
    let lines = held(&logs, ServerLogLevel::Debug).await;
    assert_eq!(
        messages(&lines),
        [
            "main: server is listening",
            "bad \u{fffd} byte",
            "error: out of memory"
        ]
    );
    assert!(lines
        .iter()
        .all(|line| line.source == ServerLogSource::Engine));
    assert_eq!(lines[2].level, ServerLogLevel::Error);
    assert_eq!(lines[0].level, ServerLogLevel::Info);
}

#[tokio::test]
async fn test_the_logs_stop_with_the_runtime() {
    // GIVEN the logs of a runtime
    let (bus, _first) = EventBus::new();
    let logs = ServerLogsHandle::spawn(&bus);
    let mut subscription = logs.subscribe(ServerLogLevel::Debug).await.unwrap();

    // WHEN the runtime shuts down
    bus.send(apollia_core::RuntimeEvent::ShutdownRequested)
        .unwrap();

    // THEN the live stream ends and the logs answer that they are gone
    assert!(subscription.live.recv().await.is_none());
    assert!(logs.clear().await.is_err());
}
