//! Composition in a running runtime: stored agents run through the real agent
//! executor over a scripted model, workflows through the real engine, an A2A
//! worker is loaded, and every tool approval is accepted as it is raised.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Mutex;

use apollia_core::{A2AConfig, AgentManifest, ProcessState, RuntimeEvent, StepBudgetConfig};
use apollia_llm::types::{
    ChatMessage, CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError,
    MessageContent, Role, StreamChunk, TokenUsage, ToolCall,
};
use apollia_llm::LlmRouter;
use apollia_mcp::session::LoadingMode;
use apollia_tools::{
    AgentOrigin, AgentRepository, StoredAgent, TaskRepository, ToolRegistryHandle,
};

use super::*;
use crate::a2a::{A2AInvoker, A2aDelegateResult, A2aError};
use crate::agents::task::{AgentRunOutcome, AgentTaskLauncher};
use crate::api::routes_agents::StubAgentLoader;
use crate::api::routes_llm::tests::{test_app_state_no_llm, MockBackend};
use crate::chat::types::ToolDecision;
use crate::chat::ChatSessionManagerHandle;
use crate::workflows::{LaunchedAgentRunner, WorkflowCall, WorkflowHost};

const ANSWER: &str = "Total: 42";

/// The tools offered to each request, in order, by the agent whose mission
/// opened its system prompt.
type Offered = Arc<Mutex<Vec<(String, Vec<ToolSpec>)>>>;

/// After a tool result, reports it; else calls the first agent or workflow
/// it is offered; else answers [`ANSWER`].
struct Scripted {
    offered: Offered,
}

impl Scripted {
    fn reply(&self, req: &CompletionRequest) -> StreamChunk {
        let mission = req
            .messages
            .iter()
            .find(|m| matches!(m.role, Role::System))
            .map(text_of)
            .unwrap_or_default();
        self.offered
            .lock()
            .unwrap()
            .push((mission, req.tools.clone()));
        if let Some(result) = req.messages.iter().rev().find_map(|m| match &m.content {
            MessageContent::ToolResult { content, .. } => Some(content.clone()),
            _ => None,
        }) {
            return StreamChunk::Text(format!("Report: {result}"));
        }
        let call = req.tools.iter().find(|t| is_call_access(&t.name));
        match call {
            Some(tool) if tool.name.starts_with(AGENT_ACCESS_PREFIX) => {
                StreamChunk::ToolCall(ToolCall {
                    id: "call-1".into(),
                    name: tool.name.clone(),
                    arguments: json!({"message": "What is the total?"}),
                })
            }
            Some(tool) => StreamChunk::ToolCall(ToolCall {
                id: "call-1".into(),
                name: tool.name.clone(),
                arguments: json!({}),
            }),
            None => StreamChunk::Text(ANSWER.into()),
        }
    }
}

fn text_of(message: &ChatMessage) -> String {
    match &message.content {
        MessageContent::Text(text) => text.clone(),
        MessageContent::ToolResult { content, .. } => content.clone(),
        MessageContent::WithToolCalls { text, .. } => text.clone(),
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
        let chunk = self.reply(&req);
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

/// `books/close` returns `{"closed": true}`; `books/ask` runs the
/// accountant in its step; `books/wait` signals `waiting` once it is
/// running, then waits until it is stopped.
struct Host {
    waiting: Arc<tokio::sync::Notify>,
}

impl WorkflowHost for Host {
    fn describe(&self, workflow_id: &str) -> Option<WorkflowDescriptor> {
        let description = match workflow_id {
            "books/close" => "Close the books of the month.",
            "books/ask" => "Ask the accountant.",
            "books/wait" => "Wait.",
            _ => return None,
        };
        Some(WorkflowDescriptor {
            id: workflow_id.into(),
            version: "1.0.0".into(),
            description: description.into(),
            steps: vec!["step".into()],
            input_schema: None,
        })
    }

    fn list(&self) -> Vec<WorkflowDescriptor> {
        ["books/ask", "books/close", "books/wait"]
            .into_iter()
            .filter_map(|id| self.describe(id))
            .collect()
    }

    fn run(&self, call: WorkflowCall) -> BoxFuture<'static, Result<Value, String>> {
        let waiting = Arc::clone(&self.waiting);
        Box::pin(async move {
            call.services.step_started("step".into()).await?;
            let outcome = match call.workflow.id.as_str() {
                "books/ask" => call
                    .services
                    .run_agent("accountant".into(), "Total?".into())
                    .await
                    .map(Value::String),
                "books/wait" => {
                    waiting.notify_one();
                    call.cancel.cancelled().await;
                    Err("stopped".to_string())
                }
                _ => Ok(json!({"closed": true})),
            };
            call.services
                .step_ended("step".into(), outcome.as_ref().err().cloned())
                .await?;
            outcome
        })
    }
}

fn delegate() -> crate::a2a::A2aDelegateFn {
    Arc::new(
        |_skill: String,
         _input: Value,
         _timeout: u64,
         _chain: Vec<apollia_core::AgentId>,
         _caller: apollia_core::AgentId,
         _invocation: Option<crate::a2a::ClientInvocation>| {
            let fut: Pin<
                Box<dyn std::future::Future<Output = Result<A2aDelegateResult, A2aError>> + Send>,
            > = Box::pin(async {
                Ok(A2aDelegateResult {
                    task_id: "task".into(),
                    run_id: None,
                    agent_name: "excel-worker".into(),
                    output: "sheet".into(),
                })
            });
            fut
        },
    )
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
        installed_at: "2026-10-02T00:00:00Z".into(),
        updated_at: "2026-10-02T00:00:00Z".into(),
    })
    .unwrap();
}

struct Fixture {
    launcher: AgentTaskLauncher<MockBackend>,
    composer: RuntimeComposer,
    repository: Arc<TaskRepository>,
    offered: Offered,
    a2a: Arc<A2AInvoker>,
    waiting: Arc<tokio::sync::Notify>,
    _dir: tempfile::TempDir,
}

/// Four stored agents, a loaded A2A worker offering one skill, the chat
/// manager over the scripted model with the composer of the runtime, and a
/// listener accepting every tool approval.
async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let agents =
        AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(dir.path())).unwrap();
    store(&agents, "accountant", "Answer about the books.", &[]);
    store(&agents, "loner", "Work alone.", &["file_read"]);
    store(
        &agents,
        "writer",
        "Write the report.",
        &["agent:accountant"],
    );
    store(
        &agents,
        "planner",
        "Plan the month.",
        &["workflow:books/close"],
    );

    let state = test_app_state_no_llm();
    let worker: AgentManifest = serde_json::from_value(json!({
        "format_version": 1,
        "name": "excel-worker",
        "version": "0.1.0",
        "description": "Reads spreadsheets.",
        "tools_required": [],
        "tools_optional": [],
        "supports_streaming": false,
        "supports_a2a": true,
        "max_concurrent_tasks": 1,
        "dangerous_tools_allowed": false,
        "tags": [],
        "execution_mode": "direct",
        "skills": [{
            "id": "read-excel",
            "name": "read-excel",
            "description": "Reads Excel files",
            "input_modes": ["text"],
            "output_modes": ["text"]
        }]
    }))
    .unwrap();
    let worker_id = state.registry_handle.register(worker).await.unwrap();
    state
        .registry_handle
        .update_state(worker_id.as_str(), ProcessState::Active)
        .await
        .unwrap();
    let a2a = Arc::new(A2AInvoker::new_for_test(
        state.registry_handle.clone(),
        delegate(),
        state.event_sender.clone(),
        A2AConfig::default(),
    ));

    let offered: Offered = Arc::default();
    let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
    backends.insert(
        "scripted".into(),
        Arc::new(Scripted {
            offered: Arc::clone(&offered),
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
        Some(Arc::clone(&a2a)),
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
    let launcher = || AgentTaskLauncher {
        router: state.router_handle.clone(),
        chat: chat.clone(),
        repository: Some(repository.clone()),
        event_bus: state.event_sender.clone(),
        observability: state.obs_config.clone(),
    };
    let agents: Arc<dyn AgentRunner> = Arc::new(LaunchedAgentRunner::new(
        launcher(),
        dir.path().to_path_buf(),
    ));
    let engine = WorkflowHandle::spawn(
        repository.clone(),
        state.event_sender.clone(),
        StepBudgetConfig::default(),
        Some(Arc::clone(&agents)),
    );
    let waiting = Arc::new(tokio::sync::Notify::new());
    engine
        .set_host(Arc::new(Host {
            waiting: Arc::clone(&waiting),
        }))
        .await
        .unwrap();
    chat.set_composer(Arc::new(RuntimeComposer::new(
        Arc::clone(&agents),
        engine.clone(),
        dir.path().to_path_buf(),
    )))
    .await;

    let approvals = chat.clone();
    let mut bus = state.event_sender.subscribe();
    tokio::spawn(async move {
        while let Ok(event) = bus.recv().await {
            if let RuntimeEvent::ChatApprovalRequired {
                session_id,
                message_id,
                tool_call_id,
                tool_name,
                ..
            } = event
            {
                let _ = approvals
                    .resolve_tool(
                        session_id,
                        message_id,
                        tool_call_id,
                        tool_name,
                        ToolDecision::Accept,
                    )
                    .await;
            }
        }
    });

    Fixture {
        launcher: launcher(),
        composer: RuntimeComposer::new(agents, engine, dir.path().to_path_buf()),
        repository,
        offered,
        a2a,
        waiting,
        _dir: dir,
    }
}

impl Fixture {
    async fn run(&self, agent: &str) -> AgentRunOutcome {
        let resolved = resolve_runnable(self._dir.path().to_path_buf(), agent, "the tests")
            .await
            .unwrap();
        let (_task, end) = self
            .launcher
            .launch_watched(&resolved, &json!({"message": "Go"}), "Go".into())
            .await
            .unwrap();
        end.await.unwrap()
    }

    /// The tools offered to the requests of the agent of `mission`.
    fn offered_to(&self, mission: &str) -> Vec<Vec<ToolSpec>> {
        self.offered
            .lock()
            .unwrap()
            .iter()
            .filter(|(prompt, _)| prompt.contains(mission))
            .map(|(_, tools)| tools.clone())
            .collect()
    }

    fn caller(&self, depth: u32, max_steps: u32) -> CallerRun {
        CallerRun {
            agent_id: "writer".into(),
            depth,
            cancel: CancellationToken::new(),
            budget: Arc::new(StepBudget::new(&StepBudgetConfig {
                max_steps,
                ..StepBudgetConfig::default()
            })),
        }
    }
}

#[tokio::test]
async fn an_agent_granted_no_agent_or_workflow_sees_no_call_tool() {
    // GIVEN a loaded A2A worker offering a skill, other stored agents, and
    //       an agent granted one native tool only
    let fx = fixture().await;

    let skills = fx.a2a.list_skills().await.unwrap();
    assert!(skills.iter().any(|s| s.skill_id == "read-excel"));

    // WHEN that agent runs
    let outcome = fx.run("loner").await;

    // THEN it answered, and none of its requests offered a call to an A2A
    //      skill, an agent or a workflow
    assert_eq!(outcome.status, "completed", "{}", outcome.output);
    let offered = fx.offered_to("Work alone.");
    assert!(!offered.is_empty());
    for tools in offered {
        let calls: Vec<&str> = tools
            .iter()
            .map(|t| t.name.as_str())
            .filter(|n| n.starts_with("a2a:") || is_call_access(n))
            .collect();
        assert!(calls.is_empty(), "call tools offered: {calls:?}");
    }
}

#[tokio::test]
async fn a_granted_agent_is_a_tool_described_by_its_mission_whose_call_runs_the_executor() {
    // GIVEN the writer, granted the accountant
    let fx = fixture().await;

    // WHEN the writer runs and calls the accountant
    let outcome = fx.run("writer").await;

    // THEN the accountant was offered under its mission and nothing else
    //      was, its answer came back through the executor, and its run is a
    //      task of its own
    assert_eq!(outcome.status, "completed", "{}", outcome.output);
    assert_eq!(outcome.output, format!("Report: {ANSWER}"));
    let first = &fx.offered_to("Write the report.")[0];
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].name, "agent:accountant");
    assert_eq!(first[0].description, "Answer about the books.");
    let tasks = fx.repository.list_recent_tasks(10).await.unwrap();
    let mut agents: Vec<&str> = tasks.iter().map(|t| t.agent_name.as_str()).collect();
    agents.sort_unstable();
    assert_eq!(agents, vec!["accountant", "writer"]);
}

#[tokio::test]
async fn a_granted_workflow_is_a_tool_described_by_its_description_whose_call_runs_it() {
    // GIVEN the planner, granted the closing of the books
    let fx = fixture().await;

    // WHEN the planner runs and calls the workflow
    let outcome = fx.run("planner").await;

    // THEN the workflow was offered under its description, ran one level
    //      below the planner, and its output came back
    assert_eq!(outcome.status, "completed", "{}", outcome.output);
    assert_eq!(outcome.output, r#"Report: {"closed":true}"#);
    let first = &fx.offered_to("Plan the month.")[0];
    assert_eq!(first[0].name, "workflow:books/close");
    assert_eq!(first[0].description, "Close the books of the month.");
    let runs = fx
        .repository
        .workflow_runs("books/close", 10)
        .await
        .unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, "completed");
    assert_eq!(runs[0].depth, 2);
}

#[tokio::test]
async fn a_called_agent_spends_from_the_budget_of_its_caller() {
    // GIVEN a caller whose budget allows ten steps
    let fx = fixture().await;
    let caller = fx.caller(1, 10);
    let before = caller.budget.steps_left();

    // WHEN it calls the accountant
    let answer = fx
        .composer
        .run_agent("accountant".into(), "Total?".into(), caller.clone())
        .await;

    // THEN the accountant answered, and its turn was taken from the caller's
    //      budget
    assert_eq!(answer.unwrap(), ANSWER);
    assert!(caller.budget.steps_left() < before);
}

#[tokio::test]
async fn stopping_the_caller_stops_the_workflow_it_called() {
    // GIVEN a caller running a workflow that waits until it is stopped
    let fx = fixture().await;
    let caller = fx.caller(1, 10);
    let token = caller.cancel.clone();
    let call = fx
        .composer
        .run_workflow("books/wait".into(), json!({}), caller);
    let stop = async {
        fx.waiting.notified().await;
        token.cancel();
    };

    // WHEN the caller is stopped once the workflow is waiting
    let (outcome, ()) = tokio::join!(call, stop);

    // THEN the workflow inherited the token: it ended canceled
    assert!(outcome.unwrap_err().contains("canceled"));
    let runs = fx.repository.workflow_runs("books/wait", 10).await.unwrap();
    assert_eq!(runs[0].status, "canceled");
}

#[tokio::test]
async fn a_workflow_called_from_the_second_level_cannot_run_an_agent_at_the_fourth() {
    // GIVEN a caller at the second level
    let fx = fixture().await;

    // WHEN it calls a workflow whose step runs an agent
    let outcome = fx
        .composer
        .run_workflow("books/ask".into(), json!({}), fx.caller(2, 10))
        .await;

    // THEN the workflow ran at the third level and its agent, at the fourth,
    //      was refused with a clear error
    let error = outcome.unwrap_err();
    assert!(error.contains("level 4"), "{error}");
    let runs = fx.repository.workflow_runs("books/ask", 10).await.unwrap();
    assert_eq!(runs[0].depth, 3);
}

#[tokio::test]
async fn the_composer_reads_the_mission_and_the_workflow_it_offers() {
    // GIVEN the runtime composer
    let fx = fixture().await;

    // WHEN it reads an agent, an unknown one, and a workflow
    let mission = fx.composer.agent_mission("accountant".into()).await;
    let ghost = fx.composer.agent_mission("ghost".into()).await;
    let workflow = fx.composer.workflow("books/close".into()).await;

    // THEN the stored mission and the declared description come back
    assert_eq!(mission.as_deref(), Some("Answer about the books."));
    assert!(ghost.is_none());
    assert_eq!(
        workflow.map(|w| w.description).as_deref(),
        Some("Close the books of the month.")
    );
}
