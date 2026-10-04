//! The compatible server on the local network: the switch, its hot restart,
//! the origin of each request, and the rule for calls to agents.
//!
//! A machine without a network route has no address on the local network;
//! there, the assertions that need one reduce to the loopback ones.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use apollia_core::config::{write_setting, PolicySettings};
use axum::http::HeaderMap;
use axum::{Extension, Json, Router};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;

use super::origin::{admit_agent_call, reachable_address, CallOrigin, AGENT_TOKEN_REQUIRED};
use super::router::serve;
use super::tests::{free_port, listens, settings_file, spawn_server};
use super::tokens::ServerTokensHandle;
use crate::eventbus::EventBus;

/// The IPv4 address of this machine on the local network, if it has one.
fn lan_ip() -> Option<Ipv4Addr> {
    let address = reachable_address(true, 1);
    let ip: Ipv4Addr = address
        .strip_prefix("http://")?
        .strip_suffix(":1")?
        .parse()
        .ok()?;
    (!ip.is_loopback()).then_some(ip)
}

/// Sends `GET path` to `ip:port` and returns the status and JSON body.
async fn get_from(ip: IpAddr, port: u16, path: &str) -> (u16, serde_json::Value) {
    let mut stream = TcpStream::connect((ip, port)).await.unwrap();
    let request = format!("GET {path} HTTP/1.1\r\nHost: apollia\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).await.unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    (status, serde_json::from_str(body).unwrap())
}

#[tokio::test]
async fn test_the_local_network_without_a_token_is_served_and_its_address_given() {
    // GIVEN a file opening the server to the local network, no token required
    let port = free_port();
    let (_dir, path) = settings_file(&format!("[server]\nport = {port}\nserve_on_lan = true\n"));
    let bus = EventBus::new().0;
    let handle = spawn_server(&path, &bus);

    // WHEN the server is started
    let status = handle.start().await.unwrap();

    // THEN it runs on every interface, gives the address of the network, and
    // a device of the network gets an answer without a token
    assert!(status.running);
    assert!(status.serve_on_lan);
    assert_eq!(status.address, reachable_address(true, port));
    assert!(listens(port).await);
    if let Some(ip) = lan_ip() {
        assert_eq!(status.address, format!("http://{ip}:{port}"));
        let (code, body) = get_from(IpAddr::V4(ip), port, "/v1/unknown").await;
        assert_eq!(code, 404, "{body}");
    }
}

#[tokio::test]
async fn test_off_by_default_the_server_stays_on_the_machine() {
    // GIVEN a file that says nothing of the local network
    let port = free_port();
    let (_dir, path) = settings_file(&format!("[server]\nport = {port}\n"));
    let bus = EventBus::new().0;
    let handle = spawn_server(&path, &bus);

    // WHEN the server is started
    let status = handle.start().await.unwrap();

    // THEN it listens on the loopback interface alone, and gives that address
    assert!(!status.serve_on_lan);
    assert_eq!(status.address, format!("http://127.0.0.1:{port}"));
    assert!(listens(port).await);
    if let Some(ip) = lan_ip() {
        assert!(TcpStream::connect((ip, port)).await.is_err());
    }
}

#[tokio::test]
async fn test_a_restart_applies_the_switch_on_the_same_port() {
    // GIVEN a server running on the machine alone
    let port = free_port();
    let (_dir, path) = settings_file(&format!("[server]\nport = {port}\n"));
    let bus = EventBus::new().0;
    let handle = spawn_server(&path, &bus);
    assert!(!handle.start().await.unwrap().serve_on_lan);

    // WHEN the switch is turned on in the file and the server restarted
    write_setting(
        &path,
        "server.serve_on_lan",
        serde_json::Value::Bool(true),
        &PolicySettings::none(),
    )
    .unwrap();
    let opened = handle.restart().await.unwrap();

    // THEN it runs on the network, on the same port; turned off and restarted
    // again, it is back on the machine alone
    assert!(opened.running);
    assert!(opened.serve_on_lan);
    assert_eq!(opened.port, port);
    if let Some(ip) = lan_ip() {
        assert!(TcpStream::connect((ip, port)).await.is_ok());
    }
    write_setting(
        &path,
        "server.serve_on_lan",
        serde_json::Value::Bool(false),
        &PolicySettings::none(),
    )
    .unwrap();
    let closed = handle.restart().await.unwrap();
    assert!(closed.running);
    assert!(!closed.serve_on_lan);
    assert!(listens(port).await);
    if let Some(ip) = lan_ip() {
        assert!(TcpStream::connect((ip, port)).await.is_err());
    }
}

#[tokio::test]
async fn test_restarting_a_stopped_server_changes_nothing() {
    // GIVEN a stopped server
    let port = free_port();
    let (_dir, path) = settings_file(&format!("[server]\nport = {port}\nserve_on_lan = true\n"));
    let bus = EventBus::new().0;
    let handle = spawn_server(&path, &bus);

    // WHEN it is restarted
    let status = handle.restart().await.unwrap();

    // THEN it stays stopped, and reports the configured choice
    assert!(!status.running);
    assert!(status.serve_on_lan);
    assert!(!listens(port).await);
}

#[tokio::test]
async fn test_each_request_carries_the_origin_of_its_connection() {
    // GIVEN the accept loop of the server on every interface, behind a
    // handler that answers, for any path, the origin it reads
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let router = Router::new().fallback(
        |Extension(origin): Extension<CallOrigin>,
         axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<SocketAddr>| async move {
            Json(serde_json::json!({ "origin": origin, "peer": peer.ip().to_string() }))
        },
    );
    let cancel = CancellationToken::new();
    let task = tokio::spawn(serve(listener, router, cancel.clone()));

    // WHEN it is called from the machine, and from its address on the network
    let (_, local) = get_from(IpAddr::V4(Ipv4Addr::LOCALHOST), port, "/origin").await;
    let remote = match lan_ip() {
        Some(ip) => Some((ip, get_from(IpAddr::V4(ip), port, "/origin").await.1)),
        None => None,
    };

    // THEN the first is the loopback, the second the local network
    assert_eq!(local["origin"], "loopback");
    assert_eq!(local["peer"], "127.0.0.1");
    if let Some((ip, body)) = remote {
        assert_eq!(body["origin"], "local_network");
        assert_eq!(body["peer"], ip.to_string());
    }
    cancel.cancel();
    task.await.unwrap();
}

async fn json_of(response: axum::response::Response) -> (u16, serde_json::Value) {
    let status = response.status().as_u16();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 16)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn test_an_agent_call_from_the_network_is_admitted_only_with_a_token_in_use() {
    // GIVEN the tokens of a server, one of them in use
    let dir = tempfile::TempDir::new().unwrap();
    let tokens = ServerTokensHandle::spawn(dir.path().join("system.db"));
    let value = tokens.create("phone".to_owned()).await.unwrap().value;
    let mut with_token = HeaderMap::new();
    with_token.insert("authorization", format!("Bearer {value}").parse().unwrap());
    let mut wrong_token = HeaderMap::new();
    wrong_token.insert("x-api-key", "apo_nothing".parse().unwrap());
    let none = HeaderMap::new();
    let path = "/v1/chat/completions";

    // WHEN calls to an agent arrive from each origin, with and without it
    let from_machine = admit_agent_call(CallOrigin::Loopback, path, &none, &tokens).await;
    let with = admit_agent_call(CallOrigin::LocalNetwork, path, &with_token, &tokens).await;
    let wrong = admit_agent_call(CallOrigin::LocalNetwork, path, &wrong_token, &tokens).await;
    let anthropic =
        admit_agent_call(CallOrigin::LocalNetwork, "/v1/messages", &none, &tokens).await;

    // THEN the machine and the token pass; the others get a readable 401 in
    // the format of their endpoint
    assert!(from_machine.is_ok());
    assert!(with.is_ok());
    let (code, body) = json_of(wrong.unwrap_err()).await;
    assert_eq!(code, 401);
    assert_eq!(body["error"]["message"], AGENT_TOKEN_REQUIRED);
    assert_eq!(body["error"]["code"], "invalid_api_key");
    let (code, body) = json_of(anthropic.unwrap_err()).await;
    assert_eq!(code, 401);
    assert_eq!(body["error"]["type"], "authentication_error");
    assert_eq!(body["error"]["message"], AGENT_TOKEN_REQUIRED);
}
