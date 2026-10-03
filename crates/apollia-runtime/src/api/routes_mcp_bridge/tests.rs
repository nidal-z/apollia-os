//! The bridge routes over a runtime with one stored agent, scripted model, and
//! one workflow, scripted host.

use std::pin::Pin;
use std::sync::Arc;

use apollia_core::StepBudgetConfig;
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, StreamChunk,
    TokenUsage,
};
use apollia_llm::LlmRouter;
use apollia_mcp::session::LoadingMode;
use apollia_tools::{AgentOrigin, AgentRepository, StoredAgent, TaskRepository};
use serde_json::{json, Value};

use super::*;
use crate::api::routes_agents::StubAgentLoader;
use crate::api::test_support::{app_state, call, MockBackend};
use crate::chat::ChatSessionManagerHandle;
use crate::workflows::{BoxFuture, WorkflowCall, WorkflowDescriptor, WorkflowHandle, WorkflowHost};

const AGENT: &str = "ledger-clerk";
const WORKFLOW: &str = "invoices/close-month";

/// Answers every turn with the same sentence.
struct Scripted;

#[async_trait::async_trait]
impl CompletionModel for Scripted {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            engine_timings: None,
            content: "Two gaps found.".into(),
            tool_calls: vec![],
            usage: TokenUsage::default(),
            finish_reason: FinishReason::Stop,
            latency_ms: 1,
            ttft_ms: None,
        })
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
    {
        Ok(Box::pin(futures::stream::iter(vec![Ok(
            StreamChunk::Text("Two gaps found.".into()),
        )])))
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

/// One workflow, `invoices/close-month`, which files the folder it is given.
struct Host;

impl WorkflowHost for Host {
    fn describe(&self, workflow_id: &str) -> Option<WorkflowDescriptor> {
        (workflow_id == WORKFLOW).then(|| WorkflowDescriptor {
            id: workflow_id.to_string(),
            version: "1.0.0".into(),
            description: "Close the month.".into(),
            steps: vec!["file".into()],
            input_schema: Some(json!({
                "type": "object",
                "properties": {"folder": {"type": "string"}},
                "required": ["folder"],
                "additionalProperties": false
            })),
        })
    }

    fn list(&self) -> Vec<WorkflowDescriptor> {
        self.describe(WORKFLOW).into_iter().collect()
    }

    fn run(&self, call: WorkflowCall) -> BoxFuture<'static, Result<Value, String>> {
        Box::pin(async move {
            call.services.step_started("file".into()).await?;
            call.services.step_ended("file".into(), None).await?;
            Ok(json!({"filed": call.inputs["folder"]}))
        })
    }
}

struct Fixture {
    state: AppState<MockBackend>,
    config: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

fn store_agent(repo: &AgentRepository, name: &str) {
    repo.save_definition(&StoredAgent {
        name: name.into(),
        origin: AgentOrigin::Interface,
        package: None,
        version: "1.0.0".into(),
        definition: json!({
            "id": name,
            "name": "Ledger clerk",
            "mission": "Reconcile the ledger.",
            "defaults": {}
        }),
        enabled: true,
        installed_at: "2026-10-01T00:00:00Z".into(),
        updated_at: "2026-10-01T00:00:00Z".into(),
    })
    .unwrap();
}

/// A runtime holding the agents `agents`, the workflow, and an
/// `apollia.toml` reading `config`.
async fn fixture(agents: &[&str], config: &str) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let repo =
        AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(dir.path())).unwrap();
    for name in agents {
        store_agent(&repo, name);
    }
    let config_path = dir.path().join("apollia.toml");
    std::fs::write(&config_path, config).unwrap();

    let mut state = app_state(Arc::new(StubAgentLoader), dir.path().to_path_buf());
    let mut backends: std::collections::HashMap<String, Arc<dyn CompletionModel>> =
        std::collections::HashMap::new();
    backends.insert("scripted".into(), Arc::new(Scripted));
    let chat = ChatSessionManagerHandle::spawn(
        &dir.path().join("chat.db"),
        Some(Arc::new(LlmRouter::with_backends(backends, "scripted"))),
        apollia_tools::ToolRegistryHandle::start(),
        Arc::new(StubAgentLoader),
        None,
        state.event_sender.clone(),
        StepBudgetConfig::default(),
        None,
        state.registry_handle.clone(),
        None,
        None,
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
    let engine = WorkflowHandle::spawn(
        tasks.clone(),
        state.event_sender.clone(),
        StepBudgetConfig::default(),
        None,
    );
    engine.set_host(Arc::new(Host)).await.unwrap();
    state.chat_manager = Some(chat);
    state.task_repository = Some(tasks);
    state.workflows = Some(engine);
    state.config_path = Some(config_path.clone());
    Fixture {
        state,
        config: config_path,
        _dir: dir,
    }
}

const ON: &str = "[mcp]\nserver_enabled = true\n";

async fn expose(fx: &Fixture, kind: &str, id: &str) {
    let (status, body) = call(
        fx.state.clone(),
        "PUT",
        "/api/v1/mcp/exposed",
        Some(json!({"kind": kind, "itemId": id, "exposed": true})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

async fn tool_names(fx: &Fixture) -> Vec<String> {
    let (status, body) = call(fx.state.clone(), "GET", "/api/v1/mcp/bridge/tools", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn test_the_daemon_refuses_the_bridge_while_the_setting_is_off() {
    // GIVEN an exposed agent and an apollia.toml without the bridge setting
    let fx = fixture(&[AGENT], "").await;
    expose(&fx, "agent", AGENT).await;

    // WHEN the bridge lists its tools, then calls the agent
    let (listed, list_body) = call(fx.state.clone(), "GET", "/api/v1/mcp/bridge/tools", None).await;
    let (called, call_body) = call(
        fx.state.clone(),
        "POST",
        "/api/v1/mcp/bridge/call",
        Some(json!({"name": "agent__ledger-clerk", "arguments": {"message": "go"}})),
    )
    .await;

    // THEN both are refused with a readable reason naming the setting
    for (status, body) in [(listed, list_body), (called, call_body)] {
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["code"], "bridge_disabled");
        assert!(body["error"]
            .as_str()
            .unwrap()
            .contains("mcp.server_enabled"));
    }
}

#[tokio::test]
async fn test_the_setting_is_read_again_at_each_opening() {
    // GIVEN the bridge turned on, with an exposed agent
    let fx = fixture(&[AGENT], ON).await;
    expose(&fx, "agent", AGENT).await;
    assert_eq!(tool_names(&fx).await, vec!["agent__ledger-clerk"]);

    // WHEN the user turns it off in the file, the daemon still running
    std::fs::write(&fx.config, "[mcp]\nserver_enabled = false\n").unwrap();
    let (status, body) = call(fx.state.clone(), "GET", "/api/v1/mcp/bridge/tools", None).await;

    // THEN the next opening is refused
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "bridge_disabled");
}

#[tokio::test]
async fn test_only_the_exposed_agents_and_workflows_are_listed() {
    // GIVEN two agents, one exposed, and the workflow exposed
    let fx = fixture(&[AGENT, "other"], ON).await;
    expose(&fx, "agent", AGENT).await;
    expose(&fx, "workflow", WORKFLOW).await;

    // WHEN the bridge lists its tools
    let (status, body) = call(fx.state.clone(), "GET", "/api/v1/mcp/bridge/tools", None).await;

    // THEN it receives the exposed agent, taking a message, and the workflow,
    //   taking its own schema, and nothing else
    assert_eq!(status, StatusCode::OK);
    let tools = body["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 2, "{body}");
    assert_eq!(tools[0]["name"], "agent__ledger-clerk");
    assert_eq!(tools[0]["inputSchema"]["required"], json!(["message"]));
    assert_eq!(
        tools[0]["description"],
        "Ledger clerk: Reconcile the ledger."
    );
    assert_eq!(tools[1]["name"], "workflow__invoices_close-month");
    assert_eq!(tools[1]["inputSchema"]["required"], json!(["folder"]));
}

#[tokio::test]
async fn test_a_call_runs_the_agent_and_returns_its_final_answer() {
    // GIVEN the bridge on and the agent exposed
    let fx = fixture(&[AGENT], ON).await;
    expose(&fx, "agent", AGENT).await;

    // WHEN the bridge calls it with a message
    let (status, body) = call(
        fx.state.clone(),
        "POST",
        "/api/v1/mcp/bridge/call",
        Some(json!({"name": "agent__ledger-clerk", "arguments": {"message": "Reconcile March."}})),
    )
    .await;

    // THEN the run went to its end and its answer comes back
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["text"], "Two gaps found.");
    assert_eq!(body["isError"], false);
    assert_eq!(body["status"], "completed");
    assert_eq!(body["kind"], "agent");
}

#[tokio::test]
async fn test_a_call_without_a_message_is_refused() {
    // GIVEN the bridge on and the agent exposed
    let fx = fixture(&[AGENT], ON).await;
    expose(&fx, "agent", AGENT).await;

    // WHEN the bridge calls it without a message
    let (status, body) = call(
        fx.state.clone(),
        "POST",
        "/api/v1/mcp/bridge/call",
        Some(json!({"name": "agent__ledger-clerk", "arguments": {}})),
    )
    .await;

    // THEN the arguments are refused
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalid_arguments");
}

#[tokio::test]
async fn test_a_call_runs_the_workflow_and_returns_what_it_returned() {
    // GIVEN the bridge on and the workflow exposed
    let fx = fixture(&[], ON).await;
    expose(&fx, "workflow", WORKFLOW).await;

    // WHEN the bridge calls it with valid inputs, then with an input it does
    //   not declare
    let (status, body) = call(
        fx.state.clone(),
        "POST",
        "/api/v1/mcp/bridge/call",
        Some(json!({"name": "workflow__invoices_close-month", "arguments": {"folder": "march"}})),
    )
    .await;
    let (refused, refusal) = call(
        fx.state.clone(),
        "POST",
        "/api/v1/mcp/bridge/call",
        Some(json!({"name": "workflow__invoices_close-month", "arguments": {"month": 3}})),
    )
    .await;

    // THEN the first returns the workflow's result, the second is refused
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["isError"], false);
    assert_eq!(
        serde_json::from_str::<Value>(body["text"].as_str().unwrap()).unwrap(),
        json!({"filed": "march"})
    );
    assert_eq!(refused, StatusCode::BAD_REQUEST);
    assert_eq!(refusal["code"], "invalid_arguments");
}

#[tokio::test]
async fn test_an_agent_that_is_not_exposed_cannot_be_called() {
    // GIVEN the bridge on and an agent that is stored but not exposed
    let fx = fixture(&[AGENT], ON).await;

    // WHEN the bridge calls it by the name it would have
    let (status, body) = call(
        fx.state.clone(),
        "POST",
        "/api/v1/mcp/bridge/call",
        Some(json!({"name": "agent__ledger-clerk", "arguments": {"message": "go"}})),
    )
    .await;

    // THEN no tool carries that name
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "unknown_tool");
}

#[tokio::test]
async fn test_two_items_whose_names_collide_are_both_left_out() {
    // GIVEN two exposed agents whose identifiers normalize to one tool name
    let fx = fixture(&["a.b", "a_b", AGENT], ON).await;
    for id in ["a.b", "a_b", AGENT] {
        expose(&fx, "agent", id).await;
    }

    // WHEN the bridge lists its tools
    let names = tool_names(&fx).await;

    // THEN neither of the two is served, the third is
    assert_eq!(names, vec!["agent__ledger-clerk"]);
}

#[tokio::test]
async fn test_the_exposure_flag_reads_and_writes_through_the_routes() {
    // GIVEN a runtime with one agent and the workflow, nothing exposed
    let fx = fixture(&[AGENT], "").await;
    let (_, empty) = call(fx.state.clone(), "GET", "/api/v1/mcp/exposed", None).await;

    // WHEN both are exposed, then the agent withdrawn
    expose(&fx, "agent", AGENT).await;
    expose(&fx, "workflow", WORKFLOW).await;
    let (_, both) = call(fx.state.clone(), "GET", "/api/v1/mcp/exposed", None).await;
    let (status, withdrawn) = call(
        fx.state.clone(),
        "PUT",
        "/api/v1/mcp/exposed",
        Some(json!({"kind": "agent", "itemId": AGENT, "exposed": false})),
    )
    .await;
    let (_, after) = call(fx.state.clone(), "GET", "/api/v1/mcp/exposed", None).await;

    // THEN the store follows each write
    assert_eq!(empty, json!({"items": []}));
    assert_eq!(
        both,
        json!({"items": [
            {"kind": "agent", "itemId": AGENT},
            {"kind": "workflow", "itemId": WORKFLOW}
        ]})
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(withdrawn["exposed"], false);
    assert_eq!(
        after,
        json!({"items": [{"kind": "workflow", "itemId": WORKFLOW}]})
    );
}

#[tokio::test]
async fn test_exposing_an_unknown_item_or_kind_is_refused() {
    // GIVEN a runtime with one agent
    let fx = fixture(&[AGENT], "").await;

    // WHEN an absent agent, an absent workflow and an unknown kind are exposed
    let mut answers = Vec::new();
    for body in [
        json!({"kind": "agent", "itemId": "ghost", "exposed": true}),
        json!({"kind": "workflow", "itemId": "acme/ghost", "exposed": true}),
        json!({"kind": "tool", "itemId": "bash_executor", "exposed": true}),
    ] {
        answers.push(call(fx.state.clone(), "PUT", "/api/v1/mcp/exposed", Some(body)).await);
    }

    // THEN each is refused and nothing is stored
    assert_eq!(answers[0].0, StatusCode::NOT_FOUND);
    assert_eq!(answers[0].1["code"], "unknown_item");
    assert_eq!(answers[1].0, StatusCode::NOT_FOUND);
    assert_eq!(answers[2].0, StatusCode::BAD_REQUEST);
    assert_eq!(answers[2].1["code"], "invalid_kind");
    let (_, listed) = call(fx.state.clone(), "GET", "/api/v1/mcp/exposed", None).await;
    assert_eq!(listed, json!({"items": []}));
}
