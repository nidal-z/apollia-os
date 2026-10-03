use std::path::PathBuf;
use std::time::Duration;

use apollia_core::config::ServerConfig;
use apollia_core::RuntimeEvent;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use super::{CompatServerError, CompatServerHandle};
use crate::eventbus::{EventBus, EventBusSender};
use crate::test_support::poll_until_async;

/// A listener on a port the system chose, standing for another program that
/// holds it. The one port site of the compatible server's tests: every other
/// test, here and in the route and supervisor tests, borrows it.
pub(crate) fn held_port() -> (std::net::TcpListener, u16) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    (listener, port)
}

/// A port free at the time of the call: held, read, released.
pub(crate) fn free_port() -> u16 {
    held_port().1
}

/// An `apollia.toml` holding `content`, in a directory the test owns.
pub(crate) fn settings_file(content: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("apollia.toml");
    std::fs::write(&path, content).unwrap();
    (dir, path)
}

pub(crate) fn file_enabled(path: &PathBuf) -> bool {
    ServerConfig::from_toml_str(&std::fs::read_to_string(path).unwrap())
        .unwrap()
        .enabled
}

pub(crate) async fn listens(port: u16) -> bool {
    TcpStream::connect(("127.0.0.1", port)).await.is_ok()
}

/// Sends `GET path` to the compatible server and returns the status code and
/// the JSON body.
async fn get(port: u16, path: &str) -> (u16, serde_json::Value) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let request = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).await.unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    (status, serde_json::from_str(body).unwrap())
}

fn bus() -> EventBusSender {
    EventBus::new().0
}

/// Waits until the actor has stopped, its listener closed.
async fn wait_gone(handle: &CompatServerHandle) {
    let gone = poll_until_async(Duration::from_secs(5), || async {
        matches!(handle.status().await, Err(CompatServerError::ActorGone))
    })
    .await;
    assert!(gone, "the compatible server did not stop");
}

#[tokio::test]
async fn test_a_runtime_whose_server_is_off_listens_on_nothing() {
    // GIVEN a file where the server is off, on a free port
    let port = free_port();
    let (_dir, path) = settings_file(&format!("[server]\nenabled = false\nport = {port}\n"));

    // WHEN the runtime starts its compatible server actor
    let bus = bus();
    let handle = CompatServerHandle::spawn(path, &bus);
    let status = handle.status().await.unwrap();

    // THEN nothing listens on the port, and the status says so
    assert!(!status.running);
    assert_eq!(status.port, port);
    assert!(!listens(port).await);
}

#[tokio::test]
async fn test_an_unknown_path_answers_404_in_the_openai_error_format() {
    // GIVEN a file where the server is on, on a free port
    let port = free_port();
    let (_dir, path) = settings_file(&format!("[server]\nenabled = true\nport = {port}\n"));
    let bus = bus();
    let handle = CompatServerHandle::spawn(path, &bus);
    assert!(handle.status().await.unwrap().running);

    // WHEN a client asks for a path the server does not serve
    let (status, body) = get(port, "/v1/unknown").await;

    // THEN the answer is a 404 carrying OpenAI's error envelope
    assert_eq!(status, 404);
    let error = &body["error"];
    assert!(
        error["message"].as_str().unwrap().contains("/v1/unknown"),
        "{body}"
    );
    assert_eq!(error["type"], "invalid_request_error");
    assert_eq!(error["code"], "unknown_url");
}

#[tokio::test]
async fn test_a_taken_port_is_refused_with_a_typed_error_and_the_runtime_goes_on() {
    // GIVEN another program holding the configured port
    let (_holder, port) = held_port();
    let (_dir, path) = settings_file(&format!("[server]\nport = {port}\n"));
    let bus = bus();
    let handle = CompatServerHandle::spawn(path.clone(), &bus);

    // WHEN the server is started
    let result = handle.start().await;

    // THEN the start is refused as a port in use, readable, the setting is not
    // written, and the actor still answers with the reason
    let error = result.unwrap_err();
    assert!(
        matches!(error, CompatServerError::PortInUse { port: p } if p == port),
        "{error:?}"
    );
    assert!(error.to_string().contains("already in use"), "{error}");
    assert!(!file_enabled(&path));
    let status = handle.status().await.unwrap();
    assert!(!status.running);
    assert_eq!(status.last_error, Some(error.to_string()));
}

#[tokio::test]
async fn test_a_server_started_before_a_restart_listens_again_after_it() {
    // GIVEN a server started on a free port, then the runtime shut down
    let port = free_port();
    let (_dir, path) = settings_file(&format!("[server]\nport = {port}\n"));
    let first_bus = bus();
    let first = CompatServerHandle::spawn(path.clone(), &first_bus);
    assert!(first.start().await.unwrap().running);
    first_bus.send(RuntimeEvent::ShutdownRequested).unwrap();
    wait_gone(&first).await;
    assert!(!listens(port).await, "the shutdown closes the listener");
    assert!(
        file_enabled(&path),
        "the shutdown leaves the setting as it was"
    );

    // WHEN the runtime starts again on the same file
    let bus = bus();
    let second = CompatServerHandle::spawn(path, &bus);
    let status = second.status().await.unwrap();

    // THEN the listener is open
    assert!(status.running);
    assert!(listens(port).await);
}

#[tokio::test]
async fn test_stopping_closes_the_port_and_writes_the_switch_off() {
    // GIVEN a started server
    let port = free_port();
    let (_dir, path) = settings_file(&format!("[server]\nport = {port}\n"));
    let bus = bus();
    let handle = CompatServerHandle::spawn(path.clone(), &bus);
    handle.start().await.unwrap();
    assert!(file_enabled(&path));

    // WHEN it is stopped
    let status = handle.stop().await.unwrap();

    // THEN the port refuses connections and the file says off
    assert!(!status.running);
    assert!(!listens(port).await);
    assert!(!file_enabled(&path));
}
