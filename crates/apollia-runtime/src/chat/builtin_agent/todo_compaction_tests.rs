use super::*;
use crate::chat::todo_actor::spawn_todo_actor;
use apollia_core::todo::{TodoItem, TodoStatus};
use apollia_llm::types::{MessageContent, Role};
use rusqlite::Connection;

fn todo_handle() -> TodoHandle {
    spawn_todo_actor(Connection::open_in_memory().expect("open"), None).expect("spawn")
}

fn item(id: &str, content: &str, status: TodoStatus) -> TodoItem {
    TodoItem {
        id: id.into(),
        content: content.into(),
        status,
        depends_on: vec![],
    }
}

fn text_of(msg: &LlmChatMessage) -> String {
    match &msg.content {
        MessageContent::Text(t) => t.clone(),
        other => format!("{other:?}"),
    }
}

#[tokio::test]
async fn test_todo_injected_after_compaction() {
    // GIVEN a session with one in_progress item persisted
    let h = todo_handle();
    h.set_items(
        "s1",
        vec![item("t1", "Analyse the logs", TodoStatus::InProgress)],
    )
    .await
    .expect("seed");
    let mut messages = vec![LlmChatMessage::system("base")];

    // WHEN the post-compaction injection runs
    BuiltInChatAgent::inject_todo_after_compaction(&h, "s1", &mut messages).await;

    // THEN a user reminder carrying the item content and status is appended
    assert_eq!(messages.len(), 2);
    let last = messages.last().expect("message present");
    assert!(matches!(last.role, Role::User));
    let body = text_of(last);
    assert!(body.contains("Analyse the logs"));
    assert!(body.contains("in_progress"));
}

#[tokio::test]
async fn test_no_injection_when_todo_empty() {
    // GIVEN a session with no todo items
    let h = todo_handle();
    let mut messages = vec![LlmChatMessage::system("base")];

    // WHEN the injection runs
    BuiltInChatAgent::inject_todo_after_compaction(&h, "s1", &mut messages).await;

    // THEN no message is appended
    assert_eq!(messages.len(), 1);
}

fn plan_handle_for_compaction_test() -> crate::chat::plan_actor::PlanHandle {
    crate::chat::plan_actor::spawn_plan_actor(
        rusqlite::Connection::open_in_memory().expect("open"),
        None,
    )
    .expect("spawn")
}

#[tokio::test]
async fn test_plan_injected_after_compaction() {
    // GIVEN a session whose plan store holds a two-step plan
    let handle = plan_handle_for_compaction_test();
    let steps: Vec<apollia_core::plan::PlanStep> = vec![
        serde_json::from_value(serde_json::json!({
            "step_id": "s1", "description": "first", "depends_on": []
        }))
        .expect("step"),
        serde_json::from_value(serde_json::json!({
            "step_id": "s2", "description": "second", "depends_on": []
        }))
        .expect("step"),
    ];
    handle
        .propose("sess", steps, Some("do the thing".into()))
        .await
        .expect("propose");
    let mut messages = vec![LlmChatMessage::system("base")];

    // WHEN the post-compaction plan injection runs
    BuiltInChatAgent::inject_plan_after_compaction(&handle, "sess", &mut messages).await;

    // THEN a single user reminder lists every step with a status token
    assert_eq!(messages.len(), 2);
    let last = messages.last().expect("message present");
    assert!(matches!(last.role, Role::User));
    let body = text_of(last);
    assert!(body.contains("active plan"), "got: {body}");
    assert!(body.contains("s1") && body.contains("s2"), "got: {body}");
    assert!(body.contains("pending"), "got: {body}");
}

#[tokio::test]
async fn test_no_plan_injection_without_plan() {
    // GIVEN a plan store with no plan for the session
    let handle = plan_handle_for_compaction_test();
    let mut messages = vec![LlmChatMessage::system("base")];

    // WHEN the injection runs
    BuiltInChatAgent::inject_plan_after_compaction(&handle, "absent", &mut messages).await;

    // THEN no message is appended
    assert_eq!(messages.len(), 1);
}

#[tokio::test]
async fn test_multiple_items_all_present_in_reminder() {
    // GIVEN a session with one in_progress, two pending, one completed
    let h = todo_handle();
    h.set_items(
        "s1",
        vec![
            item("t1", "done thing", TodoStatus::Completed),
            item("t2", "current thing", TodoStatus::InProgress),
            item("t3", "next thing", TodoStatus::Pending),
            item("t4", "later thing", TodoStatus::Pending),
        ],
    )
    .await
    .expect("seed");
    let mut messages = vec![LlmChatMessage::system("base")];

    // WHEN the injection runs
    BuiltInChatAgent::inject_todo_after_compaction(&h, "s1", &mut messages).await;

    // THEN all four items appear in creation order
    let body = text_of(messages.last().expect("message present"));
    let p1 = body.find("done thing").expect("t1 present");
    let p2 = body.find("current thing").expect("t2 present");
    let p3 = body.find("next thing").expect("t3 present");
    let p4 = body.find("later thing").expect("t4 present");
    assert!(p1 < p2 && p2 < p3 && p3 < p4);
}

#[tokio::test]
async fn test_get_items_error_is_graceful() {
    // GIVEN a handle whose actor has stopped (channel closed)
    let h = todo_handle();
    h.shutdown().await;
    tokio::task::yield_now().await;
    let mut messages = vec![LlmChatMessage::system("base")];

    // WHEN the injection runs against the dead actor
    BuiltInChatAgent::inject_todo_after_compaction(&h, "s1", &mut messages).await;

    // THEN it degrades gracefully: no panic, no message appended
    assert_eq!(messages.len(), 1);
}
