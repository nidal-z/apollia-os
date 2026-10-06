//! A tool call of the chat reaches the journal as a line a reader can
//! understand, its target included, and its arguments and output are read on
//! demand from the entries the run captured.

use super::*;
use crate::audit_journal::{
    entry_detail, summarize, AuditJournalHandle, AuditJournalSubscriber, JournalEntry,
    JournalEntryKind, JournalNature, JournalOutcome,
};
use crate::governance::{GovernanceHandle, GovernanceState};
use crate::test_support::{poll_until, poll_until_async};
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, StreamChunk as LlmStreamChunk,
    ToolCall as LlmToolCall,
};
use std::sync::atomic::{AtomicU32, Ordering};

const TOOL: &str = "file_write";
const PATH: &str = "/home/u/letter.md";
const CONTENT: &str = "SECRET-CONTENT-42";
const OUTPUT: &str = "SECRET-OUTPUT-43";
const WAIT: Duration = Duration::from_secs(5);

/// Streams one call to [`TOOL`] writing [`CONTENT`] at [`PATH`], then text.
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
                name: TOOL.into(),
                arguments: serde_json::json!({ "path": PATH, "content": CONTENT }),
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

struct OutputInvoker;

#[async_trait::async_trait]
impl ToolInvoker for OutputInvoker {
    async fn invoke(
        &self,
        _tool_name: &str,
        _arguments: &serde_json::Value,
    ) -> Result<String, String> {
        Ok(OUTPUT.to_string())
    }
}

/// Runs one exchange where the model writes a file once, the operator
/// accepting the prompt, with the journal subscribed to the bus. Returns the
/// entries of the run.
async fn journal_of_one_write() -> Vec<JournalEntry> {
    let dir = tempfile::tempdir().expect("tempdir");
    let journal = AuditJournalHandle::open(&dir.path().join("audit_journal.db"))
        .await
        .expect("journal");
    let (event_bus, _events) = tokio::sync::broadcast::channel(128);
    AuditJournalSubscriber::spawn(journal.clone(), event_bus.subscribe());
    let model: Arc<dyn CompletionModel> = Arc::new(OneWriteThenText {
        iteration: AtomicU32::new(0),
    });
    let mut backends = std::collections::HashMap::new();
    backends.insert("default".to_string(), model);
    let router = Arc::new(LlmRouter::with_backends(backends, "default"));
    let tool_registry = ToolRegistryHandle::start();
    tool_registry
        .register(apollia_tools::tools::file_write::FileWrite::descriptor())
        .await
        .expect("register file_write");
    let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
        llm_router: router,
        tool_registry: tool_registry.clone(),
        tool_invoker: Arc::new(OutputInvoker),
        event_bus,
        user_memory: None,
        todo: None,
        plan: None,
    })
    .with_governance(Some(GovernanceHandle::detached(
        GovernanceState::default(),
        None,
    )));
    let budget = StepBudget::with_max(10);
    let approvals = PendingChatApprovals::new();
    let operator = tokio::spawn({
        let approvals = approvals.clone();
        async move {
            poll_until(WAIT, || {
                approvals.resolve("sess-1::msg-1::c0", ToolDecision::Accept)
            })
            .await;
        }
    });
    let run_id = RunId::new();
    agent
        .execute(
            "sess-1",
            "msg-1",
            &run_id,
            "write it",
            &[],
            "assistant",
            &[TOOL.to_string()],
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
    operator.await.expect("operator joins");
    tool_registry.shutdown().await;
    // The subscriber drains the bus on its own task: the end of the turn is
    // the last entry of the run.
    let run = run_id.as_str().to_owned();
    let drained = poll_until_async(WAIT, || {
        let journal = journal.clone();
        let run = run.clone();
        async move {
            journal.query_run(&run).await.iter().any(|e| {
                matches!(&e.kind, JournalEntryKind::Unknown { raw_kind } if raw_kind == "ChatResponseCompleted")
            })
        }
    })
    .await;
    assert!(drained, "the journal never saw the end of the turn");
    let entries = journal.query_run(&run).await;
    journal.shutdown().await;
    entries
}

// The chat loop puts a call on the bus with its run and its call identifier,
// and the journal writes the family and the target beside them.
#[tokio::test]
async fn test_a_chat_tool_call_is_journaled_with_its_target_and_no_content() {
    // GIVEN a conversation where the model writes a file once
    // WHEN the exchange has run and the journal has drained the bus
    let entries = journal_of_one_write().await;

    // THEN the run holds the start and the end of the call, joined by the
    // identifier the model gave it
    let started = entries
        .iter()
        .find(|e| e.kind == JournalEntryKind::ToolCallStarted)
        .expect("a tool_call_started entry");
    let completed = entries
        .iter()
        .find(|e| e.kind == JournalEntryKind::ToolCallCompleted)
        .expect("a tool_call_completed entry");
    for entry in [started, completed] {
        assert_eq!(entry.payload["tool_call_id"], "c0");
        assert_eq!(entry.payload["family"], "write_or_delete");
        assert_eq!(entry.payload["target"], PATH);
        // AND neither carries the content written nor what the tool answered
        let payload = entry.payload.to_string();
        assert!(!payload.contains(CONTENT), "{payload}");
        assert!(!payload.contains(OUTPUT), "{payload}");
    }

    // AND the summary of the end of the call says who, what, on what, and how
    // it ended, still without content
    let summary = summarize(completed);
    assert_eq!(
        summary.agent.as_deref(),
        Some(crate::chat::manager::APOLLIA_CHAT_AGENT_ID)
    );
    assert_eq!(summary.nature, JournalNature::Action);
    assert_eq!(summary.family.as_deref(), Some("write_or_delete"));
    assert_eq!(summary.tool.as_deref(), Some(TOOL));
    assert_eq!(summary.target.as_deref(), Some(PATH));
    assert_eq!(summary.outcome, Some(JournalOutcome::Success));
    let said = serde_json::to_string(&summary).expect("serialise");
    assert!(!said.contains(CONTENT) && !said.contains(OUTPUT), "{said}");
}

// "See more" on a call gives its arguments and its output, read from the
// entries the run captured for replay.
#[tokio::test]
async fn test_the_detail_of_a_chat_tool_call_joins_its_arguments_and_output() {
    // GIVEN the journal of a conversation where the model wrote a file once
    let entries = journal_of_one_write().await;
    let completed = entries
        .iter()
        .find(|e| e.kind == JournalEntryKind::ToolCallCompleted)
        .expect("a tool_call_completed entry");

    // WHEN the detail of the end of the call is read
    let detail = entry_detail(&entries, completed.seq).expect("the entry exists");

    // THEN it holds the full arguments and the full output of that call
    let call = detail.tool_call.expect("a tool call detail");
    assert_eq!(call.tool_call_id, "c0");
    assert_eq!(
        call.arguments,
        Some(serde_json::json!({ "path": PATH, "content": CONTENT }))
    );
    assert_eq!(call.output, Some(serde_json::json!(OUTPUT)));
    assert_eq!(call.status.as_deref(), Some("success"));
}
