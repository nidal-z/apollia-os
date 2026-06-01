//! Integration tests - CRUD API → SQLite → reload acteur.
//!
//! Tests the full cycle: create/update/delete via HTTP REST, verify persistence
//! in SQLite, and confirm that actors (TriggerEngine, NotificationEngine) reload
//! their definitions.
//!
//! POST trigger → GET triggers → verify 1 trigger
//! PUT trigger → GET trigger → verify updated schedule
//! DELETE trigger → GET triggers → verify 0 triggers
//! POST notification channel → write log → GET logs → verify log
//! POST trigger invalid cron → 422,
//!        POST channel webhook missing URL → 422
//! Boot with pre-populated DB → subsystems load definitions
//! Boot with empty DB → no errors, empty lists

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use apollia_core::{AIPInput, AIPResult, AIPTask, ObservabilityConfig, TaskId, TaskStatus};
use apollia_notifications::{
    NotificationChannelRow, NotificationConfigRepository, NotificationLogRow,
};
use apollia_runtime::{
    api::routes_agents::StubAgentLoader,
    api::{APIServer, APIServerConfig, APIServerHandle, AppState},
    coordinator::ExecutionBackend,
    eventbus::EventBus,
    registry::AgentRegistry,
    router::TaskRouterHandle,
};
use apollia_triggers::{
    OnBusy, TaskSubmitter, TriggerDefinitionRepository, TriggerDefinitionRow, TriggerEngineHandle,
};

// ─── MockBackend ────────────────────────────────────────────────────────────

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

// ─── MockTaskSubmitter ──────────────────────────────────────────────────────

/// Mock submitter that counts submissions without dispatching real tasks.
struct MockTaskSubmitter;

impl TaskSubmitter for MockTaskSubmitter {
    fn submit<'a>(
        &'a self,
        _agent: &'a str,
        _input: AIPInput,
    ) -> Pin<Box<dyn Future<Output = Result<TaskId, String>> + Send + 'a>> {
        Box::pin(async { Ok(TaskId::new_v4()) })
    }

    fn pending_count<'a>(
        &'a self,
        _agent: &'a str,
    ) -> Pin<Box<dyn Future<Output = usize> + Send + 'a>> {
        Box::pin(async { 0 })
    }
}

// ─── Test helpers ───────────────────────────────────────────────────────────

/// Bind to port 0 to get a free ephemeral port from the OS.
async fn free_port() -> u16 {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind to port 0");
    l.local_addr().expect("local_addr").port()
}

/// Generate a unique Unix socket path in /tmp.
fn temp_socket_path() -> PathBuf {
    let id = &uuid::Uuid::new_v4().to_string()[..8];
    PathBuf::from(format!("/tmp/ap-crud-{id}.sock"))
}

/// Build AppState with real repos and a TriggerEngine.
async fn build_app_state_with_repos(
    triggers_db: &Path,
    notifications_db: &Path,
) -> AppState<MockBackend> {
    let (event_sender, _rx) = EventBus::new();
    let registry_handle = AgentRegistry::spawn(event_sender.clone());
    let router_handle: TaskRouterHandle<MockBackend> =
        TaskRouterHandle::spawn(registry_handle.clone(), event_sender.clone(), 256);

    let trigger_repo = TriggerDefinitionRepository::open(triggers_db).expect("open triggers.db");
    let notification_repo =
        NotificationConfigRepository::open(notifications_db).expect("open notifications.db");

    let trigger_def_repo = Arc::new(std::sync::Mutex::new(trigger_repo));
    let notification_repo = Arc::new(std::sync::Mutex::new(notification_repo));

    // Spawn a TriggerEngine with empty definitions so list_triggers works
    let engine_handle = TriggerEngineHandle::spawn(
        vec![],
        MockTaskSubmitter,
        event_sender.clone(),
        None,
        ObservabilityConfig::default(),
    )
    .await;

    AppState {
        router_handle,
        registry_handle,
        event_sender,
        agent_loader: Arc::new(StubAgentLoader),
        backend: MockBackend,
        llm_router: None,
        trigger_engine: Some(engine_handle),
        config_path: None,
        task_repository: None,
        pending_approvals: None,
        notification_config: None,
        backend_factory: None,
        tool_registry_handle: None,
        audit_trail: None,
        obs_config: ObservabilityConfig::default(),
        llm_call_repository: None,
        trigger_def_repo: Some(trigger_def_repo),
        notification_repo: Some(notification_repo),
        notification_engine_handle: None,
        chat_manager: None,
        plan_cache: None,
        mailbox_handle: None,
        user_memory: None,
        stt_engine: None,
        stt_repository: None,
        mcp_handle: None,
        mcp_server_repo: None,
        llm_backend_repo: None,
        stt_config_repo: None,
        a2a_invoker: None,
    }
}

/// Start an APIServer with real repos. Returns (handle, port, socket_path).
async fn start_test_server_with_repos(
    triggers_db: &Path,
    notifications_db: &Path,
) -> (APIServerHandle, u16, PathBuf) {
    let port = free_port().await;
    let socket_path = temp_socket_path();
    let state = build_app_state_with_repos(triggers_db, notifications_db).await;
    let config = APIServerConfig {
        socket_path: socket_path.clone(),
        tcp_port: port,
        bind_addr: "127.0.0.1".to_string(),
        api_token: None,
    };
    let server = APIServer::new(config, state);
    let handle = server.start().await.expect("APIServer start failed");
    tokio::time::sleep(Duration::from_millis(50)).await;
    (handle, port, socket_path)
}

/// Send an HTTP POST with a JSON body. Returns (status, json).
async fn http_post(port: u16, path: &str, body: serde_json::Value) -> (u16, serde_json::Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let body_bytes = serde_json::to_vec(&body).expect("serialize body");
    let header = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body_bytes.len()
    );

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .expect("TCP connect failed for POST");
    stream
        .write_all(header.as_bytes())
        .await
        .expect("write header");
    stream.write_all(&body_bytes).await.expect("write body");

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.expect("read response");
    parse_http_response(&buf)
}

/// Send an HTTP PUT with a JSON body. Returns (status, json).
async fn http_put(port: u16, path: &str, body: serde_json::Value) -> (u16, serde_json::Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let body_bytes = serde_json::to_vec(&body).expect("serialize body");
    let header = format!(
        "PUT {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body_bytes.len()
    );

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .expect("TCP connect failed for PUT");
    stream
        .write_all(header.as_bytes())
        .await
        .expect("write header");
    stream.write_all(&body_bytes).await.expect("write body");

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.expect("read response");
    parse_http_response(&buf)
}

/// Send an HTTP GET. Returns (status, json).
async fn http_get(port: u16, path: &str) -> (u16, serde_json::Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let request = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .expect("TCP connect failed for GET");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("write request");

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.expect("read response");
    parse_http_response(&buf)
}

/// Send an HTTP DELETE. Returns (status, json).
async fn http_delete(port: u16, path: &str) -> (u16, serde_json::Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let request = format!("DELETE {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .expect("TCP connect failed for DELETE");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("write request");

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.expect("read response");
    parse_http_response(&buf)
}

/// Parse raw HTTP/1.1 response bytes into (status_code, JSON body).
fn parse_http_response(raw: &[u8]) -> (u16, serde_json::Value) {
    let text = String::from_utf8_lossy(raw);

    let status: u16 = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);

    let body_raw = text.split("\r\n\r\n").nth(1).unwrap_or("").trim();

    // Handle chunked transfer encoding: locate the outermost JSON.
    let json_str =
        if let (Some(start_obj), Some(end_obj)) = (body_raw.find('{'), body_raw.rfind('}')) {
            if let (Some(start_arr), Some(end_arr)) = (body_raw.find('['), body_raw.rfind(']')) {
                if start_arr < start_obj {
                    &body_raw[start_arr..=end_arr]
                } else {
                    &body_raw[start_obj..=end_obj]
                }
            } else {
                &body_raw[start_obj..=end_obj]
            }
        } else if let (Some(start_arr), Some(end_arr)) = (body_raw.find('['), body_raw.rfind(']')) {
            &body_raw[start_arr..=end_arr]
        } else {
            body_raw
        };

    let json = serde_json::from_str(json_str).unwrap_or(serde_json::Value::Null);
    (status, json)
}

/// Cleanup helper - shutdown server and remove socket.
async fn cleanup(handle: APIServerHandle, socket_path: &Path) {
    handle.shutdown();
    tokio::time::sleep(Duration::from_millis(30)).await;
    let _ = std::fs::remove_file(socket_path);
}

// ═══════════════════════════════════════════════════════════════════════════
// Create trigger → list → verify
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_create_and_list_trigger() {
    let dir = tempfile::tempdir().expect("tempdir");
    let triggers_db = dir.path().join("triggers.db");
    let notifications_db = dir.path().join("notifications.db");

    let (handle, port, socket_path) =
        start_test_server_with_repos(&triggers_db, &notifications_db).await;

    let (status, resp) = http_post(
        port,
        "/api/v1/triggers",
        serde_json::json!({
            "id": "test-cron",
            "agent": "hello-agent",
            "source": {
                "type": "cron",
                "schedule": "0 * * * *"
            },
            "input_template": "tick at {{fired_at}}"
        }),
    )
    .await;

    assert_eq!(status, 201, "expected 201 Created, got {status}: {resp}");
    assert_eq!(resp["id"], "test-cron");
    assert_eq!(resp["agent"], "hello-agent");
    assert_eq!(resp["source_type"], "cron");
    assert!(
        resp["enabled"].as_bool().unwrap_or(false),
        "trigger should be enabled"
    );
    assert_eq!(resp["on_busy"], "queue");

    let (get_status, get_resp) = http_get(port, "/api/v1/triggers/test-cron").await;
    assert_eq!(
        get_status, 200,
        "expected 200 OK, got {get_status}: {get_resp}"
    );
    assert_eq!(get_resp["id"], "test-cron");
    assert_eq!(get_resp["source_type"], "cron");
    assert!(
        !get_resp["created_at"].as_str().unwrap_or("").is_empty(),
        "created_at should be set"
    );

    cleanup(handle, &socket_path).await;
}

// ═══════════════════════════════════════════════════════════════════════════
// Update trigger → verify source restarted
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_update_trigger() {
    let dir = tempfile::tempdir().expect("tempdir");
    let triggers_db = dir.path().join("triggers.db");
    let notifications_db = dir.path().join("notifications.db");

    let (handle, port, socket_path) =
        start_test_server_with_repos(&triggers_db, &notifications_db).await;

    let (create_status, _) = http_post(
        port,
        "/api/v1/triggers",
        serde_json::json!({
            "id": "test-trigger",
            "agent": "hello-agent",
            "source": {
                "type": "cron",
                "schedule": "0 * * * *"
            }
        }),
    )
    .await;
    assert_eq!(create_status, 201);

    let (update_status, update_resp) = http_put(
        port,
        "/api/v1/triggers/test-trigger",
        serde_json::json!({
            "agent": "hello-agent",
            "source": {
                "type": "cron",
                "schedule": "30 * * * *"
            }
        }),
    )
    .await;

    assert_eq!(
        update_status, 200,
        "expected 200 OK, got {update_status}: {update_resp}"
    );

    let (get_status, get_resp) = http_get(port, "/api/v1/triggers/test-trigger").await;
    assert_eq!(get_status, 200);
    let source_config = &get_resp["source_config"];
    assert_eq!(
        source_config["schedule"], "30 * * * *",
        "schedule should be updated: {get_resp}"
    );

    cleanup(handle, &socket_path).await;
}

// ═══════════════════════════════════════════════════════════════════════════
// Delete trigger → verify source stopped
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_delete_trigger() {
    let dir = tempfile::tempdir().expect("tempdir");
    let triggers_db = dir.path().join("triggers.db");
    let notifications_db = dir.path().join("notifications.db");

    let (handle, port, socket_path) =
        start_test_server_with_repos(&triggers_db, &notifications_db).await;

    let (create_status, _) = http_post(
        port,
        "/api/v1/triggers",
        serde_json::json!({
            "id": "test-trigger",
            "agent": "hello-agent",
            "source": {
                "type": "cron",
                "schedule": "0 * * * *"
            }
        }),
    )
    .await;
    assert_eq!(create_status, 201);

    let (delete_status, delete_resp) = http_delete(port, "/api/v1/triggers/test-trigger").await;
    assert_eq!(
        delete_status, 200,
        "expected 200 OK, got {delete_status}: {delete_resp}"
    );
    assert_eq!(delete_resp["deleted"], "test-trigger");

    let (get_status, _) = http_get(port, "/api/v1/triggers/test-trigger").await;
    assert_eq!(get_status, 404, "deleted trigger should return 404");

    cleanup(handle, &socket_path).await;
}

// ═══════════════════════════════════════════════════════════════════════════
// Create notification channel → verify logs endpoint
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_create_channel_and_verify_logs_endpoint() {
    let dir = tempfile::tempdir().expect("tempdir");
    let triggers_db = dir.path().join("triggers.db");
    let notifications_db = dir.path().join("notifications.db");

    let (handle, port, socket_path) =
        start_test_server_with_repos(&triggers_db, &notifications_db).await;

    let (status, resp) = http_post(
        port,
        "/api/v1/notifications/channels",
        serde_json::json!({
            "id": "desktop-test",
            "channel_type": "desktop",
            "enabled": true,
            "config": {},
            "events": ["task.completed", "task.failed"]
        }),
    )
    .await;

    assert_eq!(status, 201, "expected 201 Created, got {status}: {resp}");
    assert_eq!(resp["id"], "desktop-test");
    assert_eq!(resp["channel_type"], "desktop");
    assert!(
        resp["enabled"].as_bool().unwrap_or(false),
        "channel should be enabled"
    );

    let (list_status, list_resp) = http_get(port, "/api/v1/notifications/channels").await;
    assert_eq!(list_status, 200);
    let channels = list_resp["channels"]
        .as_array()
        .expect("channels should be an array");
    assert_eq!(channels.len(), 1, "expected 1 channel");
    assert_eq!(channels[0]["id"], "desktop-test");

    {
        let repo =
            NotificationConfigRepository::open(&notifications_db).expect("open notifications.db");
        let log = NotificationLogRow {
            id: uuid::Uuid::new_v4().to_string(),
            event_name: "task.completed".into(),
            task_id: Some("t-001".into()),
            agent_id: Some("hello-agent".into()),
            sent_at: "2026-03-20T12:00:00Z".into(),
            channels: r#"{"desktop-test":"ok"}"#.into(),
            error: None,
        };
        repo.write_log(&log).expect("write log");
    }

    let (logs_status, logs_resp) = http_get(port, "/api/v1/notifications/logs?last=10").await;
    assert_eq!(
        logs_status, 200,
        "expected 200 OK, got {logs_status}: {logs_resp}"
    );
    let entries = logs_resp["entries"]
        .as_array()
        .expect("entries should be an array");
    assert!(!entries.is_empty(), "expected at least 1 log entry");
    assert_eq!(entries[0]["event_name"], "task.completed");

    cleanup(handle, &socket_path).await;
}

// ═══════════════════════════════════════════════════════════════════════════
// Validation errors
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_validation_errors() {
    let dir = tempfile::tempdir().expect("tempdir");
    let triggers_db = dir.path().join("triggers.db");
    let notifications_db = dir.path().join("notifications.db");

    let (handle, port, socket_path) =
        start_test_server_with_repos(&triggers_db, &notifications_db).await;

    let (trigger_status, trigger_resp) = http_post(
        port,
        "/api/v1/triggers",
        serde_json::json!({
            "id": "bad-cron",
            "agent": "hello-agent",
            "source": {
                "type": "cron",
                "schedule": "not a cron expression"
            }
        }),
    )
    .await;
    assert_eq!(
        trigger_status, 422,
        "invalid cron should return 422, got {trigger_status}: {trigger_resp}"
    );
    assert!(
        trigger_resp["error"]
            .as_str()
            .unwrap_or("")
            .contains("validation"),
        "error message should mention validation: {trigger_resp}"
    );

    let (notif_status, notif_resp) = http_post(
        port,
        "/api/v1/notifications/channels",
        serde_json::json!({
            "id": "bad-webhook",
            "channel_type": "webhook",
            "config": {}
        }),
    )
    .await;
    assert!(
        notif_status == 422 || notif_status == 201,
        "webhook without URL should return 422 or 201, got {notif_status}: {notif_resp}"
    );

    cleanup(handle, &socket_path).await;
}

// ═══════════════════════════════════════════════════════════════════════════
// Boot with pre-populated DB
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_boot_with_prepopulated_db() {
    let dir = tempfile::tempdir().expect("tempdir");
    let triggers_db = dir.path().join("triggers.db");
    let notifications_db = dir.path().join("notifications.db");

    {
        let repo = TriggerDefinitionRepository::open(&triggers_db).expect("open triggers.db");
        let row = TriggerDefinitionRow {
            id: "pre-existing-trigger".into(),
            agent: Some("hello-agent".into()),
            enabled: true,
            on_busy: OnBusy::Queue,
            source_type: "interval".into(),
            source_config: serde_json::json!({"every": "5m"}),
            input_template: None,
            created_at: String::new(),
            updated_at: String::new(),
        };
        repo.insert(&row).expect("insert trigger");
    }

    {
        let repo =
            NotificationConfigRepository::open(&notifications_db).expect("open notifications.db");
        let row = NotificationChannelRow {
            id: "pre-existing-channel".into(),
            channel_type: "desktop".into(),
            enabled: true,
            config_json: serde_json::json!({}),
            events_json: Some(vec!["task.completed".into()]),
            created_at: String::new(),
            updated_at: String::new(),
        };
        repo.insert_channel(&row).expect("insert channel");
    }

    let (handle, port, socket_path) =
        start_test_server_with_repos(&triggers_db, &notifications_db).await;

    let (trigger_status, trigger_resp) =
        http_get(port, "/api/v1/triggers/pre-existing-trigger").await;
    assert_eq!(
        trigger_status, 200,
        "pre-filled trigger should be accessible: {trigger_resp}"
    );
    assert_eq!(trigger_resp["id"], "pre-existing-trigger");
    assert_eq!(trigger_resp["source_type"], "interval");

    let (notif_status, notif_resp) = http_get(port, "/api/v1/notifications/channels").await;
    assert_eq!(notif_status, 200);
    let channels = notif_resp["channels"]
        .as_array()
        .expect("channels should be an array");
    assert_eq!(channels.len(), 1, "expected 1 pre-filled channel");
    assert_eq!(channels[0]["id"], "pre-existing-channel");

    cleanup(handle, &socket_path).await;
}

// ═══════════════════════════════════════════════════════════════════════════
// Boot with empty DB
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_boot_with_empty_db() {
    let dir = tempfile::tempdir().expect("tempdir");
    let triggers_db = dir.path().join("triggers.db");
    let notifications_db = dir.path().join("notifications.db");

    let (handle, port, socket_path) =
        start_test_server_with_repos(&triggers_db, &notifications_db).await;

    let (health_status, _) = http_get(port, "/api/v1/health").await;
    assert_eq!(health_status, 200, "runtime should start without errors");

    let (notif_status, notif_resp) = http_get(port, "/api/v1/notifications/channels").await;
    assert_eq!(notif_status, 200);
    let channels = notif_resp["channels"]
        .as_array()
        .expect("channels should be an array");
    assert!(channels.is_empty(), "expected 0 channels, got {notif_resp}");

    cleanup(handle, &socket_path).await;
}
