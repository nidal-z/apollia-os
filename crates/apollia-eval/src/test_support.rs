//! Shared test helpers, compiled only under `cfg(test)`.
//!
//! A mock [`CompletionModel`] and router builder reused by the judge unit tests
//! and the runner wiring tests, so the LLM mock lives in one place.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use apollia_llm::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, LlmRouter,
    StreamChunk, TokenUsage,
};

/// A backend that always replies with a fixed string.
pub(crate) struct FixedReplyModel {
    reply: String,
}

#[async_trait::async_trait]
impl CompletionModel for FixedReplyModel {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            engine_timings: None,
            content: self.reply.clone(),
            tool_calls: vec![],
            usage: TokenUsage::default(),
            finish_reason: FinishReason::Stop,
            latency_ms: 0,
            ttft_ms: None,
        })
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
    {
        Err(LlmError::InferenceError("mock does not stream".into()))
    }

    fn is_available(&self) -> bool {
        true
    }

    fn backend_name(&self) -> &str {
        "mock"
    }

    fn model_id(&self) -> &str {
        "mock-model"
    }
}

/// Builds a single-backend router whose backend always replies `reply`.
pub(crate) fn router_replying(reply: &str) -> LlmRouter {
    let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
    backends.insert(
        "mock".to_string(),
        Arc::new(FixedReplyModel {
            reply: reply.to_string(),
        }),
    );
    LlmRouter::with_backends(backends, "mock")
}
