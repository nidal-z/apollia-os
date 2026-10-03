//! A task on a stored agent that runs no code goes through the agent executor.

use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use apollia_core::{RuntimeEvent, StepBudgetConfig};
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, StreamChunk,
    TokenUsage, ToolCall,
};
use apollia_llm::LlmRouter;
use apollia_mcp::session::LoadingMode;
use apollia_tools::{
    AgentOrigin, AgentRepository, StoredAgent, TaskRepository, ToolRegistryHandle,
};
use axum::body::Body;
use axum::http::Request;
use axum::routing::{get, post};
use axum::Router;
use serde_json::{json, Value};
use tokio::sync::broadcast;
use tower::ServiceExt;

use super::*;
use crate::api::routes_agents::StubAgentLoader;
use crate::api::routes_llm::tests::{test_app_state_no_llm, MockBackend};
use crate::api::routes_tasks::{cancel_task, get_task, submit_task};
use crate::chat::types::ToolDecision;
use crate::chat::ChatSessionManagerHandle;

const AGENT: &str = "ledger-clerk";

/// Calls `file_read` on its first turn when `tool_first`, then answers.
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

struct Fixture {
    app: Router,
    chat: ChatSessionManagerHandle,
    events: broadcast::Receiver<RuntimeEvent>,
    hitl_db: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

/// A runtime holding one stored agent without code, `ledger-clerk`, granted
/// `file_read` by its definition, whose model is `Scripted`.
async fn fixture(tool_first: bool) -> Fixture {
    fixture_with(
        tool_first,
        Some(json!(["file_read"])),
        ToolRegistryHandle::start(),
    )
    .await
}

/// The same runtime, the definition listing `access` (none when `None`) and
/// the chat built on `registry`.
async fn fixture_with(
    tool_first: bool,
    access: Option<Value>,
    registry: ToolRegistryHandle,
) -> Fixture {
    let mut defaults = serde_json::Map::new();
    if let Some(access) = access {
        defaults.insert("access".into(), access);
    }
    let dir = tempfile::tempdir().unwrap();
    let agents =
        AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(dir.path())).unwrap();
    agents
        .save_definition(&StoredAgent {
            name: AGENT.into(),
            origin: AgentOrigin::Interface,
            package: None,
            version: "1.0.0".into(),
            definition: json!({
                "id": AGENT,
                "name": "Ledger clerk",
                "mission": "Reconcile the ledger.",
                "defaults": Value::Object(defaults)
            }),
            enabled: true,
            installed_at: "2026-10-01T00:00:00Z".into(),
            updated_at: "2026-10-01T00:00:00Z".into(),
        })
        .unwrap();
    let hitl_db = apollia_core::paths::DataFile::Hitl.path(dir.path());
    let mut state = test_app_state_no_llm();
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
        None,
        None,
        LoadingMode::Eager,
        20,
        None,
        false,
    )
    .unwrap();
    state.data_dir = dir.path().to_path_buf();
    state.task_repository = Some(Arc::new(TaskRepository::open(&hitl_db).await.unwrap()));
    state.chat_manager = Some(chat.clone());
    let app = Router::new()
        .route("/api/v1/tasks", post(submit_task::<MockBackend>))
        .route(
            "/api/v1/tasks/:id",
            get(get_task::<MockBackend>).delete(cancel_task::<MockBackend>),
        )
        .with_state(state);
    Fixture {
        app,
        chat,
        events,
        hitl_db,
        _dir: dir,
    }
}

async fn call(app: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(match body {
            Some(b) => Body::from(serde_json::to_vec(&b).unwrap()),
            None => Body::empty(),
        })
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn input() -> Value {
    json!({"parts": [{"type": "text", "text": "Reconcile the ledger of March."}]})
}

async fn submit(fx: &Fixture) -> String {
    let (status, body) = call(
        &fx.app,
        "POST",
        "/api/v1/tasks",
        Some(json!({"agent_id": AGENT, "input": input()})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    body["task_id"].as_str().unwrap().to_string()
}

/// The next event `pick` accepts. The bus is the barrier: no wall-clock
/// deadline, a lagged receiver skips ahead, a closed bus fails the test.
async fn next_event<T>(
    events: &mut broadcast::Receiver<RuntimeEvent>,
    mut pick: impl FnMut(RuntimeEvent) -> Option<T>,
) -> T {
    loop {
        match events.recv().await {
            Ok(event) => {
                if let Some(found) = pick(event) {
                    return found;
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => {}
            Err(broadcast::error::RecvError::Closed) => panic!("event bus closed"),
        }
    }
}

/// The persisted row of `task_id`: agent, status, whole input, output.
fn row(db: &std::path::Path, task_id: &str) -> (String, String, Value, String) {
    let conn = rusqlite::Connection::open(db).unwrap();
    conn.query_row(
        "SELECT agent_name, status, input_json, output_text FROM tasks WHERE task_id = ?1",
        [task_id],
        |r| {
            let input: String = r.get(2)?;
            Ok((
                r.get(0)?,
                r.get(1)?,
                serde_json::from_str(&input).unwrap(),
                r.get::<_, Option<String>>(3)?.unwrap_or_default(),
            ))
        },
    )
    .unwrap()
}

// GIVEN a stored agent without code whose model answers in one turn
// WHEN a task is submitted to it
// THEN the executor runs it with no chat session, the task completes with the
//      answer, the end is published, and hitl.db holds its agent, its whole
//      input and its output
#[tokio::test]
async fn test_task_on_declarative_agent_runs_the_executor_and_is_recorded() {
    // GIVEN
    let mut fx = fixture(false).await;

    // WHEN
    let task_id = submit(&fx).await;

    // THEN
    let (success, output) = next_event(&mut fx.events, |e| match e {
        RuntimeEvent::TaskCompleted {
            task_id: t,
            success,
            output,
            ..
        } if t == task_id.as_str() => Some((success, output)),
        _ => None,
    })
    .await;
    assert!(success);
    assert_eq!(output.as_deref(), Some("Two gaps found."));
    let done = crate::test_support::poll_until_async(Duration::from_secs(10), || async {
        let (_, body) = call(&fx.app, "GET", &format!("/api/v1/tasks/{task_id}"), None).await;
        body["status"] == "completed"
    })
    .await;
    assert!(done, "the task never read as completed");
    let (_, body) = call(&fx.app, "GET", &format!("/api/v1/tasks/{task_id}"), None).await;
    assert_eq!(body["result"], "Two gaps found.");
    assert_eq!(
        row(&fx.hitl_db, &task_id),
        (
            AGENT.to_string(),
            "completed".to_string(),
            input(),
            "Two gaps found.".to_string()
        )
    );
    let sessions = fx.chat.list_sessions(None).await;
    assert!(
        sessions.is_empty(),
        "a chat session was opened: {sessions:?}"
    );
}

// GIVEN a stored agent without code whose model first calls a tool it has
//       not been allowed to run without asking
// WHEN a task is submitted
// THEN the approval is raised as a chat approval naming the task, the chat's
//      resolve command answers it, and the run goes on to complete
#[tokio::test]
async fn test_off_chat_approval_joins_the_chat_queue() {
    // GIVEN
    let mut fx = fixture(true).await;

    // WHEN
    let task_id = submit(&fx).await;
    let (session_id, message_id, call_id, tool) = next_event(&mut fx.events, |e| match e {
        RuntimeEvent::ChatApprovalRequired {
            session_id,
            message_id,
            tool_call_id,
            tool_name,
            ..
        } => Some((session_id, message_id, tool_call_id, tool_name)),
        _ => None,
    })
    .await;

    // THEN
    assert_eq!(session_id, task_id);
    assert_eq!(tool, "file_read");
    fx.chat
        .resolve_tool(session_id, message_id, call_id, tool, ToolDecision::Accept)
        .await
        .expect("the approval was no longer waiting");
    let success = next_event(&mut fx.events, |e| match e {
        RuntimeEvent::TaskCompleted {
            task_id: t,
            success,
            ..
        } if t == task_id.as_str() => Some(success),
        RuntimeEvent::ChatApprovalTimeout { .. } => Some(false),
        _ => None,
    })
    .await;
    assert!(success);
}

// GIVEN a stored agent without code whose access nobody chose, file_read
//       active as a tool of the reading family, and a model that calls it
// WHEN a task is submitted
// THEN file_read is granted by default: the call runs, reading needing no
//      consent, rather than being refused
#[tokio::test]
async fn test_an_agent_without_code_reads_by_default_on_a_task() {
    // GIVEN
    let registry = ToolRegistryHandle::start();
    registry
        .register(apollia_tools::tools::file_read::FileRead::descriptor())
        .await
        .unwrap();
    let mut fx = fixture_with(true, None, registry).await;

    // WHEN
    submit(&fx).await;
    let first = next_event(&mut fx.events, |e| match e {
        RuntimeEvent::ChatToolCallStarted { tool_name, .. } => Some(Ok(tool_name)),
        RuntimeEvent::ChatApprovalRequired { tool_name, .. } => Some(Err(tool_name)),
        _ => None,
    })
    .await;

    // THEN the reading tool runs, without asking: reading needs no consent
    assert_eq!(first, Ok("file_read".to_string()));
}

// GIVEN a task whose run waits on a tool approval
// WHEN the task is cancelled
// THEN the cancellation answers canceled once the executor has stopped, the
//      cancellation is published, and hitl.db records the task canceled
#[tokio::test]
async fn test_cancelling_the_task_stops_the_executor() {
    // GIVEN
    let mut fx = fixture(true).await;
    let task_id = submit(&fx).await;
    next_event(&mut fx.events, |e| match e {
        RuntimeEvent::ChatApprovalRequired { session_id, .. } if session_id == task_id => Some(()),
        _ => None,
    })
    .await;

    // WHEN
    let (status, body) = call(&fx.app, "DELETE", &format!("/api/v1/tasks/{task_id}"), None).await;

    // THEN
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "canceled");
    next_event(&mut fx.events, |e| match e {
        RuntimeEvent::TaskCanceled { task_id: t } if t == task_id.as_str() => Some(()),
        _ => None,
    })
    .await;
    let recorded = crate::test_support::poll_until(Duration::from_secs(10), || {
        row(&fx.hitl_db, &task_id).1 == "canceled"
    })
    .await;
    assert!(recorded, "hitl.db never recorded the cancellation");
}

// GIVEN a runtime whose agents.db holds no agent of that name
// WHEN a task is submitted to it
// THEN the coordinator path answers, as before: the agent is not found
#[tokio::test]
async fn test_unknown_agent_keeps_the_coordinator_path() {
    // GIVEN
    let fx = fixture(false).await;

    // WHEN
    let (status, _) = call(
        &fx.app,
        "POST",
        "/api/v1/tasks",
        Some(json!({"agent_id": "nobody", "input": input()})),
    )
    .await;

    // THEN
    assert_eq!(status, StatusCode::NOT_FOUND);
}
