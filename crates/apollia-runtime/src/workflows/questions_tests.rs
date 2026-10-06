//! A workflow that asks the user: the wait, the answer, the refusal, the
//! stop, and what a restart leaves of a run that waited.

use std::time::Duration;

use serde_json::json;
use tokio::sync::broadcast;

use super::*;
use crate::test_support::poll_until_async;

const WAIT: Duration = Duration::from_secs(10);
const PAY: &str = "invoices/pay-supplier";

/// One workflow, `invoices/pay-supplier`: its step `validate` asks which
/// amount to pay, its step `pay` returns the answer. A refusal fails
/// `validate` with what the user said.
struct Host;

fn asked() -> QuestionAsked {
    QuestionAsked {
        question: "Which amount for Acme?".into(),
        options: vec!["120.00".into(), "240.00".into()],
        hint: Some("an amount in euros".into()),
    }
}

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
            let question = match call.inputs.get("question").and_then(Value::as_str) {
                Some(text) => QuestionAsked {
                    question: text.to_string(),
                    options: Vec::new(),
                    hint: None,
                },
                None => asked(),
            };
            let amount = match services.ask_user(question).await {
                Ok(UserReply::Answer(amount)) => amount,
                Ok(UserReply::Declined(reason)) => {
                    let error = format!("QuestionDeclined: {reason}");
                    services
                        .step_ended("validate".into(), Some(error.clone()))
                        .await?;
                    return Err(error);
                }
                Err(error) => {
                    services
                        .step_ended("validate".into(), Some(error.clone()))
                        .await?;
                    return Err(error);
                }
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
    repository: Arc<TaskRepository>,
    engine: WorkflowHandle,
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let repository = Arc::new(
        TaskRepository::open(&dir.path().join("hitl.db"))
            .await
            .unwrap(),
    );
    let (bus, _) = broadcast::channel(64);
    let engine = WorkflowHandle::spawn(repository.clone(), bus, StepBudgetConfig::default(), None);
    engine.set_host(Arc::new(Host)).await.unwrap();
    Fixture {
        _dir: dir,
        repository,
        engine,
    }
}

/// The one question waiting, once the run asked it.
async fn waiting(engine: &WorkflowHandle) -> WorkflowQuestion {
    let asked = poll_until_async(WAIT, || async {
        !engine.pending_questions().await.unwrap().is_empty()
    })
    .await;
    assert!(asked, "the run never asked its question");
    engine.pending_questions().await.unwrap().remove(0)
}

/// The run once it left `running`.
async fn ended(fx: &Fixture, run_id: &str) -> WorkflowRunDetail {
    left(&fx.engine, run_id, "running").await
}

/// The run once its status is no longer `status`.
async fn left(engine: &WorkflowHandle, run_id: &str, status: &str) -> WorkflowRunDetail {
    let moved = poll_until_async(WAIT, || async {
        let detail = engine.detail(run_id).await.unwrap();
        detail.is_some_and(|d| d.run.status != status)
    })
    .await;
    assert!(moved, "the run stayed {status}");
    engine.detail(run_id).await.unwrap().unwrap()
}

fn statuses(detail: &WorkflowRunDetail) -> Vec<&str> {
    detail.steps.iter().map(|s| s.status.as_str()).collect()
}

#[tokio::test]
async fn a_question_pauses_the_run_in_its_step_and_is_listed_with_its_origin() {
    // GIVEN a workflow whose first step asks the user
    let fx = fixture().await;

    // WHEN it is launched
    let run = fx.engine.launch(PAY, json!({})).await.unwrap();
    let question = waiting(&fx.engine).await;

    // THEN the question waits with its run, its workflow, its step and what
    //      was asked, and the run is still running inside that step
    assert_eq!(question.run_id, run.run_id);
    assert_eq!(question.workflow_id, PAY);
    assert_eq!(question.step.as_deref(), Some("validate"));
    assert_eq!(question.asked, asked());
    assert!(chrono::DateTime::parse_from_rfc3339(&question.asked_at).is_ok());
    let detail = fx.engine.detail(&run.run_id).await.unwrap().unwrap();
    assert_eq!(detail.run.status, "running");
    assert_eq!(statuses(&detail), vec!["running", "pending"]);
}

#[tokio::test]
async fn an_answer_resumes_the_run_in_the_step_that_asked() {
    // GIVEN a run waiting on its question
    let fx = fixture().await;
    let run = fx.engine.launch(PAY, json!({})).await.unwrap();
    let question = waiting(&fx.engine).await;

    // WHEN the user gives a free answer, none of the proposed choices
    let answered = fx
        .engine
        .answer_question(&question.request_id, UserReply::Answer("180.50".into()))
        .await
        .unwrap();

    // THEN the step that asked completes, the next one runs with the answer,
    //      and nothing waits any more
    assert_eq!(answered.request_id, question.request_id);
    let detail = ended(&fx, &run.run_id).await;
    assert_eq!(detail.run.status, "completed");
    assert_eq!(detail.run.output, Some(json!({"paid": "180.50"})));
    assert_eq!(statuses(&detail), vec!["completed", "completed"]);
    assert!(fx.engine.pending_questions().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_refusal_reaches_the_step_with_its_reason() {
    // GIVEN a run waiting on its question
    let fx = fixture().await;
    let run = fx.engine.launch(PAY, json!({})).await.unwrap();
    let question = waiting(&fx.engine).await;

    // WHEN the user declines to answer
    fx.engine
        .answer_question(
            &question.request_id,
            UserReply::Declined("not this month".into()),
        )
        .await
        .unwrap();

    // THEN the workflow was told: the step failed with the reason, the next
    //      step never started, and the question is gone
    let detail = ended(&fx, &run.run_id).await;
    assert_eq!(detail.run.status, "failed");
    assert_eq!(
        detail.run.error.as_deref(),
        Some("QuestionDeclined: not this month")
    );
    assert_eq!(statuses(&detail), vec!["failed", "not_started"]);
    assert!(fx.engine.pending_questions().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_question_answered_twice_or_unknown_is_refused() {
    // GIVEN a run whose question was answered
    let fx = fixture().await;
    let run = fx.engine.launch(PAY, json!({})).await.unwrap();
    let question = waiting(&fx.engine).await;
    fx.engine
        .answer_question(&question.request_id, UserReply::Answer("120.00".into()))
        .await
        .unwrap();
    ended(&fx, &run.run_id).await;

    // WHEN it is answered again, and a question nobody asked is answered
    let twice = fx
        .engine
        .answer_question(&question.request_id, UserReply::Answer("240.00".into()))
        .await
        .unwrap_err();
    let unknown = fx
        .engine
        .answer_question("nope", UserReply::Answer("1".into()))
        .await
        .unwrap_err();

    // THEN both are refused as questions that do not wait, and the run kept
    //      its first answer
    assert!(matches!(twice, WorkflowError::UnknownQuestion(_)));
    assert!(matches!(unknown, WorkflowError::UnknownQuestion(id) if id == "nope"));
    let detail = fx.engine.detail(&run.run_id).await.unwrap().unwrap();
    assert_eq!(detail.run.output, Some(json!({"paid": "120.00"})));
}

#[tokio::test]
async fn stopping_a_run_that_waits_withdraws_its_question() {
    // GIVEN a run waiting on its question
    let fx = fixture().await;
    let run = fx.engine.launch(PAY, json!({})).await.unwrap();
    let question = waiting(&fx.engine).await;

    // WHEN the run is stopped
    let stopped = fx.engine.cancel(&run.run_id).await.unwrap();

    // THEN it ended canceled, and its question can no longer be answered
    assert_eq!(stopped.status, "canceled");
    assert!(fx.engine.pending_questions().await.unwrap().is_empty());
    let late = fx
        .engine
        .answer_question(&question.request_id, UserReply::Answer("120.00".into()))
        .await
        .unwrap_err();
    assert!(matches!(late, WorkflowError::UnknownQuestion(_)));
}

#[tokio::test]
async fn an_empty_question_is_refused_and_nothing_waits() {
    // GIVEN a workflow that asks a question with no text
    let fx = fixture().await;

    // WHEN it runs
    let run = fx
        .engine
        .run_to_end(PAY, json!({"question": "   "}))
        .await
        .unwrap();

    // THEN the step failed on the refusal and no question was ever listed
    assert_eq!(run.status, "failed");
    assert_eq!(
        run.error.as_deref(),
        Some("a question to the user cannot be empty")
    );
    assert!(fx.engine.pending_questions().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_run_that_waited_when_the_runtime_was_killed_fails_with_its_reason_at_the_next_start() {
    // GIVEN a run waiting on its question in a runtime that is killed: its
    //       memory is gone, `hitl.db` still says the run is running
    let fx = fixture().await;
    let run = fx.engine.launch(PAY, json!({})).await.unwrap();
    waiting(&fx.engine).await;

    // WHEN a runtime starts on the same store
    let (bus, _) = broadcast::channel(64);
    let restarted = WorkflowHandle::spawn(
        fx.repository.clone(),
        bus,
        StepBudgetConfig::default(),
        None,
    );
    restarted.set_host(Arc::new(Host)).await.unwrap();
    let detail = left(&restarted, &run.run_id, "running").await;

    // THEN the run is failed with a reason a user can read, the step it
    //      waited in too, and the new runtime lists no question for it
    assert_eq!(detail.run.status, "failed");
    assert_eq!(
        detail.run.error.as_deref(),
        Some("the runtime stopped during the run")
    );
    assert_eq!(statuses(&detail), vec!["failed", "not_started"]);
    assert!(restarted.pending_questions().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_run_that_waits_when_the_engine_shuts_down_fails_saying_so() {
    // GIVEN a run waiting on its question
    let fx = fixture().await;
    let run = fx.engine.launch(PAY, json!({})).await.unwrap();
    waiting(&fx.engine).await;
    let Fixture {
        _dir,
        repository,
        engine,
    } = fx;

    // WHEN the engine shuts down, every handle dropped, the process alive
    drop(engine);
    let failed = poll_until_async(WAIT, || async {
        let row = repository.workflow_run(&run.run_id).await.unwrap();
        row.is_some_and(|r| r.status == "failed")
    })
    .await;

    // THEN the run was told why its question will never be answered
    assert!(failed, "the run kept waiting on an engine that is gone");
    let row = repository.workflow_run(&run.run_id).await.unwrap().unwrap();
    assert_eq!(
        row.error.as_deref(),
        Some("the runtime stopped before the question was answered")
    );
}
