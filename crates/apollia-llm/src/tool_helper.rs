//! `ToolCallHelper`, automatic ReAct loop with tool execution.
//!
//! Orchestrates the successive LLM calls and the [`ToolInvoker`] invocation.

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use std::time::Instant;

use crate::types::{
    ChatMessage, CompletionModel, CompletionRequest, FinishReason, LlmError, ToolSpec,
};

/// Read-only view of the `StepBudget` exposed to the ReAct loop and to
/// `ctx.step_budget`.
///
/// Shared via `Arc<AtomicU32>` with the runtime's mutable budget. No mutation
/// is possible from this view.
pub struct StepBudgetView {
    step_count: Arc<AtomicU32>,
    step_limit: u32,
    tool_calls_count: Arc<AtomicU32>,
    max_tool_calls: u32,
    started_at: Instant,
}

impl StepBudgetView {
    /// Create a view from a shared atomic counter and a limit.
    ///
    /// `tool_calls_count` and `max_tool_calls` default to 0 / `u32::MAX`.
    /// `started_at` is initialized to now.
    pub fn new(step_count: Arc<AtomicU32>, step_limit: u32) -> Self {
        Self {
            step_count,
            step_limit,
            tool_calls_count: Arc::new(AtomicU32::new(0)),
            max_tool_calls: u32::MAX,
            started_at: Instant::now(),
        }
    }

    /// Create a view with full tracking of tool calls and elapsed time.
    pub fn with_tool_tracking(
        step_count: Arc<AtomicU32>,
        step_limit: u32,
        tool_calls_count: Arc<AtomicU32>,
        max_tool_calls: u32,
        started_at: Instant,
    ) -> Self {
        Self {
            step_count,
            step_limit,
            tool_calls_count,
            max_tool_calls,
            started_at,
        }
    }

    /// Create a view with no budget limit, for unit tests.
    pub fn unlimited() -> Self {
        Self {
            step_count: Arc::new(AtomicU32::new(0)),
            step_limit: u32::MAX,
            tool_calls_count: Arc::new(AtomicU32::new(0)),
            max_tool_calls: u32::MAX,
            started_at: Instant::now(),
        }
    }

    /// Return `true` if the step budget has been exhausted.
    pub fn is_exhausted(&self) -> bool {
        self.step_count.load(Ordering::Relaxed) >= self.step_limit
    }

    /// Steps remaining before reaching the limit.
    ///
    /// Returns 0 if the budget is already exhausted (no negative value).
    pub fn steps_remaining(&self) -> i64 {
        let used = self.step_count.load(Ordering::Relaxed) as i64;
        let limit = self.step_limit as i64;
        (limit - used).max(0)
    }

    /// Tool calls remaining before reaching `max_tool_calls`.
    ///
    /// Returns 0 if exhausted. Returns `i64::MAX` if unlimited (`max_tool_calls = u32::MAX`).
    pub fn tool_calls_remaining(&self) -> i64 {
        let used = self.tool_calls_count.load(Ordering::Relaxed) as i64;
        let max = self.max_tool_calls as i64;
        (max - used).max(0)
    }

    /// Seconds elapsed since the budget started.
    pub fn elapsed_secs(&self) -> f64 {
        self.started_at.elapsed().as_secs_f64()
    }
}

/// Abstraction for tool invocation from the ReAct loop.
///
/// Dependency via a trait, not a concrete type. This avoids any direct
/// dependency from `apollia-llm` to `apollia-tools` and allows injecting a
/// mock in tests.
///
/// The concrete implementation wraps `ToolRegistryHandle`.
#[async_trait::async_trait]
pub trait ToolInvoker: Send + Sync {
    /// Invoke a tool by name with the provided JSON arguments.
    ///
    /// Returns `Ok(result)` as a string or `Err(description)`. Errors are
    /// absorbed by [`ToolCallHelper`] as text results; they are never fatal to
    /// the loop.
    async fn invoke(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<String, String>;
}

/// Orchestrates the ReAct loop: LLM, tool(s), LLM, ..., final answer.
///
/// Applies two non-negotiable guardrails:
/// - `max_iters`: maximum number of LLM calls before [`LlmError::MaxIterationsReached`]
/// - [`StepBudgetView`]: agent budget checked first on every iteration
///
/// One actor, one responsibility: `ToolCallHelper` orchestrates but does not
/// execute tools directly (that is delegated to the [`ToolInvoker`]).
pub struct ToolCallHelper {
    model: Arc<dyn CompletionModel>,
    invoker: Arc<dyn ToolInvoker>,
}

impl ToolCallHelper {
    /// Create a `ToolCallHelper` from an LLM backend and a tool invoker.
    pub fn new(model: Arc<dyn CompletionModel>, invoker: Arc<dyn ToolInvoker>) -> Self {
        Self { model, invoker }
    }

    /// Run the full ReAct loop.
    ///
    /// Iterates until [`FinishReason::Stop`] or until a guardrail is exhausted.
    /// Tool calls are executed sequentially to guarantee ordering. Tool errors
    /// are absorbed as text results (never fatal).
    ///
    /// # Errors
    ///
    /// - [`LlmError::BudgetExceeded`] if the `StepBudget` is exhausted before an LLM call
    /// - [`LlmError::MaxIterationsReached`] if the loop exceeds `max_iters` iterations
    /// - [`LlmError::MaxTokensReached`] if the LLM returns [`FinishReason::Length`]
    /// - [`LlmError::InferenceError`] if the LLM returns [`FinishReason::Error`]
    pub async fn run_tools(
        &self,
        mut messages: Vec<ChatMessage>,
        tools: Vec<ToolSpec>,
        max_iters: u32,
        budget: &StepBudgetView,
    ) -> Result<String, LlmError> {
        for _iter in 0..max_iters {
            // Budget guardrail checked first.
            if budget.is_exhausted() {
                return Err(LlmError::BudgetExceeded);
            }

            let response = self
                .model
                .complete(CompletionRequest {
                    messages: messages.clone(),
                    tools: tools.clone(),
                    ..Default::default()
                })
                .await?;

            match response.finish_reason {
                FinishReason::Stop => return Ok(response.content),

                FinishReason::ToolCalls => {
                    messages.push(ChatMessage::assistant_with_calls(
                        &response.content,
                        &response.tool_calls,
                    ));
                    for call in &response.tool_calls {
                        let result = self
                            .invoker
                            .invoke(&call.name, &call.arguments)
                            .await
                            .unwrap_or_else(|e| format!("tool error: {e}"));
                        messages.push(ChatMessage::tool_result(&call.id, &result));
                    }
                }

                FinishReason::Length => return Err(LlmError::MaxTokensReached),
                FinishReason::Error => {
                    return Err(LlmError::InferenceError(
                        "backend returned error finish_reason".into(),
                    ))
                }
            }
        }
        Err(LlmError::MaxIterationsReached {
            iterations: max_iters,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CompletionResponse, StreamChunk, TokenUsage, ToolCall, ToolSpec};
    use std::pin::Pin;
    use std::sync::atomic::AtomicU32;

    use futures::Stream;

    // ── Budget helpers ─────────────────────────────────────────────────────

    fn unlimited_budget() -> StepBudgetView {
        StepBudgetView::new(Arc::new(AtomicU32::new(0)), 100)
    }

    fn exhausted_budget() -> StepBudgetView {
        StepBudgetView::new(Arc::new(AtomicU32::new(100)), 100)
    }

    // ── Response helpers ───────────────────────────────────────────────────

    fn stop_response(content: impl Into<String>) -> CompletionResponse {
        CompletionResponse {
            content: content.into(),
            tool_calls: vec![],
            usage: TokenUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                cost_usd: None,
                ..Default::default()
            },
            finish_reason: FinishReason::Stop,
            latency_ms: 0,
            ttft_ms: None,
        }
    }

    fn tool_calls_response(calls: Vec<ToolCall>) -> CompletionResponse {
        CompletionResponse {
            content: String::new(),
            tool_calls: calls,
            usage: TokenUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                cost_usd: None,
                ..Default::default()
            },
            finish_reason: FinishReason::ToolCalls,
            latency_ms: 0,
            ttft_ms: None,
        }
    }

    // ── Mock CompletionModel: immediate Stop ──────────────────────────────

    struct MockStopModel {
        content: String,
    }

    impl MockStopModel {
        fn new(content: impl Into<String>) -> Self {
            Self {
                content: content.into(),
            }
        }
    }

    #[async_trait::async_trait]
    impl CompletionModel for MockStopModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Ok(stop_response(self.content.clone()))
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
        {
            unimplemented!("not used in tests")
        }

        fn is_available(&self) -> bool {
            true
        }
        fn backend_name(&self) -> &str {
            "mock-stop"
        }
        fn model_id(&self) -> &str {
            "mock"
        }
    }

    // ── Mock CompletionModel: ToolCalls then Stop ─────────────────────────

    struct MockReActModel {
        calls: Vec<ToolCall>,
        final_content: String,
        iteration: AtomicU32,
    }

    impl MockReActModel {
        fn new(calls: Vec<ToolCall>, final_content: impl Into<String>) -> Self {
            Self {
                calls,
                final_content: final_content.into(),
                iteration: AtomicU32::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl CompletionModel for MockReActModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            let current = self.iteration.fetch_add(1, Ordering::SeqCst);
            if current == 0 {
                Ok(tool_calls_response(self.calls.clone()))
            } else {
                Ok(stop_response(self.final_content.clone()))
            }
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
        {
            unimplemented!("not used in tests")
        }

        fn is_available(&self) -> bool {
            true
        }
        fn backend_name(&self) -> &str {
            "mock-react"
        }
        fn model_id(&self) -> &str {
            "mock"
        }
    }

    // ── Mock CompletionModel: always ToolCalls (infinite) ─────────────────

    struct MockInfiniteToolCallModel;

    #[async_trait::async_trait]
    impl CompletionModel for MockInfiniteToolCallModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Ok(tool_calls_response(vec![]))
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
        {
            unimplemented!("not used in tests")
        }

        fn is_available(&self) -> bool {
            true
        }
        fn backend_name(&self) -> &str {
            "mock-infinite"
        }
        fn model_id(&self) -> &str {
            "mock"
        }
    }

    // ── Mock ToolInvoker ──────────────────────────────────────────────────

    struct MockToolInvoker {
        result: Option<String>,
        call_count: Arc<AtomicU32>,
    }

    impl MockToolInvoker {
        fn new() -> Self {
            Self {
                result: None,
                call_count: Arc::new(AtomicU32::new(0)),
            }
        }

        fn with_result(result: impl Into<String>) -> Self {
            Self {
                result: Some(result.into()),
                call_count: Arc::new(AtomicU32::new(0)),
            }
        }

        fn call_count(&self) -> u32 {
            self.call_count.load(Ordering::Relaxed)
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
            match &self.result {
                Some(r) => Ok(r.clone()),
                None => Ok("ok".into()),
            }
        }
    }

    // ── Mock ToolInvoker that always fails ────────────────────────────────

    struct MockFailingToolInvoker;

    #[async_trait::async_trait]
    impl ToolInvoker for MockFailingToolInvoker {
        async fn invoke(
            &self,
            _tool_name: &str,
            _arguments: &serde_json::Value,
        ) -> Result<String, String> {
            Err("outil non disponible".into())
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────

    /// LLM answers directly with no tool call.
    #[tokio::test]
    async fn test_ac1_stop_immediately_no_tool_call() {
        // GIVEN
        let model = Arc::new(MockStopModel::new("réponse finale"));
        let invoker = Arc::new(MockToolInvoker::new());
        let helper = ToolCallHelper::new(model, invoker.clone());

        // WHEN
        let result = helper
            .run_tools(
                vec![ChatMessage::user("question")],
                vec![],
                5,
                &unlimited_budget(),
            )
            .await;

        // THEN
        assert_eq!(result.unwrap(), "réponse finale");
        assert_eq!(invoker.call_count(), 0);
    }

    /// ReAct loop: LLM calls 1 tool then answers.
    #[tokio::test]
    async fn test_ac2_one_tool_call_then_stop() {
        // GIVEN
        let model = Arc::new(MockReActModel::new(
            vec![ToolCall {
                id: "c1".into(),
                name: "echo".into(),
                arguments: serde_json::json!({}),
            }],
            "réponse après outil",
        ));
        let invoker = Arc::new(MockToolInvoker::with_result("résultat_outil"));
        let helper = ToolCallHelper::new(model, invoker.clone());

        // WHEN
        let result = helper
            .run_tools(
                vec![ChatMessage::user("question")],
                vec![ToolSpec {
                    name: "echo".into(),
                    description: String::new(),
                    parameters: serde_json::json!({}),
                }],
                5,
                &unlimited_budget(),
            )
            .await;

        // THEN
        assert_eq!(result.unwrap(), "réponse après outil");
        assert_eq!(invoker.call_count(), 1);
    }

    /// `max_iterations` guardrail enforced.
    #[tokio::test]
    async fn test_ac3_max_iterations_reached() {
        // GIVEN: the LLM always returns ToolCalls
        let model = Arc::new(MockInfiniteToolCallModel);
        let invoker = Arc::new(MockToolInvoker::new());
        let helper = ToolCallHelper::new(model, invoker);

        // WHEN
        let result = helper
            .run_tools(vec![ChatMessage::user("q")], vec![], 3, &unlimited_budget())
            .await;

        // THEN
        assert!(matches!(
            result,
            Err(LlmError::MaxIterationsReached { iterations: 3 })
        ));
    }

    /// `StepBudget` guardrail enforced: no LLM call if the budget is exhausted.
    #[tokio::test]
    async fn test_ac4_budget_exhausted_stops_immediately() {
        // GIVEN
        let model = Arc::new(MockStopModel::new("should not be reached"));
        let invoker = Arc::new(MockToolInvoker::new());
        let helper = ToolCallHelper::new(model, invoker);

        // WHEN
        let result = helper
            .run_tools(vec![ChatMessage::user("q")], vec![], 5, &exhausted_budget())
            .await;

        // THEN
        assert!(matches!(result, Err(LlmError::BudgetExceeded)));
    }

    /// Tool error absorbed as text, the loop continues.
    #[tokio::test]
    async fn test_ac5_tool_error_absorbed_as_text() {
        // GIVEN: the ToolInvoker always returns an error
        let model = Arc::new(MockReActModel::new(
            vec![ToolCall {
                id: "c1".into(),
                name: "fail_tool".into(),
                arguments: serde_json::json!({}),
            }],
            "réponse malgré erreur",
        ));
        let invoker = Arc::new(MockFailingToolInvoker);
        let helper = ToolCallHelper::new(model, invoker);

        // WHEN
        let result = helper
            .run_tools(vec![ChatMessage::user("q")], vec![], 5, &unlimited_budget())
            .await;

        // THEN: the tool error is not fatal
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "réponse malgré erreur");
    }
}
