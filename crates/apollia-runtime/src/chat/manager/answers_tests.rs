//! The actions on one message as the actor serves them: an answer continued
//! by prefill where the engine continues it and refused elsewhere, a message
//! removed with the tool messages of its turn, an answer rewritten in place.

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollia_core::StepBudgetConfig;
use apollia_llm::types::{
    ChatMessage as LlmMessage, CompletionModel, CompletionRequest, CompletionResponse,
    FinishReason, LlmError, MessageContent, Role, StreamChunk, TokenUsage,
};
use apollia_llm::LlmRouter;
use apollia_mcp::session::LoadingMode;
use apollia_tools::ToolRegistryHandle;

use crate::chat::manager::{ChatSessionManagerHandle, Continuation, ContinuationRefusal};
use crate::chat::repository::{AppendMessageParams, ChatSessionRepository};
use crate::chat::types::{ChatError, ChatMode, ChatRole};
use crate::test_support::poll_until_async;

type Requests = Arc<Mutex<Vec<Vec<LlmMessage>>>>;

/// A backend that keeps the messages of every request streamed through it and
/// answers `reply`; `continues` is what it says of a trailing assistant
/// message.
struct Recording {
    continues: bool,
    reply: &'static str,
    requests: Requests,
}

#[async_trait::async_trait]
impl CompletionModel for Recording {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            engine_timings: None,
            content: self.reply.to_string(),
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
        self.requests
            .lock()
            .expect("requests")
            .push(req.messages.clone());
        let reply = self.reply.to_string();
        Ok(Box::pin(futures::stream::once(async move {
            Ok(StreamChunk::Text(reply))
        })))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn continues_answer(&self) -> bool {
        self.continues
    }
    fn backend_name(&self) -> &str {
        "default"
    }
    fn model_id(&self) -> &str {
        "recording"
    }
}

const SESSION: &str = "s-1";

/// A runtime on `dir` whose only backend is a [`Recording`], holding the
/// session `s-1` resumed with `messages`, and a summary of it.
async fn runtime(
    dir: &tempfile::TempDir,
    continues: bool,
    reply: &'static str,
    messages: &[(&str, ChatRole, &str)],
) -> (ChatSessionManagerHandle, Requests) {
    let db = dir.path().join("chat.db");
    let repo = ChatSessionRepository::open(&db).expect("open");
    repo.create_session(
        SESSION,
        &ChatMode::Libre,
        None,
        "",
        &[],
        "2026-10-06T10:00:00Z",
        None,
    )
    .expect("session");
    for (id, role, content) in messages {
        repo.append_message(&AppendMessageParams {
            id,
            session_id: SESSION,
            role,
            content,
            tool_calls_json: None,
            tool_name: None,
            created_at: "2026-10-06T10:00:01Z",
            metadata: None,
        })
        .expect("message");
    }
    repo.update_summary(SESSION, "a summary of the first turns")
        .expect("summary");
    drop(repo);

    let requests: Requests = Arc::new(Mutex::new(Vec::new()));
    let mut backends: std::collections::HashMap<String, Arc<dyn CompletionModel>> =
        std::collections::HashMap::new();
    backends.insert(
        "default".into(),
        Arc::new(Recording {
            continues,
            reply,
            requests: Arc::clone(&requests),
        }),
    );
    let (event_tx, _) = tokio::sync::broadcast::channel(128);
    let handle = ChatSessionManagerHandle::spawn(
        &db,
        Some(Arc::new(LlmRouter::with_backends(backends, "default"))),
        ToolRegistryHandle::start(),
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
    handle
        .resume_session(SESSION.to_string())
        .await
        .expect("resumed");
    (handle, requests)
}

/// The messages of `s-1` as `(id, text)`, in order.
async fn thread(handle: &ChatSessionManagerHandle) -> Vec<(String, String)> {
    handle
        .get_session(SESSION.to_string())
        .await
        .expect("session")
        .session
        .history
        .into_iter()
        .map(|m| (m.id, m.content))
        .collect()
}

fn text(message: &LlmMessage) -> &str {
    match &message.content {
        MessageContent::Text(text) => text,
        MessageContent::TextWithImages { text, .. } => text,
        _ => "",
    }
}

fn stored_summary(dir: &tempfile::TempDir) -> Option<String> {
    ChatSessionRepository::open(&dir.path().join("chat.db"))
        .expect("open")
        .get_summary(SESSION)
        .expect("summary")
}

const COUNTING: &[(&str, ChatRole, &str)] = &[
    ("u-1", ChatRole::User, "Count from one to five."),
    ("a-1", ChatRole::Assistant, "one, two,"),
];

#[tokio::test]
async fn test_a_continued_answer_is_sent_as_the_prefill_and_replaced_in_place() {
    // GIVEN a conversation whose last answer was cut short, on a model whose
    // engine continues an answer and returns it followed by the rest
    let dir = tempfile::tempdir().expect("tempdir");
    let (handle, requests) = runtime(&dir, true, "one, two, three, four, five", COUNTING).await;

    // WHEN the answer is continued
    let support = handle
        .continuation(SESSION.to_string())
        .await
        .expect("continuation read");
    handle
        .continue_answer(SESSION.to_string(), "a-1".to_string())
        .await
        .expect("continued");
    let landed = poll_until_async(Duration::from_secs(10), || async {
        thread(&handle).await.last().map(|m| m.1.clone())
            == Some("one, two, three, four, five".to_string())
    })
    .await;

    // THEN the request ends on the answer as an assistant message, right after
    // the one user message of the turn, with no message asking to go on; the
    // answer keeps its id and its place, and reads the continued text
    assert_eq!(
        support,
        Continuation {
            available: true,
            reason: None
        }
    );
    assert!(landed, "{:?}", thread(&handle).await);
    let sent = requests.lock().expect("requests").clone();
    assert_eq!(sent.len(), 1);
    let tail: Vec<(&Role, &str)> = sent[0]
        .iter()
        .filter(|m| m.role != Role::System)
        .map(|m| (&m.role, text(m)))
        .collect();
    assert_eq!(
        tail,
        [
            (&Role::User, "Count from one to five."),
            (&Role::Assistant, "one, two,"),
        ]
    );
    assert_eq!(
        thread(&handle).await,
        [
            ("u-1".to_string(), "Count from one to five.".to_string()),
            ("a-1".to_string(), "one, two, three, four, five".to_string()),
        ]
    );

    handle.shutdown().await;
}

#[tokio::test]
async fn test_an_engine_that_returns_only_the_rest_is_joined_to_the_answer() {
    // GIVEN the same conversation, on an engine that continues the answer and
    // returns only what follows it
    let dir = tempfile::tempdir().expect("tempdir");
    let cut = [
        ("u-1", ChatRole::User, "Count from one to five."),
        ("a-1", ChatRole::Assistant, "one, two, "),
    ];
    let (handle, _requests) = runtime(&dir, true, "three, four, five", &cut).await;

    // WHEN the answer is continued
    handle
        .continue_answer(SESSION.to_string(), "a-1".to_string())
        .await
        .expect("continued");
    let landed = poll_until_async(Duration::from_secs(10), || async {
        thread(&handle).await.last().map(|m| m.1.clone())
            == Some("one, two, three, four, five".to_string())
    })
    .await;

    // THEN the answer keeps what it said and gains the rest
    assert!(landed, "{:?}", thread(&handle).await);

    handle.shutdown().await;
}

#[tokio::test]
async fn test_a_model_that_does_not_continue_refuses_and_sends_nothing() {
    // GIVEN the same conversation on a model whose engine does not continue
    // an answer
    let dir = tempfile::tempdir().expect("tempdir");
    let (handle, requests) = runtime(&dir, false, "anything", COUNTING).await;

    // WHEN the continuation is read, then asked for anyway
    let support = handle
        .continuation(SESSION.to_string())
        .await
        .expect("continuation read");
    let refused = handle
        .continue_answer(SESSION.to_string(), "a-1".to_string())
        .await;

    // THEN it is reported unavailable with its reason, the request is refused
    // with the same reason, no request reached the model and the thread is as
    // it was: nothing was sent in its place
    assert_eq!(
        support,
        Continuation {
            available: false,
            reason: Some(ContinuationRefusal::Engine)
        }
    );
    assert!(
        matches!(&refused, Err(ChatError::ContinuationRefused(reason)) if reason == "engine"),
        "{refused:?}"
    );
    assert!(requests.lock().expect("requests").is_empty());
    assert_eq!(thread(&handle).await.len(), 2);
    assert_eq!(thread(&handle).await[1].1, "one, two,");

    handle.shutdown().await;
}

const TWO_TURNS: &[(&str, ChatRole, &str)] = &[
    ("u-1", ChatRole::User, "Where do pears grow?"),
    ("t-1", ChatRole::Tool, "orchard.txt"),
    ("a-1", ChatRole::Assistant, "Pears grow in orchards."),
    ("u-2", ChatRole::User, "And cherries?"),
    ("t-2", ChatRole::Tool, "cherries.txt"),
    ("a-2", ChatRole::Assistant, "Cherries too."),
];

#[tokio::test]
async fn test_only_the_last_answer_is_continued() {
    // GIVEN a conversation of two turns on a model that continues an answer
    let dir = tempfile::tempdir().expect("tempdir");
    let (handle, requests) = runtime(&dir, true, "more", TWO_TURNS).await;

    // WHEN the first answer, a user message and an unknown message are continued
    let earlier = handle
        .continue_answer(SESSION.to_string(), "a-1".to_string())
        .await;
    let user = handle
        .continue_answer(SESSION.to_string(), "u-2".to_string())
        .await;
    let ghost = handle
        .continue_answer(SESSION.to_string(), "ghost".to_string())
        .await;

    // THEN each is refused for what it is and nothing reached the model
    assert!(
        matches!(earlier, Err(ChatError::MessageRefused(_))),
        "{earlier:?}"
    );
    assert!(
        matches!(user, Err(ChatError::MessageRefused(_))),
        "{user:?}"
    );
    assert!(
        matches!(ghost, Err(ChatError::MessageNotFound(_))),
        "{ghost:?}"
    );
    assert!(requests.lock().expect("requests").is_empty());

    handle.shutdown().await;
}

#[tokio::test]
async fn test_deleting_an_answer_takes_the_tool_messages_of_its_turn_and_the_summary() {
    // GIVEN a conversation of two turns, each with a tool message, summarized
    let dir = tempfile::tempdir().expect("tempdir");
    let (handle, _requests) = runtime(&dir, true, "more", TWO_TURNS).await;
    assert!(stored_summary(&dir).is_some());

    // WHEN the first answer is deleted
    let deleted = handle
        .delete_message(SESSION.to_string(), "a-1".to_string())
        .await
        .expect("deleted");

    // THEN it is gone with the tool message of its turn, the other turn is
    // whole, the stored history agrees, and the summary is forgotten
    assert_eq!(deleted, 2);
    let left: Vec<String> = thread(&handle).await.into_iter().map(|m| m.0).collect();
    assert_eq!(left, ["u-1", "u-2", "t-2", "a-2"]);
    let stored: Vec<String> = ChatSessionRepository::open(&dir.path().join("chat.db"))
        .expect("open")
        .get_messages(SESSION, None)
        .expect("messages")
        .into_iter()
        .map(|m| m.id)
        .collect();
    assert_eq!(stored, left);
    assert_eq!(stored_summary(&dir), None);

    handle.shutdown().await;
}

#[tokio::test]
async fn test_deleting_a_user_message_removes_it_alone() {
    // GIVEN the same conversation
    let dir = tempfile::tempdir().expect("tempdir");
    let (handle, _requests) = runtime(&dir, true, "more", TWO_TURNS).await;

    // WHEN a user message is deleted, then a message the session does not hold
    let deleted = handle
        .delete_message(SESSION.to_string(), "u-2".to_string())
        .await
        .expect("deleted");
    let ghost = handle
        .delete_message(SESSION.to_string(), "ghost".to_string())
        .await;

    // THEN only that message is gone, and the unknown one is reported as such
    assert_eq!(deleted, 1);
    let left: Vec<String> = thread(&handle).await.into_iter().map(|m| m.0).collect();
    assert_eq!(left, ["u-1", "t-1", "a-1", "t-2", "a-2"]);
    assert!(matches!(ghost, Err(ChatError::MessageNotFound(id)) if id == "ghost"));

    handle.shutdown().await;
}

#[tokio::test]
async fn test_an_edited_answer_is_marked_and_read_by_the_next_turn() {
    // GIVEN a conversation of one turn, summarized
    let dir = tempfile::tempdir().expect("tempdir");
    let (handle, requests) = runtime(&dir, true, "noted", COUNTING).await;

    // WHEN its answer is rewritten, then a new message is sent
    handle
        .edit_answer(
            SESSION.to_string(),
            "a-1".to_string(),
            "uno, dos,".to_string(),
        )
        .await
        .expect("edited");
    let edited = handle
        .get_session(SESSION.to_string())
        .await
        .expect("session")
        .session
        .history[1]
        .clone();
    handle
        .send_message(SESSION.to_string(), "Go on.".into())
        .await
        .expect("sent");
    let asked = poll_until_async(Duration::from_secs(10), || async {
        !requests.lock().expect("requests").is_empty()
    })
    .await;

    // THEN the answer reads the new text under the same id, marked edited,
    // the summary written from the old text is forgotten, and the request of
    // the next turn carries the new text and not the old one
    assert_eq!(edited.id, "a-1");
    assert_eq!(edited.content, "uno, dos,");
    assert_eq!(
        edited.metadata.as_ref().and_then(|m| m.get("edited")),
        Some(&serde_json::Value::Bool(true))
    );
    assert_eq!(stored_summary(&dir), None);
    assert!(asked, "the next turn never reached the model");
    let sent = requests.lock().expect("requests")[0].clone();
    let answers: Vec<&str> = sent
        .iter()
        .filter(|m| m.role == Role::Assistant)
        .map(text)
        .collect();
    assert_eq!(answers, ["uno, dos,"]);

    handle.shutdown().await;
}

#[tokio::test]
async fn test_a_user_message_or_an_empty_text_is_not_rewritten_in_place() {
    // GIVEN a conversation of one turn
    let dir = tempfile::tempdir().expect("tempdir");
    let (handle, _requests) = runtime(&dir, true, "noted", COUNTING).await;

    // WHEN a user message is rewritten in place, and an answer emptied
    let user = handle
        .edit_answer(SESSION.to_string(), "u-1".to_string(), "Count.".to_string())
        .await;
    let empty = handle
        .edit_answer(SESSION.to_string(), "a-1".to_string(), "  ".to_string())
        .await;

    // THEN both are refused and the thread is as it was
    assert!(
        matches!(user, Err(ChatError::MessageRefused(_))),
        "{user:?}"
    );
    assert!(
        matches!(empty, Err(ChatError::MessageRefused(_))),
        "{empty:?}"
    );
    assert_eq!(thread(&handle).await[0].1, "Count from one to five.");
    assert_eq!(thread(&handle).await[1].1, "one, two,");

    handle.shutdown().await;
}
