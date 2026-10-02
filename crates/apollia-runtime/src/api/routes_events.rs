//! The whole event bus as one SSE stream, `GET /api/v1/events`.
//!
//! The per-task and per-session streams filter the bus and translate each event
//! into a narrower wire payload. A client that renders the runtime as a whole,
//! the desktop attached to a daemon first, needs every event in the form the
//! bus carries it, so this stream forwards each [`RuntimeEvent`] as its own
//! serde JSON, one `runtime` frame per event, and never closes on its own.
//!
//! A subscriber that falls behind the bus loses events. The loss is sent as a
//! `lagged` frame carrying the count, so the client can tell a quiet runtime
//! from a stream that dropped part of what it carried.

use std::convert::Infallible;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use tokio_stream::wrappers::{errors::BroadcastStreamRecvError, BroadcastStream};
use tokio_stream::StreamExt;

use apollia_core::RuntimeEvent;

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// SSE event name of a frame that carries one [`RuntimeEvent`].
pub const RUNTIME_FRAME: &str = "runtime";

/// SSE event name of a frame that reports events dropped for this subscriber.
pub const LAGGED_FRAME: &str = "lagged";

/// The SSE frame for one item read from the bus, `None` when the event cannot
/// be serialized.
fn frame_of(item: Result<RuntimeEvent, BroadcastStreamRecvError>) -> Option<Event> {
    match item {
        Ok(event) => match serde_json::to_string(&event) {
            Ok(json) => Some(Event::default().event(RUNTIME_FRAME).data(json)),
            Err(error) => {
                tracing::warn!(error = %error, "api.events_sse.serialize_failed");
                None
            }
        },
        Err(BroadcastStreamRecvError::Lagged(skipped)) => {
            tracing::warn!(subscriber = "api.events_sse", skipped, "eventbus.lagged");
            Some(
                Event::default()
                    .event(LAGGED_FRAME)
                    .data(serde_json::json!({ "skipped": skipped }).to_string()),
            )
        }
    }
}

/// `GET /api/v1/events`, every runtime event as SSE.
///
/// Each `runtime` frame carries one event of the bus serialized as the bus
/// carries it; a `lagged` frame reports how many events this subscriber
/// missed. The stream stays open until the client disconnects.
#[utoipa::path(
    get,
    path = "/api/v1/events",
    tag = "events",
    responses((
        status = 200,
        description = "SSE stream of every runtime event: `runtime` frames carry one event each, `lagged` frames the count of events dropped for this subscriber",
        content_type = "text/event-stream"
    ))
)]
pub async fn stream_events<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(state.event_sender.subscribe())
        .filter_map(frame_of)
        .map(Ok::<_, Infallible>);
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_lag_is_named_in_its_own_frame_rather_than_dropped() {
        // GIVEN a subscriber that missed seven events
        let item = Err(BroadcastStreamRecvError::Lagged(7));

        // WHEN the item is turned into a frame
        let frame = frame_of(item);

        // THEN a frame is sent, so the loss reaches the client
        assert!(frame.is_some(), "a lag must produce a frame");
    }

    #[test]
    fn test_an_event_becomes_a_runtime_frame() {
        // GIVEN an event of the bus
        let item = Ok(RuntimeEvent::ShutdownRequested);

        // WHEN the item is turned into a frame
        let frame = frame_of(item);

        // THEN a frame carries it
        assert!(frame.is_some(), "an event must produce a frame");
    }
}
