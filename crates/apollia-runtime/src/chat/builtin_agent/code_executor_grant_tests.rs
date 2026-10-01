//! An "always accept" on a code executor covers the current call only.
//!
//! `bash_executor` and `python_executor` take an arbitrary-code payload, so a
//! grant by name would let every later call of the same exchange run unseen.
//! These tests drive the chat loop with a scripted model that asks for the same
//! tool twice in one exchange and count the approval prompts it raises.

use super::*;
use crate::chat::types::AlwaysAcceptScope;
use crate::test_support::poll_until;
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, StreamChunk as LlmStreamChunk,
    ToolCall as LlmToolCall,
};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Upper bound on the wait for an approval prompt that should appear. Generous
/// so a loaded machine never turns a live prompt into a false negative.
const PROMPT_WAIT: Duration = Duration::from_secs(5);

/// Streams one call to `tool` on each of the first two iterations (ids `c0`,
/// `c1`), then final text.
struct TwoCallsThenText {
    tool: &'static str,
    iteration: AtomicU32,
}

#[async_trait::async_trait]
impl CompletionModel for TwoCallsThenText {
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
        let current = self.iteration.fetch_add(1, Ordering::SeqCst);
        let chunk = if current < 2 {
            LlmStreamChunk::ToolCall(LlmToolCall {
                id: format!("c{current}"),
                name: self.tool.into(),
                arguments: serde_json::json!({"command": format!("echo {current}")}),
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
        "mock-two-calls"
    }
    fn model_id(&self) -> &str {
        "mock"
    }
}

struct OkInvoker;

#[async_trait::async_trait]
impl ToolInvoker for OkInvoker {
    async fn invoke(
        &self,
        _tool_name: &str,
        _arguments: &serde_json::Value,
    ) -> Result<String, String> {
        Ok(r#"{"exit_code": 0, "stdout": "ok"}"#.to_string())
    }
}

/// What one scripted exchange produced.
struct ExchangeRun {
    response: ChatAgentResponse,
    prompts: Vec<String>,
    second_resolved: bool,
}

/// Run one exchange where the model calls `tool` twice. The first call is
/// answered "always accept"; the second call is answered "accept" if, and only
/// if, the loop raises a prompt for it within [`PROMPT_WAIT`].
async fn run_two_call_exchange(tool: &'static str) -> ExchangeRun {
    let model: Arc<dyn CompletionModel> = Arc::new(TwoCallsThenText {
        tool,
        iteration: AtomicU32::new(0),
    });
    let mut backends = std::collections::HashMap::new();
    backends.insert("default".to_string(), model);
    let router = Arc::new(LlmRouter::with_backends(backends, "default"));
    let (event_bus, mut events) = tokio::sync::broadcast::channel(128);
    let tool_registry = ToolRegistryHandle::start();
    let agent = BuiltInChatAgent::new(BuiltInChatAgentDeps {
        llm_router: router,
        tool_registry: tool_registry.clone(),
        tool_invoker: Arc::new(OkInvoker),
        event_bus,
        user_memory: None,
        a2a_invoker: None,
        todo: None,
        plan: None,
    });
    let budget = StepBudget::with_max(10);
    let approvals = PendingChatApprovals::new();
    let second_resolved = Arc::new(AtomicBool::new(false));
    let operator = tokio::spawn({
        let approvals = approvals.clone();
        let second_resolved = second_resolved.clone();
        async move {
            poll_until(PROMPT_WAIT, || {
                approvals.resolve(
                    "sess-1::msg-1::c0",
                    ToolDecision::AlwaysAccept {
                        scope: AlwaysAcceptScope::safe_default(),
                    },
                )
            })
            .await;
            let resolved = poll_until(PROMPT_WAIT, || {
                approvals.resolve("sess-1::msg-1::c1", ToolDecision::Accept)
            })
            .await;
            second_resolved.store(resolved, Ordering::SeqCst);
        }
    });

    let response = agent
        .execute(
            "sess-1",
            "msg-1",
            &RunId::new(),
            "run it twice",
            &[],
            "assistant",
            &[tool.to_string()],
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
    operator.await.expect("operator task joins");
    tool_registry.shutdown().await;

    let mut prompts = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let RuntimeEvent::ChatApprovalRequired { tool_call_id, .. } = event {
            prompts.push(tool_call_id);
        }
    }
    ExchangeRun {
        response,
        prompts,
        second_resolved: second_resolved.load(Ordering::SeqCst),
    }
}

#[tokio::test]
async fn always_on_bash_executor_prompts_again_for_the_next_call() {
    // GIVEN an exchange where the model calls bash_executor on two successive
    // iterations, and the operator answers "always" to the first prompt
    // WHEN the exchange runs
    let run = run_two_call_exchange("bash_executor").await;

    // THEN both calls ran, and the second one went through its own prompt
    assert_eq!(run.prompts, vec!["c0".to_string(), "c1".to_string()]);
    assert!(run.second_resolved, "the second call raised no prompt");
    assert_eq!(run.response.tool_calls.len(), 2);
    assert!(run
        .response
        .tool_calls
        .iter()
        .all(|c| c.status == ToolCallStatus::Executed));
    // AND the loop reports no name grant for the code executor
    assert!(run.response.newly_authorized.is_empty());
}

#[tokio::test]
async fn always_on_python_executor_is_not_granted_by_name() {
    // GIVEN the same exchange shape with python_executor
    // WHEN the operator answers "always" to the first prompt
    let run = run_two_call_exchange("python_executor").await;

    // THEN the second call is prompted and no name grant leaves the loop
    assert_eq!(run.prompts, vec!["c0".to_string(), "c1".to_string()]);
    assert!(run.response.newly_authorized.is_empty());
}

#[tokio::test]
async fn always_on_an_ordinary_tool_still_covers_the_exchange() {
    // GIVEN an exchange where the model calls file_read twice, and the
    // operator answers "always" to the first prompt
    // WHEN the exchange runs
    let run = run_two_call_exchange("file_read").await;

    // THEN only the first call prompted, the second ran under the grant
    assert_eq!(run.prompts, vec!["c0".to_string()]);
    assert!(!run.second_resolved);
    assert_eq!(run.response.tool_calls.len(), 2);
    assert!(run
        .response
        .tool_calls
        .iter()
        .all(|c| c.status == ToolCallStatus::Executed));
    // AND the grant is reported for persistence, as before
    assert_eq!(run.response.newly_authorized, vec!["file_read".to_string()]);
}

#[test]
fn correction_turn_carry_never_holds_a_code_executor() {
    // GIVEN a carried set seeded with an ordinary grant, and a turn that
    // reports grants for both code executors and for an ordinary tool
    let mut carried: HashSet<String> = HashSet::from(["web_search".to_string()]);
    let newly = vec![
        "bash_executor".to_string(),
        "file_read".to_string(),
        "python_executor".to_string(),
    ];

    // WHEN the grants are carried into the next correction turn
    super::react_loop::carry_grants(&mut carried, &newly);

    // THEN the ordinary grants are carried and no code executor is
    let expected: HashSet<String> =
        HashSet::from(["web_search".to_string(), "file_read".to_string()]);
    assert_eq!(carried, expected);
    assert!(!carried
        .iter()
        .any(|name| apollia_permissions::is_code_executor(name)));
}
