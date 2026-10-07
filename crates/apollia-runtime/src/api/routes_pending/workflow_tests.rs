//! The question a step of a workflow stopped on, in the list of decisions
//! waiting for the user, and answered or declined from it.

use std::sync::Arc;
use std::time::Duration;

use apollia_core::StepBudgetConfig;
use apollia_tools::TaskRepository;
use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend};
use crate::test_support::poll_until_async;
use crate::workflows::{
    BoxFuture, QuestionAsked, UserReply, WorkflowCall, WorkflowDescriptor, WorkflowHandle,
    WorkflowHost,
};

const WAIT: Duration = Duration::from_secs(10);
const LIST: &str = "/api/v1/pending-decisions";
const DECIDE: &str = "/api/v1/pending-decisions/decide";
const PAY: &str = "invoices/pay-supplier";

/// One workflow, `invoices/pay-supplier`: its step `validate` asks which
/// amount to pay, with two choices, its step `pay` returns the answer.
struct Host;

impl WorkflowHost for Host {
    fn describe(&self, workflow_id: &str) -> Option<WorkflowDescriptor> {
        (workflow_id == PAY).then(|| WorkflowDescriptor {
            id: workflow_id.to_string(),
            version: "1.0.0".into(),
            description: "Have an amount validated, then pay it.".into(),
            steps: vec!["validate".into(), "pay".into()],
            input_schema: None,
        })
    }

    fn list(&self) -> Vec<WorkflowDescriptor> {
        self.describe(PAY).into_iter().collect()
    }

    fn run(&self, call: WorkflowCall) -> BoxFuture<'static, Result<Value, String>> {
        Box::pin(async move {
            let services = call.services;
            services.step_started("validate".into()).await?;
            let reply = services
                .ask_user(QuestionAsked {
                    question: "Which amount for Acme?".into(),
                    options: vec!["120.00".into(), "240.00".into()],
                    hint: Some("an amount in euros".into()),
                })
                .await;
            let amount = match reply {
                Ok(UserReply::Answer(amount)) => amount,
                Ok(UserReply::Declined(reason)) => {
                    let error = format!("QuestionDeclined: {reason}");
                    services
                        .step_ended("validate".into(), Some(error.clone()))
                        .await?;
                    return Err(error);
                }
                Err(error) => return Err(error),
            };
            services.step_ended("validate".into(), None).await?;
            services.step_started("pay".into()).await?;
            services.step_ended("pay".into(), None).await?;
            Ok(json!({"paid": amount}))
        })
    }
}

struct Fixture {
    _dir: tempfile::TempDir,
    state: AppState<MockBackend>,
    engine: WorkflowHandle,
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
    let mut state = app_state(dir.path().to_path_buf());
    state.workflows = Some(engine.clone());
    Fixture {
        _dir: dir,
        state,
        engine,
    }
}

async fn items(state: &AppState<MockBackend>) -> Vec<Value> {
    let (status, body) = call(state.clone(), "GET", LIST, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["items"].as_array().cloned().unwrap_or_default()
}

/// Launches the workflow and returns its run id with the item it waits on.
async fn paused(fx: &Fixture) -> (String, Value) {
    let run = fx.engine.launch(PAY, json!({})).await.unwrap();
    let listed = poll_until_async(WAIT, || async { !items(&fx.state).await.is_empty() }).await;
    assert!(listed, "the question of the workflow was never listed");
    (run.run_id, items(&fx.state).await.remove(0))
}

async fn ended(fx: &Fixture, run_id: &str) -> apollia_tools::WorkflowRunRow {
    let done = poll_until_async(WAIT, || async {
        let detail = fx.engine.detail(run_id).await.unwrap();
        detail.is_some_and(|d| d.run.status != "running")
    })
    .await;
    assert!(done, "the run never ended");
    fx.engine.detail(run_id).await.unwrap().unwrap().run
}

#[tokio::test]
async fn test_a_paused_workflow_is_listed_as_a_question_with_its_origin() {
    // GIVEN a workflow whose step `validate` asks the user an amount
    let fx = fixture().await;

    // WHEN it runs up to its question and the decisions waiting are listed
    let (run_id, item) = paused(&fx).await;

    // THEN the pause is a question, with its choices, a free answer besides
    //      them, and the run, the workflow and the step it comes from
    assert_eq!(item["kind"], "question");
    assert_eq!(item["summary"], "Which amount for Acme?");
    assert_eq!(item["answers"], json!(["answer", "refuse"]));
    assert_eq!(item["allowOther"], true);
    assert_eq!(
        item["questions"],
        json!([{
            "id": "answer",
            "question": "Which amount for Acme?",
            "type": "single_choice",
            "options": ["120.00", "240.00"],
            "hint": "an amount in euros"
        }])
    );
    assert_eq!(item["origin"]["runId"], run_id);
    assert_eq!(item["origin"]["workflowId"], PAY);
    assert_eq!(item["origin"]["step"], "validate");
    let request_id = item["origin"]["requestId"].as_str().unwrap();
    assert_eq!(item["id"], format!("workflow-question:{request_id}"));
    assert!(item["askedAt"].as_str().is_some_and(|t| !t.is_empty()));
}

#[tokio::test]
async fn test_an_answer_from_the_list_resumes_the_workflow_in_its_step() {
    // GIVEN a workflow paused on its question
    let fx = fixture().await;
    let (run_id, item) = paused(&fx).await;

    // WHEN the item is answered with free text, none of its choices
    let (status, body) = call(
        fx.state.clone(),
        "POST",
        DECIDE,
        Some(json!({"id": item["id"], "answer": "answer", "value": "180.50"})),
    )
    .await;

    // THEN the decision is applied, the run completes with the answer, and
    //      the list is empty again
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["resumed"], 1);
    assert_eq!(body["message"], "Answered; the workflow goes on.");
    let run = ended(&fx, &run_id).await;
    assert_eq!(run.status, "completed");
    assert_eq!(run.output, Some(json!({"paid": "180.50"})));
    assert!(items(&fx.state).await.is_empty());
}

#[tokio::test]
async fn test_an_answer_given_as_the_agent_questions_are_is_taken_too() {
    // GIVEN a workflow paused on its question
    let fx = fixture().await;
    let (run_id, item) = paused(&fx).await;

    // WHEN it is answered in the shape the questions of agents take
    let (status, body) = call(
        fx.state.clone(),
        "POST",
        DECIDE,
        Some(json!({
            "id": item["id"],
            "answer": "answer",
            "answers": [{"id": "answer", "value": "240.00"}]
        })),
    )
    .await;

    // THEN the run goes on with that choice
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        ended(&fx, &run_id).await.output,
        Some(json!({"paid": "240.00"}))
    );
}

#[tokio::test]
async fn test_a_refusal_from_the_list_reaches_the_workflow_with_its_reason() {
    // GIVEN a workflow paused on its question
    let fx = fixture().await;
    let (run_id, item) = paused(&fx).await;

    // WHEN the user declines to answer, saying why
    let (status, body) = call(
        fx.state.clone(),
        "POST",
        DECIDE,
        Some(json!({"id": item["id"], "answer": "refuse", "reason": "not this month"})),
    )
    .await;

    // THEN the workflow was told and failed in the step that asked
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["message"], "Declined; the workflow is told.");
    let run = ended(&fx, &run_id).await;
    assert_eq!(run.status, "failed");
    assert_eq!(
        run.error.as_deref(),
        Some("QuestionDeclined: not this month")
    );
    assert!(items(&fx.state).await.is_empty());
}

#[tokio::test]
async fn test_an_answer_the_question_does_not_take_leaves_the_workflow_paused() {
    // GIVEN a workflow paused on its question
    let fx = fixture().await;
    let (run_id, item) = paused(&fx).await;

    // WHEN it is answered `once`, then `answer` with no text, then an item
    //      nobody asked is answered
    let (once, once_body) = call(
        fx.state.clone(),
        "POST",
        DECIDE,
        Some(json!({"id": item["id"], "answer": "once"})),
    )
    .await;
    let (empty, empty_body) = call(
        fx.state.clone(),
        "POST",
        DECIDE,
        Some(json!({"id": item["id"], "answer": "answer", "value": "  "})),
    )
    .await;
    let (unknown, unknown_body) = call(
        fx.state.clone(),
        "POST",
        DECIDE,
        Some(json!({"id": "workflow-question:nope", "answer": "answer", "value": "1"})),
    )
    .await;

    // THEN each is refused with its code, and the run still waits, listed
    assert_eq!(once, StatusCode::UNPROCESSABLE_ENTITY, "{once_body}");
    assert_eq!(once_body["code"], "ANSWER_NOT_OFFERED");
    assert_eq!(empty, StatusCode::UNPROCESSABLE_ENTITY, "{empty_body}");
    assert_eq!(empty_body["code"], "INVALID_ANSWER");
    assert_eq!(unknown, StatusCode::NOT_FOUND, "{unknown_body}");
    assert_eq!(unknown_body["code"], "UNKNOWN_ITEM");
    assert_eq!(items(&fx.state).await.len(), 1);
    let detail = fx.engine.detail(&run_id).await.unwrap().unwrap();
    assert_eq!(detail.run.status, "running");
}
