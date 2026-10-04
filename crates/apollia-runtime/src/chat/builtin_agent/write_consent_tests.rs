//! Writing a file is consented to like any other tool of the family "write or
//! delete": one card, three answers, no second prompt of its own, and a
//! refusal hands its reason back to the agent.
//!
//! The loop runs over the real `file_write` executor and its real descriptor,
//! so what is asserted is what a conversation does with a write.

use super::*;
use crate::test_support::poll_until;
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, StreamChunk as LlmStreamChunk,
    ToolCall as LlmToolCall,
};
use apollia_tools::tools::file_write::FileWrite;
use std::sync::atomic::{AtomicU32, Ordering};

/// Upper bound on the wait for an approval prompt that should appear.
const PROMPT_WAIT: Duration = Duration::from_secs(5);

/// Streams one call to `file_write` (id `c0`) writing `note.txt`, then text.
struct OneWriteThenText {
    iteration: AtomicU32,
}

#[async_trait::async_trait]
impl CompletionModel for OneWriteThenText {
    async fn complete(
        &self,
        _req: CompletionRequest,
    ) -> Result<CompletionResponse, apollia_llm::types::LlmError> {
        Err(apollia_llm::types::LlmError::InferenceError(
            "streaming path only".into(),
        ))
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<
        std::pin::Pin<
            Box<
                dyn futures::Stream<Item = Result<LlmStreamChunk, apollia_llm::types::LlmError>>
                    + Send,
            >,
        >,
        apollia_llm::types::LlmError,
    > {
        let chunk = if self.iteration.fetch_add(1, Ordering::SeqCst) == 0 {
            LlmStreamChunk::ToolCall(LlmToolCall {
                id: "c0".into(),
                name: "file_write".into(),
                arguments: serde_json::json!({ "path": "note.txt", "content": "hello" }),
            })
        } else {
            LlmStreamChunk::Text("done".to_string())
        };
        Ok(Box::pin(futures::stream::iter(vec![Ok(chunk)])))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "mock-one-write"
    }
    fn model_id(&self) -> &str {
        "mock"
    }
}

/// What one scripted exchange produced.
struct WriteRun {
    response: ChatAgentResponse,
    /// Every event of the exchange that waits on a human, by tool name.
    prompts: Vec<String>,
    /// Content of `note.txt` after the exchange, when it exists.
    written: Option<String>,
}

/// Run one exchange where the model writes `note.txt` through the real
/// `file_write`, the operator answering `answer` to the card.
async fn run_write_exchange(answer: ToolDecision) -> WriteRun {
    let sandbox = tempfile::tempdir().expect("tempdir");
    let model: Arc<dyn CompletionModel> = Arc::new(OneWriteThenText {
        iteration: AtomicU32::new(0),
    });
    let mut backends = std::collections::HashMap::new();
    backends.insert("default".to_string(), model);
    let router = Arc::new(LlmRouter::with_backends(backends, "default"));
    let (event_bus, mut events) = tokio::sync::broadcast::channel(128);
    let tool_registry = ToolRegistryHandle::start();
    tool_registry
        .register(FileWrite::descriptor())
        .await
        .expect("register file_write");
    let writer = FileWrite::new(sandbox.path().to_path_buf()).expect("file_write");
    let dispatcher = apollia_tools::executor::ToolDispatcher::new(vec![Box::new(writer)]);
    let invoker: Arc<dyn ToolInvoker> = Arc::new(
        apollia_tools::dispatcher_invoker::DispatcherToolInvoker::new(Arc::new(dispatcher)),
    );
    let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
        llm_router: router,
        tool_registry: tool_registry.clone(),
        tool_invoker: invoker,
        event_bus,
        user_memory: None,
        todo: None,
        plan: None,
    });
    let budget = StepBudget::with_max(10);
    let approvals = PendingChatApprovals::new();
    let operator = tokio::spawn({
        let approvals = approvals.clone();
        async move {
            poll_until(PROMPT_WAIT, || {
                approvals.resolve("sess-1::msg-1::c0", answer.clone())
            })
            .await
        }
    });

    let response = agent
        .execute(
            "sess-1",
            "msg-1",
            &RunId::new(),
            "write a note",
            &[],
            "assistant",
            &["file_write".to_string()],
            &HashSet::new(),
            &approvals,
            &budget,
            None,
            DEFAULT_CONTEXT_WINDOW_SIZE,
            None,
            None,
            None,
            None,
            CancellationToken::new(),
        )
        .await
        .expect("the exchange completes");
    assert!(
        operator.await.expect("operator task joins"),
        "the write raised no card"
    );
    tool_registry.shutdown().await;

    let mut prompts = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let RuntimeEvent::ChatApprovalRequired { tool_name, .. } = event {
            prompts.push(tool_name);
        }
    }
    let written = std::fs::read_to_string(sandbox.path().join("note.txt")).ok();
    WriteRun {
        response,
        prompts,
        written,
    }
}

#[tokio::test]
async fn a_write_is_asked_on_the_card_once_and_runs_when_accepted() {
    // GIVEN a model that writes a file, nobody having consented to the family
    // WHEN the operator answers "once" on the card
    let run = run_write_exchange(ToolDecision::Accept).await;

    // THEN exactly one card was raised, for the write, and nothing else waited
    // on the operator before the file was written
    assert_eq!(run.prompts, vec!["file_write".to_string()]);
    assert_eq!(run.written.as_deref(), Some("hello"));
    assert_eq!(run.response.tool_calls.len(), 1);
    assert_eq!(run.response.tool_calls[0].status, ToolCallStatus::Executed);
}

#[tokio::test]
async fn a_refused_write_hands_its_reason_to_the_agent() {
    // GIVEN a model that writes a file
    // WHEN the operator refuses on the card with a reason
    let run = run_write_exchange(ToolDecision::Refuse {
        reason: Some("the file is shared, write a copy".into()),
    })
    .await;

    // THEN nothing was written, and the agent reads the reason in the result
    // of its call
    assert_eq!(run.prompts, vec!["file_write".to_string()]);
    assert!(run.written.is_none());
    assert_eq!(run.response.tool_calls[0].status, ToolCallStatus::Refused);
    assert_eq!(
        run.response.tool_calls[0].output.as_deref(),
        Some("Tool refused by the operator. Reason: the file is shared, write a copy")
    );
}
