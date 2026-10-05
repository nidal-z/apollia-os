//! The trash as the actor serves it: a trashed conversation leaves the list
//! and is not resumed; restored, it is back with its messages and takes a
//! message again; erasing stays for good.

use std::sync::Arc;

use apollia_core::{RuntimeEvent, StepBudgetConfig};
use apollia_mcp::session::LoadingMode;
use apollia_tools::ToolRegistryHandle;

use crate::api::routes_agents::StubAgentLoader;
use crate::chat::manager::{ChatSessionManagerHandle, CreateSessionParams};
use crate::chat::repository::{AppendMessageParams, ChatSessionRepository};
use crate::chat::types::{ChatError, ChatMode, ChatRole};

fn spawn(
    dir: &tempfile::TempDir,
) -> (
    ChatSessionManagerHandle,
    tokio::sync::broadcast::Receiver<RuntimeEvent>,
) {
    let (event_tx, event_rx) = tokio::sync::broadcast::channel(128);
    let handle = ChatSessionManagerHandle::spawn(
        &dir.path().join("chat.db"),
        Some(Arc::new(apollia_llm::LlmRouter::empty())),
        ToolRegistryHandle::start(),
        Arc::new(StubAgentLoader),
        None,
        event_tx.clone(),
        StepBudgetConfig::default(),
        None,
        crate::registry::AgentRegistry::spawn(event_tx),
        None,
        None,
        None,
        LoadingMode::Eager,
        20,
        None,
        false,
    )
    .expect("spawn manager");
    (handle, event_rx)
}

async fn new_session(handle: &ChatSessionManagerHandle) -> String {
    handle
        .create_session(CreateSessionParams {
            mode: ChatMode::Libre,
            agent_name: None,
            system_prompt: None,
            tools: vec![],
            model: None,
        })
        .await
        .expect("session created")
        .id
}

/// Write two messages of `id` straight to the database, the way a past
/// exchange left them.
fn seed_messages(dir: &tempfile::TempDir, id: &str) {
    let repo = ChatSessionRepository::open(&dir.path().join("chat.db")).expect("open");
    for (n, role) in [ChatRole::User, ChatRole::Assistant].iter().enumerate() {
        let message_id = format!("{id}-m{n}");
        repo.append_message(&AppendMessageParams {
            id: &message_id,
            session_id: id,
            role,
            content: "hello",
            tool_calls_json: None,
            tool_name: None,
            created_at: "2026-10-05T10:00:00Z",
            metadata: None,
        })
        .expect("message appended");
    }
}

async fn listed(handle: &ChatSessionManagerHandle) -> Vec<String> {
    handle
        .list_sessions(None)
        .await
        .into_iter()
        .map(|s| s.id)
        .collect()
}

#[tokio::test]
async fn test_a_trashed_session_leaves_the_list_and_closes_its_stream() {
    // GIVEN a runtime holding two sessions
    let dir = tempfile::tempdir().expect("tempdir");
    let (handle, mut events) = spawn(&dir);
    let kept = new_session(&handle).await;
    let removed = new_session(&handle).await;

    // WHEN one is sent to the trash
    handle
        .trash_session(removed.clone())
        .await
        .expect("trashed");

    // THEN the list holds the other one only, the trash holds this one, and
    // its closing was announced on the bus
    assert_eq!(listed(&handle).await, [kept]);
    let trash = handle.list_trashed_sessions().await.expect("trash");
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].id, removed);
    let mut announced = false;
    while let Ok(event) = events.try_recv() {
        if matches!(&event, RuntimeEvent::ChatSessionClosed { session_id } if *session_id == removed)
        {
            announced = true;
        }
    }
    assert!(announced, "the closing of the trashed session is announced");

    handle.shutdown().await;
}

#[tokio::test]
async fn test_a_trashed_session_is_not_resumed() {
    // GIVEN a session in the trash
    let dir = tempfile::tempdir().expect("tempdir");
    let (handle, _events) = spawn(&dir);
    let id = new_session(&handle).await;
    handle.trash_session(id.clone()).await.expect("trashed");

    // WHEN it is resumed
    let resumed = handle.resume_session(id).await;

    // THEN the runtime answers that it is not found
    assert!(matches!(resumed, Err(ChatError::SessionNotFound(_))));

    handle.shutdown().await;
}

#[tokio::test]
async fn test_restoring_puts_the_session_back_with_its_messages() {
    // GIVEN a session holding two messages, sent to the trash
    let dir = tempfile::tempdir().expect("tempdir");
    let (handle, _events) = spawn(&dir);
    let id = new_session(&handle).await;
    seed_messages(&dir, &id);
    handle.trash_session(id.clone()).await.expect("trashed");

    // WHEN it is restored
    let restored = handle.restore_session(id.clone()).await.expect("restored");

    // THEN it is back in the list with its two messages, and the trash is
    // empty
    assert_eq!(restored.message_count, 2);
    assert_eq!(listed(&handle).await, std::slice::from_ref(&id));
    assert!(handle
        .list_trashed_sessions()
        .await
        .expect("trash")
        .is_empty());
    let detail = handle.get_session(id).await.expect("session read");
    assert_eq!(detail.session.history.len(), 2);

    handle.shutdown().await;
}

#[tokio::test]
async fn test_restoring_a_session_that_is_not_in_the_trash_is_refused() {
    // GIVEN a session in the list
    let dir = tempfile::tempdir().expect("tempdir");
    let (handle, _events) = spawn(&dir);
    let id = new_session(&handle).await;

    // WHEN it is restored, and when an unknown session is trashed
    let restored = handle.restore_session(id).await;
    let trashed = handle.trash_session("ghost".into()).await;

    // THEN both are refused as not found
    assert!(matches!(restored, Err(ChatError::SessionNotFound(_))));
    assert!(matches!(trashed, Err(ChatError::SessionNotFound(_))));

    handle.shutdown().await;
}

#[tokio::test]
async fn test_a_trashed_session_stays_in_the_trash_across_a_restart() {
    // GIVEN a session sent to the trash, then the runtime stopped
    let dir = tempfile::tempdir().expect("tempdir");
    let (handle, _events) = spawn(&dir);
    let id = new_session(&handle).await;
    handle.trash_session(id.clone()).await.expect("trashed");
    handle.shutdown().await;

    // WHEN a runtime starts again on the same data
    let (handle, _events) = spawn(&dir);

    // THEN the list is empty and the trash still holds the session
    assert!(listed(&handle).await.is_empty());
    let trash = handle.list_trashed_sessions().await.expect("trash");
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].id, id);

    handle.shutdown().await;
}

#[tokio::test]
async fn test_erasing_a_trashed_session_empties_the_trash() {
    // GIVEN a session in the trash
    let dir = tempfile::tempdir().expect("tempdir");
    let (handle, _events) = spawn(&dir);
    let id = new_session(&handle).await;
    handle.trash_session(id.clone()).await.expect("trashed");

    // WHEN it is erased
    handle.delete_session(id.clone()).await.expect("erased");

    // THEN neither the trash nor the runtime holds it
    assert!(handle
        .list_trashed_sessions()
        .await
        .expect("trash")
        .is_empty());
    assert!(handle.get_session(id).await.is_none());

    handle.shutdown().await;
}
