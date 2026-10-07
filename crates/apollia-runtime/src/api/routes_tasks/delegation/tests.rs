use std::sync::Arc;

use apollia_core::{A2AConfig, TaskId};
use apollia_tools::{PauseRecord, TaskRepository};
use axum::http::StatusCode;

use crate::a2a::{A2AInvoker, SidechainLogger};
use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend};

fn with_logger(state: &mut AppState<MockBackend>, logger: SidechainLogger) {
    let invoker = A2AInvoker::new(
        state.registry_handle.clone(),
        state.router_handle.clone(),
        state.event_sender.clone(),
        A2AConfig::default(),
    )
    .with_sidechain_logger(logger);
    state.a2a_invoker = Some(Arc::new(invoker));
}

#[tokio::test]
async fn test_the_tree_names_the_stored_root_and_lists_its_delegations() {
    // GIVEN a finished task t-1 of the agent veille, stored, which delegated
    // twice
    let dir = tempfile::tempdir().unwrap();
    let repo = TaskRepository::open(&apollia_core::paths::DataFile::Hitl.path(dir.path()))
        .await
        .unwrap();
    repo.save_pause(PauseRecord {
        task_id: "t-1",
        step_id: None,
        prompt: "confirm?",
        context: &serde_json::json!({}),
        payload: None,
        agent_name: Some("veille"),
        skill_id: None,
    })
    .await
    .unwrap();
    let logger = SidechainLogger::new_in_memory().unwrap();
    let parent = TaskId::from("t-1".to_owned());
    logger
        .start(&parent, "recherche", &serde_json::json!({}))
        .await;
    logger
        .start(&parent, "synthese", &serde_json::json!({}))
        .await;
    let mut state = app_state(dir.path().to_path_buf());
    state.task_repository = Some(Arc::new(repo));
    with_logger(&mut state, logger);

    // WHEN its tree is read
    let (status, tree) = call(state, "GET", "/api/v1/tasks/t-1/delegation-tree", None).await;

    // THEN the root carries the stored agent name and start date, and one
    // child per delegation in order
    assert_eq!(status, StatusCode::OK, "{tree}");
    assert_eq!(tree["agent_name"], "veille");
    assert_eq!(tree["agent_id"], "t-1");
    assert!(tree["started_at"].is_string());
    let children: Vec<&str> = tree["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["agent_name"].as_str().unwrap())
        .collect();
    assert_eq!(children, ["recherche", "synthese"]);
    assert_eq!(tree["children"][0]["status"], "running");
}

#[tokio::test]
async fn test_a_task_without_delegation_is_a_root_named_after_itself() {
    // GIVEN a task the runtime has never seen and no delegation recorded
    let dir = tempfile::tempdir().unwrap();
    let mut state = app_state(dir.path().to_path_buf());
    with_logger(&mut state, SidechainLogger::new_in_memory().unwrap());

    // WHEN its tree is read
    let (status, tree) = call(state, "GET", "/api/v1/tasks/ghost/delegation-tree", None).await;

    // THEN the root stands alone, named after the task, as the embedded
    // command answered a 404 of the sidechains
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tree["agent_name"], "ghost");
    assert_eq!(tree["status"], "unknown");
    assert!(tree["children"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_a_daemon_without_sidechain_logging_refuses_with_503() {
    // GIVEN a daemon with no A2A invoker
    let dir = tempfile::tempdir().unwrap();
    let state = app_state(dir.path().to_path_buf());

    // WHEN a tree is read
    let (status, answer) = call(state, "GET", "/api/v1/tasks/t-1/delegation-tree", None).await;

    // THEN it says what is missing
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(answer["error"], "A2A invoker not initialized");
}
