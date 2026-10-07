use std::sync::Arc;

use apollia_tools::{PauseRecord, TaskRepository};
use axum::http::StatusCode;

use crate::api::test_support::{app_state, call};

async fn store_with(dir: &std::path::Path, task_id: &str) -> Arc<TaskRepository> {
    let repo = TaskRepository::open(&apollia_core::paths::DataFile::Hitl.path(dir))
        .await
        .unwrap();
    repo.save_pause(PauseRecord {
        task_id,
        step_id: None,
        prompt: "confirm?",
        context: &serde_json::json!({}),
        payload: None,
        agent_name: Some("veille"),
        skill_id: None,
    })
    .await
    .unwrap();
    Arc::new(repo)
}

#[tokio::test]
async fn test_a_stored_task_is_deleted_once_then_reported_gone() {
    // GIVEN a daemon whose task store holds the task t-1
    let dir = tempfile::tempdir().unwrap();
    let repo = store_with(dir.path(), "t-1").await;
    let mut state = app_state(dir.path().to_path_buf());
    state.task_repository = Some(repo.clone());

    // WHEN its record is deleted twice
    let (first_status, first) =
        call(state.clone(), "DELETE", "/api/v1/tasks/t-1/record", None).await;
    let (second_status, second) = call(state, "DELETE", "/api/v1/tasks/t-1/record", None).await;

    // THEN the first removes it, the second finds nothing, as the embedded
    // command answered
    assert_eq!(first_status, StatusCode::OK, "{first}");
    assert_eq!(first["deleted"], true);
    assert_eq!(second_status, StatusCode::OK);
    assert_eq!(second["deleted"], false);
    assert!(repo.get_task_status("t-1").await.unwrap().is_none());
}

#[tokio::test]
async fn test_a_daemon_without_a_task_store_refuses_with_503() {
    // GIVEN a daemon with no task store
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(dir.path().to_path_buf());

    // WHEN a record is deleted
    let (status, answer) = call(state, "DELETE", "/api/v1/tasks/t-1/record", None).await;

    // THEN it refuses with the message of the embedded command
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(answer["error"], "task persistence is not available");
}
