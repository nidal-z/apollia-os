//! Images sent with a message: refused without vision, stored with the
//! message, handed to the model, replayed on the next turn, erased with the
//! conversation.

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollia_core::{LlmBackendConfig, LlmBackendRepository, LlmProvider, StepBudgetConfig};
use apollia_llm::message_image::MessageImage;
use apollia_llm::model_index::{
    IndexedModel, ModelCategory, ModelFormat, ModelIndex, ModelOrigin, INDEX_FILE_NAME,
};
use apollia_llm::types::{
    ChatMessage as LlmMessage, CompletionModel, CompletionRequest, CompletionResponse,
    FinishReason, LlmError, MessageContent, Role, StreamChunk, TokenUsage,
};
use apollia_llm::{Capabilities, LlmRouter, VisionEngine};
use apollia_mcp::session::LoadingMode;
use apollia_tools::ToolRegistryHandle;

use crate::api::routes_agents::StubAgentLoader;
use crate::chat::images::stored_images;
use crate::chat::manager::{ChatSessionManagerHandle, CreateSessionParams};
use crate::chat::repository::ChatSessionRepository;
use crate::chat::types::{ChatError, ChatMode, SessionStatus};
use crate::test_support::{poll_until, poll_until_async};

/// A backend that keeps the messages of every request streamed through it.
struct Recording {
    requests: Arc<Mutex<Vec<Vec<LlmMessage>>>>,
}

#[async_trait::async_trait]
impl CompletionModel for Recording {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            engine_timings: None,
            content: "seen".to_string(),
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
        if let Ok(mut requests) = self.requests.lock() {
            requests.push(req.messages);
        }
        Ok(Box::pin(futures::stream::once(async {
            Ok(StreamChunk::Text("seen".to_string()))
        })))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "recording"
    }
    fn model_id(&self) -> &str {
        "recording"
    }
}

pub(super) type Requests = Arc<Mutex<Vec<Vec<LlmMessage>>>>;

pub(super) fn spawn(dir: &tempfile::TempDir) -> (ChatSessionManagerHandle, Requests) {
    let requests: Requests = Arc::new(Mutex::new(Vec::new()));
    let mut backends: std::collections::HashMap<String, Arc<dyn CompletionModel>> =
        std::collections::HashMap::new();
    backends.insert(
        "recording".into(),
        Arc::new(Recording {
            requests: Arc::clone(&requests),
        }),
    );
    let (event_tx, _) = tokio::sync::broadcast::channel(128);
    let handle = ChatSessionManagerHandle::spawn(
        &dir.path().join("chat.db"),
        Some(Arc::new(LlmRouter::with_backends(backends, "recording"))),
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
    (handle, requests)
}

/// Make the default model of the data directory `dir` a local GGUF that
/// reads images through llama-server.
fn with_default_vision_model(dir: &tempfile::TempDir) {
    let models = crate::installed_models::models_dir(dir.path());
    std::fs::create_dir_all(&models).expect("models dir");
    let file = models.join("vision.gguf");
    ModelIndex {
        models: vec![IndexedModel {
            id: "vision".into(),
            format: ModelFormat::Gguf,
            path: file.clone(),
            parts: Vec::new(),
            annexes: Vec::new(),
            size_bytes: 0,
            origin: ModelOrigin::Import,
            sha256: None,
            origin_verified: false,
            category: ModelCategory::Language,
            parameters: None,
            modified_at: None,
            pinned_at: None,
            category_override: None,
            source_file_override: None,
            capabilities: Some(Capabilities::default().with_vision(VisionEngine::LlamaServer)),
        }],
        ..ModelIndex::default()
    }
    .save(&models.join(INDEX_FILE_NAME))
    .expect("index");
    let repo = LlmBackendRepository::open(&dir.path().join("system.db")).expect("system.db");
    repo.save(&LlmBackendConfig {
        name: "local-vision".into(),
        provider: LlmProvider::LlamaCpp,
        model: file.display().to_string(),
        config_json: serde_json::json!({}),
        is_default: true,
        enabled: true,
    })
    .expect("backend row");
}

pub(super) async fn free_chat(handle: &ChatSessionManagerHandle) -> String {
    handle
        .create_session(CreateSessionParams {
            mode: ChatMode::Libre,
            agent_name: None,
            system_prompt: None,
            tools: vec![],
            model: None,
        })
        .await
        .expect("session")
        .id
}

fn two_images() -> Vec<MessageImage> {
    vec![
        MessageImage::new("image/png", vec![1u8, 2, 3]),
        MessageImage::new("image/jpeg", vec![4u8, 5]),
    ]
}

pub(super) async fn wait_idle(handle: &ChatSessionManagerHandle, id: &str) -> bool {
    poll_until_async(Duration::from_secs(10), || async {
        handle
            .get_session(id.to_owned())
            .await
            .is_some_and(|d| d.session.status != SessionStatus::Processing)
    })
    .await
}

fn images_of(message: &LlmMessage) -> Option<&Vec<MessageImage>> {
    match &message.content {
        MessageContent::TextWithImages { images, .. } => Some(images),
        _ => None,
    }
}

#[tokio::test]
async fn test_a_message_with_two_images_is_stored_and_reaches_the_model() {
    // GIVEN a free chat whose default model reads images
    let dir = tempfile::tempdir().expect("tempdir");
    with_default_vision_model(&dir);
    let (handle, requests) = spawn(&dir);
    let id = free_chat(&handle).await;

    // WHEN a message with two images is sent
    let message_id = handle
        .send_message_with_images(id.clone(), "compare them".into(), two_images())
        .await
        .expect("accepted");

    // THEN the message read back from the database references both images,
    // whose files exist under the folder of the conversation
    let repo = ChatSessionRepository::open(&dir.path().join("chat.db")).expect("open");
    let row = repo
        .get_messages(&id, None)
        .expect("messages")
        .into_iter()
        .find(|m| m.id == message_id)
        .expect("the user message");
    let metadata: serde_json::Value =
        serde_json::from_str(row.metadata.as_deref().expect("metadata")).expect("json");
    let stored = stored_images(Some(&metadata));
    assert_eq!(stored.len(), 2);
    let folder = repo.session_images_dir(&id).expect("a folder");
    assert!(stored.iter().all(|s| folder.join(&s.file).is_file()));

    // AND the model receives the text followed by both images, in order
    let reached = poll_until(Duration::from_secs(10), || {
        requests.lock().map(|r| !r.is_empty()).unwrap_or(false)
    })
    .await;
    assert!(reached, "the turn never reached the model");
    let first = requests.lock().expect("lock")[0].clone();
    let last = first.last().expect("a user message");
    assert_eq!(last.role, Role::User);
    assert_eq!(images_of(last), Some(&two_images()));

    handle.shutdown().await;
}

#[tokio::test]
async fn test_a_follow_up_question_still_sees_the_earlier_images() {
    // GIVEN a free chat where a message with two images was answered
    let dir = tempfile::tempdir().expect("tempdir");
    with_default_vision_model(&dir);
    let (handle, requests) = spawn(&dir);
    let id = free_chat(&handle).await;
    handle
        .send_message_with_images(id.clone(), "compare them".into(), two_images())
        .await
        .expect("accepted");
    assert!(wait_idle(&handle, &id).await, "the first turn never ended");

    // WHEN a text-only follow-up is sent
    handle
        .send_message(id.clone(), "and the second one?".into())
        .await
        .expect("accepted");
    let reached = poll_until(Duration::from_secs(10), || {
        requests.lock().map(|r| r.len() >= 2).unwrap_or(false)
    })
    .await;
    assert!(reached, "the follow-up never reached the model");

    // THEN the replayed history carries the earlier images, and the new
    // message is text alone
    let second = requests.lock().expect("lock")[1].clone();
    let earlier = second
        .iter()
        .find(|m| m.role == Role::User && images_of(m).is_some())
        .expect("the earlier user message with its images");
    assert_eq!(images_of(earlier), Some(&two_images()));
    let last = second.last().expect("the follow-up");
    assert!(matches!(&last.content, MessageContent::Text(t) if t == "and the second one?"));

    handle.shutdown().await;
}

#[tokio::test]
async fn test_images_to_a_model_without_vision_are_refused_and_nothing_is_stored() {
    // GIVEN a free chat on a machine with no model that reads images
    let dir = tempfile::tempdir().expect("tempdir");
    let (handle, _requests) = spawn(&dir);
    let id = free_chat(&handle).await;

    // WHEN a message with an image is sent anyway
    let result = handle
        .send_message_with_images(id.clone(), "what is this?".into(), two_images())
        .await;

    // THEN the runtime refuses it with the typed error, and stores nothing
    assert!(matches!(result, Err(ChatError::ImagesNotSupported { .. })));
    let repo = ChatSessionRepository::open(&dir.path().join("chat.db")).expect("open");
    assert!(repo.get_messages(&id, None).expect("messages").is_empty());
    let folder = repo.session_images_dir(&id).expect("a folder");
    assert!(!folder.exists());

    handle.shutdown().await;
}

#[tokio::test]
async fn test_deleting_a_conversation_deletes_its_images() {
    // GIVEN a conversation holding a message with images
    let dir = tempfile::tempdir().expect("tempdir");
    with_default_vision_model(&dir);
    let (handle, _requests) = spawn(&dir);
    let id = free_chat(&handle).await;
    handle
        .send_message_with_images(id.clone(), "keep these".into(), two_images())
        .await
        .expect("accepted");
    assert!(wait_idle(&handle, &id).await, "the turn never ended");
    let folder = crate::chat::images::session_images_dir(dir.path(), &id);
    assert!(folder.is_dir());

    // WHEN the conversation is deleted
    handle.delete_session(id.clone()).await.expect("deleted");

    // THEN its images folder is gone
    assert!(!folder.exists());

    handle.shutdown().await;
}
