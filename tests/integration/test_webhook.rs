//! Integration tests - route `POST /webhooks/:id`.
//!
//! Covers 3 scenarios: valid POST (correct HMAC), invalid POST (wrong
//! signature), and POST on an unknown trigger (404).
//!
//! **Valid POST**: correct HMAC -> HTTP 200 + 1 submit()
//! **Invalid POST**: wrong signature -> HTTP 401 + 0 submit()
//! **Unknown trigger**: HTTP 404 + 0 submit()
//!
//! Pattern: a real TCP server (ephemeral port) plus raw HTTP requests.

use apollia_e2e_tests::reserve_port;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use hmac::{Hmac, Mac};
use sha2::Sha256;

use apollia_core::{AIPInput, ObservabilityConfig, TaskId};
use apollia_runtime::{
    api::routes_agents::StubAgentLoader, api::APIServer, api::APIServerConfig,
    api::APIServerHandle, api::AppState, coordinator::DynBackend, coordinator::ExecutionBackend,
    eventbus::EventBus, registry::AgentRegistry, router::TaskRouterHandle,
};
use apollia_triggers::{
    InputTemplate, OnBusyPolicy, TaskSubmitter, TriggerDefinition, TriggerEngineHandle,
    TriggerSourceConfig,
};

// ─── MockTaskSubmitter ────────────────────────────────────────────────────

/// Mock [`TaskSubmitter`] counting the calls to `submit()`.
///
/// `pending_count` always returns 0 (the agent is free) in the webhook tests:
/// the point is to check whether the HTTP handler reaches the submission.
struct MockTaskSubmitter {
    submit_count: Arc<AtomicU32>,
}

impl MockTaskSubmitter {
    fn new() -> (Self, Arc<AtomicU32>) {
        let count = Arc::new(AtomicU32::new(0));
        (
            Self {
                submit_count: Arc::clone(&count),
            },
            count,
        )
    }
}

impl TaskSubmitter for MockTaskSubmitter {
    fn submit<'a>(
        &'a self,
        _agent: &'a str,
        _input: AIPInput,
    ) -> Pin<Box<dyn Future<Output = Result<TaskId, String>> + Send + 'a>> {
        self.submit_count.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(TaskId::new_v4()) })
    }

    fn pending_count<'a>(
        &'a self,
        _agent: &'a str,
    ) -> Pin<Box<dyn Future<Output = usize> + Send + 'a>> {
        Box::pin(async { 0_usize })
    }
}

// ─── MockBackend ──────────────────────────────────────────────────────────

/// Minimal execution backend - never called in the webhook tests.
#[derive(Clone)]
struct MockBackend;

impl From<DynBackend> for MockBackend {
    fn from(_: DynBackend) -> Self {
        MockBackend
    }
}

impl ExecutionBackend for MockBackend {
    fn execute(
        &self,
        task: apollia_core::AIPTask,
    ) -> Pin<Box<dyn Future<Output = Result<apollia_core::AIPResult, String>> + Send>> {
        Box::pin(async move {
            Ok(apollia_core::AIPResult {
                task_id: task.task_id,
                status: apollia_core::TaskStatus::Completed,
                output: vec![],
                error: None,
                artifacts: vec![],
                input_required_data: None,
            })
        })
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────

/// Computes the HMAC-SHA256 signature in the `sha256=<hex>` format.
fn compute_hmac(secret: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC key creation failed in test helper");
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

/// Builds a webhook `TriggerDefinition`.
fn webhook_def(id: &str, secret: &str) -> TriggerDefinition {
    TriggerDefinition {
        id: id.into(),
        agent: "crm-agent".into(),
        enabled: true,
        on_busy: OnBusyPolicy::Queue { max_depth: 16 },
        source: TriggerSourceConfig::Webhook {
            secret: secret.into(),
        },
        input_template: InputTemplate("{{body}}".into()),
    }
}

/// Builds an `AppState<MockBackend>` with a started `TriggerEngine`.
///
/// Returns `(state, submit_count)`.
async fn build_webhook_state(
    defs: Vec<TriggerDefinition>,
) -> (AppState<MockBackend>, Arc<AtomicU32>) {
    let (event_sender, _rx) = EventBus::new();
    let registry_handle = AgentRegistry::spawn(event_sender.clone());
    let router_handle: TaskRouterHandle<MockBackend> =
        TaskRouterHandle::spawn(registry_handle.clone(), event_sender.clone(), 64);

    let (mock_submitter, submit_count) = MockTaskSubmitter::new();
    let engine_handle = TriggerEngineHandle::spawn(
        defs,
        mock_submitter,
        event_sender.clone(),
        None,
        ObservabilityConfig::default(),
    )
    .await;

    let state = AppState {
        router_handle,
        registry_handle,
        event_sender,
        agent_loader: Arc::new(StubAgentLoader),
        plan_gates: None,
        audit_journal: None,
        backend: MockBackend,
        llm_router: apollia_runtime::api::server::empty_shared_llm_router(),
        trigger_engine: Some(engine_handle),
        config_path: None,
        task_repository: None,
        pending_approvals: None,
        notification_config: None,
        backend_factory: None,
        tool_registry_handle: None,
        audit_trail: None,
        obs_config: apollia_core::ObservabilityConfig::default(),
        llm_call_repository: None,
        trigger_def_repo: None,
        notification_repo: None,
        notification_engine_handle: None,
        chat_manager: None,
        plan_cache: None,
        mailbox_handle: None,
        user_memory: None,
        stt_engine: apollia_runtime::api::server::empty_shared_stt_engine(),
        stt_repository: apollia_runtime::api::server::empty_shared_stt_repository(),
        data_dir: std::path::PathBuf::new(),
        mcp_handle: None,
        mcp_server_repo: None,
        llm_backend_repo: None,
        stt_config_repo: None,
        a2a_invoker: None,
        resilience_layer: None,
        runner_proxy: None,
        llama_server_supervisor: None,
    };
    (state, submit_count)
}

/// Starts an `APIServer` on a port reserved outside the ephemeral pool.
///
/// Returns `(handle, port, socket_path)`.
async fn start_test_server(state: AppState<MockBackend>) -> (APIServerHandle, u16, PathBuf) {
    let reserved_port = reserve_port();
    let port = reserved_port.port();

    let id = &uuid::Uuid::new_v4().to_string()[..8];
    let socket_path = PathBuf::from(format!("/tmp/ap-webhook-{id}.sock"));

    let config = APIServerConfig {
        socket_path: socket_path.clone(),
        tcp_port: Some(port),
        bind_addr: "127.0.0.1".to_string(),
        api_token: None,
        tls_cert_path: None,
        tls_key_path: None,
    };
    let server = APIServer::new(config, state);
    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let handle = server.start().await.expect("APIServer start failed");

    // Let the TCP listener become ready
    tokio::time::sleep(Duration::from_millis(50)).await;
    (handle, port, socket_path)
}

/// Sends an HTTP/1.1 `POST` request with custom headers.
///
/// Returns the HTTP status code.
async fn http_post_webhook(
    port: u16,
    path: &str,
    body: &[u8],
    extra_headers: &[(&str, &str)],
) -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut header = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost:{port}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in extra_headers {
        header.push_str(&format!("{name}: {value}\r\n"));
    }
    header.push_str("\r\n");

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .expect("TCP connect failed");
    stream
        .write_all(header.as_bytes())
        .await
        .expect("write header failed");
    stream.write_all(body).await.expect("write body failed");

    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .await
        .expect("read response failed");

    // Extract the status code from the first line: "HTTP/1.1 200 OK"
    String::from_utf8_lossy(&buf)
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}

/// POST with a valid HMAC signature -> HTTP 200 + 1 submit().
#[tokio::test]
async fn test_ac3_valid_webhook_returns_200() {
    // GIVEN - a server with the webhook trigger "crm-sync" (secret "test-secret")
    let def = webhook_def("crm-sync", "test-secret");
    let (state, submit_count) = build_webhook_state(vec![def]).await;
    let (_handle, port, _socket) = start_test_server(state).await;

    let body = b"{\"event\": \"lead_created\"}";
    let sig = compute_hmac("test-secret", body);

    // WHEN - a valid POST with the correct HMAC signature
    let status = http_post_webhook(
        port,
        "/webhooks/crm-sync",
        body,
        &[
            ("Content-Type", "application/json"),
            ("X-Apollia-Signature", &sig),
        ],
    )
    .await;

    // THEN - HTTP 200 comes back at once
    assert_eq!(status, 200, "expected HTTP 200 for valid webhook POST");

    // AND - 1 submit() happened in the TriggerEngine (fire-and-forget, polled)
    let submitted = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if submit_count.load(Ordering::SeqCst) >= 1 {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        submitted.unwrap_or(false),
        "expected 1 submit() after valid webhook, got {}",
        submit_count.load(Ordering::SeqCst)
    );
}

/// POST with an invalid signature -> HTTP 401 + zero submit().
#[tokio::test]
async fn test_ac4_invalid_signature_returns_401() {
    // GIVEN - the same server with the webhook trigger "crm-sync"
    let def = webhook_def("crm-sync", "test-secret");
    let (state, submit_count) = build_webhook_state(vec![def]).await;
    let (_handle, port, _socket) = start_test_server(state).await;

    let body = b"{\"event\": \"lead_created\"}";
    // Wrong signature - 64 hexadecimal zeroes
    let bad_sig = "sha256=0000000000000000000000000000000000000000000000000000000000000000";

    // WHEN - a POST carrying an invalid signature
    let status = http_post_webhook(
        port,
        "/webhooks/crm-sync",
        body,
        &[
            ("Content-Type", "application/json"),
            ("X-Apollia-Signature", bad_sig),
        ],
    )
    .await;

    // THEN - HTTP 401 comes back, and nothing is submitted
    assert_eq!(
        status, 401,
        "expected HTTP 401 for invalid webhook signature"
    );
    assert_eq!(
        submit_count.load(Ordering::SeqCst),
        0,
        "submit() must NOT be called when HMAC signature is invalid"
    );
}

// ─── Extra: 404 for an unknown trigger ────────────────────────────────────

/// Extra: POST on an unknown trigger -> HTTP 404 + zero submit().
#[tokio::test]
async fn test_webhook_unknown_trigger_returns_404() {
    // GIVEN - a server carrying the "crm-sync" trigger only
    let def = webhook_def("crm-sync", "test-secret");
    let (state, submit_count) = build_webhook_state(vec![def]).await;
    let (_handle, port, _socket) = start_test_server(state).await;

    let body = b"{}";
    let sig = compute_hmac("test-secret", body);

    // WHEN - a POST on a trigger id that does not exist
    let status = http_post_webhook(
        port,
        "/webhooks/nonexistent-trigger",
        body,
        &[("X-Apollia-Signature", &sig)],
    )
    .await;

    // THEN - HTTP 404 comes back, and nothing is submitted
    assert_eq!(status, 404, "expected HTTP 404 for unknown webhook trigger");
    assert_eq!(
        submit_count.load(Ordering::SeqCst),
        0,
        "submit() must NOT be called for unknown trigger"
    );
}
