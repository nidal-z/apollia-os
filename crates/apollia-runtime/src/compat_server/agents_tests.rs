//! Exposed agents and workflows answering `POST /v1/chat/completions` over the
//! real batch engine, agent executor and workflow engine, with a scripted
//! model: listing, the answer and its usage, the steps of a stream, refusals,
//! a run that waits for a decision, a client that goes away, and the local
//! network without a token.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use apollia_core::batch_source::SourceRegistry;
use apollia_core::{RuntimeEvent, StepBudgetConfig};
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, MessageContent,
    Role, StreamChunk, TokenUsage, ToolCall,
};
use apollia_llm::LlmRouter;
use apollia_mcp::session::LoadingMode;
use apollia_tools::{
    AgentOrigin, AgentRepository, ExposedItemRepository, ExposedKind, StoredAgent, TaskRepository,
    ToolRegistryHandle,
};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tokio::sync::{broadcast, Semaphore};
use tower::ServiceExt;

use super::agents::ExposedAgents;
use super::logs::ServerLogsHandle;
use super::models::LocalModels;
use super::origin::{CallOrigin, AGENT_TOKEN_REQUIRED};
use super::relay::RelayState;
use super::router::{compat_router, serve};
use super::tokens::ServerTokensHandle;
use crate::agents::task::AgentTaskLauncher;
use crate::api::routes_agents::StubAgentLoader;
use crate::api::routes_llm::tests::test_app_state_no_llm;
use crate::batch::{register_request_source, AgentItemRunner, BatchHandle};
use crate::chat::ChatSessionManagerHandle;
use crate::test_support::poll_until_async;
use crate::workflows::{BoxFuture, WorkflowCall, WorkflowDescriptor, WorkflowHandle, WorkflowHost};

pub(super) const CLERK: &str = "demand-clerk";
const WRITER: &str = "note-writer";
const HIDDEN: &str = "hidden-clerk";
const WORKFLOW: &str = "office/close-month";
const ANSWER: &str = "Qualified: priority high.";
const WAIT: Duration = Duration::from_secs(30);

/// On its first turn, calls `file_write` when the agent has it, `file_read`
/// otherwise, then answers. Each turn reports its tokens. A request asking to
/// answer slowly waits for `gate` before each turn answers.
struct Scripted {
    gate: Arc<Semaphore>,
}

fn text_of(message: &apollia_llm::types::ChatMessage) -> &str {
    match &message.content {
        MessageContent::Text(text) => text,
        _ => "",
    }
}

fn first_turn_tool(req: &CompletionRequest) -> Option<&'static str> {
    if req.messages.iter().any(|m| matches!(m.role, Role::Tool)) {
        None
    } else if req.tools.iter().any(|t| t.name == "file_write") {
        Some("file_write")
    } else {
        Some("file_read")
    }
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
        let slow = req
            .messages
            .iter()
            .any(|m| text_of(m).contains("Answer slowly"));
        if slow {
            drop(self.gate.acquire().await);
        }
        let chunks = match first_turn_tool(&req) {
            Some(tool) => vec![
                Ok(StreamChunk::ToolCall(ToolCall {
                    id: "c1".into(),
                    name: tool.into(),
                    arguments: json!({"path": "notes.md", "content": "x"}),
                })),
                Ok(StreamChunk::Usage(TokenUsage {
                    prompt_tokens: 100,
                    completion_tokens: 7,
                    ..TokenUsage::default()
                })),
            ],
            None => vec![
                Ok(StreamChunk::Text(ANSWER.into())),
                Ok(StreamChunk::Usage(TokenUsage {
                    prompt_tokens: 150,
                    completion_tokens: 9,
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
        "scripted"
    }
    fn model_id(&self) -> &str {
        "scripted"
    }
}

/// A host declaring one workflow that closes a month: one step, `sum`, then
/// the month it closed.
struct CloseMonth;

fn close_month() -> WorkflowDescriptor {
    WorkflowDescriptor {
        id: WORKFLOW.into(),
        version: "1.0.0".into(),
        description: "Close a month".into(),
        steps: vec!["sum".into()],
        input_schema: Some(json!({
            "type": "object",
            "properties": {"month": {"type": "string"}},
            "required": ["month"],
            "additionalProperties": false
        })),
    }
}

impl WorkflowHost for CloseMonth {
    fn describe(&self, workflow_id: &str) -> Option<WorkflowDescriptor> {
        (workflow_id == WORKFLOW).then(close_month)
    }

    fn list(&self) -> Vec<WorkflowDescriptor> {
        vec![close_month()]
    }

    fn run(&self, call: WorkflowCall) -> BoxFuture<'static, Result<Value, String>> {
        Box::pin(async move {
            call.services.step_started("sum".into()).await?;
            call.services.step_ended("sum".into(), None).await?;
            Ok(json!({"closed": call.inputs["month"]}))
        })
    }
}

fn store(repo: &AgentRepository, name: &str, mission: &str, access: &[&str]) {
    repo.save_definition(&StoredAgent {
        name: name.into(),
        origin: AgentOrigin::Interface,
        package: None,
        version: "1.0.0".into(),
        definition: json!({
            "id": name,
            "name": name,
            "mission": mission,
            "defaults": {"access": access}
        }),
        enabled: true,
        installed_at: "2026-10-04T00:00:00Z".into(),
        updated_at: "2026-10-04T00:00:00Z".into(),
    })
    .unwrap();
}

pub(super) struct Fixture {
    agents: ExposedAgents,
    pub(super) tokens: ServerTokensHandle,
    batches: BatchHandle,
    gate: Arc<Semaphore>,
    events: broadcast::Receiver<RuntimeEvent>,
    pub(super) _dir: tempfile::TempDir,
}

impl Fixture {
    pub(super) fn router(&self, tokens: Option<ServerTokensHandle>) -> Router {
        compat_router(
            tokens,
            RelayState::with_agents(
                LocalModels::default(),
                self.agents.clone(),
                Some(self.tokens.clone()),
            )
            .unwrap(),
            ServerLogsHandle::spawn(&crate::eventbus::EventBus::new().0),
            false,
        )
    }
}

/// A runtime with three agents, a workflow, and the exposure of two agents
/// and the workflow: `demand-clerk` reads, `note-writer` writes, and
/// `hidden-clerk` is not exposed.
pub(super) async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let agents_db = apollia_core::paths::DataFile::Agents.path(dir.path());
    let repo = AgentRepository::open(&agents_db).unwrap();
    store(&repo, CLERK, "Qualify each demand.", &["file_read"]);
    store(
        &repo,
        WRITER,
        "Write notes for each demand.",
        &["file_write"],
    );
    store(&repo, HIDDEN, "Qualify each demand.", &["file_read"]);
    let exposure = ExposedItemRepository::open(&agents_db).unwrap();
    exposure.set(ExposedKind::Agent, CLERK, true).unwrap();
    exposure.set(ExposedKind::Agent, WRITER, true).unwrap();
    exposure.set(ExposedKind::Workflow, WORKFLOW, true).unwrap();

    let gate = Arc::new(Semaphore::new(0));
    let mut backends: std::collections::HashMap<String, Arc<dyn CompletionModel>> =
        std::collections::HashMap::new();
    backends.insert("scripted".into(), Arc::new(Scripted { gate: gate.clone() }));
    let state = test_app_state_no_llm();
    let events = state.event_sender.subscribe();
    let registry = ToolRegistryHandle::start();
    for mut descriptor in apollia_tools::tools::native_descriptors() {
        if descriptor.name == "file_read" || descriptor.name == "file_write" {
            descriptor.input_schema = json!({ "type": "object" });
            registry.register(descriptor).await.unwrap();
        }
    }
    let chat = ChatSessionManagerHandle::spawn(
        &dir.path().join("chat.db"),
        Some(Arc::new(LlmRouter::with_backends(backends, "scripted"))),
        registry,
        Arc::new(StubAgentLoader),
        None,
        state.event_sender.clone(),
        StepBudgetConfig::default(),
        None,
        state.registry_handle.clone(),
        None,
        None,
        None,
        LoadingMode::Eager,
        20,
        None,
        false,
    )
    .unwrap();
    let tasks = Arc::new(
        TaskRepository::open(&apollia_core::paths::DataFile::Hitl.path(dir.path()))
            .await
            .unwrap(),
    );
    let mut sources = SourceRegistry::new();
    register_request_source(&mut sources).unwrap();
    let launcher = AgentTaskLauncher {
        router: state.router_handle.clone(),
        chat,
        repository: Some(tasks.clone()),
        event_bus: state.event_sender.clone(),
        observability: state.obs_config.clone(),
        plan_cache: None,
    };
    let batches = BatchHandle::spawn(
        tasks.clone(),
        Arc::new(sources),
        AgentItemRunner::new(launcher, dir.path().to_path_buf()),
        state.event_sender.clone(),
    );
    let workflows = WorkflowHandle::spawn(
        tasks.clone(),
        state.event_sender.clone(),
        StepBudgetConfig::default(),
        None,
    );
    workflows.set_host(Arc::new(CloseMonth)).await.unwrap();
    Fixture {
        agents: ExposedAgents::new(dir.path().to_path_buf(), state.event_sender.clone())
            .with_batches(Some(batches.clone()), Some(tasks))
            .with_workflows(Some(workflows)),
        tokens: ServerTokensHandle::spawn(dir.path().join("system.db")),
        batches,
        gate,
        events,
        _dir: dir,
    }
}

async fn send(
    router: &Router,
    method: &str,
    path: &str,
    body: Option<Value>,
    origin: CallOrigin,
) -> (StatusCode, String) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
        .unwrap();
    request.extensions_mut().insert(origin);
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn chat(router: &Router, body: Value) -> (StatusCode, Value) {
    let (status, text) = send(
        router,
        "POST",
        "/v1/chat/completions",
        Some(body),
        CallOrigin::Loopback,
    )
    .await;
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

fn ask(model: &str, text: &str) -> Value {
    json!({
        "model": model,
        "messages": [
            {"role": "system", "content": "Answer in one line."},
            {"role": "user", "content": text}
        ]
    })
}

#[tokio::test]
async fn test_the_exposed_agents_and_workflows_are_listed_as_models() {
    // GIVEN two exposed agents, one that is not, and an exposed workflow
    let fx = fixture().await;
    let router = fx.router(None);

    // WHEN the models are listed from the machine
    let (status, text) = send(&router, "GET", "/v1/models", None, CallOrigin::Loopback).await;

    // THEN the exposed ones are listed under `apollia/<id>` with their type,
    // and the agent not exposed is absent
    assert_eq!(status, StatusCode::OK, "{text}");
    let body: Value = serde_json::from_str(&text).unwrap();
    let listed: Vec<(String, String)> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["id"].as_str().unwrap().to_owned(),
                m["type"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        listed,
        [
            (format!("apollia/{CLERK}"), "agent".to_owned()),
            (format!("apollia/{WRITER}"), "agent".to_owned()),
            (format!("apollia/{WORKFLOW}"), "workflow".to_owned()),
        ]
    );
}

#[tokio::test]
async fn test_an_agent_answers_as_a_model_with_the_tokens_of_every_turn() {
    // GIVEN an exposed agent whose model calls a tool, then answers
    let fx = fixture().await;
    let router = fx.router(None);

    // WHEN it is called with a conversation ending on a user message
    let (status, body) = chat(
        &router,
        json!({
            "model": format!("apollia/{CLERK}"),
            "messages": [
                {"role": "user", "content": "Hello"},
                {"role": "assistant", "content": "Hi."},
                {"role": "user", "content": "Qualify the demand of Durand."}
            ]
        }),
    )
    .await;

    // THEN the final answer is the assistant message, its steps are in
    // `reasoning_content`, and the usage adds both turns
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["object"], "chat.completion");
    assert_eq!(body["model"], format!("apollia/{CLERK}"));
    let message = &body["choices"][0]["message"];
    assert_eq!(message["role"], "assistant");
    assert_eq!(message["content"], ANSWER);
    assert!(
        message["reasoning_content"]
            .as_str()
            .unwrap()
            .starts_with("Calling file_read: "),
        "{body}"
    );
    assert_eq!(body["usage"]["prompt_tokens"], 250);
    assert_eq!(body["usage"]["completion_tokens"], 16);
    assert_eq!(body["usage"]["total_tokens"], 266);

    // AND the agent ran as a batch of one item that read the last user
    // message as its request and the others as its conversation
    let batches = fx.batches.list(10).await.unwrap();
    assert_eq!(batches.len(), 1);
    let detail = fx
        .batches
        .detail(&batches[0].batch_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.items[0].status, "completed");
    assert_eq!(
        detail.items[0].input,
        json!({
            "conversation": "user: Hello\nassistant: Hi.",
            "request": "Qualify the demand of Durand."
        })
    );
}

/// The `data:` payloads of an event stream, in order, comments left out.
fn data_lines(stream: &str) -> Vec<String> {
    stream
        .split("\n\n")
        .filter_map(|event| event.strip_prefix("data: "))
        .map(str::to_owned)
        .collect()
}

#[tokio::test]
async fn test_a_stream_sends_the_steps_as_reasoning_then_the_answer() {
    // GIVEN the same agent
    let fx = fixture().await;
    let router = fx.router(None);

    // WHEN it is called with `stream: true`
    let mut body = ask(&format!("apollia/{CLERK}"), "Qualify the demand of Durand.");
    body["stream"] = json!(true);
    let (status, text) = send(
        &router,
        "POST",
        "/v1/chat/completions",
        Some(body),
        CallOrigin::Loopback,
    )
    .await;

    // THEN the chunks come in order: the role, the steps in
    // `reasoning_content`, the answer in `content`, the finish with the
    // usage, then `[DONE]`
    assert_eq!(status, StatusCode::OK, "{text}");
    let lines = data_lines(&text);
    assert_eq!(lines.last().map(String::as_str), Some("[DONE]"), "{text}");
    let chunks: Vec<Value> = lines[..lines.len() - 1]
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let kinds: Vec<&str> = chunks
        .iter()
        .map(|c| {
            let delta = &c["choices"][0]["delta"];
            if delta.get("role").is_some() {
                "role"
            } else if delta.get("reasoning_content").is_some() {
                "reasoning"
            } else if delta.get("content").is_some() {
                "content"
            } else {
                "finish"
            }
        })
        .collect();
    assert_eq!(
        kinds,
        ["role", "reasoning", "reasoning", "content", "finish"],
        "{text}"
    );
    assert!(chunks[1]["choices"][0]["delta"]["reasoning_content"]
        .as_str()
        .unwrap()
        .starts_with("Calling file_read: "));
    assert!(chunks[2]["choices"][0]["delta"]["reasoning_content"]
        .as_str()
        .unwrap()
        .starts_with("file_read "));
    assert_eq!(chunks[3]["choices"][0]["delta"]["content"], ANSWER);
    assert_eq!(chunks[4]["choices"][0]["finish_reason"], "stop");
    assert_eq!(chunks[4]["usage"]["total_tokens"], 266);
    assert!(chunks
        .iter()
        .all(|c| c["object"] == "chat.completion.chunk"));
}

#[tokio::test]
async fn test_tools_from_the_client_and_an_agent_not_exposed_are_refused() {
    // GIVEN an exposed agent and one that is not
    let fx = fixture().await;
    let router = fx.router(None);

    // WHEN the first is called with tools, and the second without
    let mut with_tools = ask(&format!("apollia/{CLERK}"), "Qualify.");
    with_tools["tools"] = json!([{"type": "function", "function": {"name": "f"}}]);
    let (tools_status, tools_body) = chat(&router, with_tools).await;
    let (hidden_status, hidden_body) =
        chat(&router, ask(&format!("apollia/{HIDDEN}"), "Qualify.")).await;

    // THEN the tools get a readable 400, the hidden agent a 404, and neither
    // ran
    assert_eq!(tools_status, StatusCode::BAD_REQUEST);
    assert_eq!(tools_body["error"]["code"], "invalid_request");
    assert!(tools_body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("cannot bring tools"));
    assert_eq!(hidden_status, StatusCode::NOT_FOUND);
    assert_eq!(hidden_body["error"]["code"], "model_not_found");
    assert!(fx.batches.list(10).await.unwrap().is_empty());
}

#[tokio::test]
async fn test_a_workflow_takes_its_inputs_as_json_and_refuses_others_with_its_schema() {
    // GIVEN an exposed workflow whose inputs require a month
    let fx = fixture().await;
    let router = fx.router(None);
    let model = format!("apollia/{WORKFLOW}");

    // WHEN it is called with text, with inputs that miss the month, and with
    // valid inputs
    let (text_status, text_body) = chat(&router, ask(&model, "close September")).await;
    let (missing_status, missing_body) = chat(&router, ask(&model, "{}")).await;
    let (status, body) = chat(&router, ask(&model, "{\"month\": \"2026-09\"}")).await;

    // THEN the first two get a 400 carrying the schema, the third the
    // workflow's result, with its step as reasoning
    for (status, body) in [(text_status, &text_body), (missing_status, &missing_body)] {
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["error"]["schema"], close_month().input_schema.unwrap());
    }
    assert!(missing_body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("month"));
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["choices"][0]["message"]["content"],
        "{\"closed\":\"2026-09\"}"
    );
    assert_eq!(
        body["choices"][0]["message"]["reasoning_content"],
        "Step sum started\nStep sum done\n"
    );
}

#[tokio::test]
async fn test_a_missing_consent_answers_approval_pending_and_the_item_waits() {
    // GIVEN an exposed agent whose model writes a file, an access nobody
    // granted and never answered "always"
    let mut fx = fixture().await;
    let router = fx.router(None);

    // WHEN it is called
    let (status, body) = chat(
        &router,
        ask(&format!("apollia/{WRITER}"), "Write the note of Durand."),
    )
    .await;

    // THEN the call answers `approval_pending`, naming the pending item and
    // the task
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "approval_pending");
    assert_eq!(body["error"]["type"], "approval_pending");
    assert_eq!(body["error"]["access"], "file_write");
    let batch_id = body["error"]["batch_id"].as_str().unwrap().to_owned();
    assert_eq!(body["error"]["pending_id"], format!("batch:{batch_id}:0"));
    let detail = fx.batches.detail(&batch_id).await.unwrap().unwrap();
    assert_eq!(body["error"]["task_id"], json!(detail.items[0].task_id));

    // AND the item waits in the list of pending decisions, its notification
    // was asked on the bus, and the end of the answer did not cancel it
    let waiting = fx.batches.awaiting_items().await.unwrap();
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].0.batch_id, batch_id);
    assert_eq!(waiting[0].1.status, "awaiting");
    let mut asked = false;
    while let Ok(event) = fx.events.try_recv() {
        if let RuntimeEvent::BatchItemAwaitingApproval {
            batch_id: b,
            access,
            ..
        } = event
        {
            asked |= b == batch_id && access == "file_write";
        }
    }
    assert!(asked, "no BatchItemAwaitingApproval for the item");
    let detail = fx.batches.detail(&batch_id).await.unwrap().unwrap();
    assert_eq!(detail.items[0].status, "awaiting");
}

/// Calls the agent through a real connection, waits until its item runs,
/// closes the connection, waits for the run to stop, and returns the state of
/// the batch and the status of its item.
async fn goes_away_during_the_run(stream_answer: bool) -> (String, String) {
    // GIVEN the server listening, and an agent whose model does not answer
    // until the test lets it
    let fx = fixture().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let cancel = tokio_util::sync::CancellationToken::new();
    tokio::spawn(serve(listener, fx.router(None), cancel.clone()));

    // WHEN a client calls it, waits until the item runs, then closes its
    // connection
    let mut body = ask(&format!("apollia/{CLERK}"), "Answer slowly.");
    body["stream"] = json!(stream_answer);
    let body = body.to_string();
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    let request = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nhost: 127.0.0.1\r\n\
         content-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    );
    tokio::io::AsyncWriteExt::write_all(&mut stream, request.as_bytes())
        .await
        .unwrap();
    let running = poll_until_async(WAIT, || async {
        match fx.batches.list(10).await.unwrap().first() {
            Some(batch) => {
                let detail = fx.batches.detail(&batch.batch_id).await.unwrap().unwrap();
                detail.items[0].status == "running"
            }
            None => false,
        }
    })
    .await;
    assert!(running, "the item never ran");
    drop(stream);

    // THEN the batch is stopped, and its run, cut while it waited for the
    // model, ends canceled
    let batch_id = fx.batches.list(10).await.unwrap()[0].batch_id.clone();
    let stopped = poll_until_async(WAIT, || async {
        fx.batches
            .detail(&batch_id)
            .await
            .unwrap()
            .unwrap()
            .batch
            .state
            == "stopped"
    })
    .await;
    assert!(stopped, "the batch was not stopped");
    let canceled = poll_until_async(WAIT, || async {
        fx.batches.detail(&batch_id).await.unwrap().unwrap().items[0].status == "canceled"
    })
    .await;
    let detail = fx.batches.detail(&batch_id).await.unwrap().unwrap();
    assert!(canceled, "the run was not canceled: {:?}", detail.items[0]);
    fx.gate.add_permits(10);
    cancel.cancel();
    (detail.batch.state, detail.items[0].status.clone())
}

#[tokio::test]
async fn test_a_client_that_goes_away_before_the_answer_stops_the_run() {
    // GIVEN a call answered in one body
    // WHEN its client goes away while the agent runs
    // THEN the run is stopped
    let (state, item) = goes_away_during_the_run(false).await;
    assert_eq!((state.as_str(), item.as_str()), ("stopped", "canceled"));
}

#[tokio::test]
async fn test_a_client_that_leaves_a_stream_stops_the_run() {
    // GIVEN a streamed call
    // WHEN its client goes away while the agent runs
    // THEN the run is stopped
    let (state, item) = goes_away_during_the_run(true).await;
    assert_eq!((state.as_str(), item.as_str()), ("stopped", "canceled"));
}

#[tokio::test]
async fn test_the_local_network_reaches_an_agent_only_with_a_token_in_use() {
    // GIVEN a server without `require_auth`, and a token in use
    let fx = fixture().await;
    let router = fx.router(None);
    let value = fx.tokens.create("phone".into()).await.unwrap().value;
    let model = format!("apollia/{CLERK}");

    // WHEN the agent is called from the local network without the token,
    // the models are listed there without it, and the agent is called with it
    let (refused, refused_text) = send(
        &router,
        "POST",
        "/v1/chat/completions",
        Some(ask(&model, "Qualify.")),
        CallOrigin::LocalNetwork,
    )
    .await;
    let (_, listed) = send(&router, "GET", "/v1/models", None, CallOrigin::LocalNetwork).await;
    let mut request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {value}"))
        .body(Body::from(ask(&model, "Qualify.").to_string()))
        .unwrap();
    request.extensions_mut().insert(CallOrigin::LocalNetwork);
    let admitted = router.clone().oneshot(request).await.unwrap().status();

    // THEN the call without a token is refused with a 401 naming the token,
    // the list shows no agent, and the call with the token answers
    assert_eq!(refused, StatusCode::UNAUTHORIZED);
    let refused: Value = serde_json::from_str(&refused_text).unwrap();
    assert_eq!(refused["error"]["message"], AGENT_TOKEN_REQUIRED);
    let listed: Value = serde_json::from_str(&listed).unwrap();
    assert_eq!(listed["data"], json!([]));
    assert_eq!(admitted, StatusCode::OK);
}
