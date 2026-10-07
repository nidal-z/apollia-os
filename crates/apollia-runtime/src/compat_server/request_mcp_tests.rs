//! A response and a message that bring an MCP server, over the real agent
//! executor, the real MCP client and a scripted model: the setting that turns
//! them off, what a request may not bring, the admission of the policy, the
//! tool loop run by the runtime instead of the engine, the wait nobody can
//! answer, and the connection gone with the call.
//!
//! The MCP server and the engine of the model are HTTP servers on ports the
//! system chose, both served by the one listener helper of the relay tests.

use std::pin::Pin;
use std::sync::{Arc, Mutex};

use apollia_core::StepBudgetConfig;
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, Role,
    StreamChunk, TokenUsage, ToolCall,
};
use apollia_llm::LlmRouter;
use apollia_mcp::manager::McpClientManagerHandle;
use apollia_mcp::session::LoadingMode;
use apollia_tools::{TaskRepository, ToolRegistryHandle};
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;
use serde_json::{json, Value};
use tokio::sync::Semaphore;
use tower::ServiceExt;

use super::agents::ExposedAgents;
use super::agents_tests::WAIT;
use super::logs::ServerLogsHandle;
use super::models::LocalModels;
use super::origin::CallOrigin;
use super::relay::RelayState;
use super::relay_tests::{registry, serve_engine};
use super::request_mcp_run::{LaunchedRequestRunner, RequestMcp};
use super::router::compat_router;
use super::tokens::ServerTokensHandle;
use crate::agents::task::AgentTaskLauncher;
use crate::api::routes_llm::tests::test_app_state_no_llm;
use crate::chat::ChatSessionManagerHandle;
use crate::governance::policy_admission::valid_verdict_with_connections;
use crate::governance::{
    ConnectionGovernance, GovernanceHandle, GovernanceSources, PolicyAdmission,
};
use crate::llama_server::LlamaServerSupervisor;
use crate::test_support::poll_until_async;

const MODEL: &str = "acme/chat";
const ANSWER: &str = "The server said: echoed.";
const TOKEN: &str = "s3cret";

/// What the fake MCP server was asked: each JSON-RPC method, with the
/// authorization it came with.
type Asked = Arc<Mutex<Vec<(String, Option<String>)>>>;

/// A streamable-HTTP MCP server with one tool, `echo`, which it declares
/// read-only.
async fn fake_mcp(State(asked): State<Asked>, headers: HeaderMap, body: Bytes) -> Response {
    let message: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let method = message["method"].as_str().unwrap_or_default().to_owned();
    let authorization = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    asked.lock().unwrap().push((method.clone(), authorization));
    let result = match method.as_str() {
        "initialize" => json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "fake", "version": "1"}
        }),
        "tools/list" => json!({"tools": [{
            "name": "echo",
            "description": "Echo a text",
            "inputSchema": {"type": "object"},
            "annotations": {"readOnlyHint": true}
        }]}),
        "tools/call" => json!({"content": [{"type": "text", "text": "echoed"}]}),
        _ => return StatusCode::ACCEPTED.into_response(),
    };
    axum::Json(json!({"jsonrpc": "2.0", "id": message["id"], "result": result})).into_response()
}

/// On its first turn, calls the `echo` tool it was offered, then answers. A
/// request asking to answer slowly waits for `gate` before its first turn.
struct Scripted {
    name: String,
    gate: Arc<Semaphore>,
}

#[async_trait::async_trait]
impl CompletionModel for Scripted {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            engine_timings: None,
            content: ANSWER.into(),
            tool_calls: vec![],
            usage: TokenUsage::default(),
            finish_reason: FinishReason::Stop,
            latency_ms: 1,
            ttft_ms: None,
        })
    }

    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
    {
        let slow = req.messages.iter().any(|m| {
            matches!(&m.content, apollia_llm::types::MessageContent::Text(t) if t.contains("Answer slowly"))
        });
        if slow {
            drop(self.gate.acquire().await);
        }
        let answered = req.messages.iter().any(|m| matches!(m.role, Role::Tool));
        let echo = req.tools.iter().find(|t| t.name.contains("echo"));
        let chunks = match (answered, echo) {
            (false, Some(tool)) => vec![
                Ok(StreamChunk::ToolCall(ToolCall {
                    id: "c1".into(),
                    name: tool.name.clone(),
                    arguments: json!({"text": "hello"}),
                })),
                Ok(StreamChunk::Usage(TokenUsage {
                    prompt_tokens: 40,
                    completion_tokens: 5,
                    ..TokenUsage::default()
                })),
            ],
            _ => vec![
                Ok(StreamChunk::Text(ANSWER.into())),
                Ok(StreamChunk::Usage(TokenUsage {
                    prompt_tokens: 60,
                    completion_tokens: 8,
                    ..TokenUsage::default()
                })),
            ],
        };
        Ok(Box::pin(futures::stream::iter(chunks)))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        &self.name
    }
    fn model_id(&self) -> &str {
        &self.name
    }
}

/// What a fixture is built with.
struct Setup {
    /// `[server] allow_request_mcp`.
    allowed: bool,
    /// Whether the organisation policy fixes that setting off.
    refused_by_policy: bool,
    /// The `[connections]` section of the organisation policy, when there is
    /// one.
    connections_policy: Option<&'static str>,
    /// Developer mode, with a fine rule allowing this tool.
    rule_on: Option<&'static str>,
}

impl Setup {
    fn allowed() -> Self {
        Self {
            allowed: true,
            refused_by_policy: false,
            connections_policy: None,
            rule_on: None,
        }
    }
}

struct Fixture {
    router: Router,
    mcp: McpClientManagerHandle,
    registry: ToolRegistryHandle,
    /// Where the fake MCP server answers.
    mcp_url: String,
    asked: Asked,
    /// What the engine of the model received: nothing, when a request is not
    /// relayed.
    relayed: Arc<Mutex<Vec<Value>>>,
    gate: Arc<Semaphore>,
    _dir: tempfile::TempDir,
    _models: tempfile::TempDir,
    _engine: Arc<LlamaServerSupervisor>,
}

impl Fixture {
    fn methods(&self) -> Vec<String> {
        self.asked
            .lock()
            .unwrap()
            .iter()
            .map(|(method, _)| method.clone())
            .collect()
    }

    /// Whether anything of a request is left: a session, or a tool.
    async fn left_over(&self) -> bool {
        let sessions = !self.mcp.status().await.is_empty();
        let tools = self
            .registry
            .list()
            .await
            .unwrap()
            .iter()
            .any(|tool| tool.name.starts_with("mcp:"));
        sessions || tools
    }
}

async fn fixture(setup: Setup) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("apollia.toml");
    std::fs::write(
        &settings,
        format!(
            "[server]\nallow_request_mcp = {}\n[desktop]\ndeveloper_mode = {}\n",
            setup.allowed,
            setup.rule_on.is_some()
        ),
    )
    .unwrap();

    let relayed: Arc<Mutex<Vec<Value>>> = Arc::default();
    let seen = relayed.clone();
    let engine_port = serve_engine(Router::new().fallback(move |body: Bytes| {
        let seen = seen.clone();
        async move {
            seen.lock()
                .unwrap()
                .push(serde_json::from_slice(&body).unwrap_or(Value::Null));
            axum::Json(json!({"relayed": true}))
        }
    }))
    .await;
    let engine =
        LlamaServerSupervisor::with_resident_for_test(&[("/models/chat.gguf", engine_port)], None)
            .await;
    let models_dir = registry();
    let models = LocalModels::new(
        Some(engine.clone()),
        None,
        Some(models_dir.path().to_path_buf()),
    )
    .with_files(settings.clone(), dir.path().join("system.db"));
    let models = if setup.refused_by_policy {
        models.under_policy(
            apollia_core::config::PolicySettings::none()
                .impose("server.allow_request_mcp", json!(false)),
        )
    } else {
        models
    };
    let backend = models.backend_row(MODEL).await;

    let asked: Asked = Arc::default();
    let mcp_port = serve_engine(Router::new().fallback(fake_mcp).with_state(asked.clone())).await;

    let gate = Arc::new(Semaphore::new(0));
    let mut backends: std::collections::HashMap<String, Arc<dyn CompletionModel>> =
        std::collections::HashMap::new();
    backends.insert(
        backend.clone(),
        Arc::new(Scripted {
            name: backend.clone(),
            gate: gate.clone(),
        }),
    );
    // A second backend is the default: a run that fell back to it would
    // answer nothing a test expects.
    backends.insert(
        "other".into(),
        Arc::new(Scripted {
            name: "other".into(),
            gate: Arc::new(Semaphore::new(0)),
        }),
    );
    let llm = Arc::new(LlmRouter::with_backends(backends, "other"));

    let state = test_app_state_no_llm();
    let tool_registry = ToolRegistryHandle::start();
    let mcp = McpClientManagerHandle::start(
        Vec::new(),
        &tool_registry,
        Some(state.event_sender.clone()),
        None,
        LoadingMode::Eager,
    )
    .await
    .unwrap();
    let chat = ChatSessionManagerHandle::spawn(
        &dir.path().join("chat.db"),
        Some(llm.clone()),
        tool_registry.clone(),
        None,
        state.event_sender.clone(),
        StepBudgetConfig::default(),
        None,
        state.registry_handle.clone(),
        None,
        Some(mcp.clone()),
        None,
        LoadingMode::Eager,
        20,
        None,
        false,
    )
    .unwrap();
    if let Some(tool) = setup.rule_on {
        use apollia_permissions::{PermissionScope, PrefixRule, PrefixRuleEngine, RuleAction};
        let db = dir.path().join("governance.db");
        apollia_tools::GovernanceDb::open(dir.path()).unwrap();
        PrefixRuleEngine::new(&db)
            .unwrap()
            .add_rule(&PrefixRule {
                tool_name: tool.to_owned(),
                arg_prefix: None,
                action: RuleAction::Allow,
                scope: PermissionScope::Global,
                ..PrefixRule::default()
            })
            .unwrap();
        let governance = GovernanceHandle::spawn(GovernanceSources {
            governance_db: Some(db),
            config_path: Some(settings.clone()),
            policy: Arc::new(apollia_permissions::PolicyVerdict::Absent),
            journal: None,
            events: None,
            forbidden_connections: Default::default(),
        });
        governance.refresh().await;
        chat.set_governance(governance).await;
    }
    let tasks = Arc::new(
        TaskRepository::open(&apollia_core::paths::DataFile::Hitl.path(dir.path()))
            .await
            .unwrap(),
    );
    let launcher = AgentTaskLauncher {
        router: state.router_handle.clone(),
        chat,
        repository: Some(tasks),
        event_bus: state.event_sender.clone(),
        observability: state.obs_config.clone(),
        plan_cache: None,
    };
    let connections = match setup.connections_policy {
        Some(section) => ConnectionGovernance::new(
            Arc::new(PolicyAdmission::new(valid_verdict_with_connections(
                section,
            ))),
            None,
        ),
        None => ConnectionGovernance::default(),
    };
    let request_mcp = RequestMcp::new(
        mcp.clone(),
        Arc::new(LaunchedRequestRunner::new(launcher)),
        connections,
        llm,
    );
    let agents = ExposedAgents::new(dir.path().to_path_buf(), state.event_sender.clone())
        .with_request_mcp(Some(request_mcp));
    let router = compat_router(
        None,
        RelayState::with_agents(
            models,
            agents,
            Some(ServerTokensHandle::spawn(dir.path().join("tokens.db"))),
        )
        .unwrap(),
        ServerLogsHandle::spawn(&crate::eventbus::EventBus::new().0),
        false,
    );
    Fixture {
        router,
        mcp,
        registry: tool_registry,
        mcp_url: format!("http://127.0.0.1:{mcp_port}/mcp"),
        asked,
        relayed,
        gate,
        _dir: dir,
        _models: models_dir,
        _engine: engine,
    }
}

async fn open(router: &Router, path: &str, body: Value, origin: CallOrigin) -> Response {
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    request.extensions_mut().insert(origin);
    router.clone().oneshot(request).await.unwrap()
}

async fn send(router: &Router, path: &str, body: Value) -> (StatusCode, Value) {
    let response = open(router, path, body, CallOrigin::Loopback).await;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn response_with(url: &str) -> Value {
    json!({
        "model": MODEL,
        "input": "Say hello through the server.",
        "tools": [{"type": "mcp", "server_label": "trial", "server_url": url,
                   "authorization": TOKEN}]
    })
}

fn message_with(url: &str) -> Value {
    json!({
        "model": MODEL,
        "max_tokens": 64,
        "messages": [{"role": "user", "content": "Say hello through the server."}],
        "mcp_servers": [{"type": "url", "name": "trial", "url": url,
                         "authorization_token": TOKEN}]
    })
}

/// The registry name of the tool of the fake server.
const ECHO: &str = "mcp:request-mcp:127.0.0.1/echo";

#[tokio::test]
async fn test_a_request_that_brings_a_server_is_refused_while_the_setting_is_off() {
    // GIVEN a runtime whose `[server] allow_request_mcp` is off, the default
    let fx = fixture(Setup {
        allowed: false,
        ..Setup::allowed()
    })
    .await;

    // WHEN a response and a message each bring an MCP server
    let (response_status, response) =
        send(&fx.router, "/v1/responses", response_with(&fx.mcp_url)).await;
    let (message_status, message) =
        send(&fx.router, "/v1/messages", message_with(&fx.mcp_url)).await;

    // THEN both are refused with a 400 that names the setting, in the format
    // of their family, and neither the server nor the engine was reached
    assert_eq!(response_status, StatusCode::BAD_REQUEST);
    assert_eq!(message_status, StatusCode::BAD_REQUEST);
    let said = response["error"]["message"].as_str().unwrap();
    assert!(
        said.contains("brought by a request are turned off"),
        "{said}"
    );
    assert!(said.contains("allow_request_mcp"), "{said}");
    assert_eq!(message["type"], "error");
    assert_eq!(message["error"]["type"], "invalid_request_error");
    assert_eq!(message["error"]["message"], response["error"]["message"]);
    assert!(fx.methods().is_empty());
    assert!(fx.relayed.lock().unwrap().is_empty());
}

#[tokio::test]
async fn test_the_policy_that_fixes_the_setting_off_refuses_the_request_whatever_the_file_says() {
    // GIVEN a runtime whose file turns `[server] allow_request_mcp` on, under
    // an organisation policy that fixes it off
    let fx = fixture(Setup {
        refused_by_policy: true,
        ..Setup::allowed()
    })
    .await;

    // WHEN a response and a message each bring an MCP server
    let (response_status, response) =
        send(&fx.router, "/v1/responses", response_with(&fx.mcp_url)).await;
    let (message_status, message) =
        send(&fx.router, "/v1/messages", message_with(&fx.mcp_url)).await;

    // THEN both are refused with a 400 that names the policy and the setting,
    // and neither the server nor the engine was reached
    assert_eq!(response_status, StatusCode::BAD_REQUEST);
    assert_eq!(message_status, StatusCode::BAD_REQUEST);
    let said = response["error"]["message"].as_str().unwrap();
    assert!(
        said.contains("refused by the organisation policy"),
        "{said}"
    );
    assert!(said.contains("server.allow_request_mcp"), "{said}");
    assert_eq!(message["error"]["message"], response["error"]["message"]);
    assert!(fx.methods().is_empty());
    assert!(fx.relayed.lock().unwrap().is_empty());
}

#[tokio::test]
async fn test_a_request_never_starts_a_process_nor_reaches_the_private_network() {
    // GIVEN a runtime that accepts the servers a request brings
    let fx = fixture(Setup::allowed()).await;
    let stdio_response = json!({
        "model": MODEL, "input": "hi",
        "tools": [{"type": "mcp", "server_label": "local", "command": "npx", "args": ["server"]}]
    });
    let stdio_message = json!({
        "model": MODEL, "max_tokens": 8,
        "messages": [{"role": "user", "content": "hi"}],
        "mcp_servers": [{"type": "stdio", "name": "local", "command": "npx"}]
    });

    // WHEN a response and a message ask for a stdio server, and a response
    // names a server on the private network
    let (stdio_a, refused_a) = send(&fx.router, "/v1/responses", stdio_response).await;
    let (stdio_b, refused_b) = send(&fx.router, "/v1/messages", stdio_message).await;
    let (private, refused_c) = send(
        &fx.router,
        "/v1/responses",
        response_with("http://192.168.1.20:3000/mcp"),
    )
    .await;

    // THEN each is a 400 saying why, and nothing was connected
    assert_eq!(stdio_a, StatusCode::BAD_REQUEST);
    assert_eq!(stdio_b, StatusCode::BAD_REQUEST);
    assert_eq!(private, StatusCode::BAD_REQUEST);
    for refused in [&refused_a, &refused_b] {
        let said = refused["error"]["message"].as_str().unwrap();
        assert!(said.contains("never starts a process"), "{said}");
    }
    let said = refused_c["error"]["message"].as_str().unwrap();
    assert!(said.contains("never one on the private network"), "{said}");
    assert!(!fx.left_over().await);
}

#[tokio::test]
async fn test_an_agent_cannot_be_brought_a_server() {
    // GIVEN a runtime that accepts the servers a request brings
    let fx = fixture(Setup::allowed()).await;
    let mut body = response_with(&fx.mcp_url);
    body["model"] = json!("apollia/demand-clerk");

    // WHEN a response names an agent and brings a server
    let (status, refused) = send(&fx.router, "/v1/responses", body).await;

    // THEN it is a 400: an agent works with its own connections
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let said = refused["error"]["message"].as_str().unwrap();
    assert!(said.contains("cannot bring it an MCP server"), "{said}");
    assert!(fx.methods().is_empty());
}

#[tokio::test]
async fn test_the_policy_that_denies_remote_connections_refuses_the_request() {
    // GIVEN an organisation policy whose `[connections]` denies the remote
    // ones
    let fx = fixture(Setup {
        connections_policy: Some("remote = \"deny\""),
        ..Setup::allowed()
    })
    .await;

    // WHEN a message brings a server that is not on this machine
    let (status, refused) = send(
        &fx.router,
        "/v1/messages",
        message_with("https://mcp.example.com/mcp"),
    )
    .await;

    // THEN it is refused with the reason of the policy, before any connection
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(refused["error"]["type"], "permission_error");
    let said = refused["error"]["message"].as_str().unwrap();
    assert!(
        said.contains("refused by the organisation policy"),
        "{said}"
    );
    assert!(said.contains("https://mcp.example.com/mcp"), "{said}");
    assert!(!fx.left_over().await);
}

#[tokio::test]
async fn test_the_runtime_runs_the_tool_loop_and_the_connection_ends_with_the_call() {
    // GIVEN a runtime in developer mode with a fine rule allowing the tool
    // of the server, and a model that calls that tool then answers
    let fx = fixture(Setup {
        rule_on: Some(ECHO),
        ..Setup::allowed()
    })
    .await;

    // WHEN a response brings the server
    let (status, answered) = send(&fx.router, "/v1/responses", response_with(&fx.mcp_url)).await;

    // THEN the model's answer comes back, the tool having been called on the
    // server with the authorization of the request
    assert_eq!(status, StatusCode::OK, "{answered}");
    assert!(answered.to_string().contains(ANSWER), "{answered}");
    // (the session may say goodbye after them, when it is removed)
    assert_eq!(
        fx.methods()[..4],
        [
            "initialize",
            "notifications/initialized",
            "tools/list",
            "tools/call"
        ]
    );
    let bearer = format!("Bearer {TOKEN}");
    assert!(fx
        .asked
        .lock()
        .unwrap()
        .iter()
        .all(|(_, authorization)| authorization.as_deref() == Some(bearer.as_str())));
    // AND the request was never relayed to the engine of the model
    assert!(fx.relayed.lock().unwrap().is_empty());
    // AND nothing of it is left: no session, no tool
    assert!(!fx.left_over().await);
}

#[tokio::test]
async fn test_the_same_loop_answers_a_message() {
    // GIVEN the same runtime
    let fx = fixture(Setup {
        rule_on: Some(ECHO),
        ..Setup::allowed()
    })
    .await;

    // WHEN a message brings the server
    let (status, answered) = send(&fx.router, "/v1/messages", message_with(&fx.mcp_url)).await;

    // THEN it is answered in the format of a message, after the tool call
    assert_eq!(status, StatusCode::OK, "{answered}");
    assert_eq!(answered["type"], "message");
    assert!(answered.to_string().contains(ANSWER), "{answered}");
    assert!(fx.methods().contains(&"tools/call".to_owned()));
    assert!(fx.relayed.lock().unwrap().is_empty());
    assert!(!fx.left_over().await);
}

#[tokio::test]
async fn test_without_a_rule_the_call_ends_on_approval_pending_and_the_tool_never_runs() {
    // GIVEN a runtime with no rule on the tool, whose server declares it
    // read-only
    let fx = fixture(Setup::allowed()).await;

    // WHEN a response brings the server and the model calls the tool
    let (status, pending) = send(&fx.router, "/v1/responses", response_with(&fx.mcp_url)).await;

    // THEN the call ends on `approval_pending` naming the tool: what the
    // server declares classifies nothing, and nobody is there to approve
    assert_eq!(status, StatusCode::CONFLICT, "{pending}");
    assert_eq!(pending["error"]["code"], "approval_pending");
    assert_eq!(pending["error"]["access"], ECHO);
    // AND the tool never ran, and the connection is gone
    assert!(!fx.methods().contains(&"tools/call".to_owned()));
    assert!(!fx.left_over().await);
}

#[tokio::test]
async fn test_a_caller_that_goes_away_leaves_no_connection() {
    // GIVEN a streamed response that brings the server, whose model has not
    // answered its first turn yet
    let fx = fixture(Setup {
        rule_on: Some(ECHO),
        ..Setup::allowed()
    })
    .await;
    let mut body = response_with(&fx.mcp_url);
    body["input"] = json!("Answer slowly, through the server.");
    body["stream"] = json!(true);
    let response = open(&fx.router, "/v1/responses", body, CallOrigin::Loopback).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        fx.left_over().await,
        "the server is connected while the call runs"
    );

    // WHEN the caller goes away before the end
    drop(response);

    // THEN the connection and its tools are removed without the run ending
    // on its own
    let gone = poll_until_async(WAIT, || async { !fx.left_over().await }).await;
    assert!(gone, "a session or a tool of the request is left");
    fx.gate.add_permits(8);
}

#[tokio::test]
async fn test_from_the_local_network_a_token_is_asked_for() {
    // GIVEN a runtime that accepts the servers a request brings
    let fx = fixture(Setup::allowed()).await;

    // WHEN a caller of the local network brings a server without a token
    let response = open(
        &fx.router,
        "/v1/responses",
        response_with(&fx.mcp_url),
        CallOrigin::LocalNetwork,
    )
    .await;

    // THEN it is refused as a call to an agent is, and nothing was connected
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(fx.methods().is_empty());
}
