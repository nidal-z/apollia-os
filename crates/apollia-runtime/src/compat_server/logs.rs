//! The logs of the compatible server: a bounded ring of its recent lines, and
//! their live stream.
//!
//! Two sources write here: one line per request the compatible server
//! answered (method, path, model, status, duration), and every line the local
//! engines print (`llama.server.line`). The ring keeps the most recent
//! [`DEFAULT_CAPACITY`] lines; past that, the oldest goes first. A reader
//! subscribes with a minimum level and receives what the ring holds, then each
//! new line as it arrives.
//!
//! This is a developer's view of the server, held in memory only. Clearing it
//! empties the ring and nothing else: the audit journal is a separate store
//! this module never reads nor writes.
//!
//! A writer never waits: a request or an engine pipe pushes with `try_send`,
//! and a line that finds the actor's inbox full is dropped rather than slowing
//! the request or stalling the pipe. A subscriber that falls behind loses the
//! lines its channel cannot hold, for the same reason.

use std::collections::VecDeque;
use std::time::Instant;

use apollia_core::RuntimeEvent;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use crate::eventbus::EventBusSender;

/// How many lines the ring holds before the oldest goes.
pub const DEFAULT_CAPACITY: usize = 2000;

/// Capacity of the actor's inbox. Every request and every engine line goes
/// through it, so it is sized for a burst of engine output.
const COMMAND_CAPACITY: usize = 1024;

/// Capacity of one subscriber's channel of live lines.
const SUBSCRIBER_CAPACITY: usize = 256;

/// How serious a line is, in increasing order.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    utoipa::ToSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum ServerLogLevel {
    /// Detail for whoever debugs the server.
    #[default]
    Debug,
    /// A request answered, an engine at work.
    Info,
    /// A request refused (4xx), an engine warning.
    Warn,
    /// A request failed (5xx), an engine error.
    Error,
}

/// Where a line comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ServerLogSource {
    /// A request the compatible server answered.
    Request,
    /// A line a local engine printed.
    Engine,
}

/// One line of the server's logs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServerLogLine {
    /// Position of the line since the runtime started; it keeps growing
    /// across a clear.
    pub seq: u64,
    /// When the line was recorded, RFC 3339 in UTC.
    pub timestamp: String,
    /// How serious it is.
    pub level: ServerLogLevel,
    /// Where it comes from.
    pub source: ServerLogSource,
    /// The HTTP method of a request line.
    pub method: Option<String>,
    /// The path a request line asked for.
    pub endpoint: Option<String>,
    /// The model a request line named, when it named one.
    pub model: Option<String>,
    /// The status a request line was answered with.
    pub status: Option<u16>,
    /// How long the server took to answer, in milliseconds. For a streamed
    /// answer, the time until the stream opened.
    pub duration_ms: Option<u64>,
    /// The text an engine printed.
    pub message: Option<String>,
}

/// The model a relayed request named, left on its response for the request
/// line.
#[derive(Debug, Clone)]
pub(crate) struct RequestedModel(pub(crate) String);

/// Why the logs could not be read or cleared.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServerLogsError {
    /// The actor that holds the logs has stopped with the runtime.
    #[error("the server logs have stopped with the runtime")]
    ActorGone,
}

/// What a reader receives when it subscribes.
#[derive(Debug)]
pub struct ServerLogSubscription {
    /// The lines the ring held at the minimum level, oldest first.
    pub backlog: Vec<ServerLogLine>,
    /// Each new line at the minimum level, as it is recorded.
    pub live: mpsc::Receiver<ServerLogLine>,
}

/// A line before the actor numbers it.
#[derive(Debug)]
struct Draft {
    level: ServerLogLevel,
    source: ServerLogSource,
    method: Option<String>,
    endpoint: Option<String>,
    model: Option<String>,
    status: Option<u16>,
    duration_ms: Option<u64>,
    message: Option<String>,
}

enum Command {
    Push(Draft),
    Subscribe {
        min_level: ServerLogLevel,
        reply: oneshot::Sender<ServerLogSubscription>,
    },
    Clear(oneshot::Sender<usize>),
}

/// Handle to the actor that holds the server's logs.
#[derive(Debug, Clone)]
pub struct ServerLogsHandle {
    tx: mpsc::Sender<Command>,
}

impl ServerLogsHandle {
    /// Spawns the actor with a ring of [`DEFAULT_CAPACITY`] lines. It stops
    /// when the bus carries [`RuntimeEvent::ShutdownRequested`] or every
    /// handle is dropped, which ends the live streams.
    #[must_use]
    pub fn spawn(bus: &EventBusSender) -> Self {
        Self::spawn_with_capacity(bus, DEFAULT_CAPACITY)
    }

    /// Spawns the actor with a ring of `capacity` lines, at least one.
    #[must_use]
    pub fn spawn_with_capacity(bus: &EventBusSender, capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(COMMAND_CAPACITY);
        let events = apollia_core::events::subscribe_resilient(bus, "compat_server.logs");
        let capacity = capacity.max(1);
        let actor = LogsActor {
            capacity,
            ring: VecDeque::with_capacity(capacity),
            next_seq: 0,
            subscribers: Vec::new(),
        };
        tokio::spawn(actor.run(rx, events));
        Self { tx }
    }

    /// Records a line a local engine printed.
    pub fn engine_line(&self, line: &str) {
        self.push(Draft {
            level: engine_level(line),
            source: ServerLogSource::Engine,
            method: None,
            endpoint: None,
            model: None,
            status: None,
            duration_ms: None,
            message: Some(line.to_owned()),
        });
    }

    fn push(&self, draft: Draft) {
        if self.tx.try_send(Command::Push(draft)).is_err() {
            tracing::event!(tracing::Level::DEBUG, "compat_server.logs.line_dropped");
        }
    }

    /// The lines the ring holds at `min_level` or above, then each new one.
    ///
    /// # Errors
    ///
    /// [`ServerLogsError::ActorGone`] once the runtime has stopped.
    pub async fn subscribe(
        &self,
        min_level: ServerLogLevel,
    ) -> Result<ServerLogSubscription, ServerLogsError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(Command::Subscribe { min_level, reply })
            .await
            .map_err(|_| ServerLogsError::ActorGone)?;
        rx.await.map_err(|_| ServerLogsError::ActorGone)
    }

    /// Empties the ring and answers how many lines it held. Only the ring:
    /// the audit journal is never touched.
    ///
    /// # Errors
    ///
    /// [`ServerLogsError::ActorGone`] once the runtime has stopped.
    pub async fn clear(&self) -> Result<usize, ServerLogsError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(Command::Clear(reply))
            .await
            .map_err(|_| ServerLogsError::ActorGone)?;
        rx.await.map_err(|_| ServerLogsError::ActorGone)
    }
}

/// The line of a request a client sent, and how it was answered: a refusal
/// (4xx) at the warning level, a failure (5xx) at the error level.
fn request_line(
    method: String,
    endpoint: String,
    model: Option<String>,
    status: u16,
    duration_ms: u64,
) -> Draft {
    let level = match status {
        500.. => ServerLogLevel::Error,
        400..=499 => ServerLogLevel::Warn,
        _ => ServerLogLevel::Info,
    };
    Draft {
        level,
        source: ServerLogSource::Request,
        method: Some(method),
        endpoint: Some(endpoint),
        model,
        status: Some(status),
        duration_ms: Some(duration_ms),
        message: None,
    }
}

/// The level an engine line reads as. `llama-server` prints no level of its
/// own on most lines, so its words decide: an error or a failure, a warning,
/// anything else as information.
fn engine_level(line: &str) -> ServerLogLevel {
    let lower = line.to_ascii_lowercase();
    if lower.contains("error") || lower.contains("failed") {
        ServerLogLevel::Error
    } else if lower.contains("warn") {
        ServerLogLevel::Warn
    } else {
        ServerLogLevel::Info
    }
}

struct Subscriber {
    min_level: ServerLogLevel,
    tx: mpsc::Sender<ServerLogLine>,
}

struct LogsActor {
    capacity: usize,
    ring: VecDeque<ServerLogLine>,
    next_seq: u64,
    subscribers: Vec<Subscriber>,
}

impl LogsActor {
    async fn run(
        mut self,
        mut rx: mpsc::Receiver<Command>,
        mut events: apollia_core::events::ResilientReceiver,
    ) {
        loop {
            tokio::select! {
                biased;
                event = events.recv() => match event {
                    Some(RuntimeEvent::ShutdownRequested) | None => break,
                    Some(_) => {}
                },
                command = rx.recv() => match command {
                    Some(Command::Push(draft)) => self.push(draft),
                    Some(Command::Subscribe { min_level, reply }) => {
                        let _ = reply.send(self.subscribe(min_level));
                    }
                    Some(Command::Clear(reply)) => {
                        let cleared = self.ring.len();
                        self.ring.clear();
                        let _ = reply.send(cleared);
                    }
                    None => break,
                },
            }
        }
    }

    fn push(&mut self, draft: Draft) {
        let line = ServerLogLine {
            seq: self.next_seq,
            timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            level: draft.level,
            source: draft.source,
            method: draft.method,
            endpoint: draft.endpoint,
            model: draft.model,
            status: draft.status,
            duration_ms: draft.duration_ms,
            message: draft.message,
        };
        self.next_seq += 1;
        self.subscribers.retain(|subscriber| {
            if line.level < subscriber.min_level {
                return !subscriber.tx.is_closed();
            }
            match subscriber.tx.try_send(line.clone()) {
                Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => true,
                Err(mpsc::error::TrySendError::Closed(_)) => false,
            }
        });
        if self.ring.len() == self.capacity {
            self.ring.pop_front();
        }
        self.ring.push_back(line);
    }

    fn subscribe(&mut self, min_level: ServerLogLevel) -> ServerLogSubscription {
        let backlog = self
            .ring
            .iter()
            .filter(|line| line.level >= min_level)
            .cloned()
            .collect();
        let (tx, live) = mpsc::channel(SUBSCRIBER_CAPACITY);
        self.subscribers.push(Subscriber { min_level, tx });
        ServerLogSubscription { backlog, live }
    }
}

/// Middleware of the compatible server: one line per request, with the model
/// the relay left on the response.
pub(crate) async fn record_request(
    State(logs): State<ServerLogsHandle>,
    request: Request,
    next: Next,
) -> Response {
    let started = Instant::now();
    let method = request.method().to_string();
    let endpoint = request.uri().path().to_owned();
    let response = next.run(request).await;
    let model = response
        .extensions()
        .get::<RequestedModel>()
        .map(|model| model.0.clone());
    let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    logs.push(request_line(
        method,
        endpoint,
        model,
        response.status().as_u16(),
        duration_ms,
    ));
    response
}
