//! What keeps an attached application informed: the event stream of the
//! runtime relayed onto a local bus, and a liveness probe that asks rather than
//! assumes.
//!
//! The relay is an actor of its own. It reads `GET /api/v1/events`, decodes
//! each `runtime` frame back into the [`RuntimeEvent`] the daemon's bus
//! carried, and publishes it on a bus local to this process. Everything the
//! embedded desktop hangs off its runtime's bus, the bridge to the webview
//! first, hangs off this one unchanged. A stream that ends or fails is
//! reopened after a pause, so a daemon restarted under the application is
//! picked up again.
//!
//! The liveness probe asks the health route on a short cadence. A heartbeat is
//! emitted only after an answer, and the first unanswered probe emits
//! `runtime:lost`, which the interface turns into its disconnected banner at
//! once rather than after its own heartbeat timeout.

use std::time::Duration;

use apollia_core::events::RuntimeEvent;
use apollia_core::EventBusSender;
use tauri::{AppHandle, Emitter};

use super::client::AttachedClient;

/// How long the relay waits before reopening a stream that ended or failed.
const REOPEN_DELAY: Duration = Duration::from_secs(1);

/// How often the liveness probe asks the runtime.
const LIVENESS_PERIOD: Duration = Duration::from_secs(2);

/// Upper bound on one SSE frame held while it is incomplete. A frame larger
/// than this is a stream this relay cannot read, and the connection is
/// dropped and reopened.
const MAX_PENDING_BYTES: usize = 8 * 1024 * 1024;

/// SSE event name of a frame carrying one runtime event, as the runtime names it.
const RUNTIME_FRAME: &str = "runtime";

/// SSE event name of a frame reporting events dropped for this subscriber.
const LAGGED_FRAME: &str = "lagged";

/// One complete SSE frame.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Frame {
    /// The `event:` field, `message` when the frame names none.
    pub event: String,
    /// The `data:` lines, joined by a newline.
    pub data: String,
}

/// Incremental reader of an SSE body.
#[derive(Debug, Default)]
pub(crate) struct SseReader {
    pending: String,
}

/// Why a stream cannot be read further.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ReadStop {
    /// An incomplete frame grew past [`MAX_PENDING_BYTES`].
    Oversized,
}

impl SseReader {
    /// Feeds one chunk of the body and returns the frames it completed.
    pub(crate) fn push(&mut self, chunk: &[u8]) -> Result<Vec<Frame>, ReadStop> {
        self.pending
            .push_str(&String::from_utf8_lossy(chunk).replace('\r', ""));
        let mut frames = Vec::new();
        while let Some(end) = self.pending.find("\n\n") {
            let block: String = self.pending.drain(..end + 2).collect();
            if let Some(frame) = parse_block(&block) {
                frames.push(frame);
            }
        }
        if self.pending.len() > MAX_PENDING_BYTES {
            return Err(ReadStop::Oversized);
        }
        Ok(frames)
    }
}

/// Parses one blank-line-terminated block, `None` for a comment-only block
/// such as the keep-alive.
fn parse_block(block: &str) -> Option<Frame> {
    let mut event = None;
    let mut data: Vec<&str> = Vec::new();
    for line in block.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event = Some(value.trim_start().to_owned());
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.strip_prefix(' ').unwrap_or(value));
        }
    }
    if event.is_none() && data.is_empty() {
        return None;
    }
    Some(Frame {
        event: event.unwrap_or_else(|| "message".to_owned()),
        data: data.join("\n"),
    })
}

/// The runtime event a frame carries, `None` for any other frame.
pub(crate) fn decode(frame: &Frame) -> Option<RuntimeEvent> {
    match frame.event.as_str() {
        RUNTIME_FRAME => match serde_json::from_str(&frame.data) {
            Ok(event) => Some(event),
            Err(error) => {
                tracing::event!(
                    tracing::Level::WARN,
                    error = %error,
                    "desktop.attached.event_undecodable"
                );
                None
            }
        },
        LAGGED_FRAME => {
            tracing::event!(
                tracing::Level::WARN,
                detail = %frame.data,
                "desktop.attached.events_lagged"
            );
            None
        }
        _ => None,
    }
}

/// Spawns the relay actor: the runtime's event stream onto `bus`, for the
/// lifetime of the application.
pub(crate) fn spawn_event_relay(client: AttachedClient, bus: EventBusSender) {
    tauri::async_runtime::spawn(async move {
        loop {
            match client.open_events().await {
                Ok(response) => {
                    tracing::event!(tracing::Level::INFO, "desktop.attached.events_opened");
                    relay_stream(response, &bus).await;
                }
                Err(error) => tracing::event!(
                    tracing::Level::DEBUG,
                    error = %error,
                    "desktop.attached.events_unavailable"
                ),
            }
            tokio::time::sleep(REOPEN_DELAY).await;
        }
    });
}

/// Relays one open stream until it ends, fails or overflows.
async fn relay_stream(mut response: reqwest::Response, bus: &EventBusSender) {
    let mut reader = SseReader::default();
    loop {
        // SAFETY: an open-ended event stream has no total size to cap; each
        // frame is bounded instead, by `MAX_PENDING_BYTES` in `SseReader`.
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(error) => {
                tracing::event!(
                    tracing::Level::DEBUG,
                    error = %error,
                    "desktop.attached.events_interrupted"
                );
                break;
            }
        };
        let frames = match reader.push(&chunk) {
            Ok(frames) => frames,
            Err(stop) => {
                tracing::event!(
                    tracing::Level::WARN,
                    stop = ?stop,
                    "desktop.attached.events_unreadable"
                );
                break;
            }
        };
        for event in frames.iter().filter_map(decode) {
            // No subscriber yet is not an error: the bridge subscribes at setup.
            let _ = bus.send(event);
        }
    }
    tracing::event!(tracing::Level::INFO, "desktop.attached.events_closed");
}

/// What one liveness probe tells the webview.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Liveness {
    /// The runtime answered: `runtime:heartbeat`.
    Heartbeat,
    /// The runtime stopped answering: `runtime:lost`.
    Lost,
}

/// What one liveness probe emits, given whether the previous probe was
/// answered.
pub(crate) fn liveness_signal(was_alive: bool, alive: bool) -> Option<Liveness> {
    match (was_alive, alive) {
        (_, true) => Some(Liveness::Heartbeat),
        (true, false) => Some(Liveness::Lost),
        (false, false) => None,
    }
}

/// Spawns the liveness probe for the lifetime of the application.
pub(crate) fn spawn_liveness(app: AppHandle, client: AttachedClient) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(LIVENESS_PERIOD);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut was_alive = true;
        loop {
            ticker.tick().await;
            let alive = client.is_alive().await;
            if was_alive && !alive {
                tracing::event!(tracing::Level::WARN, "desktop.attached.runtime_lost");
            } else if !was_alive && alive {
                tracing::event!(tracing::Level::INFO, "desktop.attached.runtime_back");
            }
            let emitted = match liveness_signal(was_alive, alive) {
                Some(Liveness::Heartbeat) => Some(app.emit("runtime:heartbeat", ())),
                Some(Liveness::Lost) => Some(app.emit("runtime:lost", ())),
                None => None,
            };
            if let Some(Err(error)) = emitted {
                tracing::event!(
                    tracing::Level::DEBUG,
                    error = %error,
                    "desktop.attached.liveness_emit_failed"
                );
            }
            was_alive = alive;
        }
    });
}
