//! The measure of one model call, and the conversation turn it belongs to.
//!
//! A call is measured once, whatever path reached the backend: the complete
//! path reads the [`CompletionResponse`] it got back, the streaming path feeds
//! each [`StreamChunk`] to a [`CallMeter`] as it passes. Both end on the same
//! [`CallMeasure`], the payload of `RuntimeEvent::LlmCallCompleted` and of the
//! aggregate a chat turn keeps under its answer.
//!
//! An unknown value stays unknown. A backend that streams nothing has no time
//! to first token, and a call with neither engine timings nor a first token
//! has no generation time: both are `None`, never zero, so a reader can tell
//! "instant" from "not measured".

use std::time::Instant;

use crate::types::{CompletionResponse, FinishReason, StreamChunk, TokenUsage};

/// The conversation turn a model call is made for.
///
/// Carried by [`crate::types::CompletionRequest::call_context`] from the chat
/// loop to the router, which stamps both identifiers on the call it publishes.
/// Never sent to a backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallContext {
    /// Conversation the call is made in.
    pub session_id: String,
    /// The turn: the identifier of the assistant message that answers it.
    pub turn_id: String,
}

/// Why a call ended, in the words the measure keeps.
///
/// A superset of [`FinishReason`]: a stream the caller dropped before its end
/// was stopped, not finished, and the backend never said why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallEnd {
    /// The model ended its answer.
    Stop,
    /// The model asked for tools.
    ToolCalls,
    /// The answer hit the token limit.
    Length,
    /// The backend failed, or refused the content.
    Error,
    /// The caller stopped reading before the end, a user's Stop.
    Cancelled,
}

impl CallEnd {
    /// The stable label the event, the store and the route carry.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::ToolCalls => "tool_calls",
            Self::Length => "length",
            Self::Error => "error",
            Self::Cancelled => "cancelled",
        }
    }
}

impl From<&FinishReason> for CallEnd {
    fn from(reason: &FinishReason) -> Self {
        match reason {
            FinishReason::Stop => Self::Stop,
            FinishReason::ToolCalls => Self::ToolCalls,
            FinishReason::Length => Self::Length,
            FinishReason::Error => Self::Error,
        }
    }
}

/// What one model call cost and how fast it went.
#[derive(Debug, Clone)]
pub struct CallMeasure {
    /// Tokens read and written, and the estimated cost.
    pub usage: TokenUsage,
    /// From dispatch to the last chunk, in milliseconds.
    pub latency_ms: u64,
    /// From dispatch to the first generated chunk. `None` when not measured.
    pub ttft_ms: Option<u64>,
    /// Time spent generating, see [`generation_ms`]. `None` when not measured.
    pub generation_ms: Option<u64>,
    /// Why the call ended.
    pub end: CallEnd,
}

impl CallMeasure {
    /// The measure of a call answered in one piece.
    #[must_use]
    pub fn of_response(response: &CompletionResponse, latency_ms: u64) -> Self {
        Self {
            usage: response.usage.clone(),
            latency_ms,
            ttft_ms: response.ttft_ms,
            generation_ms: generation_ms(
                latency_ms,
                response.ttft_ms,
                response.engine_timings.as_ref(),
            ),
            end: CallEnd::from(&response.finish_reason),
        }
    }
}

/// Time a call spent generating its answer, in milliseconds.
///
/// The engine's own decode time when it reports one (`timings.predicted_ms` of
/// llama-server); otherwise the latency minus the time to first token; `None`
/// when neither is known, since the latency alone would count the prefill as
/// generation.
#[must_use]
pub fn generation_ms(
    latency_ms: u64,
    ttft_ms: Option<u64>,
    engine_timings: Option<&serde_json::Value>,
) -> Option<u64> {
    let reported = engine_timings
        .and_then(|t| t.get("predicted_ms"))
        .and_then(serde_json::Value::as_f64)
        .filter(|ms| ms.is_finite() && *ms >= 0.0);
    if let Some(ms) = reported {
        return Some(ms.round() as u64);
    }
    ttft_ms.map(|ttft| latency_ms.saturating_sub(ttft))
}

/// Measures a streamed call chunk by chunk.
///
/// Started at dispatch, fed every item the stream yields, and closed once,
/// either at the end of the stream or when the reader lets go of it.
#[derive(Debug)]
pub struct CallMeter {
    started: Instant,
    ttft_ms: Option<u64>,
    usage: TokenUsage,
    timings: Option<serde_json::Value>,
    finish: Option<FinishReason>,
    saw_tool_call: bool,
    failed: bool,
}

impl CallMeter {
    /// A meter whose clock started at `started`, the dispatch of the request.
    #[must_use]
    pub fn new(started: Instant) -> Self {
        Self {
            started,
            ttft_ms: None,
            usage: TokenUsage::default(),
            timings: None,
            finish: None,
            saw_tool_call: false,
            failed: false,
        }
    }

    /// Account for one chunk of the stream.
    pub fn read_chunk(&mut self, chunk: &StreamChunk) {
        match chunk {
            StreamChunk::Text(_) => self.first_token(),
            StreamChunk::ToolCall(_) => {
                self.first_token();
                self.saw_tool_call = true;
            }
            StreamChunk::Usage(usage) => self.usage.merge(usage),
            StreamChunk::Timings(timings) => self.timings = Some(timings.clone()),
            StreamChunk::Finish(reason) => self.finish = Some(reason.clone()),
        }
    }

    /// Record that the stream yielded an error.
    pub fn observe_error(&mut self) {
        self.failed = true;
    }

    /// Whether the stream yielded an error.
    #[must_use]
    pub fn has_failed(&self) -> bool {
        self.failed
    }

    /// Close the measure of a stream read to its end.
    #[must_use]
    pub fn finished(&self) -> CallMeasure {
        let end = if self.failed {
            CallEnd::Error
        } else {
            match &self.finish {
                Some(reason) => CallEnd::from(reason),
                // A backend that does not say why it stopped: a tool call is
                // the only end it shows otherwise.
                None if self.saw_tool_call => CallEnd::ToolCalls,
                None => CallEnd::Stop,
            }
        };
        self.close(end)
    }

    /// Close the measure of a stream its reader let go of before the end.
    #[must_use]
    pub fn cancelled(&self) -> CallMeasure {
        self.close(CallEnd::Cancelled)
    }

    fn first_token(&mut self) {
        if self.ttft_ms.is_none() {
            self.ttft_ms = Some(elapsed_ms(self.started));
        }
    }

    fn close(&self, end: CallEnd) -> CallMeasure {
        let latency_ms = elapsed_ms(self.started);
        CallMeasure {
            usage: self.usage.clone(),
            latency_ms,
            ttft_ms: self.ttft_ms,
            generation_ms: generation_ms(latency_ms, self.ttft_ms, self.timings.as_ref()),
            end,
        }
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `timings` object of a llama-server answer, as recorded from the
    /// embedded engine on a 34-token answer.
    fn llama_server_timings() -> serde_json::Value {
        serde_json::json!({
            "cache_n": 0, "prompt_n": 412, "prompt_ms": 187.311,
            "predicted_n": 34, "predicted_ms": 512.659,
            "predicted_per_second": 66.32
        })
    }

    fn response(ttft_ms: Option<u64>, timings: Option<serde_json::Value>) -> CompletionResponse {
        CompletionResponse {
            content: "ok".into(),
            tool_calls: Vec::new(),
            usage: TokenUsage {
                prompt_tokens: 412,
                completion_tokens: 34,
                ..Default::default()
            },
            finish_reason: FinishReason::Length,
            latency_ms: 900,
            ttft_ms,
            engine_timings: timings,
        }
    }

    #[test]
    fn test_generation_time_is_the_engine_decode_time_when_reported() {
        // GIVEN a recorded llama-server answer carrying its timings
        let answer = response(Some(250), Some(llama_server_timings()));

        // WHEN it is measured
        let measure = CallMeasure::of_response(&answer, 900);

        // THEN the generation time is predicted_ms, not latency minus TTFT
        assert_eq!(measure.generation_ms, Some(513));
        assert_eq!(measure.ttft_ms, Some(250));
        assert_eq!(measure.end, CallEnd::Length);
    }

    #[test]
    fn test_generation_time_falls_back_on_latency_minus_ttft() {
        // GIVEN a recorded cloud answer: a TTFT, no engine timings
        let answer = response(Some(300), None);

        // WHEN it is measured
        let measure = CallMeasure::of_response(&answer, 900);

        // THEN the generation time is what follows the first token
        assert_eq!(measure.generation_ms, Some(600));
    }

    #[test]
    fn test_unknown_ttft_stays_unknown_never_zero() {
        // GIVEN an answer in one piece, with neither TTFT nor timings
        let answer = response(None, None);

        // WHEN it is measured
        let measure = CallMeasure::of_response(&answer, 900);

        // THEN both durations are unknown rather than zero or the latency
        assert_eq!(measure.ttft_ms, None);
        assert_eq!(measure.generation_ms, None);
    }

    #[test]
    fn test_a_stream_without_a_finish_chunk_ends_on_its_tool_call() {
        // GIVEN a meter fed a tool call and no finish reason
        let mut meter = CallMeter::new(Instant::now());
        meter.read_chunk(&StreamChunk::ToolCall(crate::types::ToolCall {
            id: "c".into(),
            name: "web_search".into(),
            arguments: serde_json::json!({}),
        }));

        // WHEN the stream ends
        let measure = meter.finished();

        // THEN the call ended on tools, and its first chunk was timed
        assert_eq!(measure.end, CallEnd::ToolCalls);
        assert!(measure.ttft_ms.is_some());
    }
}
