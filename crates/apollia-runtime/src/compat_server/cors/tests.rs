use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::Router;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tower::ServiceExt;

use super::{ALLOWED_HEADERS, ALLOWED_METHODS};
use crate::api::test_support::{app_state, StemLoader};
use crate::compat_server::logs::ServerLogsHandle;
use crate::compat_server::relay::RelayState;
use crate::compat_server::router::compat_router;
use crate::compat_server::tests::{free_port, settings_file, spawn_server};
use crate::compat_server::LocalModels;
use crate::eventbus::EventBus;

const ORIGIN: &str = "http://localhost:3000";

/// What a browser sends before a cross-origin `POST` with a bearer and a JSON
/// body.
fn preflight(path: &str) -> Request<Body> {
    Request::builder()
        .method("OPTIONS")
        .uri(path)
        .header("origin", ORIGIN)
        .header("access-control-request-method", "POST")
        .header(
            "access-control-request-headers",
            "authorization, content-type",
        )
        .body(Body::empty())
        .unwrap()
}

fn get_from_page(path: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .header("origin", ORIGIN)
        .body(Body::empty())
        .unwrap()
}

async fn answer(router: Router, request: Request<Body>) -> (StatusCode, HeaderMap) {
    let response = router.oneshot(request).await.unwrap();
    (response.status(), response.headers().clone())
}

fn cors_headers(headers: &HeaderMap) -> Vec<String> {
    headers
        .keys()
        .map(|name| name.as_str().to_owned())
        .filter(|name| name.starts_with("access-control-"))
        .collect()
}

/// Sends a raw HTTP/1.1 request to the compatible server's port and returns
/// the status code and the lowercased response head.
async fn send(port: u16, request_line_and_headers: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let request = format!(
        "{request_line_and_headers}Host: localhost\r\nOrigin: {ORIGIN}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).await.unwrap();
    let (head, _body) = raw.split_once("\r\n\r\n").unwrap();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    (status, head.to_ascii_lowercase())
}

const PREFLIGHT: &str = "OPTIONS /v1/chat/completions HTTP/1.1\r\nAccess-Control-Request-Method: POST\r\nAccess-Control-Request-Headers: authorization, content-type\r\n";
const PLAIN_GET: &str = "GET /v1/unknown HTTP/1.1\r\n";
const MODELS: &str = "GET /v1/models HTTP/1.1\r\n";

#[tokio::test]
async fn test_with_cors_on_a_preflight_gets_204_with_origin_methods_and_headers() {
    // GIVEN a compatible server started from a file turning cors on
    let port = free_port();
    let (_dir, path) = settings_file(&format!(
        "[server]\nenabled = true\nport = {port}\ncors = true\n"
    ));
    let (bus, _) = EventBus::new();
    let handle = spawn_server(&path, &bus);
    assert!(handle.status().await.unwrap().running);

    // WHEN a browser sends its preflight, then an ordinary request
    let (preflight_status, preflight_head) = send(port, PREFLIGHT).await;
    let (get_status, get_head) = send(port, PLAIN_GET).await;

    // THEN the preflight is answered 204 with the allowed origin, methods and
    // headers, and the ordinary answer carries the allowed origin
    assert_eq!(preflight_status, 204, "{preflight_head}");
    assert!(
        preflight_head.contains("access-control-allow-origin: *"),
        "{preflight_head}"
    );
    assert!(
        preflight_head.contains(&format!(
            "access-control-allow-methods: {}",
            ALLOWED_METHODS.to_ascii_lowercase()
        )),
        "{preflight_head}"
    );
    assert!(
        preflight_head.contains(&format!("access-control-allow-headers: {ALLOWED_HEADERS}")),
        "{preflight_head}"
    );
    for header in [
        "authorization",
        "content-type",
        "x-api-key",
        "anthropic-version",
    ] {
        assert!(ALLOWED_HEADERS.contains(header), "{header}");
    }
    assert_eq!(get_status, 404);
    assert!(
        get_head.contains("access-control-allow-origin: *"),
        "{get_head}"
    );
}

#[tokio::test]
async fn test_with_cors_off_by_default_no_answer_carries_a_cors_header() {
    // GIVEN a compatible server started from a file that does not name cors
    let port = free_port();
    let (_dir, path) = settings_file(&format!("[server]\nenabled = true\nport = {port}\n"));
    let (bus, _) = EventBus::new();
    let handle = spawn_server(&path, &bus);
    assert!(handle.status().await.unwrap().running);

    // WHEN a browser sends its preflight, then an ordinary request
    let (preflight_status, preflight_head) = send(port, PREFLIGHT).await;
    let (_, get_head) = send(port, PLAIN_GET).await;

    // THEN neither answer carries any cross-origin header, and the preflight
    // is refused by the route, which answers only `POST`
    assert_eq!(preflight_status, 405, "{preflight_head}");
    assert!(
        !preflight_head.contains("access-control-"),
        "{preflight_head}"
    );
    assert!(!get_head.contains("access-control-"), "{get_head}");
}

#[tokio::test]
async fn test_a_preflight_passes_without_token_and_the_next_request_stays_authenticated() {
    // GIVEN a compatible server that requires a token and answers cross-origin
    // requests, and one token of it
    let port = free_port();
    let (_dir, path) = settings_file(&format!(
        "[server]\nport = {port}\nrequire_auth = true\ncors = true\n"
    ));
    let bus = EventBus::new().0;
    let handle = spawn_server(&path, &bus);
    let token = handle
        .tokens()
        .create("page".to_owned())
        .await
        .unwrap()
        .value;
    handle.start().await.unwrap();

    // WHEN a browser sends its preflight without a token, then the request
    // without and with the token
    let (preflight_status, preflight_head) = send(port, PREFLIGHT).await;
    let (refused, refused_head) = send(port, MODELS).await;
    let bearer = format!("{MODELS}Authorization: Bearer {token}\r\n");
    let (accepted, accepted_head) = send(port, &bearer).await;

    // THEN the preflight is answered, the request without the token is refused
    // in a form the page can read, and the request with it reaches the route
    assert_eq!(preflight_status, 204, "{preflight_head}");
    assert!(
        preflight_head.contains("access-control-allow-origin: *"),
        "{preflight_head}"
    );
    assert_eq!(refused, 401, "{refused_head}");
    assert!(
        refused_head.contains("access-control-allow-origin: *"),
        "{refused_head}"
    );
    assert_eq!(accepted, 200, "{accepted_head}");
    assert!(
        accepted_head.contains("access-control-allow-origin: *"),
        "{accepted_head}"
    );
}

#[tokio::test]
async fn test_the_native_api_gains_no_cors_header_when_the_compatible_server_has_it() {
    // GIVEN a daemon whose compatible server answers cross-origin requests
    let port = free_port();
    let (dir, path) = settings_file(&format!(
        "[server]\nenabled = true\nport = {port}\ncors = true\n"
    ));
    let mut state = app_state(
        Arc::new(StemLoader { refusal: None }),
        dir.path().to_path_buf(),
    );
    state.compat_server = Some(spawn_server(&path, &state.event_sender));
    let compat = state.compat_server.clone().unwrap();
    // The actor lives as long as the bus has a sender; the router drops its own.
    let bus = state.event_sender.clone();
    assert!(compat.status().await.unwrap().running);
    let native = crate::api::server::APIServer::build_router_for_test(state);

    // WHEN a page sends a preflight and a request to the native API, and the
    // same preflight to the compatible server
    let (_, native_preflight) = answer(native.clone(), preflight("/api/v1/health")).await;
    let (health, native_get) = answer(native, get_from_page("/api/v1/health")).await;
    let router = compat_router(
        None,
        RelayState::new(LocalModels::default()).unwrap(),
        ServerLogsHandle::spawn(&bus),
        true,
    );
    let (compat_status, _) = answer(router, preflight("/v1/models")).await;
    let (wire_status, _) = send(port, PREFLIGHT).await;

    // THEN the native answers carry no cross-origin header, while the
    // compatible server answers the preflight
    assert_eq!(health, StatusCode::OK);
    assert!(
        cors_headers(&native_preflight).is_empty(),
        "{native_preflight:?}"
    );
    assert!(cors_headers(&native_get).is_empty(), "{native_get:?}");
    assert_eq!(compat_status, StatusCode::NO_CONTENT);
    assert_eq!(wire_status, 204);
}
