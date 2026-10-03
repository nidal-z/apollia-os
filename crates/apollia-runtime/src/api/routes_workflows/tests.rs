//! The workflow routes over the workflow actor and a scripted host.

use std::sync::Arc;

use apollia_core::StepBudgetConfig;
use apollia_tools::TaskRepository;
use axum::http::StatusCode;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::*;
use crate::api::test_support::{app_state, call, MockBackend, StemLoader};
use crate::workflows::{BoxFuture, WorkflowCall, WorkflowDescriptor, WorkflowHost};

/// One workflow, `invoices/close-month`: it opens `extract`, then waits for
/// its token when `hold` is set, then opens `file`.
struct Host;

impl WorkflowHost for Host {
    fn describe(&self, workflow_id: &str) -> Option<WorkflowDescriptor> {
        (workflow_id == "invoices/close-month").then(|| WorkflowDescriptor {
            id: workflow_id.to_string(),
            version: "1.0.0".into(),
            description: "Close the month.".into(),
            steps: vec!["extract".into(), "file".into()],
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "folder": {"type": "string"},
                    "hold": {"type": "boolean"},
                    "closing_day": {"type": "string", "format": "date"},
                    "note": {"type": "string", "format": "long-text"},
                    "ledger": {"type": "string", "format": "file-path"}
                },
                "required": ["folder"],
                "additionalProperties": false,
                "x-order": ["folder", "hold", "closing_day", "note", "ledger"]
            })),
        })
    }

    fn list(&self) -> Vec<WorkflowDescriptor> {
        self.describe("invoices/close-month").into_iter().collect()
    }

    fn run(&self, call: WorkflowCall) -> BoxFuture<'static, Result<Value, String>> {
        Box::pin(async move {
            call.services.step_started("extract".into()).await?;
            if call.inputs["hold"] == json!(true) {
                wait(&call.cancel).await;
                return Err("stopped".into());
            }
            call.services.step_ended("extract".into(), None).await?;
            call.services.step_started("file".into()).await?;
            call.services.step_ended("file".into(), None).await?;
            Ok(json!({"filed": call.inputs["folder"]}))
        })
    }
}

async fn wait(token: &CancellationToken) {
    token.cancelled().await;
}

struct Fixture {
    _dir: tempfile::TempDir,
    state: AppState<MockBackend>,
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let repository = Arc::new(
        TaskRepository::open(&dir.path().join("hitl.db"))
            .await
            .unwrap(),
    );
    let (bus, _) = tokio::sync::broadcast::channel(64);
    let engine = WorkflowHandle::spawn(repository, bus, StepBudgetConfig::default(), None);
    engine.set_host(Arc::new(Host)).await.unwrap();
    let mut state = app_state(
        Arc::new(StemLoader { refusal: None }),
        dir.path().to_path_buf(),
    );
    state.workflows = Some(engine);
    Fixture { _dir: dir, state }
}

/// The run `id` as the route reads it, once it is no longer running.
async fn ended(state: &AppState<MockBackend>, id: &str) -> Value {
    loop {
        let (status, body) = call(
            state.clone(),
            "GET",
            &format!("/api/v1/workflow-runs/{id}"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        if body["run"]["status"] != "running" {
            return body;
        }
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn a_launch_runs_the_workflow_and_its_steps_read_back() {
    // GIVEN the engine with one workflow
    let fx = fixture().await;

    // WHEN it is launched through the route with valid inputs
    let (status, run) = call(
        fx.state.clone(),
        "POST",
        "/api/v1/workflows/invoices/close-month/runs",
        Some(json!({"inputs": {"folder": "/data/march"}})),
    )
    .await;

    // THEN the launch is accepted, and the run reads back completed with each
    //      step, its status and its timestamps
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(run["workflowId"], "invoices/close-month");
    let id = run["runId"].as_str().unwrap().to_string();
    let detail = ended(&fx.state, &id).await;
    assert_eq!(detail["run"]["status"], "completed");
    assert_eq!(detail["run"]["output"], json!({"filed": "/data/march"}));
    let steps = detail["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2);
    assert!(steps.iter().all(|s| s["status"] == "completed"));
    assert!(steps
        .iter()
        .all(|s| s["startedAt"].is_string() && s["endedAt"].is_string()));
}

#[tokio::test]
async fn an_invalid_input_is_refused_with_the_field_at_fault() {
    // GIVEN the engine with one workflow whose `folder` is a required string
    let fx = fixture().await;

    // WHEN it is launched without `folder`, then with an input it does not declare
    let (missing, missing_body) = call(
        fx.state.clone(),
        "POST",
        "/api/v1/workflows/invoices/close-month/runs",
        Some(json!({"inputs": {}})),
    )
    .await;
    let (extra, extra_body) = call(
        fx.state.clone(),
        "POST",
        "/api/v1/workflows/invoices/close-month/runs",
        Some(json!({"inputs": {"folder": "/a", "colour": "red"}})),
    )
    .await;

    // THEN both are refused with 400 and the field named
    assert_eq!(missing, StatusCode::BAD_REQUEST);
    assert_eq!(missing_body["field"], "folder");
    assert_eq!(extra, StatusCode::BAD_REQUEST);
    assert_eq!(extra_body["field"], "colour");
}

#[tokio::test]
async fn a_workflow_reads_with_the_kind_of_each_input_in_its_order() {
    // GIVEN the engine with one workflow taking a date, a long text and a file
    let fx = fixture().await;

    // WHEN it is read, and an unknown one
    let (status, body) = call(
        fx.state.clone(),
        "GET",
        "/api/v1/workflows/invoices/close-month",
        None,
    )
    .await;
    let (unknown, _) = call(
        fx.state.clone(),
        "GET",
        "/api/v1/workflows/invoices/other",
        None,
    )
    .await;

    // THEN each input carries its kind, in the declared order, and the
    //      unknown workflow is not found
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["workflowId"], "invoices/close-month");
    assert_eq!(body["steps"], json!(["extract", "file"]));
    let inputs: Vec<(&str, &str, bool)> = body["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            (
                i["name"].as_str().unwrap(),
                i["kind"].as_str().unwrap(),
                i["required"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        inputs,
        [
            ("folder", "text", true),
            ("hold", "boolean", false),
            ("closing_day", "date", false),
            ("note", "long_text", false),
            ("ledger", "file", false),
        ]
    );
    assert_eq!(unknown, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_malformed_date_and_an_absent_file_are_refused_before_the_run() {
    // GIVEN the engine with one workflow, and a folder with no `missing.pdf`
    let fx = fixture().await;
    let absent = fx._dir.path().join("missing.pdf");

    // WHEN it is launched with a day that does not exist, then with that file
    let (date, date_body) = call(
        fx.state.clone(),
        "POST",
        "/api/v1/workflows/invoices/close-month/runs",
        Some(json!({"inputs": {"folder": "/a", "closing_day": "2026-02-30"}})),
    )
    .await;
    let (file, file_body) = call(
        fx.state.clone(),
        "POST",
        "/api/v1/workflows/invoices/close-month/runs",
        Some(json!({"inputs": {"folder": "/a", "ledger": absent.to_str().unwrap()}})),
    )
    .await;

    // THEN both are refused with 400, the field named and the code typed, and
    //      no run was recorded
    assert_eq!(date, StatusCode::BAD_REQUEST);
    assert_eq!(date_body["field"], "closing_day");
    assert_eq!(date_body["code"], "malformed_date");
    assert!(date_body["error"]
        .as_str()
        .unwrap()
        .ends_with("[malformed_date]"));
    assert_eq!(file, StatusCode::BAD_REQUEST);
    assert_eq!(file_body["field"], "ledger");
    assert_eq!(file_body["code"], "missing_file");
    let (_, history) = call(
        fx.state.clone(),
        "GET",
        "/api/v1/workflows/invoices/close-month/runs",
        None,
    )
    .await;
    assert_eq!(history["runs"], json!([]));
}

#[tokio::test]
async fn an_unknown_workflow_and_an_unknown_run_are_not_found() {
    // GIVEN the engine with one workflow
    let fx = fixture().await;

    // WHEN an unknown workflow is launched and an unknown run read and stopped
    let (launch, _) = call(
        fx.state.clone(),
        "POST",
        "/api/v1/workflows/invoices/other/runs",
        Some(json!({"inputs": {}})),
    )
    .await;
    let (read, _) = call(fx.state.clone(), "GET", "/api/v1/workflow-runs/nope", None).await;
    let (stop, _) = call(
        fx.state.clone(),
        "POST",
        "/api/v1/workflow-runs/nope/cancel",
        None,
    )
    .await;

    // THEN each is 404
    assert_eq!(launch, StatusCode::NOT_FOUND);
    assert_eq!(read, StatusCode::NOT_FOUND);
    assert_eq!(stop, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_history_lists_the_runs_of_one_workflow_newest_first() {
    // GIVEN two finished runs of the workflow
    let fx = fixture().await;
    let mut ids = Vec::new();
    for folder in ["/a", "/b"] {
        let (_, run) = call(
            fx.state.clone(),
            "POST",
            "/api/v1/workflows/invoices/close-month/runs",
            Some(json!({"inputs": {"folder": folder}})),
        )
        .await;
        let id = run["runId"].as_str().unwrap().to_string();
        ended(&fx.state, &id).await;
        ids.push(id);
    }

    // WHEN its history is read with a limit, and without
    let (status, all) = call(
        fx.state.clone(),
        "GET",
        "/api/v1/workflows/invoices/close-month/runs",
        None,
    )
    .await;
    let (_, one) = call(
        fx.state.clone(),
        "GET",
        "/api/v1/workflows/invoices/close-month/runs?limit=1",
        None,
    )
    .await;

    // THEN the runs come newest first, within the limit
    assert_eq!(status, StatusCode::OK);
    let listed: Vec<&str> = all["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["runId"].as_str().unwrap())
        .collect();
    assert_eq!(listed, vec![ids[1].as_str(), ids[0].as_str()]);
    assert_eq!(one["runs"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn stopping_a_run_answers_once_it_ended_and_a_second_stop_conflicts() {
    // GIVEN a run held in its first step
    let fx = fixture().await;
    let (_, run) = call(
        fx.state.clone(),
        "POST",
        "/api/v1/workflows/invoices/close-month/runs",
        Some(json!({"inputs": {"folder": "/a", "hold": true}})),
    )
    .await;
    let id = run["runId"].as_str().unwrap().to_string();

    // WHEN it is stopped, then stopped again
    let uri = format!("/api/v1/workflow-runs/{id}/cancel");
    let (status, stopped) = call(fx.state.clone(), "POST", &uri, None).await;
    let (again, _) = call(fx.state.clone(), "POST", &uri, None).await;

    // THEN the first answers the run canceled, its steps closed, and the second conflicts
    assert_eq!(status, StatusCode::OK);
    assert_eq!(stopped["status"], "canceled");
    let detail = ended(&fx.state, &id).await;
    let statuses: Vec<&str> = detail["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["status"].as_str().unwrap())
        .collect();
    assert_eq!(statuses, vec!["canceled", "not_started"]);
    assert_eq!(again, StatusCode::CONFLICT);
}

#[tokio::test]
async fn without_an_engine_the_routes_answer_503() {
    // GIVEN a state with no workflow engine
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(
        Arc::new(StemLoader { refusal: None }),
        dir.path().to_path_buf(),
    );

    // WHEN a workflow is launched
    let (status, _) = call(
        state,
        "POST",
        "/api/v1/workflows/invoices/close-month/runs",
        Some(json!({"inputs": {}})),
    )
    .await;

    // THEN the answer is 503
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}
