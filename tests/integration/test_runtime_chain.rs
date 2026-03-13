//! Integration tests — full runtime chain via HTTP API (STORY-049).
//!
//! Tests the complete flow: start_agent → submit_task → poll → Completed
//! via real HTTP calls to a live APIServer bound on a free TCP port.
//! No Python dependency — uses MockBackend for instant task completion.
//!
//! AC-1: POST /api/v1/agents returns 201 + state "active"
//! AC-2: POST /api/v1/tasks returns 202 + task_id
//! AC-3: GET /api/v1/tasks/{id} polled to "completed"
//! AC-4: Submit by manifest name (not UUID) returns 202
//! AC-5: TCP port and Unix socket are released after test

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use apollia_core::{AIPResult, AIPTask, TaskStatus};
use apollia_runtime::{
    api::routes_agents::StubAgentLoader,
    api::{APIServer, APIServerConfig, APIServerHandle, AppState},
    coordinator::ExecutionBackend,
    eventbus::EventBus,
    registry::AgentRegistry,
    router::TaskRouterHandle,
};

// --- MockBackend: completes every task instantly ---

/// Backend that resolves tasks synchronously with Completed status.
#[derive(Clone)]
struct MockBackend;

impl From<apollia_runtime::coordinator::DynBackend> for MockBackend {
    fn from(_: apollia_runtime::coordinator::DynBackend) -> Self {
        MockBackend
    }
}

impl ExecutionBackend for MockBackend {
    fn execute(
        &self,
        task: AIPTask,
    ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
        Box::pin(async move {
            Ok(AIPResult {
                task_id: task.task_id,
                status: TaskStatus::Completed,
                output: vec![],
                error: None,
                artifacts: vec![],
                input_required_data: None,
            })
        })
    }
}

// --- Test helpers ---

/// Bind to port 0 to get a free ephemeral port from the OS.
async fn free_port() -> u16 {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    l.local_addr().unwrap().port()
}

/// Generate a unique Unix socket path in /tmp.
fn temp_socket_path() -> PathBuf {
    let id = &uuid::Uuid::new_v4().to_string()[..8];
    PathBuf::from(format!("/tmp/ap-chain-{id}.sock"))
}

/// Build a self-contained AppState with real actors and MockBackend.
fn build_app_state() -> AppState<MockBackend> {
    let (event_sender, _rx) = EventBus::new();
    let registry_handle = AgentRegistry::spawn(event_sender.clone());
    let router_handle: TaskRouterHandle<MockBackend> =
        TaskRouterHandle::spawn(registry_handle.clone(), event_sender.clone(), 256);
    AppState {
        router_handle,
        registry_handle,
        event_sender,
        agent_loader: Arc::new(StubAgentLoader),
        backend: MockBackend,
        llm_router: None,
        trigger_engine: None,
        config_path: None,
        task_repository: None,
        pending_approvals: None,
        notification_config: None,
        pipeline_engine: None,
        backend_factory: None,
        tool_registry_handle: None,
        audit_trail: None,
    }
}

/// Start an APIServer on a free port. Returns (handle, tcp_port, socket_path).
async fn start_test_server() -> (APIServerHandle, u16, PathBuf) {
    let port = free_port().await;
    let socket_path = temp_socket_path();
    let state = build_app_state();
    let config = APIServerConfig {
        socket_path: socket_path.clone(),
        tcp_port: port,
    };
    let server = APIServer::new(config, state);
    let handle = server.start().await.expect("APIServer start failed");
    // Allow listeners to be ready before sending requests.
    tokio::time::sleep(Duration::from_millis(30)).await;
    (handle, port, socket_path)
}

/// Send an HTTP POST with a JSON body to the given TCP port + path.
/// Returns (http_status_code, response_json).
async fn http_post(port: u16, path: &str, body: serde_json::Value) -> (u16, serde_json::Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let body_bytes = serde_json::to_vec(&body).unwrap();
    let header = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body_bytes.len()
    );

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .expect("TCP connect failed for POST");
    stream.write_all(header.as_bytes()).await.unwrap();
    stream.write_all(&body_bytes).await.unwrap();

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    parse_http_response(&buf)
}

/// Send an HTTP GET to the given TCP port + path.
/// Returns (http_status_code, response_json).
async fn http_get(port: u16, path: &str) -> (u16, serde_json::Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let request = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .expect("TCP connect failed for GET");
    stream.write_all(request.as_bytes()).await.unwrap();

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    parse_http_response(&buf)
}

/// Parse raw HTTP/1.1 response bytes into (status_code, JSON body).
fn parse_http_response(raw: &[u8]) -> (u16, serde_json::Value) {
    let text = String::from_utf8_lossy(raw);

    // Status line: "HTTP/1.1 201 Created"
    let status: u16 = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);

    // Body follows the blank line (headers / body separator).
    let body_raw = text.split("\r\n\r\n").nth(1).unwrap_or("").trim();

    // Handle chunked transfer encoding: locate the outermost JSON object.
    let json_str = match (body_raw.find('{'), body_raw.rfind('}')) {
        (Some(start), Some(end)) if end >= start => &body_raw[start..=end],
        _ => body_raw,
    };

    let json = serde_json::from_str(json_str).unwrap_or(serde_json::Value::Null);
    (status, json)
}

/// Poll GET /api/v1/tasks/{task_id} until the status leaves "working" / "submitted".
/// Panics if the timeout (2 s) is exceeded.
async fn poll_until_terminal(port: u16, task_id: &str) -> String {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let (_, resp) = http_get(port, &format!("/api/v1/tasks/{task_id}")).await;
        let status = resp["status"].as_str().unwrap_or("").to_string();
        if status != "working" && status != "submitted" && !status.is_empty() {
            return status;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("timeout polling task {task_id} — last status: '{status}'");
        }
        tokio::task::yield_now().await;
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

// AC-1 — POST /api/v1/agents returns 201 Created with state "active"
#[tokio::test]
async fn test_start_agent_via_api_returns_active() {
    // GIVEN a runtime with MockBackend + StubAgentLoader
    let (handle, port, socket_path) = start_test_server().await;

    // WHEN POST /api/v1/agents with a path that StubAgentLoader can resolve
    let (status, resp) = http_post(
        port,
        "/api/v1/agents",
        serde_json::json!({ "agent_path": "agents/mock-agent.py" }),
    )
    .await;

    // THEN 201 Created
    assert_eq!(status, 201, "expected 201 Created, got {status}: {resp}");
    // AND state is "active"
    assert_eq!(resp["state"], "active", "expected state 'active': {resp}");
    // AND agent_id is a non-empty string
    let agent_id = resp["agent_id"]
        .as_str()
        .expect("agent_id must be a string");
    assert!(!agent_id.is_empty(), "agent_id should not be empty");

    // AC-5: cleanup
    handle.shutdown();
    tokio::time::sleep(Duration::from_millis(20)).await;
    let _ = std::fs::remove_file(&socket_path);
}

// AC-2 — POST /api/v1/tasks returns 202 Accepted with a task_id
#[tokio::test]
async fn test_submit_task_via_api_accepted() {
    // GIVEN an active agent registered via the API
    let (handle, port, socket_path) = start_test_server().await;

    let (_, agent_resp) = http_post(
        port,
        "/api/v1/agents",
        serde_json::json!({ "agent_path": "agents/mock-agent.py" }),
    )
    .await;
    let agent_id = agent_resp["agent_id"].as_str().expect("agent_id missing");

    // WHEN POST /api/v1/tasks with the agent UUID
    let (status, resp) = http_post(
        port,
        "/api/v1/tasks",
        serde_json::json!({ "agent_id": agent_id, "input": { "prompt": "hello" } }),
    )
    .await;

    // THEN 202 Accepted
    assert_eq!(status, 202, "expected 202 Accepted, got {status}: {resp}");
    // AND task_id is a non-empty string
    let task_id = resp["task_id"].as_str().expect("task_id must be a string");
    assert!(!task_id.is_empty(), "task_id should not be empty");

    // AC-5: cleanup
    handle.shutdown();
    tokio::time::sleep(Duration::from_millis(20)).await;
    let _ = std::fs::remove_file(&socket_path);
}

// AC-3 + AC-4 — task submitted by manifest name reaches "completed" status
#[tokio::test]
async fn test_task_completes_end_to_end_mock_backend() {
    // GIVEN an agent registered with manifest.name = "mock-agent"
    let (handle, port, socket_path) = start_test_server().await;

    http_post(
        port,
        "/api/v1/agents",
        serde_json::json!({ "agent_path": "agents/mock-agent.py" }),
    )
    .await;

    // WHEN POST /api/v1/tasks using the manifest name (not the UUID) — AC-4
    let (status, task_resp) = http_post(
        port,
        "/api/v1/tasks",
        serde_json::json!({ "agent_id": "mock-agent", "input": { "prompt": "run!" } }),
    )
    .await;

    // THEN 202 Accepted (name resolution worked)
    assert_eq!(
        status, 202,
        "submit by name should return 202, got {status}: {task_resp}"
    );
    let task_id = task_resp["task_id"].as_str().expect("task_id missing");

    // WHEN polling GET /api/v1/tasks/{task_id} until terminal — AC-3
    let final_status = poll_until_terminal(port, task_id).await;

    // THEN status is "completed" (MockBackend completes instantly)
    assert_eq!(
        final_status, "completed",
        "expected 'completed', got '{final_status}'"
    );

    // AC-5: cleanup
    handle.shutdown();
    tokio::time::sleep(Duration::from_millis(20)).await;
    let _ = std::fs::remove_file(&socket_path);
}

// AC-5 — TCP port and Unix socket are released after server shutdown
#[tokio::test]
async fn test_cleanup_socket_and_port_after_shutdown() {
    // GIVEN a running APIServer
    let (handle, port, socket_path) = start_test_server().await;

    // Verify it's reachable
    let (health_status, _) = http_get(port, "/api/v1/health").await;
    assert_eq!(
        health_status, 200,
        "server should be reachable before shutdown"
    );
    assert!(
        socket_path.exists(),
        "Unix socket file should exist while server is running"
    );

    // WHEN the server is shut down
    handle.shutdown();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // THEN the TCP port no longer accepts connections
    let connect_result = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}")).await;
    assert!(
        connect_result.is_err(),
        "TCP port {port} should be released after shutdown"
    );

    // AND the socket file can be cleaned up (no lock held by server)
    let _ = std::fs::remove_file(&socket_path);
    assert!(
        !socket_path.exists(),
        "socket file should be removable after shutdown"
    );
}
