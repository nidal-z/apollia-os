//! The free chat routes its turns to the backend the session prefers.

use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use apollia_core::StepBudgetConfig;
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, StreamChunk,
    TokenUsage,
};
use apollia_llm::LlmRouter;
use apollia_mcp::session::LoadingMode;
use apollia_tools::ToolRegistryHandle;

use crate::chat::manager::{ChatSessionManagerHandle, CreateSessionParams};
use crate::chat::types::{ChatMode, SessionConfigUpdate};
use crate::test_support::poll_until;

/// A backend that counts the turns streamed through it.
struct Counting {
    name: &'static str,
    streams: Arc<AtomicU32>,
}

#[async_trait::async_trait]
impl CompletionModel for Counting {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            engine_timings: None,
            content: self.name.to_string(),
            tool_calls: vec![],
            usage: TokenUsage::default(),
            finish_reason: FinishReason::Stop,
            latency_ms: 1,
            ttft_ms: None,
        })
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
    {
        self.streams.fetch_add(1, Ordering::SeqCst);
        let name = self.name.to_string();
        Ok(Box::pin(futures::stream::once(async move {
            Ok(StreamChunk::Text(name))
        })))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        self.name
    }
    fn model_id(&self) -> &str {
        self.name
    }
}

fn spawn(dir: &tempfile::TempDir, router: Arc<LlmRouter>) -> ChatSessionManagerHandle {
    let (event_tx, _) = tokio::sync::broadcast::channel(128);
    ChatSessionManagerHandle::spawn(
        &dir.path().join("chat.db"),
        Some(router),
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
    .expect("spawn manager")
}

// GIVEN a router whose default is `default`, also holding `preferred`, and a
//       free chat session whose stored backend is `preferred`
// WHEN a message is sent
// THEN the turn streams through `preferred`, never through the default
#[tokio::test]
async fn test_free_chat_turn_routes_to_the_preferred_backend() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let default_streams = Arc::new(AtomicU32::new(0));
    let preferred_streams = Arc::new(AtomicU32::new(0));
    let mut backends: std::collections::HashMap<String, Arc<dyn CompletionModel>> =
        std::collections::HashMap::new();
    backends.insert(
        "default".into(),
        Arc::new(Counting {
            name: "default",
            streams: Arc::clone(&default_streams),
        }),
    );
    backends.insert(
        "preferred".into(),
        Arc::new(Counting {
            name: "preferred",
            streams: Arc::clone(&preferred_streams),
        }),
    );
    let handle = spawn(
        &dir,
        Arc::new(LlmRouter::with_backends(backends, "default")),
    );
    let info = handle
        .create_session(CreateSessionParams {
            mode: ChatMode::Libre,
            agent_name: None,
            system_prompt: None,
            tools: vec![],
            model: None,
        })
        .await
        .unwrap();
    handle
        .update_session(
            info.id.clone(),
            SessionConfigUpdate {
                llm_backend: Some(Some("preferred".into())),
                ..SessionConfigUpdate::default()
            },
        )
        .await
        .unwrap();

    // WHEN
    handle
        .send_message(info.id.clone(), "Bonjour".into())
        .await
        .unwrap();
    let streamed = poll_until(Duration::from_secs(10), || {
        preferred_streams.load(Ordering::SeqCst) + default_streams.load(Ordering::SeqCst) > 0
    })
    .await;

    // THEN
    assert!(streamed, "the turn never reached a backend");
    assert_eq!(preferred_streams.load(Ordering::SeqCst), 1);
    assert_eq!(default_streams.load(Ordering::SeqCst), 0);

    handle.shutdown().await;
}

/// A router whose default is `default`, also holding `chosen`, with the
/// stream counters of both.
fn two_backends() -> (Arc<LlmRouter>, Arc<AtomicU32>, Arc<AtomicU32>) {
    let default_streams = Arc::new(AtomicU32::new(0));
    let chosen_streams = Arc::new(AtomicU32::new(0));
    let mut backends: std::collections::HashMap<String, Arc<dyn CompletionModel>> =
        std::collections::HashMap::new();
    backends.insert(
        "default".into(),
        Arc::new(Counting {
            name: "default",
            streams: Arc::clone(&default_streams),
        }),
    );
    backends.insert(
        "chosen".into(),
        Arc::new(Counting {
            name: "chosen",
            streams: Arc::clone(&chosen_streams),
        }),
    );
    (
        Arc::new(LlmRouter::with_backends(backends, "default")),
        default_streams,
        chosen_streams,
    )
}

// GIVEN a router whose default is `default`, also holding `chosen`
// WHEN a free chat session is created with the model `chosen` and a message
//      is sent, then the manager restarts on the same store
// THEN the turn streams through `chosen`, and the session still names it
#[tokio::test]
async fn test_a_session_created_with_a_model_runs_on_it_and_keeps_it() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let (router, default_streams, chosen_streams) = two_backends();
    let handle = spawn(&dir, Arc::clone(&router));

    // WHEN
    let info = handle
        .create_session(CreateSessionParams {
            mode: ChatMode::Libre,
            agent_name: None,
            system_prompt: None,
            tools: vec![],
            model: Some(" chosen ".into()),
        })
        .await
        .unwrap();
    handle
        .send_message(info.id.clone(), "Bonjour".into())
        .await
        .unwrap();
    let streamed = poll_until(Duration::from_secs(10), || {
        chosen_streams.load(Ordering::SeqCst) + default_streams.load(Ordering::SeqCst) > 0
    })
    .await;
    let live = handle.session_model(info.id.clone()).await.unwrap();
    handle.shutdown().await;
    let reopened = spawn(&dir, router);
    let stored = reopened.session_model(info.id.clone()).await.unwrap();

    // THEN
    assert!(streamed, "the turn never reached a backend");
    assert_eq!(chosen_streams.load(Ordering::SeqCst), 1);
    assert_eq!(default_streams.load(Ordering::SeqCst), 0);
    assert_eq!(live.as_deref(), Some("chosen"));
    assert_eq!(stored.as_deref(), Some("chosen"));

    reopened.shutdown().await;
}
