//! The token check of the compatible server, over a real listener.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use super::tests::{free_port, settings_file, spawn_server};
use super::CompatServerHandle;
use crate::eventbus::{EventBus, EventBusSender};

/// Sends `method path` with `headers` and returns the status and JSON body.
async fn send(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
) -> (u16, serde_json::Value) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let mut request =
        format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("Content-Length: 0\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).await.unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    (status, serde_json::from_str(body).unwrap())
}

/// A running compatible server whose `[server]` section carries `extra`, on a
/// free port, and one token of it named editor.
struct Running {
    dir: tempfile::TempDir,
    handle: CompatServerHandle,
    port: u16,
    token: String,
    _bus: EventBusSender,
}

async fn server_with_token(extra: &str) -> Running {
    let port = free_port();
    let (dir, path) = settings_file(&format!("[server]\nport = {port}\n{extra}"));
    let bus = EventBus::new().0;
    let handle = spawn_server(&path, &bus);
    let token = handle
        .tokens()
        .create("editor".to_owned())
        .await
        .unwrap()
        .value;
    handle.start().await.unwrap();
    Running {
        dir,
        handle,
        port,
        token,
        _bus: bus,
    }
}

const REQUIRED: &str = "require_auth = true\n";

#[tokio::test]
async fn test_without_a_token_an_openai_endpoint_answers_401_in_the_openai_format() {
    // GIVEN a server that requires a token
    let Running { port, .. } = &server_with_token(REQUIRED).await;
    let port = *port;

    // WHEN an OpenAI endpoint is called without one
    let (status, body) = send(port, "GET", "/v1/models", &[]).await;

    // THEN it answers 401 in the OpenAI envelope
    assert_eq!(status, 401, "{body}");
    assert_eq!(body["error"]["type"], "invalid_request_error");
    assert_eq!(body["error"]["code"], "invalid_api_key");
    assert!(body["error"]["message"].is_string());
}

#[tokio::test]
async fn test_without_a_token_the_anthropic_endpoint_answers_401_in_the_anthropic_format() {
    // GIVEN a server that requires a token
    let Running { port, .. } = &server_with_token(REQUIRED).await;
    let port = *port;

    // WHEN the messages endpoint is called with a wrong key
    let (status, body) = send(port, "POST", "/v1/messages", &[("x-api-key", "apo_wrong")]).await;

    // THEN it answers 401 in the Anthropic envelope
    assert_eq!(status, 401, "{body}");
    assert_eq!(body["type"], "error");
    assert_eq!(body["error"]["type"], "authentication_error");
}

#[tokio::test]
async fn test_a_bearer_token_in_use_opens_the_server_and_stamps_its_use() {
    // GIVEN a server that requires a token, and a token never used
    let Running {
        handle,
        port,
        token,
        ..
    } = &server_with_token(REQUIRED).await;
    let port = *port;
    assert_eq!(handle.tokens().list().await.unwrap()[0].last_used_at, None);

    // WHEN a client presents it as a bearer
    let bearer = format!("Bearer {token}");
    let (status, body) = send(port, "GET", "/v1/unknown", &[("Authorization", &bearer)]).await;

    // THEN the request reaches the router (404 for a path it does not serve)
    // and the token's last use is stamped
    assert_eq!(status, 404, "{body}");
    assert!(handle.tokens().list().await.unwrap()[0]
        .last_used_at
        .is_some());
}

#[tokio::test]
async fn test_a_token_in_x_api_key_opens_the_server() {
    // GIVEN a server that requires a token
    let Running { port, token, .. } = &server_with_token(REQUIRED).await;
    let port = *port;

    // WHEN a client presents it in x-api-key, as Anthropic clients do
    let (status, body) = send(port, "POST", "/v1/messages", &[("x-api-key", token)]).await;

    // THEN the request reaches the endpoint, which refuses the empty body in
    // Anthropic's format rather than the token
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["error"]["type"], "invalid_request_error", "{body}");
}

#[tokio::test]
async fn test_a_revoked_token_answers_401() {
    // GIVEN a server that requires a token, and the token revoked
    let Running {
        handle,
        port,
        token,
        ..
    } = &server_with_token(REQUIRED).await;
    let port = *port;
    handle
        .tokens()
        .revoke(apollia_core::server_token::TokenSelector::Name(
            "editor".to_owned(),
        ))
        .await
        .unwrap();

    // WHEN a client presents it
    let bearer = format!("Bearer {token}");
    let (status, body) = send(port, "GET", "/v1/models", &[("Authorization", &bearer)]).await;

    // THEN it is refused
    assert_eq!(status, 401, "{body}");
}

#[tokio::test]
async fn test_the_internal_token_does_not_open_the_compatible_server() {
    // GIVEN a server that requires a token, and the desktop's internal token
    let Running { dir, port, .. } = &server_with_token(REQUIRED).await;
    let port = *port;
    let internal = crate::api::load_or_generate_token(dir.path()).unwrap();

    // WHEN a client presents the internal token
    let bearer = format!("Bearer {internal}");
    let (status, body) = send(port, "GET", "/v1/models", &[("Authorization", &bearer)]).await;

    // THEN it is refused
    assert_eq!(status, 401, "{body}");
}

#[tokio::test]
async fn test_a_server_that_does_not_require_a_token_answers_without_one() {
    // GIVEN a server with require_auth left off
    let Running { port, .. } = &server_with_token("").await;
    let port = *port;

    // WHEN a client calls it without a token
    let (status, body) = send(port, "GET", "/v1/unknown", &[]).await;

    // THEN the request reaches the router
    assert_eq!(status, 404, "{body}");
}
