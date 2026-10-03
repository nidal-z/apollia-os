//! The whole event bus as one SSE stream, `GET /api/v1/events`.
//!
//! The per-task and per-session streams filter the bus and translate each event
//! into a narrower wire payload. A client that renders the runtime as a whole,
//! the desktop attached to a daemon first, needs every event in the form the
//! bus carries it, so this stream forwards each [`RuntimeEvent`] as its own
//! serde JSON, one `runtime` frame per event, and never closes on its own.
//!
//! A client that opens leases on local models (see
//! [`crate::residency`]) names itself with `?client=`: when its stream ends,
//! every lease it held is released, so a closed window keeps no model loaded.
//!
//! A subscriber that falls behind the bus loses events. The loss is sent as a
//! `lagged` frame carrying the count, so the client can tell a quiet runtime
//! from a stream that dropped part of what it carried.

use std::convert::Infallible;

use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use tokio_stream::wrappers::{errors::BroadcastStreamRecvError, BroadcastStream};
use tokio_stream::StreamExt;

use apollia_core::RuntimeEvent;

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Query of `GET /api/v1/events`.
#[derive(Debug, Default, serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct EventsQuery {
    /// The client holding the stream, as it names itself when it opens
    /// leases on local models. When the stream ends, its leases are released.
    #[serde(default)]
    pub client: Option<String>,
}

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
/// missed. The stream stays open until the client disconnects, which releases
/// the leases of the `client` it names.
#[utoipa::path(
    get,
    path = "/api/v1/events",
    tag = "events",
    params(EventsQuery),
    responses((
        status = 200,
        description = "SSE stream of every runtime event: `runtime` frames carry one event each, `lagged` frames the count of events dropped for this subscriber",
        content_type = "text/event-stream"
    ))
)]
pub async fn stream_events<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<EventsQuery>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let leases = query
        .client
        .filter(|client| !client.trim().is_empty())
        .and_then(|client| {
            crate::residency::ClientLeases::new(
                client,
                state.llama_server_supervisor.clone(),
                state.mlx_engine.clone(),
            )
        });
    // The guard lives as long as the stream: dropping the stream, when the
    // client goes away, releases its leases.
    let stream = BroadcastStream::new(state.event_sender.subscribe())
        .filter_map(frame_of)
        .map(move |frame| {
            let _held = &leases;
            Ok::<_, Infallible>(frame)
        });
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

    #[tokio::test]
    async fn test_the_end_of_a_client_s_stream_releases_its_leases() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        // GIVEN a resident model whose only conversation a client holds, and
        //       that client's event stream open
        let sup =
            crate::llama_server::LlamaServerSupervisor::with_stand_in_models(&["/models/a.gguf"])
                .await;
        let residency = crate::residency::Residency::new(None, None, None);
        sup.attach_residency(std::sync::Arc::clone(&residency));
        residency.open_lease("desk", "c1", "/models/a.gguf");
        let mut state = crate::api::routes_llm::test_app_state_no_llm();
        state.llama_server_supervisor = Some(std::sync::Arc::clone(&sup));
        let router = axum::Router::new()
            .route(
                "/api/v1/events",
                axum::routing::get(stream_events::<crate::api::routes_llm::MockBackend>),
            )
            .with_state(state);
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/events?client=desk")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        let held_while_open = residency.is_held("/models/a.gguf");

        // WHEN the client goes away and its stream is dropped
        drop(response);

        // THEN its lease is released at once, which hands the model to the
        //      unload of a last conversation closed
        assert!(held_while_open);
        assert!(!residency.is_held("/models/a.gguf"));
    }
}
