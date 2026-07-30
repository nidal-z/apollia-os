//! Replay injectors: trait adapters that feed captured inputs back into the
//! agentic loop in place of the live sources.
//!
//! Each injector wraps the matching cursor from a [`crate::replay::ReplayBundle`]
//! and hands out the captured values in `step_ordinal` order. The injectors hold
//! no comparison logic (Principle #5): they only replay. Comparison lives in the
//! [`crate::replay::harness`].

use std::pin::Pin;
use std::sync::Mutex;

use apollia_llm::tool_helper::ToolInvoker;
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, StreamChunk,
    TokenUsage, ToolCall,
};
use futures::Stream;

use crate::replay::capture::{
    ClockReplayCursor, LlmReplayCursor, RandomReplayCursor, ToolReplayCursor,
};
use crate::replay::nondeterminism::{ClockSource, RandomSource};

/// Logical backend name reported by the replay backend.
const REPLAY_BACKEND: &str = "replay";

/// Rebuild a [`ToolCall`] from its captured JSON form (`{id, name, arguments}`).
fn tool_call_from_value(value: &serde_json::Value) -> ToolCall {
    ToolCall {
        id: value
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        name: value
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        arguments: value
            .get("arguments")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    }
}

/// `CompletionModel` that replays captured LLM responses.
///
/// Each call to [`CompletionModel::complete`] (or [`CompletionModel::stream`])
/// consumes the next captured response, in `step_ordinal` order. The cursor sits
/// behind a `Mutex` because the trait methods take `&self`.
pub struct ReplayBackend {
    cursor: Mutex<LlmReplayCursor>,
    model_id: String,
}

impl ReplayBackend {
    /// Wrap an LLM cursor as a replay backend.
    #[must_use]
    pub fn new(cursor: LlmReplayCursor) -> Self {
        Self {
            cursor: Mutex::new(cursor),
            model_id: REPLAY_BACKEND.to_string(),
        }
    }

    /// Pull and rebuild the next captured response.
    fn next_response(&self) -> Result<CompletionResponse, LlmError> {
        let mut cursor = self
            .cursor
            .lock()
            .map_err(|_| LlmError::InferenceError("replay cursor poisoned".into()))?;
        let snapshot = cursor
            .next()
            .map_err(|e| LlmError::InferenceError(e.to_string()))?;

        let tool_calls: Vec<ToolCall> = snapshot
            .tool_calls
            .iter()
            .map(tool_call_from_value)
            .collect();
        let finish_reason = if snapshot.stream_truncated {
            FinishReason::Error
        } else if tool_calls.is_empty() {
            FinishReason::Stop
        } else {
            FinishReason::ToolCalls
        };
        let usage = TokenUsage {
            prompt_tokens: snapshot.prompt_tokens,
            completion_tokens: snapshot.completion_tokens,
            ..TokenUsage::default()
        };

        Ok(CompletionResponse {
            engine_timings: None,
            content: snapshot.content,
            tool_calls,
            usage,
            finish_reason,
            latency_ms: 0,
            ttft_ms: None,
        })
    }
}

#[async_trait::async_trait]
impl CompletionModel for ReplayBackend {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.next_response()
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        let response = self.next_response()?;
        let mut chunks: Vec<Result<StreamChunk, LlmError>> = Vec::new();
        if !response.content.is_empty() {
            chunks.push(Ok(StreamChunk::Text(response.content)));
        }
        for call in response.tool_calls {
            chunks.push(Ok(StreamChunk::ToolCall(call)));
        }
        Ok(Box::pin(futures::stream::iter(chunks)))
    }

    fn is_available(&self) -> bool {
        true
    }

    fn backend_name(&self) -> &str {
        REPLAY_BACKEND
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }
}

/// `ToolInvoker` that replays captured tool outputs in order.
pub struct ReplayToolInvoker {
    cursor: Mutex<ToolReplayCursor>,
}

impl ReplayToolInvoker {
    /// Wrap a tool cursor as a replay invoker.
    #[must_use]
    pub fn new(cursor: ToolReplayCursor) -> Self {
        Self {
            cursor: Mutex::new(cursor),
        }
    }
}

#[async_trait::async_trait]
impl ToolInvoker for ReplayToolInvoker {
    async fn invoke(
        &self,
        _tool_name: &str,
        _arguments: &serde_json::Value,
    ) -> Result<String, String> {
        let mut cursor = self
            .cursor
            .lock()
            .map_err(|_| "replay tool cursor poisoned".to_string())?;
        let snapshot = cursor.next().map_err(|e| e.to_string())?;
        // The captured output was a string body; return it verbatim, otherwise
        // re-serialize the JSON value.
        match snapshot.output {
            serde_json::Value::String(s) => Ok(s),
            other => Ok(other.to_string()),
        }
    }
}

/// `ClockSource` that replays captured timestamps in order.
///
/// Exhaustion yields 0: the live agent has no clock consumer today, so this is a
/// degenerate case kept total rather than panicking.
pub struct ReplayClock {
    cursor: Mutex<ClockReplayCursor>,
}

impl ReplayClock {
    /// Wrap a clock cursor as a replay clock.
    #[must_use]
    pub fn new(cursor: ClockReplayCursor) -> Self {
        Self {
            cursor: Mutex::new(cursor),
        }
    }
}

impl ClockSource for ReplayClock {
    fn now_ms(&self) -> u64 {
        self.cursor
            .lock()
            .ok()
            .and_then(|mut c| c.next().ok())
            .map_or(0, |sample| sample.timestamp_ms)
    }
}

/// `RandomSource` that replays captured random draws in order.
///
/// Exhaustion yields zeroed bytes: kept total rather than panicking.
pub struct ReplayRandom {
    cursor: Mutex<RandomReplayCursor>,
}

impl ReplayRandom {
    /// Wrap a random cursor as a replay random source.
    #[must_use]
    pub fn new(cursor: RandomReplayCursor) -> Self {
        Self {
            cursor: Mutex::new(cursor),
        }
    }
}

impl RandomSource for ReplayRandom {
    fn random_bytes(&self) -> [u8; 16] {
        let bytes = self
            .cursor
            .lock()
            .ok()
            .and_then(|mut c| c.next().ok())
            .map(|sample| sample.bytes)
            .unwrap_or_default();
        let mut out = [0u8; 16];
        for (slot, byte) in out.iter_mut().zip(bytes.iter()) {
            *slot = *byte;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit_journal::entry::{JournalEntry, JournalEntryKind};
    use crate::replay::capture::LlmCompletionSnapshot;
    use apollia_core::events::RunId;

    fn llm_entry(run: &RunId, ordinal: u32, content: &str) -> JournalEntry {
        let snap = LlmCompletionSnapshot {
            run_id: run.clone(),
            step_ordinal: ordinal,
            backend_name: "local".into(),
            model_id: "m".into(),
            content: content.into(),
            tool_calls: vec![],
            prompt_tokens: 0,
            completion_tokens: 0,
            cost_usd: None,
            stream_truncated: false,
        };
        JournalEntry {
            seq: u64::from(ordinal),
            run_id: run.as_str().to_string(),
            ts: "2026-06-10T00:00:00Z".into(),
            kind: JournalEntryKind::LlmCompletion,
            payload: serde_json::to_value(snap).expect("serialize"),
            prev_hash: "x".into(),
            hash: "y".into(),
            signature: None,
            signing_key_id: None,
        }
    }

    // AC-4: the replay backend hands out captured responses in ordinal order.
    #[tokio::test]
    async fn test_replay_backend_consumes_in_ordinal_order() {
        // GIVEN a backend over three captured responses
        let run = RunId::new();
        let entries = vec![
            llm_entry(&run, 0, "first"),
            llm_entry(&run, 1, "second"),
            llm_entry(&run, 2, "third"),
        ];
        let cursor = LlmReplayCursor::from_journal(&entries, &run).expect("cursor");
        let backend = ReplayBackend::new(cursor);

        // WHEN complete() is called repeatedly
        // THEN the responses come back in order, then exhaust with an error
        let req = CompletionRequest::default();
        assert_eq!(
            backend.complete(req.clone()).await.expect("0").content,
            "first"
        );
        assert_eq!(
            backend.complete(req.clone()).await.expect("1").content,
            "second"
        );
        assert_eq!(
            backend.complete(req.clone()).await.expect("2").content,
            "third"
        );
        assert!(backend.complete(req).await.is_err());
    }
}
