//! The A2A card lists the agents that take a message and the workflows, and
//! `invoke` launches and cancels either.

use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use apollia_core::{A2AConfig, RuntimeEvent, StepBudgetConfig};
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, StreamChunk,
    TokenUsage, ToolCall,
};
use apollia_llm::LlmRouter;
use apollia_mcp::session::LoadingMode;
use apollia_tools::{
    AgentOrigin, AgentRepository, StoredAgent, TaskRepository, ToolRegistryHandle,
};
use axum::http::StatusCode;
use serde_json::{json, Value};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::a2a::{A2AInvoker, AliasTarget};
use crate::api::routes_agents::StubAgentLoader;
use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend};
use crate::chat::ChatSessionManagerHandle;
use crate::workflows::{BoxFuture, WorkflowCall, WorkflowDescriptor, WorkflowHandle, WorkflowHost};

const AGENT: &str = "ledger-clerk";
const WORKFLOW: &str = "invoices/close-month";

/// Calls `file_read` on its first turn when `tool_first`, which waits for an
/// approval; otherwise answers at once.
struct Scripted {
    tool_first: bool,
    turns: AtomicU32,
}

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
        let turn = self.turns.fetch_add(1, Ordering::SeqCst);
        let chunk = if self.tool_first && turn == 0 {
            StreamChunk::ToolCall(ToolCall {
                id: "c1".into(),
                name: "file_read".into(),
                arguments: json!({"path": "march.csv"}),
            })
        } else {
            StreamChunk::Text("Two gaps found.".into())
        };
        Ok(Box::pin(futures::stream::iter(vec![Ok(chunk)])))
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

/// `invoices/close-month`: it files `folder`, or waits for its token when
/// `hold` is set.
struct Host;

impl WorkflowHost for Host {
    fn describe(&self, workflow_id: &str) -> Option<WorkflowDescriptor> {
        (workflow_id == WORKFLOW).then(|| WorkflowDescriptor {
            id: workflow_id.to_string(),
            version: "2.1.0".into(),
            description: "Close the month.".into(),
            steps: vec!["file".into()],
            input_schema: Some(schema()),
        })
    }

    fn list(&self) -> Vec<WorkflowDescriptor> {
        self.describe(WORKFLOW).into_iter().collect()
    }

    fn run(&self, call: WorkflowCall) -> BoxFuture<'static, Result<Value, String>> {
        Box::pin(async move {
            call.services.step_started("file".into()).await?;
            if call.inputs["hold"] == json!(true) {
                wait(&call.cancel).await;
                return Err("stopped".into());
            }
            call.services.step_ended("file".into(), None).await?;
            Ok(json!({"filed": call.inputs["folder"]}))
        })
    }
}

async fn wait(token: &CancellationToken) {
    token.cancelled().await;
}

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {"folder": {"type": "string"}, "hold": {"type": "boolean"}},
        "required": ["folder"],
        "additionalProperties": false
    })
}

struct Fixture {
    state: AppState<MockBackend>,
    invoker: Arc<A2AInvoker>,
    events: broadcast::Receiver<RuntimeEvent>,
    _dir: tempfile::TempDir,
}

/// A runtime with one stored agent without code, `ledger-clerk`, one
/// workflow, `invoices/close-month`, and the A2A invoker.
async fn fixture(tool_first: bool) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(dir.path()))
        .unwrap()
        .save_definition(&StoredAgent {
            name: AGENT.into(),
            origin: AgentOrigin::Interface,
            package: None,
            version: "1.0.0".into(),
            definition: json!({
                "id": AGENT,
                "name": "Ledger clerk",
                "mission": "Reconcile the ledger.",
                "defaults": {"access": ["file_read"]}
            }),
            enabled: true,
            installed_at: "2026-10-01T00:00:00Z".into(),
            updated_at: "2026-10-01T00:00:00Z".into(),
        })
        .unwrap();
    let mut state = app_state(Arc::new(StubAgentLoader), dir.path().to_path_buf());
    let events = state.event_sender.subscribe();
    let mut backends: std::collections::HashMap<String, Arc<dyn CompletionModel>> =
        std::collections::HashMap::new();
    backends.insert(
        "scripted".into(),
        Arc::new(Scripted {
            tool_first,
            turns: AtomicU32::new(0),
        }),
    );
    let chat = ChatSessionManagerHandle::spawn(
        &dir.path().join("chat.db"),
        Some(Arc::new(LlmRouter::with_backends(backends, "scripted"))),
        ToolRegistryHandle::start(),
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
    let repository = Arc::new(
        TaskRepository::open(&apollia_core::paths::DataFile::Hitl.path(dir.path()))
            .await
            .unwrap(),
    );
    let engine = WorkflowHandle::spawn(
        repository.clone(),
        state.event_sender.clone(),
        StepBudgetConfig::default(),
        None,
    );
    engine.set_host(Arc::new(Host)).await.unwrap();
    let invoker = Arc::new(A2AInvoker::new(
        state.registry_handle.clone(),
        state.router_handle.clone(),
        state.event_sender.clone(),
        A2AConfig::default(),
    ));
    state.task_repository = Some(repository);
    state.chat_manager = Some(chat);
    state.workflows = Some(engine);
    state.a2a_invoker = Some(invoker.clone());
    Fixture {
        state,
        invoker,
        events,
        _dir: dir,
    }
}

/// Waits until `name` is bound, the moment a `DELETE` can reach it.
async fn bound(invoker: &A2AInvoker, name: &str) -> AliasTarget {
    loop {
        if let Some(target) = invoker.aliases().resolve_target(name) {
            return target;
        }
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn the_card_lists_the_agents_and_the_workflows() {
    // GIVEN a runtime with one agent without code and one workflow
    let fx = fixture(false).await;

    // WHEN the host reads the card and the flat list
    let (agents_status, agents) = call(fx.state.clone(), "GET", "/api/v1/a2a/agents", None).await;
    let (skills_status, skills) = call(fx.state.clone(), "GET", "/api/v1/a2a/skills", None).await;

    // THEN the agent is listed with a message entry, the workflow with its
    //      typed inputs, each under the identifier invoke takes
    assert_eq!(agents_status, StatusCode::OK, "{agents}");
    let agents = agents["agents"].as_array().unwrap();
    let agent = agents
        .iter()
        .find(|a| a["agent_id"] == "agent:ledger-clerk")
        .expect("the agent is on the card");
    assert_eq!(agent["kind"], "agent");
    assert_eq!(agent["name"], "Ledger clerk");
    assert_eq!(agent["input_schema"]["required"], json!(["message"]));
    assert_eq!(agent["skills"][0]["id"], "agent:ledger-clerk");
    let workflow = agents
        .iter()
        .find(|a| a["agent_id"] == "workflow:invoices/close-month")
        .expect("the workflow is on the card");
    assert_eq!(workflow["kind"], "workflow");
    assert_eq!(workflow["version"], "2.1.0");
    assert_eq!(workflow["input_schema"], schema());

    assert_eq!(skills_status, StatusCode::OK, "{skills}");
    let listed: Vec<(String, String)> = skills["skills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["skill_id"].as_str().unwrap().to_string(),
                s["kind"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(
        listed.contains(&("agent:ledger-clerk".to_string(), "agent".to_string())),
        "{listed:?}"
    );
    assert!(
        listed.contains(&(
            "workflow:invoices/close-month".to_string(),
            "workflow".to_string()
        )),
        "{listed:?}"
    );
}

#[tokio::test]
async fn invoke_runs_a_workflow_by_its_identifier() {
    // GIVEN the workflow on the card
    let fx = fixture(false).await;

    // WHEN it is invoked with valid inputs
    let (status, body) = call(
        fx.state.clone(),
        "POST",
        "/api/v1/a2a/invoke",
        Some(json!({
            "skill_id": "workflow:invoices/close-month",
            "input": {"folder": "/data/march"}
        })),
    )
    .await;

    // THEN the call answers once the run has ended, with what it returned
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["skill_id"], "workflow:invoices/close-month");
    assert_eq!(body["agent_name"], WORKFLOW);
    assert_eq!(body["result"]["status"], "completed");
    assert_eq!(
        body["result"]["output"][0]["data"],
        json!({"filed": "/data/march"})
    );
}

#[tokio::test]
async fn invoke_refuses_workflow_inputs_that_do_not_fit_and_unknown_targets() {
    // GIVEN the workflow and the agent on the card
    let fx = fixture(false).await;

    // WHEN the workflow is invoked without its required input, an unknown
    //      workflow and an unknown agent are invoked, and the agent is given no
    //      message
    let invoke = |skill_id: &str, input: Value| {
        call(
            fx.state.clone(),
            "POST",
            "/api/v1/a2a/invoke",
            Some(json!({"skill_id": skill_id, "input": input})),
        )
    };
    let (bad_input, bad_body) = invoke("workflow:invoices/close-month", json!({})).await;
    let (no_workflow, _) = invoke("workflow:invoices/nothing", json!({})).await;
    let (no_agent, _) = invoke("agent:nobody", json!({"message": "hello"})).await;
    let (no_message, _) = invoke("agent:ledger-clerk", json!({"text": "hello"})).await;

    // THEN each is refused before anything runs, for the reason it has
    assert_eq!(bad_input, StatusCode::BAD_REQUEST);
    assert!(
        bad_body["error"].as_str().unwrap().contains("folder"),
        "{bad_body}"
    );
    assert_eq!(no_workflow, StatusCode::NOT_FOUND);
    assert_eq!(no_agent, StatusCode::NOT_FOUND);
    assert_eq!(no_message, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn invoke_runs_an_agent_on_a_message() {
    // GIVEN the agent on the card, whose model answers in one turn
    let fx = fixture(false).await;

    // WHEN it is invoked with a message
    let (status, body) = call(
        fx.state.clone(),
        "POST",
        "/api/v1/a2a/invoke",
        Some(json!({
            "skill_id": "agent:ledger-clerk",
            "input": {"message": "Reconcile the ledger of March."}
        })),
    )
    .await;

    // THEN the call answers with the agent's answer and the task it ran as
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["agent_name"], AGENT);
    assert_eq!(body["result"]["output"][0]["text"], "Two gaps found.");
    assert!(!body["result"]["task_id"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn a_workflow_invocation_is_cancelled_by_its_client_id() {
    // GIVEN a workflow invoked under a client id, holding until stopped
    let fx = fixture(false).await;
    let state = fx.state.clone();
    let invocation = tokio::spawn(async move {
        call(
            state,
            "POST",
            "/api/v1/a2a/invoke",
            Some(json!({
                "skill_id": "workflow:invoices/close-month",
                "input": {"folder": "/data/march", "hold": true},
                "invocation_id": "close-42"
            })),
        )
        .await
    });
    let target = bound(&fx.invoker, "close-42").await;
    assert!(matches!(target, AliasTarget::WorkflowRun(_)), "{target:?}");

    // WHEN the client cancels it by the id it chose
    let (status, body) = call(fx.state.clone(), "DELETE", "/api/v1/tasks/close-42", None).await;

    // THEN the run is stopped, the invocation answers that it was cancelled,
    //      and the name is released
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "canceled");
    let (invoked, answer) = invocation.await.unwrap();
    assert_eq!(invoked, StatusCode::CONFLICT, "{answer}");
    assert!(answer["error"].as_str().unwrap().contains("cancelled"));
    assert!(fx.invoker.aliases().is_empty());
}

#[tokio::test]
async fn an_agent_invocation_is_cancelled_by_its_client_id() {
    // GIVEN an agent invoked under a client id, its run waiting on an approval
    let mut fx = fixture(true).await;
    let state = fx.state.clone();
    let invocation = tokio::spawn(async move {
        call(
            state,
            "POST",
            "/api/v1/a2a/invoke",
            Some(json!({
                "skill_id": "agent:ledger-clerk",
                "input": "Reconcile the ledger of March.",
                "invocation_id": "ledger-7"
            })),
        )
        .await
    });
    loop {
        match fx.events.recv().await {
            Ok(RuntimeEvent::ChatApprovalRequired { .. }) => break,
            Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
            Err(broadcast::error::RecvError::Closed) => panic!("event bus closed"),
        }
    }
    let target = bound(&fx.invoker, "ledger-7").await;
    assert!(matches!(target, AliasTarget::Task(_)), "{target:?}");

    // WHEN the client cancels it by the id it chose
    let (status, body) = call(fx.state.clone(), "DELETE", "/api/v1/tasks/ledger-7", None).await;

    // THEN the task is cancelled and the invocation answers that it was
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "canceled");
    let (invoked, answer) = invocation.await.unwrap();
    assert_eq!(invoked, StatusCode::CONFLICT, "{answer}");
    assert!(fx.invoker.aliases().is_empty());
}
