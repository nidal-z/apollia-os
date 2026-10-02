//! The batch routes over the real agent executor and a scripted model.

use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use apollia_core::batch_source::{BatchSource, MemorySource, SourceRegistry};
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
use axum::body::Body;
use axum::http::Request;
use serde_json::{json, Value};
use tokio::sync::Semaphore;
use tower::ServiceExt;

use super::*;
use crate::agents::task::AgentTaskLauncher;
use crate::api::routes_agents::StubAgentLoader;
use crate::api::routes_llm::tests::{test_app_state_no_llm, MockBackend};
use crate::batch::AgentItemRunner;
use crate::chat::ChatSessionManagerHandle;
use crate::test_support::{poll_until, poll_until_async};

const AGENT: &str = "request-clerk";
const WAIT: Duration = Duration::from_secs(30);

/// Answers every item in one turn, once the test lets it through, and counts
/// how many turns are open at once.
struct Gated {
    gate: Arc<Semaphore>,
    open: Arc<AtomicU32>,
    widest: Arc<AtomicU32>,
}

#[async_trait::async_trait]
impl CompletionModel for Gated {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            engine_timings: None,
            content: "Handled.".into(),
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
        let now = self.open.fetch_add(1, Ordering::SeqCst) + 1;
        self.widest.fetch_max(now, Ordering::SeqCst);
        let permit = self.gate.acquire().await;
        self.open.fetch_sub(1, Ordering::SeqCst);
        drop(permit);
        Ok(Box::pin(futures::stream::iter(vec![Ok(
            StreamChunk::Text("Handled.".into()),
        )])))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "gated"
    }
    fn model_id(&self) -> &str {
        "gated"
    }
}

struct Fixture {
    app: Router,
    gate: Arc<Semaphore>,
    open: Arc<AtomicU32>,
    widest: Arc<AtomicU32>,
    hitl_db: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

/// A runtime with one stored agent without code, a gated model, and a
/// `memory` source of ten requests.
async fn fixture() -> Fixture {
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
                "name": "Request clerk",
                "mission": "Answer each customer request."
            }),
            enabled: true,
            installed_at: "2026-10-02T00:00:00Z".into(),
            updated_at: "2026-10-02T00:00:00Z".into(),
        })
        .unwrap();
    let gate = Arc::new(Semaphore::new(0));
    let open = Arc::new(AtomicU32::new(0));
    let widest = Arc::new(AtomicU32::new(0));
    let mut backends: std::collections::HashMap<String, Arc<dyn CompletionModel>> =
        std::collections::HashMap::new();
    backends.insert(
        "gated".into(),
        Arc::new(Gated {
            gate: gate.clone(),
            open: open.clone(),
            widest: widest.clone(),
        }),
    );
    let mut state = test_app_state_no_llm();
    let chat = ChatSessionManagerHandle::spawn(
        &dir.path().join("chat.db"),
        Some(Arc::new(LlmRouter::with_backends(backends, "gated"))),
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
                let mut source = MemorySource::new(&spec.location, &["client", "request"]);
                for i in 0..10 {
                    source = source.with_item(
                        &format!("row-{}", i + 2),
                        &[("client", &format!("client {i}")), ("request", "Refund")],
                    );
                }
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
    ));
    state.data_dir = dir.path().to_path_buf();
    state.task_repository = Some(repo);
    state.chat_manager = Some(chat);
    let app = batch_routes::<MockBackend>().with_state(state);
    Fixture {
        app,
        gate,
        open,
        widest,
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

fn launch_body(kind: &str, concurrency: u32) -> Value {
    json!({
        "agent": AGENT,
        "source": {"kind": kind, "location": "/data/requests.xlsx"},
        "concurrency": concurrency
    })
}

// GIVEN a stored agent without code, a model that holds each turn until let
//       through, and a source of ten requests
// WHEN a batch of concurrency 3 is launched over HTTP and the model is let
//      through once three turns are open
// THEN no more than three items ever run at once, the ten items complete, and
//      each item's task joins a completed row of `tasks` under that agent
#[tokio::test]
async fn test_ten_items_run_as_tasks_three_at_a_time() {
    // GIVEN
    let fx = fixture().await;

    // WHEN
    let (status, launched) = call(
        &fx.app,
        "POST",
        "/api/v1/batches",
        Some(launch_body("memory", 3)),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{launched}");
    let batch_id = launched["batchId"].as_str().unwrap().to_string();
    assert_eq!(launched["total"], 10);
    assert_eq!(launched["concurrency"], 3);
    let three_open = poll_until(WAIT, || fx.open.load(Ordering::SeqCst) == 3).await;
    assert!(three_open, "three items never ran at once");
    fx.gate.add_permits(100);

    // THEN
    let completed = poll_until_async(WAIT, || async {
        let (_, body) = call(&fx.app, "GET", &format!("/api/v1/batches/{batch_id}"), None).await;
        body["batch"]["state"] == "completed"
    })
    .await;
    let (_, detail) = call(&fx.app, "GET", &format!("/api/v1/batches/{batch_id}"), None).await;
    assert!(completed, "the batch never completed: {detail}");
    assert_eq!(fx.widest.load(Ordering::SeqCst), 3);
    assert_eq!(detail["batch"]["completed"], 10);
    assert_eq!(detail["batch"]["failed"], 0);
    let items = detail["items"].as_array().unwrap();
    assert_eq!(items.len(), 10);
    assert_eq!(items[0]["origin"], "row-2");
    assert_eq!(items[0]["output"], "Handled.");
    let conn = rusqlite::Connection::open(&fx.hitl_db).unwrap();
    let joined: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM batch_items i JOIN tasks t ON t.task_id = i.task_id \
             WHERE i.batch_id = ?1 AND t.status = 'completed' AND t.agent_name = ?2",
            [batch_id.as_str(), AGENT],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(joined, 10);
    let (_, list) = call(&fx.app, "GET", "/api/v1/batches", None).await;
    assert_eq!(list["batches"][0]["batchId"], batch_id.as_str());
}

// GIVEN the runtime above
// WHEN batches are launched on a source type no reader serves, on an unknown
//      agent, and an unknown batch is read
// THEN they are refused as such and nothing is recorded
#[tokio::test]
async fn test_refusals_of_the_batch_routes() {
    // GIVEN
    let fx = fixture().await;

    // WHEN
    let (kind_status, kind_body) = call(
        &fx.app,
        "POST",
        "/api/v1/batches",
        Some(launch_body("spreadsheet", 1)),
    )
    .await;
    let mut stranger = launch_body("memory", 1);
    stranger["agent"] = json!("nobody");
    let (agent_status, _) = call(&fx.app, "POST", "/api/v1/batches", Some(stranger)).await;
    let (missing_status, _) = call(&fx.app, "GET", "/api/v1/batches/missing", None).await;

    // THEN
    assert_eq!(kind_status, StatusCode::BAD_REQUEST);
    assert!(
        kind_body["error"]
            .as_str()
            .unwrap()
            .contains("unknown source type 'spreadsheet'"),
        "{kind_body}"
    );
    assert_eq!(agent_status, StatusCode::BAD_REQUEST);
    assert_eq!(missing_status, StatusCode::NOT_FOUND);
    let (_, list) = call(&fx.app, "GET", "/api/v1/batches", None).await;
    assert_eq!(list["batches"], json!([]));
}

// GIVEN a runtime without a batch engine
// WHEN the batches are listed
// THEN the route answers 503
#[tokio::test]
async fn test_no_batch_engine_answers_503() {
    // GIVEN
    let app = batch_routes::<MockBackend>().with_state(test_app_state_no_llm());

    // WHEN
    let (status, _) = call(&app, "GET", "/api/v1/batches", None).await;

    // THEN
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}
