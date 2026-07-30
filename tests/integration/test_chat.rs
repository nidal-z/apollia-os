//! Integration tests - chat subsystem end-to-end.
//!
//! Tests the complete chat flow: HTTP API → ChatSessionManager →
//! BuiltInChatAgent (mock LLM) → EventBus → response.
//!
//! Chat Libre simple exchange (create → send → response)
//! Chat Libre tool call + HITL accept
//! Chat Libre "Always Accept" (second call skips approval)
//! Chat Libre "Refuse" (tool refused, LLM continues)
//! Chat Agent with hello-agent (gated by python-tests)
//! History accumulation (3 exchanges → 6 messages)
//! Close session (DELETE → 409 on subsequent message)
//! Budget exhausted (infinite tool loop → ChatError)

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::stream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use apollia_core::{AIPResult, AIPTask, RuntimeEvent, StepBudgetConfig, TaskStatus};
use apollia_llm::types::{
    CompletionRequest, CompletionResponse, FinishReason, LlmError, StreamChunk, TokenUsage,
    ToolCall,
};
use apollia_llm::{CompletionModel, LlmRouter};
use apollia_runtime::{
    api::routes_agents::StubAgentLoader,
    api::{APIServer, APIServerConfig, APIServerHandle, AppState},
    chat::ChatSessionManagerHandle,
    coordinator::{DynBackend, ExecutionBackend},
    eventbus::EventBus,
    registry::AgentRegistry,
    router::TaskRouterHandle,
};
use apollia_tools::ToolRegistryHandle;

// ─── MockChatModel ──────────────────────────────────────────────────────────

/// Mock [`CompletionModel`] that pops predefined responses from a queue.
///
/// Each call to `complete()` or `stream()` consumes the next response in order.
/// If the queue is empty, returns a fallback "no more responses" text.
struct MockChatModel {
    /// Queue of responses to return, consumed in FIFO order.
    responses: Mutex<VecDeque<CompletionResponse>>,
}

impl MockChatModel {
    /// Create a new mock model with the given response sequence.
    fn new(responses: Vec<CompletionResponse>) -> Self {
        Self {
            responses: Mutex::new(VecDeque::from(responses)),
        }
    }
}

/// Build a text-only response with no tool calls.
fn text_response(content: &str) -> CompletionResponse {
    CompletionResponse {
        engine_timings: None,
        content: content.to_string(),
        tool_calls: vec![],
        usage: TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            cost_usd: None,
            cache_read_input_tokens: 0,
            cache_write_input_tokens: 0,
        },
        finish_reason: FinishReason::Stop,
        latency_ms: 1,
        ttft_ms: None,
    }
}

/// Build a response that requests a tool call.
fn tool_call_response(tool_name: &str, arguments: serde_json::Value) -> CompletionResponse {
    CompletionResponse {
        engine_timings: None,
        content: String::new(),
        tool_calls: vec![ToolCall {
            id: format!("call_{tool_name}"),
            name: tool_name.to_string(),
            arguments,
        }],
        usage: TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            cost_usd: None,
            cache_read_input_tokens: 0,
            cache_write_input_tokens: 0,
        },
        finish_reason: FinishReason::ToolCalls,
        latency_ms: 1,
        ttft_ms: None,
    }
}

#[async_trait::async_trait]
impl CompletionModel for MockChatModel {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let response = self
            .responses
            .lock()
            .map_err(|e| LlmError::InferenceError(e.to_string()))?
            .pop_front()
            .unwrap_or_else(|| text_response("(no more mock responses)"));
        Ok(response)
    }

    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
    {
        let response = self.complete(req).await?;
        let mut chunks: Vec<Result<StreamChunk, LlmError>> = Vec::new();

        if !response.content.is_empty() {
            // Emit each word as a separate token for realistic streaming
            for word in response.content.split_whitespace() {
                chunks.push(Ok(StreamChunk::Text(format!("{word} "))));
            }
        }

        for call in response.tool_calls {
            chunks.push(Ok(StreamChunk::ToolCall(call)));
        }

        Ok(Box::pin(stream::iter(chunks)))
    }

    fn is_available(&self) -> bool {
        true
    }

    fn backend_name(&self) -> &str {
        "mock-chat"
    }

    fn model_id(&self) -> &str {
        "mock-chat-v1"
    }
}

// ─── MockBackend ────────────────────────────────────────────────────────────

/// Backend that completes tasks instantly (not used by chat, but required by AppState).
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
    PathBuf::from(format!("/tmp/ap-chat-{id}.sock"))
}

/// Generate a unique temporary directory for chat.db.
fn temp_chat_db() -> PathBuf {
    let id = &uuid::Uuid::new_v4().to_string()[..8];
    let dir = std::env::temp_dir().join(format!("apollia-chat-test-{id}"));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir.join("chat.db")
}

/// Build an AppState with a real ChatSessionManager wired to a mock LLM.
fn build_chat_app_state(
    mock_model: MockChatModel,
    budget_config: StepBudgetConfig,
) -> (
    AppState<MockBackend>,
    tokio::sync::broadcast::Receiver<RuntimeEvent>,
) {
    let (event_sender, _rx) = EventBus::new();
    let event_rx = event_sender.subscribe();
    let registry_handle = AgentRegistry::spawn(event_sender.clone());
    let router_handle: TaskRouterHandle<MockBackend> =
        TaskRouterHandle::spawn(registry_handle.clone(), event_sender.clone(), 256);

    let tool_registry = ToolRegistryHandle::start();

    // Build LlmRouter with mock model
    let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
    backends.insert("mock-chat".to_string(), Arc::new(mock_model));
    let llm_router = Arc::new(LlmRouter::with_backends(backends, "mock-chat"));

    let db_path = temp_chat_db();
    let chat_manager = ChatSessionManagerHandle::spawn(
        &db_path,
        Some(llm_router.clone()),
        tool_registry.clone(),
        Arc::new(StubAgentLoader),
        None, // no ChatAgentRunner for libre-only tests
        event_sender.clone(),
        budget_config,
        None, // no user memory in tests
        registry_handle.clone(),
        None, // no A2A invoker in tests
        None, // no project context in tests
        None, // no project repo in tests
        None, // no MCP handle in tests
        None, // no chat tools config in tests
        apollia_mcp::session::LoadingMode::Eager,
        20,    // tool_search_limit
        None,  // no hook executor in tests
        false, // plan_mode_default
    )
    .expect("ChatSessionManager spawn");

    let state = AppState {
        router_handle,
        registry_handle,
        event_sender,
        agent_loader: Arc::new(StubAgentLoader),
        plan_gates: None,
        audit_journal: None,
        backend: MockBackend,
        llm_router: apollia_runtime::api::server::shared_llm_router_from(Some(llm_router)),
        trigger_engine: None,
        config_path: None,
        task_repository: None,
        pending_approvals: None,
        notification_config: None,
        backend_factory: None,
        tool_registry_handle: Some(tool_registry),
        audit_trail: None,
        obs_config: apollia_core::ObservabilityConfig::default(),
        llm_call_repository: None,
        trigger_def_repo: None,
        notification_repo: None,
        notification_engine_handle: None,
        chat_manager: Some(chat_manager),
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

    (state, event_rx)
}

/// Start an APIServer with chat support on a free port.
async fn start_chat_server(
    mock_model: MockChatModel,
    budget_config: StepBudgetConfig,
) -> (
    APIServerHandle,
    u16,
    PathBuf,
    tokio::sync::broadcast::Receiver<RuntimeEvent>,
) {
    let port = free_port().await;
    let socket_path = temp_socket_path();
    let (state, event_rx) = build_chat_app_state(mock_model, budget_config);
    let config = APIServerConfig {
        socket_path: socket_path.clone(),
        tcp_port: Some(port),
        bind_addr: "127.0.0.1".to_string(),
        api_token: None,
        tls_cert_path: None,
        tls_key_path: None,
    };
    let server = APIServer::new(config, state);
    let handle = server.start().await.expect("APIServer start failed");
    tokio::time::sleep(Duration::from_millis(50)).await;
    (handle, port, socket_path, event_rx)
}

/// Start a chat server with default budget.
async fn start_chat_server_default(
    mock_model: MockChatModel,
) -> (
    APIServerHandle,
    u16,
    PathBuf,
    tokio::sync::broadcast::Receiver<RuntimeEvent>,
) {
    start_chat_server(mock_model, StepBudgetConfig::default()).await
}

/// Send an HTTP POST with a JSON body. Returns (status, json).
async fn http_post(port: u16, path: &str, body: serde_json::Value) -> (u16, serde_json::Value) {
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

/// Send an HTTP GET. Returns (status, json).
async fn http_get(port: u16, path: &str) -> (u16, serde_json::Value) {
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

    // Handle chunked transfer encoding: extract outermost JSON object or array
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

/// Collect runtime events from the EventBus until the expected one is found.
/// Returns all collected events.
async fn collect_events_until(
    event_rx: &mut tokio::sync::broadcast::Receiver<RuntimeEvent>,
    predicate: impl Fn(&RuntimeEvent) -> bool,
    timeout: Duration,
) -> Vec<RuntimeEvent> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut events = Vec::new();

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }

        match tokio::time::timeout(remaining, event_rx.recv()).await {
            Ok(Ok(event)) => {
                let matched = predicate(&event);
                events.push(event);
                if matched {
                    break;
                }
            }
            // The broadcast buffer overflowed between exchanges: keep receiving
            // rather than giving up, so a later matching event is not missed.
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }

    events
}

/// Cleanup: shutdown server and remove socket file.
fn cleanup(handle: APIServerHandle, socket_path: &PathBuf) {
    handle.shutdown();
    let _ = std::fs::remove_file(socket_path);
}

// ─── Tests ──────────────────────────────────────────────────────────────────

/// Chat Libre: create session → send message → receive response.
#[tokio::test]
async fn test_chat_libre_simple_exchange() {
    // GIVEN a mock LLM that returns "Bonjour !"
    let model = MockChatModel::new(vec![text_response("Bonjour !")]);
    let (handle, port, socket_path, mut event_rx) = start_chat_server_default(model).await;

    // WHEN creating a Libre session
    let (status, session) = http_post(
        port,
        "/api/v1/sessions",
        serde_json::json!({ "mode": "libre" }),
    )
    .await;
    assert_eq!(status, 201, "expected 201 Created, got {status}: {session}");
    let session_id = session["id"].as_str().expect("session id");

    // AND sending a message
    let (msg_status, _msg_resp) = http_post(
        port,
        &format!("/api/v1/sessions/{session_id}/messages"),
        serde_json::json!({ "content": "Hello" }),
    )
    .await;
    assert_eq!(msg_status, 202, "expected 202 Accepted");

    // THEN ChatResponseCompleted is emitted with "Bonjour !"
    let events = collect_events_until(
        &mut event_rx,
        |e| matches!(e, RuntimeEvent::ChatResponseCompleted { .. }),
        Duration::from_secs(5),
    )
    .await;

    let completed = events.iter().find(|e| {
        matches!(
            e,
            RuntimeEvent::ChatResponseCompleted { content, .. } if content.contains("Bonjour")
        )
    });
    assert!(
        completed.is_some(),
        "expected ChatResponseCompleted with 'Bonjour', got events: {events:?}"
    );

    // AND ChatMessageSent was emitted
    let sent = events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::ChatMessageSent { .. }));
    assert!(sent, "expected ChatMessageSent event");

    // AND GET /sessions/:id returns messages
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (get_status, detail) = http_get(port, &format!("/api/v1/sessions/{session_id}")).await;
    assert_eq!(get_status, 200);
    let msg_count = detail["message_count"].as_u64().unwrap_or(0);
    assert!(
        msg_count >= 2,
        "expected at least 2 messages (user + assistant), got {msg_count}"
    );

    cleanup(handle, &socket_path);
}

/// Chat Libre: tool call + HITL accept flow.
#[tokio::test]
async fn test_chat_libre_tool_call_hitl_accept() {
    // GIVEN a mock LLM: first returns tool_call, then returns text after tool result
    let model = MockChatModel::new(vec![
        tool_call_response("bash_executor", serde_json::json!({"command": "ls"})),
        text_response("Voici les fichiers"),
    ]);
    let (handle, port, socket_path, mut event_rx) =
        start_chat_server(model, StepBudgetConfig::default()).await;

    // WHEN creating a session with bash_executor tool
    let (_, session) = http_post(
        port,
        "/api/v1/sessions",
        serde_json::json!({ "mode": "libre", "tools": ["bash_executor"] }),
    )
    .await;
    let session_id = session["id"].as_str().expect("session id");

    // AND sending a message
    let (msg_status, _) = http_post(
        port,
        &format!("/api/v1/sessions/{session_id}/messages"),
        serde_json::json!({ "content": "List files" }),
    )
    .await;
    assert_eq!(msg_status, 202);

    // THEN ChatApprovalRequired is emitted
    let events = collect_events_until(
        &mut event_rx,
        |e| matches!(e, RuntimeEvent::ChatApprovalRequired { .. }),
        Duration::from_secs(5),
    )
    .await;

    let approval_event = events.iter().find(|e| {
        matches!(
            e,
            RuntimeEvent::ChatApprovalRequired {
                tool_name,
                ..
            } if tool_name == "bash_executor"
        )
    });
    assert!(
        approval_event.is_some(),
        "expected ChatApprovalRequired for bash_executor"
    );

    // Extract the correlation keys from the approval event. The pending slot is
    // keyed by tool_call_id, so a client that omits it cannot resolve the right
    // approval; the desktop reads both off the same event.
    let (approval_message_id, approval_tool_call_id) = match approval_event {
        Some(RuntimeEvent::ChatApprovalRequired {
            message_id,
            tool_call_id,
            ..
        }) => (message_id.clone(), tool_call_id.clone()),
        _ => panic!("expected ChatApprovalRequired"),
    };

    // WHEN accepting the tool call
    let (auth_status, _) = http_post(
        port,
        &format!("/api/v1/sessions/{session_id}/authorize"),
        serde_json::json!({
            "message_id": approval_message_id,
            "tool_call_id": approval_tool_call_id,
            "tool_name": "bash_executor",
            "decision": "accept"
        }),
    )
    .await;
    assert_eq!(auth_status, 200, "expected 200 OK for authorize");

    // THEN ChatResponseCompleted is emitted with "Voici les fichiers"
    let events = collect_events_until(
        &mut event_rx,
        |e| matches!(e, RuntimeEvent::ChatResponseCompleted { .. }),
        Duration::from_secs(5),
    )
    .await;

    let completed = events.iter().find(|e| {
        matches!(
            e,
            RuntimeEvent::ChatResponseCompleted { content, .. } if content.contains("Voici")
        )
    });
    assert!(
        completed.is_some(),
        "expected ChatResponseCompleted with 'Voici les fichiers'"
    );

    cleanup(handle, &socket_path);
}

/// Chat Libre: a session-scoped "Always Accept" auto-authorizes the tool for the
/// rest of the session, so a later exchange runs it without asking again.
///
/// Uses a non-code-executor tool on purpose: a code executor (bash/python) is
/// deliberately never blanket-authorized (its always-accept downgrades to a
/// one-time approval, see `chat/manager/exchange.rs` `is_code_executor`), so it
/// would re-ask. Completion (not an approval, which pauses the turn) is the
/// deterministic terminal signal here.
#[tokio::test]
async fn test_chat_libre_always_accept_persists_for_session() {
    // GIVEN mock LLM: exchange1 = tool_call → text, exchange2 = tool_call → text
    let model = MockChatModel::new(vec![
        tool_call_response("file_read", serde_json::json!({"path": "/tmp/a.txt"})),
        text_response("First response"),
        tool_call_response("file_read", serde_json::json!({"path": "/tmp/b.txt"})),
        text_response("Second response"),
    ]);
    let (handle, port, socket_path, mut event_rx) =
        start_chat_server(model, StepBudgetConfig::default()).await;

    // Create a libre session that offers file_read.
    let (_, session) = http_post(
        port,
        "/api/v1/sessions",
        serde_json::json!({ "mode": "libre", "tools": ["file_read"] }),
    )
    .await;
    let session_id = session["id"].as_str().expect("session id");

    // Exchange 1: send message, wait for approval, always_accept
    let _ = http_post(
        port,
        &format!("/api/v1/sessions/{session_id}/messages"),
        serde_json::json!({ "content": "First" }),
    )
    .await;

    let events = collect_events_until(
        &mut event_rx,
        |e| matches!(e, RuntimeEvent::ChatApprovalRequired { .. }),
        Duration::from_secs(5),
    )
    .await;

    let (msg_id, tool_call_id) = match events
        .iter()
        .find(|e| matches!(e, RuntimeEvent::ChatApprovalRequired { .. }))
    {
        Some(RuntimeEvent::ChatApprovalRequired {
            message_id,
            tool_call_id,
            ..
        }) => (message_id.clone(), tool_call_id.clone()),
        _ => panic!("expected ChatApprovalRequired for the first file_read"),
    };

    let _ = http_post(
        port,
        &format!("/api/v1/sessions/{session_id}/authorize"),
        serde_json::json!({
            "message_id": msg_id,
            "tool_call_id": tool_call_id,
            "tool_name": "file_read",
            "decision": "always_accept"
        }),
    )
    .await;

    // Wait for exchange 1 to complete.
    let _ = collect_events_until(
        &mut event_rx,
        |e| matches!(e, RuntimeEvent::ChatResponseCompleted { .. }),
        Duration::from_secs(5),
    )
    .await;

    // Exchange 2: file_read is now session-authorized, so the turn runs to
    // completion WITHOUT another approval.
    let _ = http_post(
        port,
        &format!("/api/v1/sessions/{session_id}/messages"),
        serde_json::json!({ "content": "Second" }),
    )
    .await;

    let events2 = collect_events_until(
        &mut event_rx,
        |e| matches!(e, RuntimeEvent::ChatResponseCompleted { .. }),
        Duration::from_secs(10),
    )
    .await;

    let had_approval = events2
        .iter()
        .any(|e| matches!(e, RuntimeEvent::ChatApprovalRequired { .. }));
    let completed = events2
        .iter()
        .any(|e| matches!(e, RuntimeEvent::ChatResponseCompleted { .. }));
    assert!(
        !had_approval,
        "second exchange must NOT ask for approval after a session-scoped always-accept"
    );
    assert!(completed, "second exchange should run to completion");

    cleanup(handle, &socket_path);
}

/// Chat Libre: refuse tool → LLM continues without tool result.
#[tokio::test]
async fn test_chat_libre_refuse() {
    // GIVEN mock LLM: tool_call → (after refusal) text response
    let model = MockChatModel::new(vec![
        tool_call_response("bash_executor", serde_json::json!({"command": "rm -rf /"})),
        text_response("D'accord, je ne fais rien."),
    ]);
    let (handle, port, socket_path, mut event_rx) =
        start_chat_server(model, StepBudgetConfig::default()).await;

    let (_, session) = http_post(
        port,
        "/api/v1/sessions",
        serde_json::json!({ "mode": "libre", "tools": ["bash_executor"] }),
    )
    .await;
    let session_id = session["id"].as_str().expect("session id");

    // Send message
    let _ = http_post(
        port,
        &format!("/api/v1/sessions/{session_id}/messages"),
        serde_json::json!({ "content": "Delete everything" }),
    )
    .await;

    // Wait for approval
    let events = collect_events_until(
        &mut event_rx,
        |e| matches!(e, RuntimeEvent::ChatApprovalRequired { .. }),
        Duration::from_secs(5),
    )
    .await;

    let (msg_id, tool_call_id) = match events
        .iter()
        .find(|e| matches!(e, RuntimeEvent::ChatApprovalRequired { .. }))
    {
        Some(RuntimeEvent::ChatApprovalRequired {
            message_id,
            tool_call_id,
            ..
        }) => (message_id.clone(), tool_call_id.clone()),
        _ => panic!("expected ChatApprovalRequired"),
    };

    // WHEN refusing the tool
    let (auth_status, _) = http_post(
        port,
        &format!("/api/v1/sessions/{session_id}/authorize"),
        serde_json::json!({
            "message_id": msg_id,
            "tool_call_id": tool_call_id,
            "tool_name": "bash_executor",
            "decision": "refuse"
        }),
    )
    .await;
    assert_eq!(auth_status, 200);

    // THEN ChatResponseCompleted is emitted (LLM continues)
    let events = collect_events_until(
        &mut event_rx,
        |e| matches!(e, RuntimeEvent::ChatResponseCompleted { .. }),
        Duration::from_secs(5),
    )
    .await;

    let completed = events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::ChatResponseCompleted { .. }));
    assert!(completed, "expected ChatResponseCompleted after refusal");

    cleanup(handle, &socket_path);
}

/// History accumulation: 3 exchanges → 6 messages with sequential seq.
#[tokio::test]
async fn test_chat_history_accumulation() {
    // GIVEN a mock LLM that returns "Reply N" for each exchange
    let model = MockChatModel::new(vec![
        text_response("Reply 1"),
        text_response("Reply 2"),
        text_response("Reply 3"),
    ]);
    let (handle, port, socket_path, mut event_rx) = start_chat_server_default(model).await;

    let (_, session) = http_post(
        port,
        "/api/v1/sessions",
        serde_json::json!({ "mode": "libre" }),
    )
    .await;
    let session_id = session["id"].as_str().expect("session id");

    // Send 3 messages, waiting for each response before sending the next
    for i in 1..=3 {
        let _ = http_post(
            port,
            &format!("/api/v1/sessions/{session_id}/messages"),
            serde_json::json!({ "content": format!("Message {i}") }),
        )
        .await;

        let _ = collect_events_until(
            &mut event_rx,
            |e| matches!(e, RuntimeEvent::ChatResponseCompleted { .. }),
            Duration::from_secs(5),
        )
        .await;

        // Wait for the manager to process the ExchangeComplete command
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    // THEN GET /sessions/:id returns session with 6 messages
    let (get_status, detail) = http_get(port, &format!("/api/v1/sessions/{session_id}")).await;
    assert_eq!(get_status, 200);

    let msg_count = detail["message_count"].as_u64().unwrap_or(0);
    assert_eq!(
        msg_count, 6,
        "expected 6 messages (3 user + 3 assistant), got {msg_count}"
    );

    // Verify messages are in chronological order with increasing seq
    let history = detail["session"]["history"]
        .as_array()
        .expect("history array");
    assert_eq!(history.len(), 6, "history should have 6 messages");

    let mut prev_seq: u32 = 0;
    for msg in history {
        let seq = msg["seq"].as_u64().unwrap_or(0) as u32;
        assert!(
            seq > prev_seq,
            "seq should be strictly increasing: prev={prev_seq}, current={seq}"
        );
        prev_seq = seq;
    }

    cleanup(handle, &socket_path);
}

/// Close session: DELETE → ChatSessionClosed → 409 on subsequent message.
#[tokio::test]
async fn test_chat_close_session() {
    let model = MockChatModel::new(vec![]);
    let (handle, port, socket_path, mut event_rx) = start_chat_server_default(model).await;

    // Create a session
    let (_, session) = http_post(
        port,
        "/api/v1/sessions",
        serde_json::json!({ "mode": "libre" }),
    )
    .await;
    let session_id = session["id"].as_str().expect("session id");

    // WHEN closing the session
    let (del_status, _) = http_delete(port, &format!("/api/v1/sessions/{session_id}")).await;
    assert_eq!(del_status, 200, "expected 200 OK for DELETE");

    // THEN ChatSessionClosed is emitted
    let events = collect_events_until(
        &mut event_rx,
        |e| matches!(e, RuntimeEvent::ChatSessionClosed { .. }),
        Duration::from_secs(3),
    )
    .await;

    let closed = events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::ChatSessionClosed { .. }));
    assert!(closed, "expected ChatSessionClosed event");

    // WHEN sending a message to the closed session
    let (msg_status, _) = http_post(
        port,
        &format!("/api/v1/sessions/{session_id}/messages"),
        serde_json::json!({ "content": "encore" }),
    )
    .await;

    // THEN 409 Conflict (session closed)
    assert_eq!(
        msg_status, 409,
        "expected 409 Conflict for message on closed session"
    );

    cleanup(handle, &socket_path);
}

/// Budget exhausted: infinite tool loop → ChatError after N iterations.
#[tokio::test]
async fn test_chat_budget_exhausted() {
    // GIVEN a mock LLM that ALWAYS returns tool calls (infinite loop)
    let mut responses = Vec::new();
    for _ in 0..20 {
        responses.push(tool_call_response(
            "bash_executor",
            serde_json::json!({"command": "echo loop"}),
        ));
    }
    let model = MockChatModel::new(responses);

    // AND a tight budget: max_steps=3
    let budget = StepBudgetConfig {
        max_steps: 3,
        max_tool_calls: 100,
        wall_clock_secs: 300,
    };

    let (handle, port, socket_path, mut event_rx) = start_chat_server(model, budget).await;

    // Create session with bash_executor tool
    let (_, session) = http_post(
        port,
        "/api/v1/sessions",
        serde_json::json!({ "mode": "libre", "tools": ["bash_executor"] }),
    )
    .await;
    let session_id = session["id"].as_str().expect("session id");

    // Authorize bash_executor via always_accept on the first tool call.
    // After that, subsequent calls in this and future exchanges are auto-authorized,
    // allowing the ReAct loop to hit the budget limit without HITL blocking.

    let _ = http_post(
        port,
        &format!("/api/v1/sessions/{session_id}/messages"),
        serde_json::json!({ "content": "Start loop" }),
    )
    .await;

    // Wait for first approval
    let events = collect_events_until(
        &mut event_rx,
        |e| matches!(e, RuntimeEvent::ChatApprovalRequired { .. }),
        Duration::from_secs(5),
    )
    .await;

    let (msg_id, tool_call_id) = match events
        .iter()
        .find(|e| matches!(e, RuntimeEvent::ChatApprovalRequired { .. }))
    {
        Some(RuntimeEvent::ChatApprovalRequired {
            message_id,
            tool_call_id,
            ..
        }) => (message_id.clone(), tool_call_id.clone()),
        _ => panic!("expected ChatApprovalRequired"),
    };

    // Always accept to authorize for future calls
    let _ = http_post(
        port,
        &format!("/api/v1/sessions/{session_id}/authorize"),
        serde_json::json!({
            "message_id": msg_id,
            "tool_call_id": tool_call_id,
            "tool_name": "bash_executor",
            "decision": "always_accept"
        }),
    )
    .await;

    // The first exchange will continue with the remaining mock responses.
    // The budget check happens BEFORE each LLM call, so with max_steps=3:
    // - Step 1: tool_call (authorized via always_accept)
    // - Step 2: tool_call (auto-authorized)
    // - Step 3: tool_call (auto-authorized)
    // - Step 4: budget exhausted → ChatError
    // But the first exchange already used 1 step for the HITL call.
    // After the tool result is injected, the loop continues.
    // The model has 20 tool_call responses, so it will keep going until budget.

    // Wait for either ChatError or ChatResponseCompleted
    let events = collect_events_until(
        &mut event_rx,
        |e| {
            matches!(
                e,
                RuntimeEvent::ChatError { .. } | RuntimeEvent::ChatResponseCompleted { .. }
            )
        },
        Duration::from_secs(10),
    )
    .await;

    let budget_error = events.iter().find(|e| {
        matches!(
            e,
            RuntimeEvent::ChatError { error, .. } if error.to_lowercase().contains("budget")
        )
    });
    assert!(
        budget_error.is_some(),
        "expected ChatError with 'budget' in message, got events: {events:?}"
    );

    // AND the session should still be active (not closed)
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (get_status, detail) = http_get(port, &format!("/api/v1/sessions/{session_id}")).await;
    assert_eq!(get_status, 200);
    let status = detail["session"]["status"].as_str().unwrap_or("unknown");
    assert_ne!(
        status, "Closed",
        "session should remain active after budget exhaustion"
    );

    cleanup(handle, &socket_path);
}
