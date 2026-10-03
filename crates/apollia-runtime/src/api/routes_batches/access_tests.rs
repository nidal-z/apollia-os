//! The accesses of a batch over the real agent executor: listed before the
//! launch, granted for the batch in `family_consents`, withdrawn at its end,
//! and an item that reaches for an access nobody granted waits.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use apollia_core::batch_source::{BatchSource, MemorySource, SourceRegistry};
use apollia_core::{RuntimeEvent, StepBudgetConfig};
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, Role,
    StreamChunk, TokenUsage, ToolCall,
};
use apollia_llm::LlmRouter;
use apollia_mcp::session::LoadingMode;
use apollia_tools::{
    ActionFamily, AgentOrigin, AgentRepository, StoredAgent, TaskRepository, ToolRegistryHandle,
};
use axum::body::Body;
use axum::http::Request;
use serde_json::{json, Value};
use tokio::sync::{broadcast, Semaphore};
use tower::ServiceExt;

use super::*;
use crate::agents::task::AgentTaskLauncher;
use crate::api::routes_agents::StubAgentLoader;
use crate::api::routes_llm::tests::{test_app_state_no_llm, MockBackend};
use crate::batch::AgentItemRunner;
use crate::chat::ChatSessionManagerHandle;
use crate::test_support::{collect_events_until, poll_until_async};

const AGENT: &str = "outreach-clerk";
const WAIT: Duration = Duration::from_secs(30);

/// Calls `tool` on the first turn of each run, once the test lets it through,
/// then answers.
struct Reaching {
    tool: String,
    gate: Arc<Semaphore>,
}

#[async_trait::async_trait]
impl CompletionModel for Reaching {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            engine_timings: None,
            content: "Sent.".into(),
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
        let first_turn = !req.messages.iter().any(|m| matches!(m.role, Role::Tool));
        let chunk = if first_turn {
            drop(self.gate.acquire().await);
            StreamChunk::ToolCall(ToolCall {
                id: "c1".into(),
                name: self.tool.clone(),
                arguments: json!({"to": "client@example.org", "body": "Your refund."}),
            })
        } else {
            StreamChunk::Text("Sent.".into())
        };
        Ok(Box::pin(futures::stream::iter(vec![Ok(chunk)])))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "reaching"
    }
    fn model_id(&self) -> &str {
        "reaching"
    }
}

struct Fixture {
    app: Router,
    gate: Arc<Semaphore>,
    events: broadcast::Receiver<RuntimeEvent>,
    tool: String,
    data_dir: std::path::PathBuf,
    hitl_db: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

/// The tool of a connected account that sends outside the machine: a
/// sensitive access that can be granted for a batch.
fn sending_tool() -> String {
    let accounts = [(
        apollia_auth::ConnectorProvider::Google,
        vec!["ada@example.com".to_owned()],
    )];
    crate::connectors_bridge::connector_descriptors_for(&accounts)
        .into_iter()
        .find(|d| d.family == ActionFamily::SendExternal)
        .map(|d| d.name)
        .expect("a connector operation that sends")
}

/// A runtime with one agent that reads files and sends through a connector,
/// a model that reaches for the sending tool, and a source of two requests.
async fn fixture() -> Fixture {
    let tool = sending_tool();
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
                "name": "Outreach clerk",
                "mission": "Answer each client by mail.",
                "defaults": {"access": ["file_read", tool]}
            }),
            enabled: true,
            installed_at: "2026-10-02T00:00:00Z".into(),
            updated_at: "2026-10-02T00:00:00Z".into(),
        })
        .unwrap();
    let gate = Arc::new(Semaphore::new(0));
    let mut backends: std::collections::HashMap<String, Arc<dyn CompletionModel>> =
        std::collections::HashMap::new();
    backends.insert(
        "reaching".into(),
        Arc::new(Reaching {
            tool: tool.clone(),
            gate: gate.clone(),
        }),
    );
    let mut state = test_app_state_no_llm();
    let events = state.event_sender.subscribe();
    let chat = ChatSessionManagerHandle::spawn(
        &dir.path().join("chat.db"),
        Some(Arc::new(LlmRouter::with_backends(backends, "reaching"))),
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
    let hitl_db = apollia_core::paths::DataFile::Hitl.path(dir.path());
    let repo = Arc::new(TaskRepository::open(&hitl_db).await.unwrap());
    let mut sources = SourceRegistry::new();
    sources
        .register(
            "memory",
            Box::new(|spec| {
                let source = MemorySource::new(&spec.location, &["client"])
                    .with_item("row-2", &[("client", "Durand")])
                    .with_item("row-3", &[("client", "Martin")]);
                Ok(Box::new(source) as Box<dyn BatchSource>)
            }),
        )
        .unwrap();
    let launcher = AgentTaskLauncher {
        router: state.router_handle.clone(),
        chat: chat.clone(),
        repository: Some(repo.clone()),
        event_bus: state.event_sender.clone(),
        observability: state.obs_config.clone(),
    };
    state.batches = Some(BatchHandle::spawn(
        repo.clone(),
        Arc::new(sources),
        AgentItemRunner::new(launcher, dir.path().to_path_buf()),
        state.event_sender.clone(),
    ));
    state.data_dir = dir.path().to_path_buf();
    state.task_repository = Some(repo);
    state.chat_manager = Some(chat);
    Fixture {
        app: batch_routes::<MockBackend>().with_state(state),
        gate,
        events,
        tool,
        data_dir: dir.path().to_path_buf(),
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

fn launch_body(grants: &[&str]) -> Value {
    json!({
        "agent": AGENT,
        "source": {"kind": "memory", "location": "/data/clients.csv"},
        "grants": grants
    })
}

/// The scopes of the consents `governance.db` holds for the agent.
fn consent_scopes(data_dir: &std::path::Path) -> Vec<(String, String)> {
    let path = apollia_core::paths::DataFile::Governance.path(data_dir);
    if !path.exists() {
        return Vec::new();
    }
    let conn = rusqlite::Connection::open(path).unwrap();
    let mut stmt = conn
        .prepare("SELECT family, scope FROM family_consents WHERE agent_id = ?1 ORDER BY id")
        .unwrap();
    let rows = stmt
        .query_map([AGENT], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    rows
}

async fn batch(app: &Router, id: &str) -> Value {
    call(app, "GET", &format!("/api/v1/batches/{id}"), None)
        .await
        .1
}

async fn wait_ended(app: &Router, id: &str) -> Value {
    let ended = poll_until_async(WAIT, || async {
        batch(app, id).await["batch"]["endedAt"].is_string()
    })
    .await;
    let detail = batch(app, id).await;
    assert!(ended, "the batch never ended: {detail}");
    detail
}

// GIVEN an agent that reads files and sends through a connector
// WHEN its accesses are listed before a launch
// THEN only the sending access is listed, with its family, and it can be granted
#[tokio::test]
async fn test_the_sensitive_accesses_are_listed_before_the_launch() {
    // GIVEN
    let fx = fixture().await;

    // WHEN
    let (status, body) = call(
        &fx.app,
        "GET",
        &format!("/api/v1/batches/accesses?agent={AGENT}"),
        None,
    )
    .await;

    // THEN
    assert_eq!(status, StatusCode::OK, "{body}");
    let accesses = body["accesses"].as_array().unwrap();
    assert_eq!(accesses.len(), 1, "{body}");
    assert_eq!(accesses[0]["accessId"], fx.tool.as_str());
    assert_eq!(accesses[0]["family"], "send_external");
    assert_eq!(accesses[0]["grantable"], true);
}

// GIVEN a batch launched granting the sending access
// WHEN it runs
// THEN the grant is in family_consents under the batch's scope while it runs,
//      the items use the access without stopping, and the grant is withdrawn
//      once the batch has ended
#[tokio::test]
async fn test_a_granted_access_is_recorded_for_the_batch_and_expires_with_it() {
    // GIVEN
    let fx = fixture().await;

    // WHEN
    let (status, launched) = call(
        &fx.app,
        "POST",
        "/api/v1/batches",
        Some(launch_body(&[fx.tool.as_str()])),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{launched}");
    let id = launched["batchId"].as_str().unwrap().to_string();
    let during = consent_scopes(&fx.data_dir);
    fx.gate.add_permits(10);

    // THEN
    assert_eq!(
        during,
        vec![("send_external".to_string(), format!("lot:{id}"))]
    );
    let detail = wait_ended(&fx.app, &id).await;
    assert_eq!(detail["batch"]["awaiting"], 0, "{detail}");
    assert_eq!(detail["batch"]["state"], "completed", "{detail}");
    let withdrawn =
        poll_until_async(WAIT, || async { consent_scopes(&fx.data_dir).is_empty() }).await;
    assert!(
        withdrawn,
        "the grant outlived the batch: {:?}",
        consent_scopes(&fx.data_dir)
    );
}

// GIVEN a batch launched with nothing granted
// WHEN its first item reaches for the sending access
// THEN that item waits for a decision instead of failing, the batch goes on
//      with the second, the wait is published, the batch ends with its
//      counters, and the run's task is recorded canceled
#[tokio::test]
async fn test_an_access_not_granted_puts_the_item_in_wait_and_the_batch_goes_on() {
    // GIVEN
    let mut fx = fixture().await;

    // WHEN
    let (status, launched) = call(&fx.app, "POST", "/api/v1/batches", Some(launch_body(&[]))).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{launched}");
    let id = launched["batchId"].as_str().unwrap().to_string();
    fx.gate.add_permits(10);

    // THEN
    let detail = wait_ended(&fx.app, &id).await;
    assert_eq!(detail["batch"]["awaiting"], 2, "{detail}");
    assert_eq!(detail["batch"]["failed"], 0, "{detail}");
    let items = detail["items"].as_array().unwrap();
    assert!(items.iter().all(|i| i["status"] == "awaiting"), "{detail}");
    assert!(
        items[0]["output"].as_str().unwrap().contains(&fx.tool),
        "{detail}"
    );
    let (mut seen, found) = collect_events_until(
        WAIT,
        &mut fx.events,
        |e| matches!(e, RuntimeEvent::BatchEnded { batch_id, .. } if batch_id == &id),
    )
    .await;
    assert!(found, "no end of batch published: {seen:?}");
    let end = seen.pop().expect("the end of the batch");
    let waited: Vec<(u64, String)> = seen
        .into_iter()
        .filter_map(|e| match e {
            RuntimeEvent::BatchItemAwaitingApproval { rank, access, .. } => Some((rank, access)),
            _ => None,
        })
        .collect();
    assert_eq!(waited, vec![(0, fx.tool.clone()), (1, fx.tool.clone())]);
    assert!(matches!(
        end,
        RuntimeEvent::BatchEnded {
            total: 2,
            completed: 0,
            failed: 0,
            awaiting: 2,
            ..
        }
    ));
    let conn = rusqlite::Connection::open(&fx.hitl_db).unwrap();
    let canceled: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM batch_items i JOIN tasks t ON t.task_id = i.task_id \
             WHERE i.batch_id = ?1 AND t.status = 'canceled'",
            [id.as_str()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(canceled, 2);
}

// GIVEN a batch whose two items wait for the sending access
// WHEN its waiting items are retried with that access granted
// THEN both run again from their input, use the access, and complete
#[tokio::test]
async fn test_a_waiting_item_runs_again_once_its_access_is_granted() {
    // GIVEN
    let fx = fixture().await;
    let (_, launched) = call(&fx.app, "POST", "/api/v1/batches", Some(launch_body(&[]))).await;
    let id = launched["batchId"].as_str().unwrap().to_string();
    fx.gate.add_permits(100);
    let first = wait_ended(&fx.app, &id).await;
    assert_eq!(first["batch"]["awaiting"], 2, "{first}");
    let inputs: Vec<Value> = first["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["input"].clone())
        .collect();

    // WHEN
    let (status, retried) = call(
        &fx.app,
        "POST",
        &format!("/api/v1/batches/{id}/retry"),
        Some(json!({"grants": [fx.tool.as_str()]})),
    )
    .await;

    // THEN
    assert_eq!(status, StatusCode::OK, "{retried}");
    let done = poll_until_async(WAIT, || async {
        batch(&fx.app, &id).await["batch"]["completed"] == 2
    })
    .await;
    let detail = batch(&fx.app, &id).await;
    assert!(done, "the waiting items never completed: {detail}");
    let again: Vec<Value> = detail["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["input"].clone())
        .collect();
    assert_eq!(again, inputs);
    assert_eq!(inputs[0], json!({"client": "Durand"}));
    assert_eq!(detail["batch"]["grants"], json!([fx.tool.as_str()]));
}

// GIVEN a batch whose first item is running, held by the model
// WHEN the batch is paused, then stopped, over HTTP
// THEN the pause is answered paused, the stop cancels the running item through
//      its task, the second item never runs, and the batch ends stopped
#[tokio::test]
async fn test_stop_over_http_cancels_the_running_item_by_its_task() {
    // GIVEN
    let fx = fixture().await;
    let (_, launched) = call(&fx.app, "POST", "/api/v1/batches", Some(launch_body(&[]))).await;
    let id = launched["batchId"].as_str().unwrap().to_string();
    let running = poll_until_async(WAIT, || async {
        batch(&fx.app, &id).await["items"][0]["status"] == "running"
    })
    .await;
    assert!(running, "the first item never ran");

    // WHEN
    let (pause_status, paused) = call(
        &fx.app,
        "POST",
        &format!("/api/v1/batches/{id}/pause"),
        None,
    )
    .await;
    let (stop_status, stopped) =
        call(&fx.app, "POST", &format!("/api/v1/batches/{id}/stop"), None).await;
    let (missing, _) = call(&fx.app, "POST", "/api/v1/batches/missing/stop", None).await;

    // THEN
    assert_eq!(pause_status, StatusCode::OK, "{paused}");
    assert_eq!(paused["state"], "paused");
    assert_eq!(stop_status, StatusCode::OK, "{stopped}");
    assert_eq!(stopped["state"], "stopped");
    assert_eq!(missing, StatusCode::NOT_FOUND);
    let detail = wait_ended(&fx.app, &id).await;
    let settled = poll_until_async(WAIT, || async {
        batch(&fx.app, &id).await["batch"]["canceled"] == 2
    })
    .await;
    let detail = if settled {
        batch(&fx.app, &id).await
    } else {
        detail
    };
    assert!(settled, "the running item was never cancelled: {detail}");
    let task_id = detail["items"][0]["taskId"].as_str().unwrap().to_string();
    let conn = rusqlite::Connection::open(&fx.hitl_db).unwrap();
    let status: String = conn
        .query_row(
            "SELECT status FROM tasks WHERE task_id = ?1",
            [task_id.as_str()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "canceled");
    assert!(detail["items"][1]["taskId"].is_null(), "{detail}");
    let (again, _) = call(
        &fx.app,
        "POST",
        &format!("/api/v1/batches/{id}/resume"),
        None,
    )
    .await;
    assert_eq!(again, StatusCode::CONFLICT);
}
