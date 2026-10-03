//! A streamed call, measured as it is read and published when it ends.
//!
//! The router hands its caller the backend's stream wrapped in an
//! [`ObservedStream`]: every chunk passes through untouched, a [`CallMeter`]
//! reads it on the way, and `LlmCallCompleted` goes out once, when the stream
//! ends or when the reader drops it first (a user's Stop, which the measure
//! records as `cancelled`).

use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use futures::Stream;

use apollia_core::events::{EventBusSender, RuntimeEvent};

use crate::call_measure::{CallContext, CallMeasure, CallMeter};
use crate::types::{LlmError, StreamChunk};

/// The stream type every backend returns.
pub(crate) type ChunkStream = Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>;

/// Who answered a streamed call, and for which turn.
#[derive(Debug, Clone)]
pub struct ObservedCall {
    /// Logical name of the backend that answers.
    pub backend: String,
    /// Model the backend serves.
    pub model: String,
    /// The conversation turn, from the request.
    pub context: Option<CallContext>,
    /// Where `LlmCallCompleted` goes. `None` measures without publishing.
    pub bus: Option<EventBusSender>,
    /// When the request was dispatched, the origin of every duration.
    pub started: Instant,
}

/// The `LlmCallCompleted` a measured call publishes.
#[must_use]
pub fn call_completed_event(
    backend: &str,
    model: &str,
    context: Option<&CallContext>,
    measure: &CallMeasure,
) -> RuntimeEvent {
    RuntimeEvent::LlmCallCompleted {
        backend: backend.to_owned(),
        model: model.to_owned(),
        task_id: None,
        step_id: None,
        prompt_tokens: measure.usage.prompt_tokens,
        completion_tokens: measure.usage.completion_tokens,
        latency_ms: measure.latency_ms,
        cost_usd: measure.usage.cost_usd,
        run_id: None,
        session_id: context.map(|c| c.session_id.clone()),
        turn_id: context.map(|c| c.turn_id.clone()),
        ttft_ms: measure.ttft_ms,
        finish_reason: Some(measure.end.as_str().to_owned()),
        generation_ms: measure.generation_ms,
    }
}

/// Wrap a backend's stream so the call is measured and published.
pub fn observe_stream(
    inner: Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>,
    call: ObservedCall,
) -> Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>> {
    let meter = CallMeter::new(call.started);
    Box::pin(ObservedStream {
        inner,
        meter,
        call,
        published: false,
    })
}

struct ObservedStream {
    inner: ChunkStream,
    meter: CallMeter,
    call: ObservedCall,
    published: bool,
}

impl ObservedStream {
    fn publish(&mut self, measure: &CallMeasure) {
        if self.published {
            return;
        }
        self.published = true;
        if let Some(bus) = &self.call.bus {
            // Fire-and-forget, as on the complete path: a bus without
            // subscribers is not the caller's failure.
            let _ = bus.send(call_completed_event(
                &self.call.backend,
                &self.call.model,
                self.call.context.as_ref(),
                measure,
            ));
        }
    }
}

impl Stream for ObservedStream {
    type Item = Result<StreamChunk, LlmError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let polled = this.inner.as_mut().poll_next(cx);
        match &polled {
            Poll::Ready(Some(Ok(chunk))) => this.meter.read_chunk(chunk),
            Poll::Ready(Some(Err(_))) => this.meter.observe_error(),
            Poll::Ready(None) => {
                let measure = this.meter.finished();
                this.publish(&measure);
            }
            Poll::Pending => {}
        }
        polled
    }
}

impl Drop for ObservedStream {
    fn drop(&mut self) {
        if !self.published {
            let measure = if self.meter.has_failed() {
                self.meter.finished()
            } else {
                self.meter.cancelled()
            };
            self.publish(&measure);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures::StreamExt;

    use crate::types::{FinishReason, TokenUsage};

    fn chunks(items: Vec<Result<StreamChunk, LlmError>>) -> ChunkStream {
        Box::pin(futures::stream::iter(items))
    }

    fn call(bus: &EventBusSender) -> ObservedCall {
        ObservedCall {
            backend: "local".into(),
            model: "qwen".into(),
            context: Some(CallContext {
                session_id: "s-1".into(),
                turn_id: "a-1".into(),
            }),
            bus: Some(bus.clone()),
            started: Instant::now(),
        }
    }

    #[tokio::test]
    async fn test_a_stream_read_to_its_end_publishes_its_turn_and_measure() {
        // GIVEN a stream that yields text, usage, the engine timings and a stop
        let (bus, mut rx) = tokio::sync::broadcast::channel(8);
        let inner = chunks(vec![
            Ok(StreamChunk::Text("Bonjour".into())),
            Ok(StreamChunk::Usage(TokenUsage {
                prompt_tokens: 12,
                completion_tokens: 3,
                ..Default::default()
            })),
            Ok(StreamChunk::Timings(
                serde_json::json!({ "predicted_n": 3, "predicted_ms": 41.6 }),
            )),
            Ok(StreamChunk::Finish(FinishReason::Stop)),
        ]);

        // WHEN the caller reads it to the end
        let read: Vec<_> = observe_stream(inner, call(&bus)).collect().await;

        // THEN every chunk passes through and one LlmCallCompleted carries the
        // turn, the tokens, a measured TTFT, the engine's decode time and the stop
        assert_eq!(read.len(), 4);
        match rx.try_recv().expect("one event") {
            RuntimeEvent::LlmCallCompleted {
                session_id,
                turn_id,
                prompt_tokens,
                completion_tokens,
                ttft_ms,
                finish_reason,
                generation_ms,
                ..
            } => {
                assert_eq!(session_id.as_deref(), Some("s-1"));
                assert_eq!(turn_id.as_deref(), Some("a-1"));
                assert_eq!((prompt_tokens, completion_tokens), (12, 3));
                assert!(ttft_ms.is_some());
                assert_eq!(finish_reason.as_deref(), Some("stop"));
                assert_eq!(generation_ms, Some(42));
            }
            other => panic!("unexpected event {other:?}"),
        }
        assert!(rx.try_recv().is_err(), "published once");
    }

    #[tokio::test]
    async fn test_a_stream_dropped_before_its_end_is_published_as_cancelled() {
        // GIVEN a stream with more to say
        let (bus, mut rx) = tokio::sync::broadcast::channel(8);
        let inner = chunks(vec![
            Ok(StreamChunk::Text("un".into())),
            Ok(StreamChunk::Text("deux".into())),
        ]);
        let mut observed = observe_stream(inner, call(&bus));

        // WHEN the reader takes one chunk and lets go, as a Stop does
        let _ = observed.next().await;
        drop(observed);

        // THEN the call is published once, as cancelled
        match rx.try_recv().expect("one event") {
            RuntimeEvent::LlmCallCompleted { finish_reason, .. } => {
                assert_eq!(finish_reason.as_deref(), Some("cancelled"));
            }
            other => panic!("unexpected event {other:?}"),
        }
        assert!(rx.try_recv().is_err(), "published once");
    }

    #[tokio::test]
    async fn test_a_stream_that_never_produced_a_token_has_no_ttft() {
        // GIVEN a stream that fails before any token
        let (bus, mut rx) = tokio::sync::broadcast::channel(8);
        let inner = chunks(vec![Err(LlmError::InferenceError("engine gone".into()))]);

        // WHEN it is read to its end
        let _read: Vec<_> = observe_stream(inner, call(&bus)).collect().await;

        // THEN the call ends in error, with neither TTFT nor generation time
        match rx.try_recv().expect("one event") {
            RuntimeEvent::LlmCallCompleted {
                finish_reason,
                ttft_ms,
                generation_ms,
                ..
            } => {
                assert_eq!(finish_reason.as_deref(), Some("error"));
                assert_eq!(ttft_ms, None);
                assert_eq!(generation_ms, None);
            }
            other => panic!("unexpected event {other:?}"),
        }
    }
}
