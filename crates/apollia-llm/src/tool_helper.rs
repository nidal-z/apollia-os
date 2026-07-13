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

/// View of the `StepBudget` shared with the ReAct loop and the runtime.
///
/// Backed by `Arc<AtomicU32>` counters shared with the owning budget. Reads
/// (`is_exhausted`, `steps_remaining`, `tool_calls_remaining`) never mutate.
/// The [`increment_steps`](Self::increment_steps) and
/// [`increment_tool_calls`](Self::increment_tool_calls) methods advance the
/// shared counters so a chokepoint that only holds the view (for instance the
/// AIP tool and LLM proxies on the Direct path) can enforce the budget the
/// runtime supervises (principle #7).
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

    /// Advance the shared step counter by one.
    ///
    /// Visible to the owning budget through the shared `Arc<AtomicU32>`, so the
    /// runtime supervising that budget sees the increment.
    pub fn increment_steps(&self) {
        self.step_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Advance the shared tool-call counter by one.
    ///
    /// Visible to the owning budget through the shared `Arc<AtomicU32>`.
    pub fn increment_tool_calls(&self) {
        self.tool_calls_count.fetch_add(1, Ordering::Relaxed);
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
        // Generate the grammar once: the tool set is stable across iterations.
        // Local backend with tools only; cloud backends ignore the field and keep None.
        let grammar: Option<String> = if !tools.is_empty() && self.model.is_local() {
            let gbnf = crate::grammar::tool_specs_to_gbnf(&tools);
            if gbnf.is_empty() {
                tracing::warn!("tool_specs_to_gbnf produced empty grammar, running unconstrained");
                None
            } else {
                Some(gbnf)
            }
        } else {
            None
        };

        for _iter in 0..max_iters {
            // Budget guardrail checked first, then this iteration is charged as one
            // step so the shared budget advances even on the Direct path where no
            // other component counts the ReAct iterations (principle #7).
            if budget.is_exhausted() {
                return Err(LlmError::BudgetExceeded);
            }
            budget.increment_steps();

            let response = self
                .model
                .complete(CompletionRequest {
                    messages: messages.clone(),
                    tools: tools.clone(),
                    grammar: grammar.clone(),
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
    use std::sync::Mutex;

    use futures::Stream;

    // ── Budget helpers ─────────────────────────────────────────────────────

    fn unlimited_budget() -> StepBudgetView {
        StepBudgetView::new(Arc::new(AtomicU32::new(0)), 100)
    }

    fn exhausted_budget() -> StepBudgetView {
        StepBudgetView::new(Arc::new(AtomicU32::new(100)), 100)
    }

    /// Increments advance the shared counters and drive exhaustion.
    #[test]
    fn test_step_budget_view_increments_shared_counters() {
        // GIVEN a view over shared counters with a 2-step, 3-tool-call budget
        let steps = Arc::new(AtomicU32::new(0));
        let tools = Arc::new(AtomicU32::new(0));
        let view = StepBudgetView::with_tool_tracking(
            Arc::clone(&steps),
            2,
            Arc::clone(&tools),
            3,
            std::time::Instant::now(),
        );

        // WHEN the step budget is spent through the view
        view.increment_steps();
        assert!(!view.is_exhausted());
        view.increment_steps();

        // THEN the shared counter reflects it and the view reports exhaustion
        assert_eq!(steps.load(Ordering::Relaxed), 2);
        assert!(view.is_exhausted());

        // AND tool-call increments are tracked independently
        view.increment_tool_calls();
        assert_eq!(view.tool_calls_remaining(), 2);
        assert_eq!(tools.load(Ordering::Relaxed), 1);
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
    async fn test_stop_immediately_no_tool_call() {
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
    async fn test_one_tool_call_then_stop() {
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
    async fn test_max_iterations_reached() {
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
    async fn test_budget_exhausted_stops_immediately() {
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

    /// Each ReAct iteration charges one step against the shared budget, so a
    /// step-limited budget stops the loop before `max_iters` (Direct-path
    /// enforcement, principle #7).
    #[tokio::test]
    async fn test_run_tools_charges_a_step_per_iteration() {
        // GIVEN a model that always requests a tool and a budget of 2 steps
        let model = Arc::new(MockInfiniteToolCallModel);
        let invoker = Arc::new(MockToolInvoker::new());
        let helper = ToolCallHelper::new(model, invoker);
        let budget = StepBudgetView::new(Arc::new(AtomicU32::new(0)), 2);

        // WHEN run_tools loops with a generous max_iters (5)
        let result = helper
            .run_tools(vec![ChatMessage::user("q")], vec![], 5, &budget)
            .await;

        // THEN it stops on the step budget (2), not on max_iters (5): each
        // iteration was charged as a step.
        assert!(matches!(result, Err(LlmError::BudgetExceeded)));
        assert_eq!(budget.steps_remaining(), 0);
    }

    /// Tool error absorbed as text, the loop continues.
    #[tokio::test]
    async fn test_tool_error_absorbed_as_text() {
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

    // ── GBNF grammar wiring ───────────────────────────────────

    /// Mock that captures every `CompletionRequest` it receives and reports a
    /// configurable `is_local`.
    struct CapturingMock {
        is_local_backend: bool,
        captured: Mutex<Vec<CompletionRequest>>,
    }

    impl CapturingMock {
        fn new(is_local_backend: bool) -> Self {
            Self {
                is_local_backend,
                captured: Mutex::new(vec![]),
            }
        }
    }

    #[async_trait::async_trait]
    impl CompletionModel for CapturingMock {
        async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            self.captured.lock().unwrap().push(req);
            Ok(stop_response("done"))
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
            if self.is_local_backend {
                "local"
            } else {
                "anthropic"
            }
        }
        fn model_id(&self) -> &str {
            "mock"
        }
        fn is_local(&self) -> bool {
            self.is_local_backend
        }
    }

    fn one_tool() -> Vec<ToolSpec> {
        vec![ToolSpec {
            name: "search_web".into(),
            description: "search".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": ["query"]
            }),
        }]
    }

    /// local backend with tools injects a non-empty grammar that
    /// names the tool and its property.
    #[tokio::test]
    async fn test_local_backend_with_tools_injects_grammar() {
        // GIVEN a local backend and a non-empty tool set
        let model = Arc::new(CapturingMock::new(true));
        let invoker = Arc::new(MockToolInvoker::new());
        let helper = ToolCallHelper::new(model.clone(), invoker);

        // WHEN run_tools is called
        let _ = helper
            .run_tools(
                vec![ChatMessage::user("test")],
                one_tool(),
                3,
                &unlimited_budget(),
            )
            .await;

        // THEN every request carries a grammar naming the tool and its property
        let captured = model.captured.lock().unwrap();
        assert!(!captured.is_empty());
        let gbnf = captured[0]
            .grammar
            .as_deref()
            .expect("grammar should be Some for a local backend");
        assert!(gbnf.contains("search_web"), "tool name absent from grammar");
        assert!(gbnf.contains("query"), "tool property absent from grammar");
    }

    /// cloud backend leaves the grammar None.
    #[tokio::test]
    async fn test_cloud_backend_grammar_stays_none() {
        // GIVEN a cloud backend and a non-empty tool set
        let model = Arc::new(CapturingMock::new(false));
        let invoker = Arc::new(MockToolInvoker::new());
        let helper = ToolCallHelper::new(model.clone(), invoker);

        // WHEN run_tools is called
        let _ = helper
            .run_tools(
                vec![ChatMessage::user("test")],
                one_tool(),
                3,
                &unlimited_budget(),
            )
            .await;

        // THEN the grammar stays None
        let captured = model.captured.lock().unwrap();
        assert!(!captured.is_empty());
        assert!(
            captured[0].grammar.is_none(),
            "grammar should be None for a cloud backend"
        );
    }

    /// empty tool set leaves the grammar None even on a local backend.
    #[tokio::test]
    async fn test_no_tools_grammar_is_none() {
        // GIVEN a local backend and an empty tool set
        let model = Arc::new(CapturingMock::new(true));
        let invoker = Arc::new(MockToolInvoker::new());
        let helper = ToolCallHelper::new(model.clone(), invoker);

        // WHEN run_tools is called
        let _ = helper
            .run_tools(
                vec![ChatMessage::user("test")],
                vec![],
                3,
                &unlimited_budget(),
            )
            .await;

        // THEN the grammar stays None (nothing to constrain)
        let captured = model.captured.lock().unwrap();
        assert!(!captured.is_empty());
        assert!(
            captured[0].grammar.is_none(),
            "grammar should be None without tools"
        );
    }
}
