use serde_json::json;

use super::*;

async fn repo(dir: &tempfile::TempDir) -> TaskRepository {
    TaskRepository::open(&dir.path().join("hitl.db"))
        .await
        .unwrap()
}

fn new_run(id: &str, workflow: &str) -> NewWorkflowRun {
    NewWorkflowRun {
        run_id: id.into(),
        workflow_id: workflow.into(),
        parent_run_id: None,
        depth: 1,
        inputs: json!({"folder": "/data/march"}),
        steps: vec!["extract".into(), "check".into(), "file".into()],
    }
}

fn statuses(steps: &[WorkflowStepRow]) -> Vec<&str> {
    steps.iter().map(|s| s.status.as_str()).collect()
}

// GIVEN a workflow declaring three steps
// WHEN a run of it is recorded
// THEN the run is running with its inputs and every step is pending, in declared order
#[tokio::test]
async fn create_workflow_run_records_the_run_and_its_pending_steps() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(&dir).await;

    let row = repo
        .create_workflow_run(new_run("r-1", "invoices/close-month"))
        .await
        .unwrap();

    assert_eq!(row.status, "running");
    assert_eq!(row.depth, 1);
    assert_eq!(row.inputs["folder"], "/data/march");
    assert!(row.ended_at.is_none());
    let steps = repo.workflow_steps("r-1").await.unwrap();
    let names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["extract", "check", "file"]);
    assert_eq!(statuses(&steps), vec!["pending"; 3]);
}

// GIVEN a recorded run
// WHEN its first step opens and completes with an output, then its second fails
// THEN each step carries its status, its timestamps, its output or its error
#[tokio::test]
async fn steps_move_through_running_to_their_end() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(&dir).await;
    repo.create_workflow_run(new_run("r-1", "invoices/close-month"))
        .await
        .unwrap();

    assert!(repo.start_workflow_step("r-1", "extract").await.unwrap());
    repo.finish_workflow_step(
        "r-1",
        "extract",
        WorkflowStepEnd {
            status: "completed",
            output: Some("3 invoices".into()),
            error: None,
        },
    )
    .await
    .unwrap();
    assert!(repo.start_workflow_step("r-1", "check").await.unwrap());
    repo.finish_workflow_step(
        "r-1",
        "check",
        WorkflowStepEnd {
            status: "failed",
            output: None,
            error: Some("ValueError: total".into()),
        },
    )
    .await
    .unwrap();

    let steps = repo.workflow_steps("r-1").await.unwrap();
    assert_eq!(statuses(&steps), vec!["completed", "failed", "pending"]);
    assert_eq!(steps[0].output.as_deref(), Some("3 invoices"));
    assert!(steps[0].started_at.is_some() && steps[0].ended_at.is_some());
    assert_eq!(steps[1].error.as_deref(), Some("ValueError: total"));
    assert!(steps[2].started_at.is_none());
}

// GIVEN a run whose second step is running
// WHEN the run is ended as canceled
// THEN the running step is canceled and the step never opened is not started
#[tokio::test]
async fn a_canceled_run_cancels_its_open_step_and_marks_the_rest_not_started() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(&dir).await;
    repo.create_workflow_run(new_run("r-1", "invoices/close-month"))
        .await
        .unwrap();
    repo.start_workflow_step("r-1", "extract").await.unwrap();
    repo.finish_workflow_step(
        "r-1",
        "extract",
        WorkflowStepEnd {
            status: "completed",
            output: None,
            error: None,
        },
    )
    .await
    .unwrap();
    repo.start_workflow_step("r-1", "check").await.unwrap();

    let run = repo
        .end_workflow_run("r-1", "canceled", None, Some("stopped".into()))
        .await
        .unwrap();

    assert_eq!(run.status, "canceled");
    assert!(run.ended_at.is_some());
    let steps = repo.workflow_steps("r-1").await.unwrap();
    assert_eq!(
        statuses(&steps),
        vec!["completed", "canceled", "not_started"]
    );
}

// GIVEN a run that returned a value
// WHEN it is ended as completed
// THEN its output is read back as JSON
#[tokio::test]
async fn a_completed_run_keeps_its_output() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(&dir).await;
    repo.create_workflow_run(new_run("r-1", "invoices/close-month"))
        .await
        .unwrap();

    repo.end_workflow_run("r-1", "completed", Some(json!({"total": 42})), None)
        .await
        .unwrap();

    let run = repo.workflow_run("r-1").await.unwrap().unwrap();
    assert_eq!(run.output, Some(json!({"total": 42})));
    assert_eq!(run.status, "completed");
}

// GIVEN three runs of two workflows
// WHEN the history of one workflow is read with a limit
// THEN only its runs come back, newest first, within the limit
#[tokio::test]
async fn workflow_runs_lists_one_workflow_newest_first() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(&dir).await;
    repo.create_workflow_run(new_run("r-1", "invoices/close-month"))
        .await
        .unwrap();
    repo.create_workflow_run(new_run("r-2", "invoices/other"))
        .await
        .unwrap();
    repo.create_workflow_run(new_run("r-3", "invoices/close-month"))
        .await
        .unwrap();

    let all = repo
        .workflow_runs("invoices/close-month", 10)
        .await
        .unwrap();
    let one = repo.workflow_runs("invoices/close-month", 1).await.unwrap();

    let ids: Vec<&str> = all.iter().map(|r| r.run_id.as_str()).collect();
    assert_eq!(ids, vec!["r-3", "r-1"]);
    assert_eq!(one.len(), 1);
}

// GIVEN a run left running by a process that stopped
// WHEN the interrupted runs are failed at start
// THEN the run is failed with its reason and its steps are closed
#[tokio::test]
async fn interrupted_runs_are_failed() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(&dir).await;
    repo.create_workflow_run(new_run("r-1", "invoices/close-month"))
        .await
        .unwrap();
    repo.start_workflow_step("r-1", "extract").await.unwrap();

    let failed = repo.fail_interrupted_workflow_runs().await.unwrap();

    assert_eq!(failed, 1);
    let run = repo.workflow_run("r-1").await.unwrap().unwrap();
    assert_eq!(run.status, "failed");
    assert!(run.error.unwrap().contains("stopped"));
    let steps = repo.workflow_steps("r-1").await.unwrap();
    assert_eq!(
        statuses(&steps),
        vec!["failed", "not_started", "not_started"]
    );
}
