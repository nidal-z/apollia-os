//! A step's agent runs through the real agent executor, over a scripted
//! model: recorded as a task, spending from the workflow's budget.

use std::pin::Pin;
use std::sync::Arc;

use apollia_core::StepBudgetConfig;
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, StreamChunk,
    TokenUsage,
};
use apollia_llm::LlmRouter;
use apollia_mcp::session::LoadingMode;
use apollia_tools::{
    AgentOrigin, AgentRepository, StoredAgent, TaskRepository, ToolRegistryHandle,
};
use serde_json::{json, Value};

use super::*;
use crate::agents::task::AgentTaskLauncher;
use crate::api::routes_agents::StubAgentLoader;
use crate::api::routes_llm::tests::test_app_state_no_llm;
use crate::chat::ChatSessionManagerHandle;
use crate::workflows::{WorkflowCall, WorkflowDescriptor, WorkflowHandle, WorkflowHost};

const AGENT: &str = "accountant";
const ANSWER: &str = "Total: 42";

/// Answers every request in one turn, without a tool.
struct Answers;

#[async_trait::async_trait]
impl CompletionModel for Answers {
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
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
    {
        Ok(Box::pin(futures::stream::iter(vec![Ok(
            StreamChunk::Text(ANSWER.into()),
        )])))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "answers"
    }
    fn model_id(&self) -> &str {
        "answers"
    }
}

/// `books/close`: its step `ask` runs the agent, its step `file` does nothing.
struct Host;

impl WorkflowHost for Host {
    fn describe(&self, workflow_id: &str) -> Option<WorkflowDescriptor> {
        (workflow_id == "books/close").then(|| WorkflowDescriptor {
            id: workflow_id.into(),
            version: "1.0.0".into(),
            description: "Close the books.".into(),
            steps: vec!["ask".into(), "file".into()],
            input_schema: None,
        })
    }

    fn list(&self) -> Vec<WorkflowDescriptor> {
        self.describe("books/close").into_iter().collect()
    }

    fn run(&self, call: WorkflowCall) -> BoxFuture<'static, Result<Value, String>> {
        Box::pin(async move {
            call.services.step_started("ask".into()).await?;
            let answer = match call
                .services
                .run_agent(AGENT.into(), "What is the total?".into())
                .await
            {
                Ok(answer) => answer,
                Err(error) => {
                    call.services
                        .step_ended("ask".into(), Some(error.clone()))
                        .await?;
                    return Err(error);
                }
            };
            call.services.step_ended("ask".into(), None).await?;
            call.services.step_started("file".into()).await?;
            call.services.step_ended("file".into(), None).await?;
            Ok(json!(answer))
        })
    }
}

struct Fixture {
    engine: WorkflowHandle,
    repository: Arc<TaskRepository>,
    _dir: tempfile::TempDir,
}

/// A stored agent without code, the chat manager over the scripted model, and
/// the workflow engine whose budget allows `max_steps` steps.
async fn fixture(max_steps: u32) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(dir.path()))
        .unwrap()
        .save_definition(&StoredAgent {
            name: AGENT.into(),
            origin: AgentOrigin::Interface,
            package: None,
            version: "1.0.0".into(),
            definition: json!({"id": AGENT, "name": "Accountant", "mission": "Answer about the books."}),
            enabled: true,
            installed_at: "2026-10-02T00:00:00Z".into(),
            updated_at: "2026-10-02T00:00:00Z".into(),
        })
        .unwrap();
    let mut backends: std::collections::HashMap<String, Arc<dyn CompletionModel>> =
        std::collections::HashMap::new();
    backends.insert("answers".into(), Arc::new(Answers));
    let state = test_app_state_no_llm();
    let chat = ChatSessionManagerHandle::spawn(
        &dir.path().join("chat.db"),
        Some(Arc::new(LlmRouter::with_backends(backends, "answers"))),
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
    let launcher = AgentTaskLauncher {
        router: state.router_handle.clone(),
        chat,
        repository: Some(repository.clone()),
        event_bus: state.event_sender.clone(),
        observability: state.obs_config.clone(),
    };
    let budget = StepBudgetConfig {
        max_steps,
        ..StepBudgetConfig::default()
    };
    let engine = WorkflowHandle::spawn(
        repository.clone(),
        state.event_sender.clone(),
        budget,
        Some(Arc::new(LaunchedAgentRunner::new(
            launcher,
            dir.path().to_path_buf(),
        ))),
    );
    engine.set_host(Arc::new(Host)).await.unwrap();
    Fixture {
        engine,
        repository,
        _dir: dir,
    }
}

#[tokio::test]
async fn the_agent_of_a_step_runs_through_the_executor_as_a_task() {
    // GIVEN a budget wide enough for both steps and the agent's turn
    let fx = fixture(3).await;

    // WHEN the workflow runs
    let run = fx
        .engine
        .run_to_end("books/close", json!({}))
        .await
        .unwrap();

    // THEN the agent answered through the executor, its answer is the step's
    //      output, and its run is a task of its own, completed
    assert_eq!(run.status, "completed", "{:?}", run.error);
    assert_eq!(run.output, Some(json!(ANSWER)));
    let steps = fx.repository.workflow_steps(&run.run_id).await.unwrap();
    assert_eq!(steps[0].output.as_deref(), Some(ANSWER));
    let tasks = fx.repository.list_recent_tasks(10).await.unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].agent_name, AGENT);
    assert_eq!(tasks[0].output_text.as_deref(), Some(ANSWER));
}

#[tokio::test]
async fn the_agent_spends_from_the_workflow_budget() {
    // GIVEN a budget of two steps: the step `ask`, then the agent's one turn
    let fx = fixture(2).await;

    // WHEN the workflow runs
    let run = fx
        .engine
        .run_to_end("books/close", json!({}))
        .await
        .unwrap();

    // THEN the agent answered, but its turn spent the last step, so the step
    //      `file` was refused: the agent drew on the workflow's budget
    assert_eq!(run.status, "failed");
    assert!(run.error.unwrap().contains("budget"));
    let steps = fx.repository.workflow_steps(&run.run_id).await.unwrap();
    let statuses: Vec<&str> = steps.iter().map(|s| s.status.as_str()).collect();
    assert_eq!(statuses, vec!["completed", "not_started"]);
    assert_eq!(steps[0].output.as_deref(), Some(ANSWER));
}

#[tokio::test]
async fn an_agent_given_a_spent_budget_does_not_run() {
    // GIVEN a budget of one step, spent by opening `ask`
    let fx = fixture(1).await;

    // WHEN the workflow runs
    let run = fx
        .engine
        .run_to_end("books/close", json!({}))
        .await
        .unwrap();

    // THEN the executor refused the agent's turn for the budget, and the step failed
    assert_eq!(run.status, "failed");
    let steps = fx.repository.workflow_steps(&run.run_id).await.unwrap();
    assert_eq!(steps[0].status, "failed");
    assert!(steps[0]
        .error
        .as_deref()
        .unwrap_or_default()
        .contains(AGENT));
    let tasks = fx.repository.list_recent_tasks(10).await.unwrap();
    assert_eq!(tasks.len(), 1);
    let status = fx
        .repository
        .get_task_status(&tasks[0].task_id)
        .await
        .unwrap();
    assert_eq!(status.as_deref(), Some("failed"));
}
