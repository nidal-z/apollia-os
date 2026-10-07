//! The conversation that creates an agent offers the draft tool, and no
//! other conversation does.

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollia_core::StepBudgetConfig;
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, StreamChunk,
    TokenUsage, ToolCall,
};
use apollia_llm::LlmRouter;
use apollia_mcp::session::LoadingMode;
use apollia_tools::tools::{file_read, file_write};
use apollia_tools::ToolRegistryHandle;
use serde_json::json;

use crate::chat::agent_draft_tool::{AgentDraft, AGENT_DRAFT_TOOL_NAME};
use crate::chat::manager::{ChatSessionManagerHandle, CreateSessionParams};
use crate::chat::types::{ChatMode, ToolCallStatus};
use crate::test_support::poll_until;

/// What one turn was offered: the names of its tools and its messages.
#[derive(Clone)]
struct Offered {
    tools: Vec<String>,
    messages: String,
}

/// Proposes an agent on its first turn, then answers; keeps what each turn
/// was offered.
struct Drafting {
    seen: Arc<Mutex<Vec<Offered>>>,
}

#[async_trait::async_trait]
impl CompletionModel for Drafting {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            engine_timings: None,
            content: "Done.".into(),
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
        let turn = {
            let mut seen = self.seen.lock().unwrap();
            seen.push(Offered {
                tools: req.tools.iter().map(|t| t.name.clone()).collect(),
                messages: format!("{:?}", req.messages),
            });
            seen.len()
        };
        let offered = req.tools.iter().any(|t| t.name == AGENT_DRAFT_TOOL_NAME);
        let chunk = if turn == 1 && offered {
            StreamChunk::ToolCall(ToolCall {
                id: "c1".into(),
                name: AGENT_DRAFT_TOOL_NAME.into(),
                arguments: json!({
                    "name": "Veille",
                    "mission": "Watch the press.",
                    "access": ["file_read", "file_write", "mail_send"],
                    "sources": ["profile"],
                }),
            })
        } else {
            StreamChunk::Text("Done.".into())
        };
        Ok(Box::pin(futures::stream::iter(vec![Ok(chunk)])))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "drafting"
    }
    fn model_id(&self) -> &str {
        "drafting"
    }
}

/// A manager on a registry holding `file_read` and `file_write`, whose model
/// records what it is offered.
async fn spawn(dir: &tempfile::TempDir) -> (ChatSessionManagerHandle, Arc<Mutex<Vec<Offered>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut backends: std::collections::HashMap<String, Arc<dyn CompletionModel>> =
        std::collections::HashMap::new();
    backends.insert(
        "drafting".into(),
        Arc::new(Drafting {
            seen: Arc::clone(&seen),
        }),
    );
    let registry = ToolRegistryHandle::start();
    registry
        .register(file_read::FileRead::descriptor())
        .await
        .unwrap();
    registry
        .register(file_write::FileWrite::descriptor())
        .await
        .unwrap();
    let (event_tx, _) = tokio::sync::broadcast::channel(128);
    let handle = ChatSessionManagerHandle::spawn(
        &dir.path().join("chat.db"),
        Some(Arc::new(LlmRouter::with_backends(backends, "drafting"))),
        registry,
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
    (handle, seen)
}

async fn create(handle: &ChatSessionManagerHandle, tools: Vec<String>) -> String {
    handle
        .create_session(CreateSessionParams {
            mode: ChatMode::Libre,
            agent_name: None,
            system_prompt: None,
            tools,
            model: None,
        })
        .await
        .unwrap()
        .id
}

// GIVEN a free chat created with the draft tool in its list
// WHEN a message is sent and the model proposes an agent
// THEN the turn is offered the draft tool alone, under the instructions that
//      list what the machine holds, and the answer carries the draft: the
//      reading access checked, the writing one unchecked, the missing one out
#[tokio::test]
async fn test_the_creation_conversation_offers_the_draft_tool_and_keeps_the_draft() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let (handle, seen) = spawn(&dir).await;
    let id = create(&handle, vec![AGENT_DRAFT_TOOL_NAME.to_owned()]).await;

    // WHEN
    handle
        .send_message(id.clone(), "I want an agent that watches the press".into())
        .await
        .unwrap();
    let answered = poll_until(Duration::from_secs(10), || seen.lock().unwrap().len() >= 2).await;
    assert!(answered, "the turn never came back to the model");
    let mut calls = Vec::new();
    for _ in 0..200 {
        let detail = handle.get_session(id.clone()).await.unwrap();
        calls = detail
            .session
            .history
            .iter()
            .filter_map(|m| m.tool_calls.clone())
            .flatten()
            .collect();
        if !calls.is_empty() {
            break;
        }
        tokio::task::yield_now().await;
    }

    // THEN
    let first = seen.lock().unwrap()[0].clone();
    assert_eq!(first.tools, [AGENT_DRAFT_TOOL_NAME]);
    assert!(first.messages.contains("Accesses on this machine"));
    assert!(first.messages.contains("`file_write` (sensitive)"));
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].tool_name, AGENT_DRAFT_TOOL_NAME);
    assert_eq!(calls[0].status, ToolCallStatus::Executed);
    let output: serde_json::Value =
        serde_json::from_str(calls[0].output.as_deref().unwrap()).unwrap();
    let draft: AgentDraft = serde_json::from_value(output["draft"].clone()).unwrap();
    let access: Vec<(&str, bool)> = draft
        .access
        .iter()
        .map(|a| (a.id.as_str(), a.checked))
        .collect();
    assert_eq!(access, [("file_read", true), ("file_write", false)]);
    assert_eq!(draft.unavailable, ["mail_send"]);
    assert_eq!(draft.sources, ["profile"]);

    handle.shutdown().await;
}

// GIVEN a free chat created without a tool list, then one created with the
//       tools of the registry
// WHEN a message is sent in each
// THEN neither turn is offered the draft tool
#[tokio::test]
async fn test_an_ordinary_conversation_is_not_offered_the_draft_tool() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let (handle, seen) = spawn(&dir).await;
    let plain = create(&handle, Vec::new()).await;
    let listed = create(&handle, vec!["file_read".to_owned()]).await;

    // WHEN
    handle.send_message(plain, "Hello".into()).await.unwrap();
    handle.send_message(listed, "Hello".into()).await.unwrap();
    let answered = poll_until(Duration::from_secs(10), || seen.lock().unwrap().len() >= 2).await;

    // THEN
    assert!(answered, "the turns never reached the model");
    for offered in seen.lock().unwrap().iter() {
        assert!(
            !offered.tools.iter().any(|t| t == AGENT_DRAFT_TOOL_NAME),
            "{:?}",
            offered.tools
        );
        assert!(!offered.messages.contains("Accesses on this machine"));
    }

    handle.shutdown().await;
}
