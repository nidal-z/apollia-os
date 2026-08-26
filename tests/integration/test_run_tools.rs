//! End-to-end tests - the ReAct loop through `ToolCallHelper`.
//!
//! Checks the behaviour of the ReAct loop from the public API of `apollia-llm`:
//! - immediate stop on `FinishReason::Stop`;
//! - one tool call, then the final answer;
//! - the `max_iterations` guard;
//! - the `StepBudget` guard, exhausted;
//! - tool errors absorbed as a textual result.
//!
//! No Python dependency - Rust mocks only.

use std::pin::Pin;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use futures::Stream;

use apollia_llm::{
    ChatMessage, CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError,
    StepBudgetView, StreamChunk, TokenUsage, ToolCall, ToolCallHelper, ToolInvoker, ToolSpec,
};

// ─────────────────────────────────────────────
// Mock CompletionModel: immediate Stop answer
// ─────────────────────────────────────────────

/// Mock LLM that always returns `FinishReason::Stop` with the given content.
struct MockStopModel {
    response: String,
    call_count: Arc<AtomicU32>,
}

impl MockStopModel {
    fn new(response: impl Into<String>) -> (Arc<Self>, Arc<AtomicU32>) {
        let count = Arc::new(AtomicU32::new(0));
        (
            Arc::new(Self {
                response: response.into(),
                call_count: count.clone(),
            }),
            count,
        )
    }
}

#[async_trait::async_trait]
impl CompletionModel for MockStopModel {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        Ok(CompletionResponse {
            engine_timings: None,
            content: self.response.clone(),
            tool_calls: vec![],
            usage: TokenUsage {
                prompt_tokens: 5,
                completion_tokens: 5,
                cost_usd: None,
                cache_read_input_tokens: 0,
                cache_write_input_tokens: 0,
            },
            finish_reason: FinishReason::Stop,
            latency_ms: 0,
            ttft_ms: None,
        })
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        let content = self.response.clone();
        Ok(Box::pin(futures::stream::once(async move {
            Ok(StreamChunk::Text(content))
        })))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "mock-stop"
    }
    fn model_id(&self) -> &str {
        "mock-v1"
    }
}

// ─────────────────────────────────────────────
// Mock CompletionModel : ReAct (1 ToolCalls → Stop)
// ─────────────────────────────────────────────

/// Mock LLM that emits one tool call on the first call, then `Stop` on the second.
struct MockReActModel {
    tool_name: String,
    final_content: String,
    call_count: Arc<AtomicU32>,
}

impl MockReActModel {
    fn new(
        tool_name: impl Into<String>,
        final_content: impl Into<String>,
    ) -> (Arc<Self>, Arc<AtomicU32>) {
        let count = Arc::new(AtomicU32::new(0));
        (
            Arc::new(Self {
                tool_name: tool_name.into(),
                final_content: final_content.into(),
                call_count: count.clone(),
            }),
            count,
        )
    }
}

#[async_trait::async_trait]
impl CompletionModel for MockReActModel {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let current = self.call_count.fetch_add(1, Ordering::SeqCst);
        if current == 0 {
            Ok(CompletionResponse {
                engine_timings: None,
                content: String::new(),
                tool_calls: vec![ToolCall {
                    id: "call_01".into(),
                    name: self.tool_name.clone(),
                    arguments: serde_json::json!({}),
                }],
                usage: TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 2,
                    cost_usd: None,
                    cache_read_input_tokens: 0,
                    cache_write_input_tokens: 0,
                },
                finish_reason: FinishReason::ToolCalls,
                latency_ms: 0,
                ttft_ms: None,
            })
        } else {
            Ok(CompletionResponse {
                engine_timings: None,
                content: self.final_content.clone(),
                tool_calls: vec![],
                usage: TokenUsage {
                    prompt_tokens: 5,
                    completion_tokens: 5,
                    cost_usd: None,
                    cache_read_input_tokens: 0,
                    cache_write_input_tokens: 0,
                },
                finish_reason: FinishReason::Stop,
                latency_ms: 0,
                ttft_ms: None,
            })
        }
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        unimplemented!("not used in this test")
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "mock-react"
    }
    fn model_id(&self) -> &str {
        "mock-v1"
    }
}

// ─────────────────────────────────────────────
// Mock CompletionModel: always ToolCalls (endless)
// ─────────────────────────────────────────────

/// Mock LLM that always returns `FinishReason::ToolCalls` - to exercise the guard.
struct MockInfiniteToolCallModel {
    call_count: Arc<AtomicU32>,
}

impl MockInfiniteToolCallModel {
    fn new() -> (Arc<Self>, Arc<AtomicU32>) {
        let count = Arc::new(AtomicU32::new(0));
        (
            Arc::new(Self {
                call_count: count.clone(),
            }),
            count,
        )
    }
}

#[async_trait::async_trait]
impl CompletionModel for MockInfiniteToolCallModel {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        Ok(CompletionResponse {
            engine_timings: None,
            content: String::new(),
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "echo".into(),
                arguments: serde_json::json!({}),
            }],
            usage: TokenUsage {
                prompt_tokens: 5,
                completion_tokens: 2,
                cost_usd: None,
                cache_read_input_tokens: 0,
                cache_write_input_tokens: 0,
            },
            finish_reason: FinishReason::ToolCalls,
            latency_ms: 0,
            ttft_ms: None,
        })
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        unimplemented!("not used in this test")
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "mock-infinite"
    }
    fn model_id(&self) -> &str {
        "mock-v1"
    }
}

// ─────────────────────────────────────────────
// Mock ToolInvoker
// ─────────────────────────────────────────────

/// Mock `ToolInvoker` returning a configurable result.
struct MockToolInvoker {
    result: String,
    call_count: Arc<AtomicU32>,
}

impl MockToolInvoker {
    fn new(result: impl Into<String>) -> (Arc<Self>, Arc<AtomicU32>) {
        let count = Arc::new(AtomicU32::new(0));
        (
            Arc::new(Self {
                result: result.into(),
                call_count: count.clone(),
            }),
            count,
        )
    }
}

#[async_trait::async_trait]
impl ToolInvoker for MockToolInvoker {
    async fn invoke(
        &self,
        _tool_name: &str,
        _arguments: &serde_json::Value,
    ) -> Result<String, String> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        Ok(self.result.clone())
    }
}

/// Mock `ToolInvoker` that always fails.
struct FailingToolInvoker;

#[async_trait::async_trait]
impl ToolInvoker for FailingToolInvoker {
    async fn invoke(
        &self,
        tool_name: &str,
        _arguments: &serde_json::Value,
    ) -> Result<String, String> {
        Err(format!("outil '{tool_name}' non disponible"))
    }
}

// ─────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────

fn make_tool_spec(name: &str) -> ToolSpec {
    ToolSpec {
        name: name.to_string(),
        description: "test tool".to_string(),
        parameters: serde_json::json!({}),
    }
}

fn user_message(text: &str) -> ChatMessage {
    ChatMessage::user(text)
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

/// The loop stops at once when the LLM returns `Stop`.
#[tokio::test]
async fn test_react_loop_stops_on_finish_stop() {
    // GIVEN a mock that answers Stop straight away
    let (model, model_count) = MockStopModel::new("direct answer");
    let (invoker, invoker_count) = MockToolInvoker::new("ok");
    let helper = ToolCallHelper::new(model, invoker);
    let budget = StepBudgetView::unlimited();

    // WHEN the ReAct loop runs
    let result = helper
        .run_tools(
            vec![user_message("question")],
            vec![make_tool_spec("echo")],
            5,
            &budget,
        )
        .await;

    // THEN the result is Ok, with the content of the mock
    assert_eq!(result.unwrap(), "direct answer");
    // AND the LLM was called exactly once
    assert_eq!(
        model_count.load(Ordering::Relaxed),
        1,
        "the LLM must be called exactly once"
    );
    // AND no tool was invoked
    assert_eq!(
        invoker_count.load(Ordering::Relaxed),
        0,
        "no tool must be invoked"
    );
}

/// ReAct loop: one tool call then the final answer (2 LLM calls, 1 tool call).
#[tokio::test]
async fn test_react_loop_calls_tool_once() {
    // GIVEN a ReAct mock: ToolCalls on the first call, Stop on the second
    let (model, model_count) = MockReActModel::new("echo", "final answer");
    let (invoker, invoker_count) = MockToolInvoker::new("tool_result");
    let helper = ToolCallHelper::new(model, invoker);
    let budget = StepBudgetView::unlimited();

    // WHEN the ReAct loop runs
    let result = helper
        .run_tools(
            vec![user_message("question")],
            vec![make_tool_spec("echo")],
            5,
            &budget,
        )
        .await;

    // THEN the result is Ok, with the final answer
    assert_eq!(result.unwrap(), "final answer");
    // AND the LLM was called exactly twice (ToolCalls + Stop)
    assert_eq!(
        model_count.load(Ordering::Relaxed),
        2,
        "the LLM must be called exactly twice"
    );
    // AND the tool was invoked exactly once
    assert_eq!(
        invoker_count.load(Ordering::Relaxed),
        1,
        "the tool must be invoked exactly once"
    );
}

/// `max_iterations` guard: the endless loop is stopped after N iterations.
#[tokio::test]
async fn test_react_loop_max_iterations_guard() {
    // GIVEN a mock that always returns ToolCalls (a potentially endless loop)
    let (model, model_count) = MockInfiniteToolCallModel::new();
    let (invoker, _) = MockToolInvoker::new("ok");
    let helper = ToolCallHelper::new(model, invoker);
    let budget = StepBudgetView::unlimited();

    // WHEN the loop runs with max_iterations = 3
    let result = helper
        .run_tools(vec![user_message("q")], vec![], 3, &budget)
        .await;

    // THEN MaxIterationsReached(3) is returned
    assert!(
        matches!(
            result,
            Err(LlmError::MaxIterationsReached { iterations: 3 })
        ),
        "expected MaxIterationsReached(3), got: {result:?}"
    );
    // AND exactly three LLM calls happened
    assert_eq!(
        model_count.load(Ordering::Relaxed),
        3,
        "the LLM must be called exactly three times"
    );
}

/// `StepBudget` guard: no LLM call at all when the budget is spent.
#[tokio::test]
async fn test_react_loop_budget_exhausted_guard() {
    // GIVEN a budget already spent (100/100 steps)
    let counter = Arc::new(AtomicU32::new(100));
    let budget = StepBudgetView::new(counter, 100);
    let (model, model_count) = MockStopModel::new("must not be reached");
    let (invoker, _) = MockToolInvoker::new("ok");
    let helper = ToolCallHelper::new(model, invoker);

    // WHEN the loop runs on a spent budget
    let result = helper
        .run_tools(vec![user_message("q")], vec![], 5, &budget)
        .await;

    // THEN BudgetExceeded is returned at once
    assert!(
        matches!(result, Err(LlmError::BudgetExceeded)),
        "expected BudgetExceeded, got: {result:?}"
    );
    // AND no LLM call happened
    assert_eq!(
        model_count.load(Ordering::Relaxed),
        0,
        "no LLM call must happen on a spent budget"
    );
}

/// Tool errors are absorbed as text, and the loop carries on.
///
/// Checks that a `ToolInvoker::invoke` returning `Err` does not break the loop:
/// the error is passed to the LLM as a textual result.
#[tokio::test]
async fn test_react_loop_tool_error_absorbed() {
    // GIVEN a ReAct mock plus a ToolInvoker that always fails
    let (model, _) = MockReActModel::new("fail_tool", "answer despite the error");
    let invoker = Arc::new(FailingToolInvoker);
    let helper = ToolCallHelper::new(model, invoker);
    let budget = StepBudgetView::unlimited();

    // WHEN the loop runs
    let result = helper
        .run_tools(vec![user_message("q")], vec![], 5, &budget)
        .await;

    // THEN the loop does not panic and returns Ok (the error is absorbed as text)
    assert!(result.is_ok(), "a tool error must not be fatal: {result:?}");
    assert_eq!(result.unwrap(), "answer despite the error");
}
